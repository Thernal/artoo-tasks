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
use artoo_plugin_sdk::kit::fresh::Slot;
// The store's own `Ask` is why a board is read.
use crate::model::Ask as SliceAsk;
use crate::model::{
    self, Board, BoardId, BoardInfo, Field, Issue, Kind, Move, News, Person, Row, Tracker, Write,
};
use crate::page::Page;
use crate::slices::{self, Filter, How, Moved, SliceId, Slices, Step};
use artoo_plugin_sdk::gpui::{
    App, AppContext, Context, Entity, Global, Image, ImageFormat, Task, WeakEntity,
};
use artoo_plugin_sdk::serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
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
    /// What the minute's refresh of a board read whole brought that waits to be shown: `1 moved ↑`.
    pub held: HashMap<BoardId, crate::held::Waiting>,
    /// A snapshot's read that never answers (`force_fresh`): the board is drawn as being read.
    forced_reading: bool,
    /// When each board's columns, epics and sprints were read last.
    info_at: HashMap<BoardId, i64>,
    row_reads: Flights<BoardId>,
    info_reads: Flights<BoardId>,
    move_reads: Flights<String>,
    /// Each board's slices as read (a Jira board's): which is shown, their issues and counts.
    pub slices: HashMap<BoardId, Slices>,
    slice_reads: Flights<(BoardId, SliceId)>,
    count_reads: Flights<(BoardId, SliceId)>,
    /// Each epic's issues in all, by site and epic: `41 in all`.
    pub epic_counts: HashMap<(String, String), Read<usize>>,
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
            held: HashMap::new(),
            forced_reading: false,
            info_at: HashMap::new(),
            row_reads: Flights::default(),
            info_reads: Flights::default(),
            move_reads: Flights::default(),
            slices: HashMap::new(),
            slice_reads: Flights::default(),
            count_reads: Flights::default(),
            epic_counts: HashMap::new(),
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

    /// The folders of the projects whose space reads account `host`, by folder name; a trailing `/` is the same folder.
    pub fn readers(&self, host: &str) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = (self.links.iter())
            .filter(|(_, hosts)| hosts.iter().any(|h| h == host))
            .map(|(project, _)| PathBuf::from(project.trim_end_matches('/')))
            .collect();
        paths.sort_by_key(|p| (p.file_name().map(|n| n.to_owned()), p.clone()));
        paths.dedup();
        paths
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
                        this.start_slices(&key, cx);
                    }
                },
            );
            self.info_reads.hold(&board, number, task);
        }
        self.start_slices(&board, cx);
        // The slice in view is read again; one being read for the first time reads on by itself.
        if ask != Ask::Show
            && let Some(view) = self.slices.get(&board).and_then(|s| s.view.clone())
        {
            let how = if ask == Ask::Refresh {
                How::Refresh
            } else {
                How::Again
            };
            self.read_slice(&board, view, how, cx);
        }
        // A board read by slice reads only the slice in view.
        if self.by_slice(&board) {
            return;
        }
        if let Some(number) = self.begin_rows(&board, ask) {
            let (id, key) = (board.1.clone(), board.clone());
            let quiet = ask == Ask::Refresh;
            let task = read_task(
                cx,
                move || model::board_issues(&site, &id),
                move |this, read, cx| {
                    if this.put_rows(&key, number, read, quiet, now()) {
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

    /// Read `number` of `board`'s issues answered; whether they were taken. A `quiet` one (the minute's refresh)
    /// holds what moved (`held`).
    fn put_rows(
        &mut self,
        board: &BoardId,
        number: u64,
        read: Result<Vec<Row>, String>,
        quiet: bool,
        now: i64,
    ) -> bool {
        if !self.row_reads.land(board, number) {
            return false;
        }
        match read {
            Ok(rows) => {
                let rows = match self.rows.get(board) {
                    Some(Read::Done(before)) if quiet && !self.from_disk.contains(board) => {
                        let (shown, waiting) = crate::held::merge(before, rows);
                        match waiting {
                            Some(waiting) => self.held.insert(board.clone(), waiting),
                            None => self.held.remove(board),
                        };
                        shown
                    }
                    _ => {
                        self.held.remove(board);
                        rows
                    }
                };
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
        for (board, slice) in self.slice_reads.cancel(|k| unwanted(&k.0)) {
            if let Some(s) = self.slices.get_mut(&board) {
                s.dropped(&slice);
            }
        }
        for (board, slice) in self.count_reads.cancel(|k| unwanted(&k.0)) {
            if let Some(r) = self
                .slices
                .get_mut(&board)
                .and_then(|s| s.reads.get_mut(&slice))
            {
                r.counting = false;
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
        self.slices.retain(|b, _| b.0 != host);
        self.epic_counts.retain(|e, _| e.0 != host);
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

    /// Issue `key`'s page in the browser: its own address once read; a Jira key's is known before.
    pub fn browse_url(&self, key: &str) -> Option<String> {
        if let Some(issue) = self.issue(key).filter(|i| !i.url.is_empty()) {
            return Some(issue.url.clone());
        }
        (self.site_for(key))
            .filter(|site| site.kind == crate::trackers::JIRA)
            .map(|site| format!("{}/browse/{key}", site.url))
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
        let slices_before = board.as_ref().and_then(|b| self.slices.get(b).cloned());
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
                    if let (Some(b), Some(before)) = (board.clone(), rows_before) {
                        this.rows.insert(b, before);
                    }
                    // Only the rows go back: what was read of the slices since stays.
                    if let (Some(b), Some(before)) = (board, slices_before)
                        && let Some(now) = this.slices.get_mut(&b)
                    {
                        for (slice, read) in now.reads.iter_mut() {
                            if let Some(was) = before.reads.get(slice) {
                                read.rows = was.rows.clone();
                            }
                        }
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
        for read in self.slices.values_mut().flat_map(|s| s.reads.values_mut()) {
            read.rows
                .iter_mut()
                .filter(|r| r.key == key)
                .for_each(&edit);
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
            .chain(
                (self.slices.values())
                    .flat_map(|s| s.reads.values())
                    .flat_map(|r| r.rows.iter()),
            )
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

/// One step of slice `ask`'s read of board `board`: its page, the first with its counts, asked alongside.
fn read_step(site: &Site, board: &str, ask: SliceAsk) -> Result<Step, String> {
    let t = model::tracker(site);
    let (counts, page) = std::thread::scope(|scope| {
        let counts = (ask.start == 0).then(|| scope.spawn(|| counts(&*t, board, &ask)));
        let page = t.slice(board, &ask);
        (
            counts.map(|c| c.join().unwrap_or_else(|_| Err(String::new()))),
            page,
        )
    });
    let counts = counts.transpose()?;
    Ok(Step {
        start: ask.start,
        page: page?,
        count: counts.map(|c| c.0),
        mine: counts.and_then(|c| c.1),
    })
}

/// How many issues slice `ask` of board `board` holds as asked, and how many of the whole slice are yours (none when
/// that could not be read): two counts, side by side.
fn counts(t: &dyn Tracker, board: &str, ask: &SliceAsk) -> Result<(usize, Option<usize>), String> {
    let mine = SliceAsk {
        slice: ask.slice.clone(),
        mine: true,
        ..Default::default()
    };
    let count = SliceAsk {
        start: 0,
        max: 0,
        ..ask.clone()
    };
    std::thread::scope(|scope| {
        let mine = scope.spawn(|| t.slice(board, &mine).ok().map(|p| p.total));
        let count = t.slice(board, &count)?.total;
        Ok((count, mine.join().ok().flatten()))
    })
}

impl Store {
    /// Whether board `board` is read by slice: a Jira board. Another tracker's board is one list, its `rows`.
    fn by_slice(&self, board: &BoardId) -> bool {
        self.site_of(&board.0)
            .is_some_and(|s| s.kind == crate::trackers::JIRA)
    }

    /// Board `board`'s slices once its sprints are known: each active sprint's and the Backlog's counts, and the
    /// slice it opens on.
    fn start_slices(&mut self, board: &BoardId, cx: &mut Context<Self>) {
        if !self.by_slice(board) {
            return;
        }
        let Some(Read::Done(info)) = self.info.get(board) else {
            return;
        };
        let mut counted: Vec<SliceId> = slices::active(info)
            .map(|s| SliceId::Sprint(s.id))
            .collect();
        if !info.sprintless {
            counted.push(SliceId::Backlog);
        }
        for slice in counted {
            self.read_count(board, slice, cx);
        }
        self.open_slice(board, cx);
    }

    /// The slice board `board` opens on, shown once what decides it is read.
    fn open_slice(&mut self, board: &BoardId, cx: &mut Context<Self>) {
        let Some(Read::Done(info)) = self.info.get(board) else {
            return;
        };
        let remembered = (self.kept.slices.get(board)).and_then(|k| SliceId::parse(&k.slice));
        let slices = self.slices.entry(board.clone()).or_default();
        if slices.view.is_some() {
            return;
        }
        if let Some(open) = slices.opens(remembered.as_ref(), info) {
            self.show_slice(board, open, cx);
        }
    }

    /// Slice `slice` of board `board` shown: read when it was not, and kept as the board's.
    pub fn show_slice(&mut self, board: &BoardId, slice: SliceId, cx: &mut Context<Self>) {
        self.slices.entry(board.clone()).or_default().view = Some(slice.clone());
        self.read_slice(board, slice, How::Show, cx);
        self.show_rows(board, cx);
        self.keep_slice(board);
        cx.notify();
    }

    /// The slice in view of `board` kept with its name and count, for Boards and the next start.
    fn keep_slice(&mut self, board: &BoardId) {
        let Some(slices) = self.slices.get(board) else {
            return;
        };
        let Some(view) = slices.view.clone() else {
            return;
        };
        let info = self.info.get(board).and_then(Read::done);
        let past = match &slices.past {
            Some(Read::Done(past)) => past.as_slice(),
            _ => &[],
        };
        let count = match &view {
            SliceId::All => info.and_then(|info| {
                slices::active(info)
                    .map(|s| slices.reads.get(&SliceId::Sprint(s.id))?.count)
                    .sum()
            }),
            slice => slices.reads.get(slice).and_then(|r| r.count),
        };
        let kept = keep::KeptSlice {
            slice: view.key(),
            name: slices::name(&view, info, past),
            count,
        };
        if self.kept.remember_slice(board.clone(), kept) {
            self.kept.write();
        }
    }

    /// Whether slice `slice` of board `board` is shown: the one in view, or one of All's sprints.
    fn shows(&self, board: &BoardId, slice: &SliceId) -> bool {
        let Some(view) = self.slices.get(board).and_then(|s| s.view.as_ref()) else {
            return false;
        };
        view == slice
            || (*view == SliceId::All && matches!(slice, SliceId::Sprint(_)))
                && (self.info.get(board).and_then(Read::done))
                    .is_some_and(|i| slices::active(i).any(|s| *slice == SliceId::Sprint(s.id)))
    }

    /// The issues of `board`'s slice in view as the board's `rows`, what the page and its parts read: once some are
    /// read, whatever reads on; until then on their way, or why not. Rows the Mac kept stay until the slice's land.
    fn show_rows(&mut self, board: &BoardId, cx: &mut Context<Self>) {
        let Some(slices) = self.slices.get(board) else {
            return;
        };
        let Some(view) = slices.view.clone() else {
            return;
        };
        let reads: Vec<&slices::SliceRead> = match &view {
            SliceId::All => (self.info.get(board).and_then(Read::done))
                .map(|i| {
                    slices::active(i)
                        .filter_map(|s| slices.reads.get(&SliceId::Sprint(s.id)))
                        .collect()
                })
                .unwrap_or_default(),
            slice => slices.reads.get(slice).into_iter().collect(),
        };
        let some = (reads.iter()).any(|r| !r.rows.is_empty() || r.state == Some(Read::Done(())));
        let failed = (reads.iter()).find_map(|r| match &r.state {
            Some(Read::Failed(why)) if r.rows.is_empty() => Some(why.clone()),
            _ => None,
        });
        let at = reads.iter().filter_map(|r| r.at).max();
        if some {
            let rows = match (&view, self.info.get(board).and_then(Read::done)) {
                (SliceId::All, Some(info)) => slices.all_rows(info).cloned().collect(),
                _ => reads.iter().flat_map(|r| r.rows.iter().cloned()).collect(),
            };
            self.rows.insert(board.clone(), Read::Done(rows));
            self.stale.remove(board);
            let whole = reads.iter().all(|r| r.state != Some(Read::Reading));
            if let Some(at) = at.filter(|_| whole) {
                self.read_at.insert(board.clone(), at);
                self.from_disk.remove(board);
                self.keep_board(board, cx);
            }
        } else if let Some(why) = failed.filter(|_| reads.iter().all(|r| r.rows.is_empty())) {
            if self.from_disk.contains(board) {
                self.stale.insert(board.clone(), why);
            } else {
                self.rows.insert(board.clone(), Read::Failed(why));
            }
        } else if !self.from_disk.contains(board) {
            self.rows.insert(board.clone(), Read::Reading);
        }
    }

    /// Board `board`'s slices the sprint picker counts: each active and future sprint, and the Backlog.
    pub fn count_sprints(&mut self, board: &BoardId, cx: &mut Context<Self>) {
        let Some(Read::Done(info)) = self.info.get(board) else {
            return;
        };
        let mut counted: Vec<SliceId> = (info.sprints.iter())
            .filter(|s| s.state != "closed")
            .map(|s| SliceId::Sprint(s.id))
            .collect();
        if !info.sprintless {
            counted.push(SliceId::Backlog);
        }
        for slice in counted {
            self.read_count(board, slice, cx);
        }
    }

    /// The slices of the board picked, when it is read by slice.
    pub fn board_slices(&self) -> Option<&Slices> {
        let board = self.board.as_ref()?;
        if !self.by_slice(board) {
            return None;
        }
        self.slices.get(board)
    }

    /// The slices the board `board` shows, when it is read by slice: the active sprints' for All.
    fn view_slices(&self, board: &BoardId) -> Vec<SliceId> {
        let Some(view) = self.slices.get(board).and_then(|s| s.view.clone()) else {
            return Vec::new();
        };
        match view {
            SliceId::All => (self.info.get(board).and_then(Read::done))
                .map(|i| slices::active(i).map(|s| SliceId::Sprint(s.id)).collect())
                .unwrap_or_default(),
            view => vec![view],
        }
    }

    /// Whether a read of what the board picked shows is on its way: its issues (its slice's), or its columns while
    /// none were read.
    pub fn reading_shown(&self) -> bool {
        let Some(board) = self.board.as_ref() else {
            return false;
        };
        if self.forced_reading {
            return true;
        }
        let info =
            self.info_reads.running(board) && !matches!(self.info.get(board), Some(Read::Done(_)));
        if self.by_slice(board) {
            return info
                || (self.view_slices(board).into_iter())
                    .any(|slice| self.slice_reads.running(&(board.clone(), slice)));
        }
        info || self.row_reads.running(board)
    }

    /// Why the last read of what the board picked shows failed, and whether issues read before it are shown.
    pub fn failure_shown(&self) -> Option<(String, bool)> {
        let board = self.board.as_ref()?;
        let kept = matches!(self.rows.get(board), Some(Read::Done(_)));
        if let Some(why) = self.stale.get(board) {
            return Some((why.clone(), kept));
        }
        if let Some(Read::Failed(why)) = self.rows.get(board) {
            return Some((why.clone(), false));
        }
        if let Some(Read::Failed(why)) = self.info.get(board)
            && !kept
        {
            return Some((why.clone(), false));
        }
        let slices = self.slices.get(board)?;
        (self.view_slices(board).iter())
            .filter_map(|slice| slices.reads.get(slice))
            .find_map(|r| match &r.state {
                Some(Read::Failed(why)) => Some((why.clone(), kept && !r.rows.is_empty())),
                _ => None,
            })
    }

    /// What the board picked holds back from its last refresh: `1 moved ↑`.
    pub fn waiting(&self) -> Option<crate::held::Waiting> {
        let board = self.board.as_ref()?;
        if let Some(held) = self.held.get(board).filter(|h| h.count() > 0) {
            return Some(held.clone());
        }
        self.slices.get(board)?.waiting(&self.view_slices(board))
    }

    /// What the board's freshness slot says (round 41): read for the first time, read again under what is shown, as
    /// the Mac kept it, current, changes waiting, its site not reached, nothing read.
    pub fn slot(&self) -> Option<Slot> {
        let board = self.board.as_ref()?;
        let at = (self.read_at.get(board)).map(|at| artoo_plugin_sdk::kit::format::clock(*at));
        let kept = matches!(self.rows.get(board), Some(Read::Done(_)));
        let from_disk = self.from_disk.contains(board);
        if self.reading_shown() {
            return Some(match (&at, kept) {
                (Some(at), true) if from_disk => Slot::Kept(at.clone()),
                (_, true) => Slot::Refreshing,
                (_, false) => Slot::Reading,
            });
        }
        if let Some((_, kept)) = self.failure_shown() {
            return Some(match at {
                Some(at) if kept => Slot::CantReach(at),
                _ => Slot::CantRead,
            });
        }
        let Some(at) = at.filter(|_| kept) else {
            return Some(Slot::Reading);
        };
        if from_disk {
            return Some(Slot::Kept(at));
        }
        Some(match self.waiting() {
            Some(waiting) => Slot::Waiting(at, waiting.count(), waiting.kind()),
            None => Slot::Updated(at),
        })
    }

    /// What the board picked holds back shown: the slot's `1 moved ↑` clicked.
    pub fn let_in(&mut self, cx: &mut Context<Self>) {
        let Some(board) = self.board.clone() else {
            return;
        };
        if let Some(held) = self.held.remove(&board) {
            self.rows.insert(board.clone(), Read::Done(held.rows));
        }
        if self.slices.get_mut(&board).is_some_and(|s| s.let_in()) {
            self.show_rows(&board, cx);
            self.keep_slice(&board);
        }
        cx.notify();
    }

    /// The board picked in a state of round 41's page (`tasks.html?s=`) for a snapshot, from what the fixture read and
    /// with no tracker: `loading` / `slice` (read again from nothing), `refreshing`, `new` (a refresh that moved one
    /// issue to another column and gave another a comment), `error` (the refresh not answered, the board kept),
    /// `failed` (nothing kept), `kept` (as the Mac kept it). The reads on their way are dropped.
    pub fn force_fresh(&mut self, state: &str, cx: &mut Context<Self>) {
        let Some(board) = self.board.clone() else {
            return;
        };
        self.drop_reads(|_| true);
        let views = self.view_slices(&board);
        let by_slice = self.by_slice(&board) && !views.is_empty();
        let why = "acme.atlassian.net did not answer in 10 s".to_owned();
        match state {
            "loading" | "slice" | "failed" => {
                self.forced_reading = state != "failed";
                self.read_at.remove(&board);
                let failed = state == "failed";
                if let Some(slices) = self.slices.get_mut(&board) {
                    for view in &views {
                        let r = slices.reads.entry(view.clone()).or_default();
                        r.rows.clear();
                        r.state = Some(match failed {
                            true => Read::Failed(why.clone()),
                            false => Read::Reading,
                        });
                        if state == "loading" {
                            (r.count, r.mine) = (None, None);
                        }
                    }
                }
                let read = match failed {
                    true => Read::Failed(why.clone()),
                    false => Read::Reading,
                };
                self.rows.insert(board.clone(), read);
                if by_slice {
                    self.show_rows(&board, cx);
                }
            }
            "refreshing" => self.forced_reading = true,
            "kept" => {
                self.from_disk.insert(board.clone());
            }
            "error" => {
                if let Some(slices) = self.slices.get_mut(&board) {
                    for view in &views {
                        if let Some(r) = slices.reads.get_mut(view) {
                            r.state = Some(Read::Failed(why.clone()));
                        }
                    }
                }
                if !by_slice {
                    self.stale.insert(board.clone(), why);
                }
            }
            "new" => {
                let change = |rows: &[Row]| {
                    let mut read = rows.to_vec();
                    let top = |r: &&mut Row| r.parent.is_none();
                    let other = (rows.iter().rev()).find_map(|r| {
                        (rows.first()?.state.id != r.state.id).then(|| r.state.clone())
                    });
                    if let (Some(first), Some(state)) = (read.iter_mut().find(top), other) {
                        first.state = state;
                    }
                    if let Some(talked) = read.iter_mut().skip(1).find(|r| r.comments > 0) {
                        talked.comments += 1;
                    }
                    read
                };
                if by_slice {
                    if let Some(r) = (self.slices.get_mut(&board))
                        .and_then(|sl| sl.reads.get_mut(views.first()?))
                    {
                        let read = change(&r.rows);
                        (r.rows, r.held) = crate::held::merge(&r.rows, read);
                    }
                    self.show_rows(&board, cx);
                } else if let Some(Read::Done(rows)) = self.rows.get(&board) {
                    let (shown, waiting) = crate::held::merge(rows, change(rows));
                    self.rows.insert(board.clone(), Read::Done(shown));
                    if let Some(waiting) = waiting {
                        self.held.insert(board.clone(), waiting);
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// Slice `slice` of board `board` read as `how` asks, page after page while it reads on, each shown as it comes.
    pub fn read_slice(
        &mut self,
        board: &BoardId,
        slice: SliceId,
        how: How,
        cx: &mut Context<Self>,
    ) {
        if slice == SliceId::All {
            let active: Vec<SliceId> = (self.info.get(board).and_then(Read::done))
                .map(|info| {
                    slices::active(info)
                        .map(|s| SliceId::Sprint(s.id))
                        .collect()
                })
                .unwrap_or_default();
            for sprint in active {
                self.read_slice(board, sprint, how, cx);
            }
            return;
        }
        let Some(site) = self.site_of(&board.0) else {
            return;
        };
        let key = (board.clone(), slice);
        if let Some((number, ask)) = self.begin_slice(&key, how) {
            self.slice_step(site, key, number, ask, cx);
        }
    }

    /// The read of `key`'s slice that `how` calls for, marked as on its way: its number and what is asked first. One
    /// already on its way is left to go on, unless it is read again.
    fn begin_slice(&mut self, key: &(BoardId, SliceId), how: How) -> Option<(u64, SliceAsk)> {
        if how != How::Again && self.slice_reads.running(key) {
            return None;
        }
        let ask = self
            .slices
            .entry(key.0.clone())
            .or_default()
            .begin(&key.1, how)?;
        Some((self.slice_reads.begin(key, true)?, ask))
    }

    /// Step `number` of `key`'s slice read, `ask`, sent; the next one once it is in, while the slice reads on.
    fn slice_step(
        &mut self,
        site: Site,
        key: (BoardId, SliceId),
        number: u64,
        ask: SliceAsk,
        cx: &mut Context<Self>,
    ) {
        let (s, id, k) = (site.clone(), key.0.1.clone(), key.clone());
        let task = read_task(
            cx,
            move || read_step(&s, &id, ask),
            move |this, read, cx| {
                if let Some((number, next)) = this.put_slice(&k, number, read, now()) {
                    this.slice_step(site, k.clone(), number, next, cx);
                }
                if this.shows(&k.0, &k.1) {
                    this.show_rows(&k.0, cx);
                    this.keep_slice(&k.0);
                }
            },
        );
        self.slice_reads.hold(&key, number, task);
    }

    /// Step `number` of `key`'s slice read answered: taken in when it is still the one wanted; the next step's number
    /// and ask while the slice reads on.
    fn put_slice(
        &mut self,
        key: &(BoardId, SliceId),
        number: u64,
        read: Result<Step, String>,
        now: i64,
    ) -> Option<(u64, SliceAsk)> {
        if !self.slice_reads.land(key, number) {
            return None;
        }
        let slices = self.slices.get_mut(&key.0)?;
        match read {
            Ok(step) => {
                let next = slices.take(&key.1, step, now)?;
                Some((self.slice_reads.begin(key, true)?, next))
            }
            Err(why) => {
                slices.fail(&key.1, why);
                None
            }
        }
    }

    /// Board `board`'s closed sprints, read once Past sprints is opened.
    pub fn read_past(&mut self, board: &BoardId, cx: &mut Context<Self>) {
        let Some(site) = self.site_of(&board.0) else {
            return;
        };
        let slices = self.slices.entry(board.clone()).or_default();
        if matches!(slices.past, Some(Read::Reading | Read::Done(_))) {
            return;
        }
        slices.past = Some(Read::Reading);
        let (id, board) = (board.1.clone(), board.clone());
        spawn_read(
            cx,
            move || model::past_sprints(&site, &id),
            move |this, read, _| {
                this.slices.entry(board).or_default().past = Some(read_of(read));
            },
        );
    }

    /// Slice `slice` of board `board` counted, unless it is or is being.
    fn read_count(&mut self, board: &BoardId, slice: SliceId, cx: &mut Context<Self>) {
        let key = (board.clone(), slice);
        let Some(site) = self.site_of(&board.0) else {
            return;
        };
        if self.count_reads.running(&key) {
            return;
        }
        let Some(ask) = (self.slices.entry(board.clone()).or_default()).begin_count(&key.1, false)
        else {
            return;
        };
        let Some(number) = self.count_reads.begin(&key, false) else {
            return;
        };
        let (id, k) = (board.1.clone(), key.clone());
        let task = read_task(
            cx,
            move || counts(&*model::tracker(&site), &id, &ask),
            move |this, read, cx| {
                if this.put_count(&k, number, read) {
                    this.open_slice(&k.0, cx);
                }
            },
        );
        self.count_reads.hold(&key, number, task);
    }

    /// Count read `number` of `key`'s slice answered; whether it was taken.
    fn put_count(
        &mut self,
        key: &(BoardId, SliceId),
        number: u64,
        read: Result<(usize, Option<usize>), String>,
    ) -> bool {
        if !self.count_reads.land(key, number) {
            return false;
        }
        if let Some(slices) = self.slices.get_mut(&key.0) {
            slices.take_count(&key.1, read);
        }
        true
    }
}

impl Store {
    /// The Backlog's next page of board `board`.
    pub fn load_more(&mut self, board: &BoardId, cx: &mut Context<Self>) {
        self.read_slice(board, SliceId::Backlog, How::More, cx);
    }

    /// The Backlog's filter of board `board` typed: asked of the tracker once typing rests, the Backlog read again.
    pub fn filter_backlog(&mut self, board: &BoardId, filter: Filter, cx: &mut Context<Self>) {
        let slices = self.slices.entry(board.clone()).or_default();
        if slices.filter == filter {
            return;
        }
        slices.filter = filter.clone();
        let board = board.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            this.update(cx, |this, cx| {
                if this.slices.get(&board).is_some_and(|s| s.filter == filter) {
                    this.read_slice(&board, SliceId::Backlog, How::Again, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Epic `epic`'s issues in all, on board `board`'s site, read once.
    pub fn read_epic_count(&mut self, board: &BoardId, epic: &str, cx: &mut Context<Self>) {
        let key = (board.0.clone(), epic.to_owned());
        let Some(site) = self.site_of(&board.0) else {
            return;
        };
        if matches!(
            self.epic_counts.get(&key),
            Some(Read::Reading | Read::Done(_))
        ) {
            return;
        }
        self.epic_counts.insert(key.clone(), Read::Reading);
        let (id, ask) = (board.1.clone(), SliceAsk::epic(epic));
        spawn_read(
            cx,
            move || model::slice(&site, &id, &ask).map(|p| p.total),
            move |this, read, _| {
                this.epic_counts.insert(key, read_of(read));
            },
        );
    }
}

impl Store {
    /// Issues `keys` of board `board` moved to `to` (a sprint, or the Backlog; its top with `top`): shown at once, then
    /// sent, one write an issue. An issue the tracker refuses goes back where it was, its reason on its `sprint` field;
    /// the others stay moved. `done` hears the move as it stands, for its toast and Undo.
    pub fn move_to(
        &mut self,
        board: &BoardId,
        keys: Vec<String>,
        to: SliceId,
        top: bool,
        done: impl FnOnce(&mut Store, Moved, &mut Context<Store>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(site) = self.site_of(&board.0) else {
            return;
        };
        let sprint = (to.sprint()).map(|id| {
            (self.info.get(board).and_then(Read::done))
                .and_then(|i| i.sprints.iter().find(|s| s.id == id).cloned())
                .unwrap_or(model::Sprint {
                    id,
                    ..Default::default()
                })
        });
        let known: HashMap<String, Row> = (keys.iter())
            .filter_map(|k| Some((k.clone(), self.row(k)?.clone())))
            .collect();
        let me = self.me.get(&board.0).cloned();
        let slices = self.slices.entry(board.clone()).or_default();
        // The Backlog's first issue, that the issues go before at its top: as read, unless a filter left some out.
        let in_order = (slices.reads.get(&SliceId::Backlog)).is_none_or(|r| r.in_order());
        let first = (slices.reads.get(&SliceId::Backlog))
            .filter(|r| r.in_order())
            .and_then(|r| r.rows.iter().find(|r| !r.subtask && !keys.contains(&r.key)))
            .map(|r| r.key.clone());
        let moved = slices.place(&keys, &to, top, sprint.clone(), me.as_ref(), |k| {
            known.get(k).cloned()
        });
        for key in moved.keys() {
            self.edit_row(&key, |r| r.sprint = sprint.clone());
            let at: FieldKey = (key, "sprint".to_owned());
            self.refused.remove(&at);
            self.saving.insert(at);
        }
        self.show_rows(board, cx);
        cx.notify();
        let (id, board, sent) = (board.1.clone(), board.clone(), moved.clone());
        spawn_read(
            cx,
            move || {
                let t = model::tracker(&site);
                // None read: Jira's first, an issue moved left out.
                let first = first.or_else(|| {
                    let ask = SliceAsk {
                        max: sent.placed.len() + 1,
                        ..SliceAsk::backlog()
                    };
                    let page = t.slice(&id, &ask).ok()?;
                    (page.rows.into_iter())
                        .find(|r| !r.subtask && sent.placed.iter().all(|p| p.key != r.key))
                        .map(|r| r.key)
                });
                // Moved to the top out of a filtered Backlog: what followed each is read first, for Undo.
                let mut sent = sent;
                if sent.top && !in_order {
                    slices::follow(&mut sent, |start| {
                        let ask = SliceAsk {
                            start,
                            max: 100,
                            ..SliceAsk::backlog()
                        };
                        t.slice(&id, &ask).ok()
                    });
                }
                let answers = sent
                    .placed
                    .iter()
                    .map(|p| {
                        let write = Write::Sprint {
                            key: p.key.clone(),
                            sprint: sent.to.sprint(),
                            before: if sent.top { first.clone() } else { None },
                        };
                        (p.key.clone(), t.write(&write))
                    })
                    .collect::<Vec<_>>();
                Ok((sent, answers))
            },
            move |this, read, cx| {
                let (moved, answers) = match read {
                    Ok(read) => read,
                    Err(_) => (moved, Vec::new()),
                };
                let moved = this.settle_move(&board, moved, &answers);
                this.show_rows(&board, cx);
                this.keep_slice(&board);
                done(this, moved, cx);
            },
        );
    }

    /// Move `moved` of board `board` as the tracker answered each issue: one refused goes back where it was with its
    /// reason on its `sprint` field; one moved to the Backlog but not ranked at its top keeps the tracker's words on its
    /// `rank`. The move as it stands.
    fn settle_move(
        &mut self,
        board: &BoardId,
        mut moved: Moved,
        answers: &[(String, Result<Value, String>)],
    ) -> Moved {
        let mut refused = Vec::new();
        for (key, answer) in answers {
            self.saving.remove(&(key.clone(), "sprint".to_owned()));
            match answer {
                Err(why) => {
                    self.refused
                        .insert((key.clone(), "sprint".into()), why.clone());
                    refused.push(key.clone());
                }
                Ok(v) => {
                    if let Some(said) = v["unranked"].as_str() {
                        self.refused
                            .insert((key.clone(), "rank".into()), said.to_owned());
                    }
                }
            }
        }
        if refused.is_empty() {
            return moved;
        }
        let me = self.me.get(&board.0).cloned();
        if let Some(slices) = self.slices.get_mut(board) {
            slices.take_back(&moved, &refused, me.as_ref());
        }
        for p in moved.placed.iter().filter(|p| refused.contains(&p.key)) {
            let sprint = p.sprint();
            self.edit_row(&p.key, |r| r.sprint = sprint.clone());
        }
        moved.placed.retain(|p| !refused.contains(&p.key));
        moved
    }

    /// Move `moved` of board `board` taken back, with no word of its own: each issue into the slice it came from,
    /// ranked before the issue that followed it there. One the tracker refuses to take back has its reason on its
    /// `sprint` field, and the slices are read again.
    pub fn undo_move(&mut self, board: &BoardId, moved: Moved, cx: &mut Context<Self>) {
        let Some(site) = self.site_of(&board.0) else {
            return;
        };
        let me = self.me.get(&board.0).cloned();
        if let Some(slices) = self.slices.get_mut(board) {
            slices.take_back(&moved, &moved.keys(), me.as_ref());
        }
        for p in &moved.placed {
            let sprint = p.sprint();
            self.edit_row(&p.key, |r| r.sprint = sprint.clone());
        }
        self.show_rows(board, cx);
        cx.notify();
        let writes = slices::undo_writes(&moved);
        let board = board.clone();
        spawn_read(
            cx,
            move || {
                let t = model::tracker(&site);
                Ok((writes.iter())
                    .map(|w| (w.key().unwrap_or_default().to_owned(), t.write(w)))
                    .collect::<Vec<_>>())
            },
            move |this, read, cx| {
                let mut refused = false;
                for (key, answer) in read.unwrap_or_default() {
                    if let Err(why) = answer {
                        this.refused.insert((key, "sprint".into()), why);
                        refused = true;
                    }
                }
                if refused && let Some(view) = this.slices.get(&board).and_then(|s| s.view.clone())
                {
                    this.read_slice(&board, view, How::Again, cx);
                }
                this.keep_slice(&board);
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_plugin_sdk::kit::{format::clock, fresh};

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
    fn a_card_moved_by_someone_else_waits_in_its_place_until_let_in() {
        let mut store = test_store();
        let id = board("1").key();
        store.board = Some(id.clone());
        let moved = |r: Row| Row {
            state: model::State {
                id: "3".into(),
                ..r.state.clone()
            },
            ..r
        };
        let first = store.begin_rows(&id, Ask::Show).unwrap();
        store.put_rows(&id, first, Ok(vec![row("PAY-1"), row("PAY-2")]), false, 60);
        assert_eq!(store.slot(), Some(Slot::Updated(clock(60))));
        // The minute's refresh: PAY-1 moved to another column, PAY-2 has a comment more.
        let read = vec![
            moved(row("PAY-1")),
            Row {
                comments: 1,
                ..row("PAY-2")
            },
        ];
        let refresh = store.begin_rows(&id, Ask::Refresh).unwrap();
        assert_eq!(store.slot(), Some(Slot::Refreshing));
        store.put_rows(&id, refresh, Ok(read.clone()), true, 120);
        let shown = store.board_rows().unwrap();
        assert_eq!(shown[0], row("PAY-1"));
        assert_eq!(shown[1].comments, 1);
        assert_eq!(
            store.slot(),
            Some(Slot::Waiting(clock(120), 1, fresh::Held::MovedAbove))
        );
        // Shown: what was read whole; ⌘R's read shows all at once.
        let waiting = store.held.remove(&id).unwrap();
        assert_eq!(waiting.rows, read);
        let again = store.begin_rows(&id, Ask::Reload).unwrap();
        store.put_rows(&id, again, Ok(vec![row("PAY-1")]), false, 180);
        assert!(store.held.is_empty());
        assert_eq!(store.slot(), Some(Slot::Updated(clock(180))));
    }

    #[test]
    fn the_slot_says_a_site_not_reached_over_what_was_read_and_nothing_read() {
        let mut store = test_store();
        let id = board("1").key();
        store.board = Some(id.clone());
        let first = store.begin_rows(&id, Ask::Show).unwrap();
        assert_eq!(store.slot(), Some(Slot::Reading));
        store.put_rows(&id, first, Err("no answer".into()), false, 60);
        assert_eq!(store.slot(), Some(Slot::CantRead));
        let again = store.begin_rows(&id, Ask::Reload).unwrap();
        store.put_rows(&id, again, Ok(vec![row("PAY-1")]), false, 90);
        let refresh = store.begin_rows(&id, Ask::Refresh).unwrap();
        store.put_rows(&id, refresh, Err("no answer".into()), true, 150);
        assert_eq!(store.slot(), Some(Slot::CantReach(clock(90))));
        assert_eq!(store.board_rows().map(Vec::len), Some(1));
        store.from_disk.insert(id);
        store.stale.clear();
        assert_eq!(store.slot(), Some(Slot::Kept(clock(90))));
    }

    #[test]
    fn an_older_answer_is_ignored() {
        let mut store = test_store();
        let id = board("1").key();
        let old = store.begin_rows(&id, Ask::Show).unwrap();
        let new = store.begin_rows(&id, Ask::Written).unwrap();
        assert!(store.put_rows(&id, new, Ok(vec![row("PAY-2")]), false, 2));
        assert!(!store.put_rows(&id, old, Ok(vec![row("PAY-1")]), false, 3));
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
        assert!(!store.put_rows(&left, number, Ok(vec![row("PAY-1")]), false, 1));
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
        store.put_rows(&id, rows, Ok(Vec::new()), false, 100);
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
        store.put_rows(&id, read, Err("no answer".into()), false, 90);
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

    fn slice_key(slice: SliceId) -> (BoardId, SliceId) {
        (board("7").key(), slice)
    }

    #[test]
    fn a_slice_reads_on_page_by_page_and_an_overtaken_page_is_left_out() {
        let mut store = test_store();
        let key = slice_key(SliceId::Sprint(34));
        let (first, ask) = store.begin_slice(&key, How::Show).unwrap();
        assert_eq!((ask.start, ask.max), (0, slices::PAGE));
        assert_eq!(store.begin_slice(&key, How::Show), None);
        let page = |start: usize, n: usize| model::Page {
            rows: (start..start + n)
                .map(|i| row(&format!("PAY-{i}")))
                .collect(),
            total: 60,
        };
        let step = Step {
            start: 0,
            page: page(0, 50),
            count: Some(58),
            mine: Some(4),
        };
        let (second, next) = store.put_slice(&key, first, Ok(step), 1).unwrap();
        assert_eq!(next.start, 50);
        let read = &store.slices[&key.0].reads[&key.1];
        assert_eq!((read.read(), read.count), (50, Some(58)));
        // ⌘R overtakes the page on its way.
        let (again, _) = store.begin_slice(&key, How::Again).unwrap();
        let late = Step {
            start: 50,
            page: page(50, 10),
            ..Default::default()
        };
        assert_eq!(store.put_slice(&key, second, Ok(late), 2), None);
        assert_eq!(store.slices[&key.0].reads[&key.1].rows.len(), 50);
        assert_eq!(store.put_slice(&key, again, Err("504".into()), 3), None);
        let read = &store.slices[&key.0].reads[&key.1];
        assert_eq!(read.state, Some(Read::Failed("504".into())));
        assert_eq!(read.rows.len(), 50);
        // Another slice of the board is not touched by it.
        assert!(
            store
                .begin_slice(&slice_key(SliceId::Backlog), How::Show)
                .is_some()
        );
        store.drop_reads(|_| true);
        let backlog = &store.slices[&key.0].reads[&SliceId::Backlog];
        assert_eq!(backlog.state, None);
    }

    #[test]
    fn a_move_refused_goes_back_with_the_trackers_reason() {
        let mut store = test_store();
        let board = board("7").key();
        let read = |keys: &[&str], sprint: Option<u64>| {
            let mut read = slices::SliceRead::default();
            read.rows = (keys.iter())
                .map(|k| Row {
                    sprint: sprint.map(|id| model::Sprint {
                        id,
                        ..Default::default()
                    }),
                    ..row(k)
                })
                .collect();
            read.count = Some(keys.len());
            read.state = Some(Read::Done(()));
            read
        };
        let slices = store.slices.entry(board.clone()).or_default();
        slices
            .reads
            .insert(SliceId::Sprint(34), read(&["PAY-1", "PAY-2"], Some(34)));
        slices
            .reads
            .insert(SliceId::Backlog, read(&["PAY-9"], None));
        let keys = vec!["PAY-1".to_owned(), "PAY-2".to_owned()];
        let moved = slices.place(&keys, &SliceId::Backlog, true, None, None, |_| None);
        let answers = vec![
            (
                "PAY-1".to_owned(),
                Ok(artoo_plugin_sdk::serde_json::json!({"unranked": "Ranking is off"})),
            ),
            ("PAY-2".to_owned(), Err("You may not move PAY-2".to_owned())),
        ];
        let stands = store.settle_move(&board, moved, &answers);
        assert_eq!(stands.keys(), ["PAY-1"]);
        let reads = &store.slices[&board].reads;
        let keys_in = |slice: SliceId| -> Vec<String> {
            reads[&slice].rows.iter().map(|r| r.key.clone()).collect()
        };
        assert_eq!(keys_in(SliceId::Sprint(34)), ["PAY-2"]);
        assert_eq!(keys_in(SliceId::Backlog), ["PAY-1", "PAY-9"]);
        assert_eq!(reads[&SliceId::Sprint(34)].count, Some(1));
        assert_eq!(reads[&SliceId::Backlog].count, Some(2));
        let said = |key: &str, field: &str| {
            store
                .refused
                .get(&(key.to_owned(), field.to_owned()))
                .cloned()
        };
        assert_eq!(
            said("PAY-2", "sprint").as_deref(),
            Some("You may not move PAY-2")
        );
        assert_eq!(said("PAY-1", "rank").as_deref(), Some("Ranking is off"));
        assert_eq!(
            slices::undo_writes(&stands),
            [Write::Sprint {
                key: "PAY-1".into(),
                sprint: Some(34),
                // The issue after it was moved too: nothing to go back before.
                before: None
            }]
        );
    }

    #[test]
    fn an_accounts_readers_are_each_folder_once_by_its_name() {
        let mut store = test_store();
        for p in ["/a/web", "/z/pay", "/z/pay/", "/b/other"] {
            let host = if p == "/b/other" {
                "trello.com"
            } else {
                "acme.atlassian.net"
            };
            store.links.insert(p.into(), vec![host.into()]);
        }
        assert_eq!(
            store.readers("acme.atlassian.net"),
            [PathBuf::from("/z/pay"), PathBuf::from("/a/web")]
        );
    }
}
