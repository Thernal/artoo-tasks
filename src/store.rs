//! What the plugin has read of the trackers, one for the app: every site named and who the token is there, their
//! boards and each board's columns, epics, sprints and issues, each issue opened with its moves and its edit screen,
//! the people an issue may be given to, the pictures of its attachments. Every read runs in the background, the
//! sites side by side; its result is kept, and the views drawn again.
//!
//! A change is shown at once and sent (`change`): taken back with the tracker's reason on its field when refused.
//! What is kept on the Mac — drafts, an agent's comments waiting for Post, how each board is shown, the boards
//! opened lately, a description being edited — is in the plugin's folder (`keep`).

use crate::cache;
use crate::flights::Flights;
use crate::jira::{self, Site};
use crate::keep;
use crate::model::{
    self, Board, BoardId, BoardInfo, Field, Issue, Kind, Move, News, Person, Row, Write,
};
use crate::page::Page;
use artoo_plugin_sdk::gpui::{
    App, AppContext, Context, Entity, Global, Image, ImageFormat, Task, WeakEntity,
};
use artoo_plugin_sdk::serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

/// How long a board's columns, epics and sprints are taken as read before a refresh asks for them again, seconds.
const INFO_AGE: i64 = 600;

/// Why a board is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// To be shown: read when it was not yet, or only kept from before.
    Show,
    /// The minute's refresh, or the board coming back on screen: its issues, the rest once old.
    Refresh,
    /// Read again by hand (⌘R, Retry): all of it.
    Reload,
    /// Something was written to the tracker: a read on its way may not have it.
    Written,
}

/// A read: on its way, done, or why not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read<T> {
    Reading,
    Done(T),
    Failed(String),
}

impl<T> Read<T> {
    pub fn done(&self) -> Option<&T> {
        match self {
            Read::Done(t) => Some(t),
            _ => None,
        }
    }
}

/// Now, Unix seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A field of an issue, as a change and a refusal are kept by: `state`, `assignee`, `summary`, `labels`, …
pub type FieldKey = (String, String);

/// What someone else changed on an issue since it was opened: who, which fields, when; the fields not looked at yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changed {
    pub who: String,
    pub fields: Vec<String>,
    pub at: i64,
    pub unseen: HashSet<String>,
}

/// A kind of link: `(name, inward, outward)`.
pub type LinkKind = (String, String, String);

pub struct Store {
    /// Every account kept, whichever project reads it.
    pub sites: Vec<Site>,
    /// Which accounts each project's space reads.
    pub links: crate::links::Links,
    /// The project whose space was shown last: the board picked is among its accounts'.
    pub here: Option<String>,
    /// Sites named with no token, said in the space.
    pub missing: Vec<String>,
    /// Who the token is, by site.
    pub me: HashMap<String, Person>,
    /// Every site's boards; failed only when no site's were read.
    pub boards: Option<Read<Vec<Board>>>,
    /// The sites whose boards were not read, and why: said under the others'.
    pub unread: Vec<(String, String)>,
    /// The board picked.
    pub board: Option<BoardId>,
    /// Only the issues assigned to you.
    pub mine: bool,
    /// The people picked beside Mine · All: their issues alone.
    pub faces: Vec<String>,
    pub info: HashMap<BoardId, Read<BoardInfo>>,
    /// Each board's issues, sub-tasks among them.
    pub rows: HashMap<BoardId, Read<Vec<Row>>>,
    /// When each board was read last, for a board shown as last read when its site is not reached.
    pub read_at: HashMap<BoardId, i64>,
    /// Why a board could not be read again, over it as it was.
    pub stale: HashMap<BoardId, String>,
    /// The boards painted as kept on the Mac, not read since the start.
    pub from_disk: HashSet<BoardId>,
    /// When each board's columns, epics and sprints were read last.
    info_at: HashMap<BoardId, i64>,
    row_reads: Flights<BoardId>,
    info_reads: Flights<BoardId>,
    move_reads: Flights<String>,
    /// The boards' lists asked for so far: only the last one asked is taken.
    boards_asked: u64,
    /// The boards read last, as the Mac keeps them.
    cache: cache::Cache,
    /// The pages made, whose screens the minute's loop reads again.
    pages: Vec<WeakEntity<Page>>,
    pub issues: HashMap<String, Read<Issue>>,
    pub moves: HashMap<String, Read<Vec<Move>>>,
    /// When each issue's moves were read, for its workflow's head.
    pub moves_at: HashMap<String, i64>,
    pub screens: HashMap<String, Read<Vec<Field>>>,
    pub people: HashMap<String, Read<Vec<Person>>>,
    pub link_kinds: HashMap<String, Read<Vec<LinkKind>>>,
    pub kinds: HashMap<(String, String), Read<Vec<Kind>>>,
    pub create_screens: HashMap<(String, String, String), Read<Vec<Field>>>,
    pub projects: HashMap<String, Read<Vec<(String, String)>>>,
    pub images: HashMap<String, Read<Arc<Image>>>,
    /// A change the tracker refused, on its field, with its reason.
    pub refused: HashMap<FieldKey, String>,
    /// The fields being saved: their controls dim.
    pub saving: HashSet<FieldKey>,
    /// What others changed on an issue opened.
    pub changed: HashMap<String, Changed>,
    pub kept: keep::Kept,
    /// What others did lately on your issues, by site, the newest first: the Inbox lists it.
    pub news: Vec<(String, News)>,
    /// The news read before, by `news_key`: one not among them is new.
    news_known: HashSet<String>,
    /// The sites whose news was read at least once: what a site's first read finds is not toasted.
    news_read: HashSet<String>,
    /// News new since the last read, not toasted yet.
    pub news_fresh: Vec<(String, News)>,
    /// The news a toast's Open opens: the last toasted.
    pub news_last: Option<(String, String)>,
    /// The news is read as time goes.
    news_on: bool,
    /// The minutes since the news was read.
    news_waited: u32,
}

/// A site's news as it answered.
type SiteNews = (String, Result<Vec<News>, String>);

/// Whether the news is read after `waited` minutes: each minute while in front, every third behind.
fn news_due(front: bool, waited: u32) -> bool {
    waited >= if front { 1 } else { 3 }
}

/// A news' id among the others: its site, issue, kind and change.
pub fn news_key(host: &str, n: &News) -> String {
    format!("{host}:{}:{:?}:{}", n.row.key, n.told, n.id)
}

struct Shared(Entity<Store>);

impl Global for Shared {}

/// The app's store, when it was made.
pub fn peek(cx: &App) -> Option<Entity<Store>> {
    cx.try_global::<Shared>().map(|s| s.0.clone())
}

/// The app's store, made the first time it is asked for.
pub fn get(cx: &mut App) -> Entity<Store> {
    if let Some(shared) = cx.try_global::<Shared>() {
        return shared.0.clone();
    }
    crate::trackers::read_offered(cx);
    let (sites, missing) = jira::sites();
    let kept = keep::Kept::read();
    let cache = keep::folder()
        .map(|dir| cache::Cache::read(dir.join("boards.json")))
        .unwrap_or_default();
    let store = cx.new(|_| Store::blank(sites, missing, kept, cache));
    cx.set_global(Shared(store.clone()));
    store.update(cx, |s, cx| {
        // The board shown last is read beside the boards' list, not after it.
        s.read_boards(cx);
        s.read_board(Ask::Show, cx);
        s.keep_reading(cx);
    });
    store
}

impl Store {
    /// Nothing read yet: the sites named, what is kept.
    fn blank(
        sites: Vec<Site>,
        missing: Vec<String>,
        kept: keep::Kept,
        cache: cache::Cache,
    ) -> Self {
        let mut store = Store {
            sites,
            links: crate::links::read(),
            here: None,
            missing,
            me: HashMap::new(),
            boards: None,
            unread: Vec::new(),
            board: kept.last_board.clone(),
            mine: false,
            faces: Vec::new(),
            info: HashMap::new(),
            rows: HashMap::new(),
            read_at: HashMap::new(),
            stale: HashMap::new(),
            from_disk: HashSet::new(),
            info_at: HashMap::new(),
            row_reads: Flights::default(),
            info_reads: Flights::default(),
            move_reads: Flights::default(),
            boards_asked: 0,
            cache,
            pages: Vec::new(),
            issues: HashMap::new(),
            moves: HashMap::new(),
            moves_at: HashMap::new(),
            screens: HashMap::new(),
            people: HashMap::new(),
            link_kinds: HashMap::new(),
            kinds: HashMap::new(),
            create_screens: HashMap::new(),
            projects: HashMap::new(),
            images: HashMap::new(),
            refused: HashMap::new(),
            saving: HashSet::new(),
            changed: HashMap::new(),
            kept,
            news: Vec::new(),
            news_known: HashSet::new(),
            news_read: HashSet::new(),
            news_fresh: Vec::new(),
            news_last: None,
            news_on: false,
            news_waited: 0,
        };
        store.paint_kept();
        store
    }

    /// The boards kept on the Mac, as read when they were.
    fn paint_kept(&mut self) {
        for kept in &self.cache.boards {
            let id = kept.board.key();
            self.info.insert(id.clone(), Read::Done(kept.info.clone()));
            self.rows.insert(id.clone(), Read::Done(kept.rows.clone()));
            self.read_at.insert(id.clone(), kept.at);
            self.from_disk.insert(id);
        }
    }

    /// The boards kept on the Mac whose accounts are still named.
    fn kept_boards(&self) -> impl Iterator<Item = &Board> {
        (self.cache.boards.iter())
            .map(|s| &s.board)
            .filter(|b| self.sites.iter().any(|s| s.key() == b.site))
    }
}

/// A store with nothing read and nothing kept, for a test.
#[cfg(test)]
pub fn test_store() -> Store {
    Store::blank(
        Vec::new(),
        Vec::new(),
        keep::Kept::default(),
        cache::Cache::default(),
    )
}

/// Reads `what` in the background and keeps it at `put` once in.
fn spawn_read<T: Send + 'static>(
    cx: &mut Context<Store>,
    what: impl FnOnce() -> Result<T, String> + Send + 'static,
    put: impl FnOnce(&mut Store, Result<T, String>, &mut Context<Store>) + 'static,
) {
    read_task(cx, what, put).detach();
}

/// `spawn_read`, given up when its task is dropped.
fn read_task<T: Send + 'static>(
    cx: &mut Context<Store>,
    what: impl FnOnce() -> Result<T, String> + Send + 'static,
    put: impl FnOnce(&mut Store, Result<T, String>, &mut Context<Store>) + 'static,
) -> Task<()> {
    let task = cx.background_spawn(async move { what() });
    cx.spawn(async move |this, cx| {
        let read = task.await;
        this.update(cx, |this, cx| {
            put(this, read, cx);
            cx.notify();
        })
        .ok();
    })
}

fn read_of<T>(r: Result<T, String>) -> Read<T> {
    match r {
        Ok(t) => Read::Done(t),
        Err(why) => Read::Failed(why),
    }
}

impl Store {
    /// The site of `host`, when it is named.
    pub fn site_of(&self, host: &str) -> Option<Site> {
        self.sites.iter().find(|s| s.key() == host).cloned()
    }

    /// Whether `who` is the token's person on `host`.
    pub fn is_me(&self, host: &str, who: Option<&Person>) -> bool {
        who.is_some_and(|p| self.me.get(host).is_some_and(|me| me.id == p.id))
    }

    /// Every site's boards and who the token is there, read once, side by side.
    pub fn read_boards(&mut self, cx: &mut Context<Self>) {
        if self.boards.is_some() || self.sites.is_empty() {
            return;
        }
        self.boards = Some(Read::Reading);
        self.boards_asked += 1;
        let asked = self.boards_asked;
        let reads: Vec<_> = (self.sites.iter().cloned())
            .map(|site| {
                cx.background_spawn(async move {
                    let me = model::myself(&site).ok();
                    (site.key().to_owned(), me, model::boards(&site))
                })
            })
            .collect();
        cx.spawn(async move |this, cx| {
            let (mut boards, mut unread, mut mes) = (Vec::new(), Vec::new(), Vec::new());
            for read in reads {
                let (host, me, got) = read.await;
                if let Some(me) = me {
                    mes.push((host.clone(), me));
                }
                match got {
                    Ok(more) => boards.extend(more),
                    Err(why) => unread.push((host, why)),
                }
            }
            this.update(cx, |this, cx| {
                if this.boards_asked != asked {
                    return;
                }
                this.me.extend(mes);
                this.add_kept(&mut boards, &unread);
                this.board = this.board_among(&boards);
                this.boards = Some(if boards.is_empty() && !unread.is_empty() {
                    Read::Failed(
                        unread
                            .iter()
                            .map(|(_, w)| w.clone())
                            .collect::<Vec<_>>()
                            .join(" · "),
                    )
                } else {
                    Read::Done(boards)
                });
                this.unread = unread;
                this.read_board(Ask::Show, cx);
                // Who you are is known now: what others did on your issues.
                if this.news_on {
                    this.read_news(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The boards of a site not reached that the Mac keeps, listed with the others'.
    fn add_kept(&self, boards: &mut Vec<Board>, unread: &[(String, String)]) {
        let kept: Vec<Board> = (self.kept_boards())
            .filter(|k| unread.iter().any(|(host, _)| *host == k.site))
            .filter(|k| !boards.iter().any(|b| b.key() == k.key()))
            .cloned()
            .collect();
        boards.extend(kept);
    }

    /// `page`'s screen read again by the minute's loop for as long as it is there.
    pub fn watch_page(&mut self, page: WeakEntity<Page>) {
        self.pages.push(page);
    }

    /// The one loop, each minute: what each page in front shows is read again, and the news when it is due.
    fn keep_reading(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                let Ok(pages) = this.update(cx, |s, _| {
                    s.pages.retain(|p| p.upgrade().is_some());
                    s.pages.clone()
                }) else {
                    break;
                };
                // The pages ask the store themselves, so they are told outside its own update.
                let mut front = false;
                for page in pages {
                    page.update(cx, |page, cx| {
                        if page.front {
                            front = true;
                            page.refresh(cx);
                        }
                    })
                    .ok();
                }
                let Ok(()) = this.update(cx, |s, cx| s.news_minute(front, cx)) else {
                    break;
                };
            }
        })
        .detach();
    }

    /// A minute gone, a window of Artoo in front or not.
    fn news_minute(&mut self, front: bool, cx: &mut Context<Self>) {
        if !self.news_on {
            return;
        }
        self.news_waited += 1;
        if news_due(front, self.news_waited) {
            self.read_news(cx);
        }
    }

    /// The news read now and as time goes on, from the first tab that lists it; once.
    pub fn watch_news(&mut self, cx: &mut Context<Self>) {
        if self.news_on || self.sites.is_empty() {
            return;
        }
        self.news_on = true;
        self.read_news(cx);
    }

    /// What others did lately on your issues, every site's where who you are is known; the boards (and you) read
    /// first.
    fn read_news(&mut self, cx: &mut Context<Self>) {
        self.news_waited = 0;
        if self.boards.is_none() {
            return self.read_boards(cx);
        }
        let sites: Vec<(Site, Person)> = (self.sites.iter())
            .filter_map(|s| self.me.get(s.key()).map(|m| (s.clone(), m.clone())))
            .collect();
        if sites.is_empty() {
            return;
        }
        // The projects' names, for where each news is.
        for (site, _) in &sites {
            self.read_kinds(site.key(), "", cx);
        }
        spawn_read(
            cx,
            move || {
                Ok((sites.iter())
                    .map(|(site, me)| (site.key().to_owned(), model::news(site, me)))
                    .collect::<Vec<_>>())
            },
            |this, read: Result<Vec<SiteNews>, String>, _| {
                if let Ok(read) = read {
                    this.take_news(read);
                }
            },
        );
    }

    /// Each site's news as read; a site not read keeps what it had.
    fn take_news(&mut self, mut read: Vec<SiteNews>) {
        let mut news = Vec::new();
        for site in &self.sites {
            let host = site.key();
            let got = (read.iter().position(|(h, _)| h == host)).map(|i| read.swap_remove(i).1);
            let Some(Ok(got)) = got else {
                news.extend(self.news.iter().filter(|(h, _)| h == host).cloned());
                continue;
            };
            let first = self.news_read.insert(host.to_owned());
            for n in got {
                let new = self.news_known.insert(news_key(host, &n));
                if new && !first {
                    self.news_fresh.push((host.to_owned(), n.clone()));
                }
                news.push((host.to_owned(), n));
            }
        }
        self.news = news;
    }

    /// The boards read again: a site not reached tried once more (Retry).
    pub fn read_boards_again(&mut self, cx: &mut Context<Self>) {
        self.boards = None;
        self.unread.clear();
        self.read_boards(cx);
    }

    /// The trackers named again (Settings → Accounts saved): every site's boards read anew.
    pub fn sites_again(&mut self, cx: &mut Context<Self>) {
        crate::trackers::read_offered(cx);
        let (sites, missing) = jira::sites();
        self.sites = sites;
        self.missing = missing;
        self.me.clear();
        self.projects.clear();
        // Asked with the accounts as they were.
        self.drop_reads(|_| true);
        self.read_boards_again(cx);
        cx.notify();
    }

    /// Whether the space of the project shown reads account `host`; every account while no project was shown (an
    /// agent's read).
    fn reads_here(&self, host: &str) -> bool {
        match &self.here {
            Some(project) => (self.links.get(project)).is_some_and(|h| h.iter().any(|x| x == host)),
            None => true,
        }
    }

    /// The board to show among `boards`: the project shown's own, the one its space was last on; else, no project
    /// shown, the one picked; else the board opened lately among those it reads, else its first. A board picked in
    /// one project never follows into another reading the same account.
    fn board_among(&self, boards: &[Board]) -> Option<BoardId> {
        let here: Vec<&Board> = (boards.iter())
            .filter(|b| self.reads_here(&b.site))
            .collect();
        let read = |b: &BoardId| here.iter().any(|x| x.key() == *b);
        let own = match &self.here {
            Some(project) => self.kept.boards.get(project).cloned(),
            None => self.board.clone(),
        };
        own.filter(read).or_else(|| {
            (self.kept.recent.iter())
                .find(|r| read(r))
                .cloned()
                .or_else(|| here.first().map(|b| b.key()))
        })
    }

    /// The projects whose space reads account `host`, named by their folders' names (Artoo gives a plugin no project
    /// marks in 2.0.0), in order.
    pub fn readers(&self, host: &str) -> Vec<String> {
        let mut names: Vec<String> = (self.links.iter())
            .filter(|(_, hosts)| hosts.iter().any(|h| h == host))
            .map(|(project, _)| {
                let folder = project.trim_end_matches('/');
                folder.rsplit('/').next().unwrap_or(folder).to_owned()
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// When the boards of `host` shown were last read, for a host not reached now: the oldest of the boards kept.
    pub fn last_reached(&self, host: &str) -> Option<i64> {
        (self.stale.keys())
            .filter(|b| b.0 == host)
            .filter_map(|b| self.read_at.get(b).copied())
            .min()
    }

    /// The accounts project `project`'s space reads.
    pub fn sites_in(&self, project: &str) -> Vec<Site> {
        let hosts = self.links.get(project);
        (self.sites.iter())
            .filter(|s| hosts.is_some_and(|h| h.iter().any(|x| x == s.key())))
            .cloned()
            .collect()
    }

    /// The accounts kept that project `project`'s space does not read yet.
    pub fn sites_not_in(&self, project: &str) -> Vec<Site> {
        let hosts = self.links.get(project);
        (self.sites.iter())
            .filter(|s| !hosts.is_some_and(|h| h.iter().any(|x| x == s.key())))
            .cloned()
            .collect()
    }

    /// Project `project`'s accounts whose boards were not read, and why.
    pub fn unread_in(&self, project: &str) -> Vec<(String, String)> {
        let hosts = self.links.get(project);
        (self.unread.iter())
            .filter(|(h, _)| hosts.is_some_and(|x| x.contains(h)))
            .cloned()
            .collect()
    }

    /// The boards of project `project`'s accounts; failed when none was read and one of its accounts was not.
    pub fn boards_in(&self, project: &str) -> Option<Read<Vec<Board>>> {
        let hosts = self.links.get(project)?;
        let unread = self.unread_in(project);
        let failed = || {
            let why: Vec<String> = unread.iter().map(|(_, w)| w.clone()).collect();
            Read::Failed(why.join(" · "))
        };
        Some(match self.boards.as_ref()? {
            Read::Reading => {
                let kept: Vec<Board> = (self.kept_boards())
                    .filter(|b| hosts.contains(&b.site))
                    .cloned()
                    .collect();
                if kept.is_empty() {
                    Read::Reading
                } else {
                    Read::Done(kept)
                }
            }
            Read::Failed(_) if unread.is_empty() => Read::Done(Vec::new()),
            Read::Failed(_) => failed(),
            Read::Done(boards) => {
                let ours: Vec<Board> = (boards.iter())
                    .filter(|b| hosts.contains(&b.site))
                    .cloned()
                    .collect();
                if ours.is_empty() && !unread.is_empty() {
                    failed()
                } else {
                    Read::Done(ours)
                }
            }
        })
    }

    /// Project `project`'s space shown: the board picked is one of its accounts'.
    pub fn show_project(&mut self, project: &str, cx: &mut Context<Self>) {
        if self.here.as_deref() == Some(project) {
            return;
        }
        self.here = Some(project.to_owned());
        self.pick_here(cx);
    }

    /// Account `host` read by project `project`'s space (`on`), or no longer; the board picked again when it is the
    /// project shown.
    pub fn link(
        &mut self,
        project: &str,
        host: &str,
        on: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        crate::links::set(&mut self.links, project, host, on)?;
        if self.here.as_deref() == Some(project) {
            self.pick_here(cx);
        }
        self.read_boards(cx);
        cx.notify();
        Ok(())
    }

    fn pick_here(&mut self, cx: &mut Context<Self>) {
        let boards = match &self.boards {
            Some(Read::Done(boards)) => boards.clone(),
            // The list on its way: the project's own board at once, when the Mac keeps it.
            _ => {
                let kept: Vec<Board> = self.kept_boards().cloned().collect();
                let own = (self.here.as_ref()).and_then(|p| self.kept.boards.get(p));
                if !own.is_some_and(|own| kept.iter().any(|b| b.key() == *own)) {
                    return cx.notify();
                }
                kept
            }
        };
        let board = self.board_among(&boards);
        if board != self.board {
            self.board = board;
            self.faces.clear();
            self.read_board(Ask::Show, cx);
        }
        cx.notify();
    }

    /// Board `id` picked: kept as the one opened last and among the boards opened lately.
    pub fn pick_board(&mut self, id: BoardId, cx: &mut Context<Self>) {
        self.board = Some(id.clone());
        self.faces.clear();
        self.kept.opened_board(id, self.here.as_deref());
        self.read_board(Ask::Show, cx);
        cx.notify();
    }

    /// The board picked: its columns, epics and sprints, and its issues, as `ask` needs them, quietly (what was read
    /// stays on screen, and a failure says so over it). A board left is no longer read.
    pub fn read_board(&mut self, ask: Ask, cx: &mut Context<Self>) {
        let Some(board) = self.board.clone() else {
            return;
        };
        self.drop_reads(|b| *b != board);
        let Some(site) = self.site_of(&board.0) else {
            return;
        };
        if let Some(number) = self.begin_info(&board, ask, now()) {
            let (s, id, key) = (site.clone(), board.1.clone(), board.clone());
            let task = read_task(
                cx,
                move || model::board_info(&s, &id),
                move |this, read, cx| {
                    if this.put_info(&key, number, read, now()) {
                        this.keep_board(&key, cx);
                    }
                },
            );
            self.info_reads.hold(&board, number, task);
        }
        if let Some(number) = self.begin_rows(&board, ask) {
            let (id, key) = (board.1.clone(), board.clone());
            let task = read_task(
                cx,
                move || model::board_issues(&site, &id),
                move |this, read, cx| {
                    if this.put_rows(&key, number, read, now()) {
                        this.keep_board(&key, cx);
                    }
                },
            );
            self.row_reads.hold(&board, number, task);
        }
    }

    /// The read of `board`'s columns, epics and sprints that `ask` calls for, marked as on its way.
    fn begin_info(&mut self, board: &BoardId, ask: Ask, now: i64) -> Option<u64> {
        let read = self.info.get(board);
        let at = self.info_at.get(board);
        let wanted = match ask {
            Ask::Reload => true,
            Ask::Show => read.is_none() || (matches!(read, Some(Read::Done(_))) && at.is_none()),
            Ask::Refresh | Ask::Written => at.is_none_or(|at| now - at >= INFO_AGE),
        };
        // `Reading` with no read on its way was never answered.
        let wanted = wanted || matches!(read, Some(Read::Reading));
        let number = wanted.then(|| self.info_reads.begin(board, false))??;
        self.info.entry(board.clone()).or_insert(Read::Reading);
        Some(number)
    }

    /// The read of `board`'s issues that `ask` calls for, marked as on its way.
    fn begin_rows(&mut self, board: &BoardId, ask: Ask) -> Option<u64> {
        let wanted = ask != Ask::Show
            || self.from_disk.contains(board)
            || matches!(self.rows.get(board), None | Some(Read::Reading));
        let number = wanted.then(|| self.row_reads.begin(board, ask == Ask::Written))??;
        self.rows.entry(board.clone()).or_insert(Read::Reading);
        Some(number)
    }

    /// Read `number` of `board`'s columns, epics and sprints answered; whether something new was taken. A part not
    /// read stays as it was, as the whole does when none was.
    fn put_info(
        &mut self,
        board: &BoardId,
        number: u64,
        read: Result<BoardInfo, String>,
        now: i64,
    ) -> bool {
        if !self.info_reads.land(board, number) {
            return false;
        }
        match read {
            Ok(mut info) => {
                if let Some(Read::Done(before)) = self.info.get(board) {
                    if info.epics_unread {
                        info.epics = before.epics.clone();
                    }
                    if info.sprints_unread {
                        info.sprints = before.sprints.clone();
                    }
                }
                self.info.insert(board.clone(), Read::Done(info));
                self.info_at.insert(board.clone(), now);
                true
            }
            Err(why) => {
                if !matches!(self.info.get(board), Some(Read::Done(_))) {
                    self.info.insert(board.clone(), Read::Failed(why));
                }
                false
            }
        }
    }

    /// Read `number` of `board`'s issues answered; whether they were taken.
    fn put_rows(
        &mut self,
        board: &BoardId,
        number: u64,
        read: Result<Vec<Row>, String>,
        now: i64,
    ) -> bool {
        if !self.row_reads.land(board, number) {
            return false;
        }
        match read {
            Ok(rows) => {
                self.rows.insert(board.clone(), Read::Done(rows));
                self.read_at.insert(board.clone(), now);
                self.stale.remove(board);
                self.from_disk.remove(board);
                true
            }
            // Read before: kept, the failure said over it.
            Err(why) => {
                if matches!(self.rows.get(board), Some(Read::Done(_))) {
                    self.stale.insert(board.clone(), why);
                } else {
                    self.rows.insert(board.clone(), Read::Failed(why));
                }
                false
            }
        }
    }

    /// The reads of the boards `unwanted` names given up: nothing waits for them.
    fn drop_reads(&mut self, unwanted: impl Fn(&BoardId) -> bool) {
        for board in self.row_reads.cancel(&unwanted) {
            if self.rows.get(&board) == Some(&Read::Reading) {
                self.rows.remove(&board);
            }
        }
        for board in self.info_reads.cancel(&unwanted) {
            if self.info.get(&board) == Some(&Read::Reading) {
                self.info.remove(&board);
            }
        }
    }

    /// `board` as it is read now, kept on the Mac for the next start.
    fn keep_board(&mut self, board: &BoardId, cx: &mut Context<Self>) {
        // Issues still as the Mac keeps them are not written back as new.
        if self.from_disk.contains(board) {
            return;
        }
        let known = match &self.boards {
            Some(Read::Done(boards)) => boards.iter().find(|b| b.key() == *board),
            _ => None,
        };
        let (Some(known), Some(Read::Done(info)), Some(Read::Done(rows)), Some(&at)) = (
            known.or_else(|| self.cache.board(board)),
            self.info.get(board),
            self.rows.get(board),
            self.read_at.get(board),
        ) else {
            return;
        };
        let snapshot = cache::Snapshot {
            board: known.clone(),
            info: info.clone(),
            rows: rows.clone(),
            at,
        };
        self.cache.put(snapshot);
        self.write_cache(cx);
    }

    fn write_cache(&self, cx: &mut Context<Self>) {
        if let Some(writing) = self.cache.writing() {
            cx.background_spawn(async move { writing() }).detach();
        }
    }

    /// What was read and kept of account `host`'s boards let go: the account removed, or another token typed for it.
    pub fn forget_boards(&mut self, host: &str, cx: &mut Context<Self>) {
        self.drop_reads(|b| b.0 == host);
        self.rows.retain(|b, _| b.0 != host);
        self.info.retain(|b, _| b.0 != host);
        self.read_at.retain(|b, _| b.0 != host);
        self.info_at.retain(|b, _| b.0 != host);
        self.stale.retain(|b, _| b.0 != host);
        self.from_disk.retain(|b| b.0 != host);
        if self.cache.forget(host) {
            self.write_cache(cx);
        }
    }

    /// Issue `key`, read when it is not yet, or again with `again`: from its site when it is known, else from the first
    /// site that has it. Read again, what changed since is said (`changed`) when someone else changed it.
    pub fn read_issue(
        &mut self,
        key: &str,
        site: Option<&str>,
        again: bool,
        cx: &mut Context<Self>,
    ) {
        self.read_issue_then(key, site, again, true, |_, _| {}, cx)
    }

    /// Issue `key` looked up for ⌘K on site `host` alone: read once, its pictures left until it is shown
    /// (`read_issue` reads them then); `then` once it is read or refused.
    pub fn look_up(
        &mut self,
        key: &str,
        host: &str,
        then: impl FnOnce(&mut Store, &mut Context<Store>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.read_issue_then(key, Some(host), false, false, then, cx)
    }

    /// `read_issue`, its pictures read too with `pictures`, `then` once the read is answered.
    fn read_issue_then(
        &mut self,
        key: &str,
        site: Option<&str>,
        again: bool,
        pictures: bool,
        then: impl FnOnce(&mut Store, &mut Context<Store>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let sites = match site.and_then(|host| self.site_of(host)) {
            Some(site) => vec![site],
            None => self.sites.clone(),
        };
        if sites.is_empty() {
            return;
        }
        if !again && self.issues.contains_key(key) {
            // Read before without its pictures (⌘K looked it up): they are read as it is shown.
            if pictures {
                self.read_images(key, cx);
            }
            return;
        }
        if !self.issues.contains_key(key) {
            self.issues.insert(key.to_owned(), Read::Reading);
        }
        let key = key.to_owned();
        let asked = key.clone();
        spawn_read(
            cx,
            move || model::read_issue_on(&sites, &asked),
            move |this, read, cx| {
                match read {
                    Ok(issue) => {
                        if let Some(Read::Done(before)) = this.issues.get(&key).cloned() {
                            this.note_change(&before, &issue);
                        }
                        this.issues.insert(key.clone(), Read::Done(issue));
                        if pictures {
                            this.read_images(&key, cx);
                        }
                    }
                    Err(_) if matches!(this.issues.get(&key), Some(Read::Done(_))) => {}
                    Err(why) => {
                        this.issues.insert(key, Read::Failed(why));
                    }
                }
                then(this, cx);
            },
        );
    }

    /// What someone else changed between `before` and `after`: the fields that differ, by the one who changed it
    /// last when the tracker says, kept until looked at. A change of yours being saved is not someone else's.
    fn note_change(&mut self, before: &Issue, after: &Issue) {
        if after.row.updated == before.row.updated {
            return;
        }
        let key = after.row.key.clone();
        let (a, b) = (&before.row, &after.row);
        let mut fields = Vec::new();
        let mut differ = |name: &str, changed: bool| {
            if changed && !self.saving.contains(&(key.clone(), name.to_lowercase())) {
                fields.push(name.to_owned());
            }
        };
        differ("State", a.state.id != b.state.id);
        differ("Assignee", a.assignee != b.assignee);
        differ("Priority", a.priority != b.priority);
        differ("Summary", a.summary != b.summary);
        differ("Labels", a.labels != b.labels);
        differ(
            "Epic",
            a.epic.as_ref().map(|e| &e.key) != b.epic.as_ref().map(|e| &e.key),
        );
        differ(
            "Sprint",
            a.sprint.as_ref().map(|s| s.id) != b.sprint.as_ref().map(|s| s.id),
        );
        differ("Description", before.description != after.description);
        differ("Comments", before.comments.len() != after.comments.len());
        if fields.is_empty() {
            return;
        }
        // Who: the last comment's author when only comments came, else the tracker does not say in the issue.
        let who = (fields == ["Comments"])
            .then(|| after.comments.last().and_then(|c| c.author.clone()))
            .flatten()
            .map_or_else(|| "Someone".to_owned(), |p| p.name);
        let entry = self.changed.entry(key).or_default();
        entry.who = who;
        entry.at = now();
        entry.unseen.extend(fields.iter().cloned());
        entry.fields = fields;
    }

    /// The pictures among issue `key`'s attachments, each read once.
    fn read_images(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(issue) = self.issue(key).cloned() else {
            return;
        };
        let Some(site) = self.site_of(&issue.site) else {
            return;
        };
        for a in issue.attachments.iter().filter(|a| a.image()) {
            if self.images.contains_key(&a.id) {
                continue;
            }
            self.images.insert(a.id.clone(), Read::Reading);
            let (s, url, id, mime) = (
                site.clone(),
                a.content.clone(),
                a.id.clone(),
                a.mime.clone(),
            );
            spawn_read(
                cx,
                move || model::attachment_bytes(&s, &url),
                move |this, read, _| {
                    let format = match mime.as_str() {
                        "image/jpeg" => ImageFormat::Jpeg,
                        "image/gif" => ImageFormat::Gif,
                        "image/webp" => ImageFormat::Webp,
                        "image/svg+xml" => ImageFormat::Svg,
                        _ => ImageFormat::Png,
                    };
                    this.images.insert(
                        id,
                        read_of(read.map(|bytes| Arc::new(Image::from_bytes(format, bytes)))),
                    );
                },
            );
        }
    }

    /// The moves issue `key`'s workflow allows from its state now.
    pub fn read_moves(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(site) = self.site_for(key) else {
            return;
        };
        let key = key.to_owned();
        let Some(number) = self.move_reads.begin(&key, false) else {
            return;
        };
        self.moves.insert(key.clone(), Read::Reading);
        let (k, put) = (key.clone(), key.clone());
        let task = read_task(
            cx,
            move || model::moves(&site, &k),
            move |this, read, _| {
                if this.move_reads.land(&put, number) {
                    this.moves_at.insert(put.clone(), now());
                    this.moves.insert(put, read_of(read));
                }
            },
        );
        self.move_reads.hold(&key, number, task);
    }

    /// Issue `key`'s edit screen, once.
    pub fn read_screen(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.screens.contains_key(key) {
            return;
        }
        let Some(site) = self.site_for(key) else {
            return;
        };
        self.screens.insert(key.to_owned(), Read::Reading);
        let (k, put) = (key.to_owned(), key.to_owned());
        spawn_read(
            cx,
            move || model::edit_screen(&site, &k),
            move |this, read, _| {
                this.screens.insert(put, read_of(read));
            },
        );
    }

    /// The people issue `key` may go to, once.
    pub fn read_people(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.people.contains_key(key) {
            return;
        }
        let Some(site) = self.site_for(key) else {
            return;
        };
        self.people.insert(key.to_owned(), Read::Reading);
        let (k, put) = (key.to_owned(), key.to_owned());
        spawn_read(
            cx,
            move || model::assignable(&site, &k, ""),
            move |this, read, _| {
                this.people.insert(put, read_of(read));
            },
        );
    }

    /// The kinds of link of `host`, once.
    pub fn read_link_kinds(&mut self, host: &str, cx: &mut Context<Self>) {
        if self.link_kinds.contains_key(host) {
            return;
        }
        let Some(site) = self.site_of(host) else {
            return;
        };
        self.link_kinds.insert(host.to_owned(), Read::Reading);
        let put = host.to_owned();
        spawn_read(
            cx,
            move || model::link_kinds(&site),
            move |this, read, _| {
                this.link_kinds.insert(put, read_of(read));
            },
        );
    }

    /// The projects of `host` and the kinds of issue of `project` there, once each.
    pub fn read_kinds(&mut self, host: &str, project: &str, cx: &mut Context<Self>) {
        let Some(site) = self.site_of(host) else {
            return;
        };
        if !self.projects.contains_key(host) {
            self.projects.insert(host.to_owned(), Read::Reading);
            let (s, put) = (site.clone(), host.to_owned());
            spawn_read(
                cx,
                move || model::projects(&s),
                move |this, read, _| {
                    this.projects.insert(put, read_of(read));
                },
            );
        }
        let key = (host.to_owned(), project.to_owned());
        if project.is_empty() || self.kinds.contains_key(&key) {
            return;
        }
        self.kinds.insert(key.clone(), Read::Reading);
        let p = project.to_owned();
        spawn_read(
            cx,
            move || model::kinds(&site, &p),
            move |this, read, _| {
                this.kinds.insert(key, read_of(read));
            },
        );
    }

    /// The create screen of `project`'s kind `kind` on `host`, once.
    pub fn read_create_screen(
        &mut self,
        host: &str,
        project: &str,
        kind: &str,
        cx: &mut Context<Self>,
    ) {
        let key = (host.to_owned(), project.to_owned(), kind.to_owned());
        if self.create_screens.contains_key(&key) {
            return;
        }
        let Some(site) = self.site_of(host) else {
            return;
        };
        self.create_screens.insert(key.clone(), Read::Reading);
        let (p, k) = (project.to_owned(), kind.to_owned());
        spawn_read(
            cx,
            move || model::create_screen(&site, &p, &k),
            move |this, read, _| {
                this.create_screens.insert(key, read_of(read));
            },
        );
    }

    /// The site issue `key` is on: the one it was read from, else the board's, else the first.
    pub fn site_for(&self, key: &str) -> Option<Site> {
        let host = (self.issue(key).map(|i| i.site.clone()))
            .or_else(|| self.board.as_ref().map(|b| b.0.clone()))?;
        self.site_of(&host).or_else(|| self.sites.first().cloned())
    }

    /// A change to issue `key`'s `field`: shown at once (`apply` on what is read of it), sent, and taken back when the
    /// tracker refuses, its reason kept on the field; then the issue and the board read again, quietly. `done` hears
    /// how it went.
    pub fn change(
        &mut self,
        key: &str,
        field: &str,
        write: Write,
        apply: impl FnOnce(&mut Store),
        done: impl FnOnce(&mut Store, Result<Value, String>, &mut Context<Store>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(site) = self.site_for(key) else {
            return;
        };
        let at: FieldKey = (key.to_owned(), field.to_owned());
        let (issue_before, board) = (self.issues.get(key).cloned(), self.board.clone());
        let rows_before = board.as_ref().and_then(|b| self.rows.get(b).cloned());
        self.refused.remove(&at);
        self.saving.insert(at.clone());
        apply(self);
        cx.notify();
        let host = site.key().to_owned();
        spawn_read(
            cx,
            move || model::write(&site, &write),
            move |this, read, cx| {
                this.saving.remove(&at);
                if let Err(why) = &read {
                    if let Some(before) = issue_before {
                        this.issues.insert(at.0.clone(), before);
                    }
                    if let (Some(b), Some(before)) = (board, rows_before) {
                        this.rows.insert(b, before);
                    }
                    this.refused.insert(at.clone(), why.clone());
                }
                let ok = read.is_ok();
                done(this, read, cx);
                if ok {
                    this.moves.remove(&at.0);
                    this.move_reads.cancel(|key| *key == at.0);
                    this.read_issue(&at.0, Some(&host), true, cx);
                    this.read_board(Ask::Written, cx);
                }
            },
        );
    }

    /// Each issue `key` is shown as, in the board's rows and as read whole, changed by `edit`.
    pub fn edit_row(&mut self, key: &str, edit: impl Fn(&mut Row)) {
        for read in self.rows.values_mut() {
            if let Read::Done(rows) = read {
                rows.iter_mut().filter(|r| r.key == key).for_each(&edit);
            }
        }
        if let Some(Read::Done(issue)) = self.issues.get_mut(key) {
            edit(&mut issue.row);
            issue
                .subtasks
                .iter_mut()
                .filter(|r| r.key == key)
                .for_each(&edit);
        }
        for read in self.issues.values_mut() {
            if let Read::Done(issue) = read {
                issue
                    .subtasks
                    .iter_mut()
                    .filter(|r| r.key == key)
                    .for_each(&edit);
            }
        }
    }

    /// Issue `key` as read whole, changed by `edit`.
    pub fn edit_issue(&mut self, key: &str, edit: impl FnOnce(&mut Issue)) {
        if let Some(Read::Done(issue)) = self.issues.get_mut(key) {
            edit(issue);
        }
    }

    /// The board picked, as read.
    pub fn picked(&self) -> Option<&Board> {
        match &self.boards {
            Some(Read::Done(boards)) => boards.iter().find(|b| Some(b.key()) == self.board),
            _ => (self.kept_boards())
                .find(|b| Some(b.key()) == self.board && self.reads_here(&b.site)),
        }
    }

    /// Issue `key`, when it was read.
    pub fn issue(&self, key: &str) -> Option<&Issue> {
        self.issues.get(key).and_then(Read::done)
    }

    /// Issue `key` as a row: from the board's rows, else as read whole.
    pub fn row(&self, key: &str) -> Option<&Row> {
        (self.rows.values())
            .filter_map(Read::done)
            .flat_map(|rows| rows.iter())
            .find(|r| r.key == key)
            .or_else(|| self.issue(key).map(|i| &i.row))
    }

    /// The board picked's issues as read, sub-tasks among them.
    pub fn board_rows(&self) -> Option<&Vec<Row>> {
        self.board
            .as_ref()
            .and_then(|b| self.rows.get(b))
            .and_then(Read::done)
    }

    /// The board picked's columns, epics and sprints.
    pub fn board_info(&self) -> Option<&BoardInfo> {
        self.board
            .as_ref()
            .and_then(|b| self.info.get(b))
            .and_then(Read::done)
    }

    /// The epic `key` with its colour from the board when the issue's did not say it.
    pub fn epic(&self, epic: &model::Epic) -> model::Epic {
        let mut e = epic.clone();
        if e.color == 0
            && let Some(known) = self
                .board_info()
                .and_then(|i| i.epics.iter().find(|x| x.key == e.key))
        {
            e.color = known.color;
        }
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(id: &str) -> Board {
        Board {
            id: id.into(),
            name: format!("Board {id}"),
            site: "acme.atlassian.net".into(),
            ..Default::default()
        }
    }

    fn row(key: &str) -> Row {
        Row {
            key: key.into(),
            ..Default::default()
        }
    }

    fn site(host: &str) -> Site {
        Site {
            url: format!("https://{host}"),
            kind: crate::trackers::JIRA.into(),
            ..Default::default()
        }
    }

    #[test]
    fn two_reads_of_one_board_make_one_call() {
        let mut store = test_store();
        let id = board("1").key();
        assert!(store.begin_rows(&id, Ask::Show).is_some());
        assert!(store.begin_info(&id, Ask::Show, 0).is_some());
        for ask in [Ask::Show, Ask::Refresh, Ask::Reload] {
            assert_eq!(store.begin_rows(&id, ask), None);
            assert_eq!(store.begin_info(&id, ask, 0), None);
        }
    }

    #[test]
    fn an_older_answer_is_ignored() {
        let mut store = test_store();
        let id = board("1").key();
        let old = store.begin_rows(&id, Ask::Show).unwrap();
        let new = store.begin_rows(&id, Ask::Written).unwrap();
        assert!(store.put_rows(&id, new, Ok(vec![row("PAY-2")]), 2));
        assert!(!store.put_rows(&id, old, Ok(vec![row("PAY-1")]), 3));
        assert_eq!(store.rows[&id], Read::Done(vec![row("PAY-2")]));
        assert_eq!(store.read_at[&id], 2);
    }

    #[test]
    fn a_board_left_is_no_longer_read() {
        let mut store = test_store();
        let (left, shown) = (board("1").key(), board("2").key());
        let number = store.begin_rows(&left, Ask::Show).unwrap();
        store.drop_reads(|b| *b != shown);
        assert!(!store.rows.contains_key(&left));
        assert!(!store.put_rows(&left, number, Ok(vec![row("PAY-1")]), 1));
        assert!(store.begin_rows(&left, Ask::Show).is_some());
    }

    #[test]
    fn a_refresh_reads_the_issues_and_the_rest_once_old() {
        let mut store = test_store();
        let id = board("1").key();
        let (info, rows) = (
            store.begin_info(&id, Ask::Show, 0).unwrap(),
            store.begin_rows(&id, Ask::Show).unwrap(),
        );
        store.put_info(&id, info, Ok(BoardInfo::default()), 100);
        store.put_rows(&id, rows, Ok(Vec::new()), 100);
        assert_eq!(store.begin_rows(&id, Ask::Show), None);
        assert!(store.begin_rows(&id, Ask::Refresh).is_some());
        assert_eq!(store.begin_info(&id, Ask::Show, 160), None);
        assert_eq!(store.begin_info(&id, Ask::Refresh, 160), None);
        assert!(
            store
                .begin_info(&id, Ask::Refresh, 100 + INFO_AGE)
                .is_some()
        );
        store.drop_reads(|_| true);
        assert!(store.begin_info(&id, Ask::Reload, 160).is_some());
    }

    #[test]
    fn epics_and_sprints_not_read_stay_as_they_were() {
        let mut store = test_store();
        let id = board("1").key();
        let had = BoardInfo {
            epics: vec![model::Epic {
                key: "PAY-1".into(),
                ..Default::default()
            }],
            sprints: vec![model::Sprint {
                id: 7,
                ..Default::default()
            }],
            ..Default::default()
        };
        let first = store.begin_info(&id, Ask::Show, 0).unwrap();
        store.put_info(&id, first, Ok(had.clone()), 1);
        let next = store.begin_info(&id, Ask::Reload, 2).unwrap();
        let missed = BoardInfo {
            epics_unread: true,
            sprints_unread: true,
            ..Default::default()
        };
        store.put_info(&id, next, Ok(missed), 2);
        let now = store.info[&id].done().unwrap();
        assert_eq!((&now.epics, &now.sprints), (&had.epics, &had.sprints));
        // Read, and none: they are gone.
        let last = store.begin_info(&id, Ask::Reload, 3).unwrap();
        store.put_info(&id, last, Ok(BoardInfo::default()), 3);
        assert!(store.info[&id].done().unwrap().epics.is_empty());
        let failed = store.begin_info(&id, Ask::Reload, 4).unwrap();
        store.put_info(&id, failed, Err("no answer".into()), 4);
        assert!(store.info[&id].done().is_some());
    }

    #[test]
    fn a_site_not_read_keeps_its_news() {
        let mut store = test_store();
        store.sites = vec![site("a.example"), site("b.example")];
        let news = |id: &str| News {
            id: id.into(),
            ..Default::default()
        };
        let ids = |store: &Store| -> Vec<String> {
            store.news.iter().map(|(_, n)| n.id.clone()).collect()
        };
        store.take_news(vec![
            ("a.example".into(), Ok(vec![news("1")])),
            ("b.example".into(), Err("no answer".into())),
        ]);
        store.take_news(vec![
            ("a.example".into(), Err("no answer".into())),
            ("b.example".into(), Ok(vec![news("2")])),
        ]);
        assert_eq!(ids(&store), ["1", "2"]);
        // A site's first read toasts nothing.
        assert!(store.news_fresh.is_empty());
        store.take_news(vec![("a.example".into(), Ok(vec![news("3"), news("1")]))]);
        assert_eq!(ids(&store), ["3", "1", "2"]);
        assert_eq!(store.news_fresh.len(), 1);
    }

    #[test]
    fn the_news_waits_longer_behind() {
        assert!(news_due(true, 1));
        assert!(!news_due(false, 2));
        assert!(news_due(false, 3));
    }

    #[test]
    fn offline_the_last_board_shows_as_kept_and_marked_stale() {
        let mut cache = cache::Cache::default();
        cache.put(cache::Snapshot {
            board: board("1"),
            info: BoardInfo::default(),
            rows: vec![row("PAY-1")],
            at: 50,
        });
        let kept = keep::Kept {
            last_board: Some(board("1").key()),
            ..Default::default()
        };
        let mut store = Store::blank(vec![site("acme.atlassian.net")], Vec::new(), kept, cache);
        let id = board("1").key();
        // Painted before anything answers.
        assert_eq!(store.picked(), Some(&board("1")));
        assert_eq!(store.board_rows(), Some(&vec![row("PAY-1")]));
        assert!(store.from_disk.contains(&id));
        let read = store.begin_rows(&id, Ask::Show).expect("read all the same");
        let unread = [("acme.atlassian.net".to_owned(), "no answer".to_owned())];
        let mut boards = Vec::new();
        store.add_kept(&mut boards, &unread);
        assert_eq!(boards, [board("1")]);
        store.put_rows(&id, read, Err("no answer".into()), 90);
        assert_eq!(store.stale[&id], "no answer");
        assert_eq!(store.read_at[&id], 50);
        assert_eq!(store.board_rows(), Some(&vec![row("PAY-1")]));
    }

    #[test]
    fn two_projects_on_one_account_keep_their_own_board() {
        let mut store = test_store();
        let boards = [board("1"), board("2")];
        for p in ["/w/pay", "/w/web"] {
            store
                .links
                .insert(p.into(), vec!["acme.atlassian.net".into()]);
        }
        // As `opened_board` keeps them, without writing the Mac's file.
        store.kept.boards.insert("/w/pay".into(), board("2").key());
        store.kept.boards.insert("/w/web".into(), board("1").key());
        // Board 1 picked last, in the other project.
        store.board = Some(board("1").key());
        store.here = Some("/w/pay".into());
        assert_eq!(store.board_among(&boards), Some(board("2").key()));
        store.here = Some("/w/web".into());
        assert_eq!(store.board_among(&boards), Some(board("1").key()));
    }
}
