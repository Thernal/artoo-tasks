//! What the plugin has read of the trackers, one for the app: every site named and who the token is there, their
//! boards and each board's columns, epics, sprints and issues, each issue opened with its moves and its edit screen,
//! the people an issue may be given to, the pictures of its attachments. Every read runs in the background, the
//! sites side by side; its result is kept, and the views drawn again.
//!
//! A change is shown at once and sent (`change`): taken back with the tracker's reason on its field when refused.
//! What is kept on the Mac — drafts, an agent's comments waiting for Post, how each board is shown, the boards
//! opened lately, a description being edited — is in the plugin's folder (`keep`).

use crate::jira::{self, Site};
use crate::keep;
use crate::model::{
    self, Board, BoardId, BoardInfo, Field, Issue, Kind, Move, News, Person, Row, Write,
};
use artoo_plugin_sdk::gpui::{App, AppContext, Context, Entity, Global, Image, ImageFormat};
use artoo_plugin_sdk::serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
    /// News read at least once: what the first read finds is not toasted.
    news_read: bool,
    /// News new since the last read, not toasted yet.
    pub news_fresh: Vec<(String, News)>,
    /// The news a toast's Open opens: the last toasted.
    pub news_last: Option<(String, String)>,
    /// The news is read each minute.
    news_on: bool,
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
    let store = cx.new(|_| Store::blank(sites, missing, kept));
    cx.set_global(Shared(store.clone()));
    store
}

impl Store {
    /// Nothing read yet: the sites named, what is kept.
    fn blank(sites: Vec<Site>, missing: Vec<String>, kept: keep::Kept) -> Self {
        Store {
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
            news_read: false,
            news_fresh: Vec::new(),
            news_last: None,
            news_on: false,
        }
    }
}

/// A store with nothing read and nothing kept, for a test.
#[cfg(test)]
pub fn test_store() -> Store {
    Store::blank(Vec::new(), Vec::new(), keep::Kept::default())
}

/// Reads `what` in the background and keeps it at `put` once in.
fn spawn_read<T: Send + 'static>(
    cx: &mut Context<Store>,
    what: impl FnOnce() -> Result<T, String> + Send + 'static,
    put: impl FnOnce(&mut Store, Result<T, String>, &mut Context<Store>) + 'static,
) {
    let task = cx.background_spawn(async move { what() });
    cx.spawn(async move |this, cx| {
        let read = task.await;
        this.update(cx, |this, cx| {
            put(this, read, cx);
            cx.notify();
        })
        .ok();
    })
    .detach();
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
        self.sites.iter().find(|s| s.host() == host).cloned()
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
        let reads: Vec<_> = (self.sites.iter().cloned())
            .map(|site| {
                cx.background_spawn(async move {
                    let me = model::myself(&site).ok();
                    (site.host().to_owned(), me, model::boards(&site))
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
                this.me.extend(mes);
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
                this.read_board(false, cx);
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

    /// The news read now and each minute after, from the first tab that lists it; once.
    pub fn watch_news(&mut self, cx: &mut Context<Self>) {
        if self.news_on || self.sites.is_empty() {
            return;
        }
        self.news_on = true;
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(()) = this.update(cx, |s, cx| s.read_news(cx)) else {
                    break;
                };
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(60))
                    .await;
            }
        })
        .detach();
    }

    /// What others did lately on your issues, every site's where who you are is known; the boards (and you) read
    /// first.
    fn read_news(&mut self, cx: &mut Context<Self>) {
        if self.boards.is_none() {
            return self.read_boards(cx);
        }
        let sites: Vec<(Site, Person)> = (self.sites.iter())
            .filter_map(|s| self.me.get(s.host()).map(|m| (s.clone(), m.clone())))
            .collect();
        if sites.is_empty() {
            return;
        }
        // The projects' names, for where each news is.
        for (site, _) in &sites {
            self.read_kinds(site.host(), "", cx);
        }
        spawn_read(
            cx,
            move || {
                Ok((sites.iter())
                    .flat_map(|(site, me)| {
                        let host = site.host().to_owned();
                        (model::news(site, me).unwrap_or_default().into_iter())
                            .map(move |n| (host.clone(), n))
                    })
                    .collect::<Vec<_>>())
            },
            |this, read: Result<Vec<(String, News)>, String>, _| {
                let Ok(news) = read else {
                    return;
                };
                let fresh = (news.iter())
                    .filter(|(h, n)| !this.news_known.contains(&news_key(h, n)))
                    .cloned();
                if this.news_read {
                    this.news_fresh.extend(fresh);
                }
                this.news_read = true;
                (this.news_known).extend(news.iter().map(|(h, n)| news_key(h, n)));
                this.news = news;
            },
        );
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
            .filter(|s| hosts.is_some_and(|h| h.iter().any(|x| x == s.host())))
            .cloned()
            .collect()
    }

    /// The accounts kept that project `project`'s space does not read yet.
    pub fn sites_not_in(&self, project: &str) -> Vec<Site> {
        let hosts = self.links.get(project);
        (self.sites.iter())
            .filter(|s| !hosts.is_some_and(|h| h.iter().any(|x| x == s.host())))
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
            Read::Reading => Read::Reading,
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
            _ => return cx.notify(),
        };
        let board = self.board_among(&boards);
        if board != self.board {
            self.board = board;
            self.faces.clear();
            self.read_board(false, cx);
        }
        cx.notify();
    }

    /// Board `id` picked: kept as the one opened last and among the boards opened lately.
    pub fn pick_board(&mut self, id: BoardId, cx: &mut Context<Self>) {
        self.board = Some(id.clone());
        self.faces.clear();
        self.kept.opened_board(id, self.here.as_deref());
        self.read_board(false, cx);
        cx.notify();
    }

    /// The board picked: its columns, epics and sprints, and its issues; read when they are not yet, or again with
    /// `again` (quietly: what was read stays on screen, and a failure says so over it).
    pub fn read_board(&mut self, again: bool, cx: &mut Context<Self>) {
        let Some(board) = self.board.clone() else {
            return;
        };
        let Some(site) = self.site_of(&board.0) else {
            return;
        };
        if again || !self.info.contains_key(&board) {
            if !self.info.contains_key(&board) {
                self.info.insert(board.clone(), Read::Reading);
            }
            let (s, id, key) = (site.clone(), board.1.clone(), board.clone());
            spawn_read(
                cx,
                move || model::board_info(&s, &id),
                move |this, read, _| {
                    if read.is_ok() || !matches!(this.info.get(&key), Some(Read::Done(_))) {
                        this.info.insert(key, read_of(read));
                    }
                },
            );
        }
        if !again && self.rows.contains_key(&board) {
            return;
        }
        if !self.rows.contains_key(&board) {
            self.rows.insert(board.clone(), Read::Reading);
        }
        let id = board.1.clone();
        spawn_read(
            cx,
            move || model::board_issues(&site, &id),
            move |this, read, _| match read {
                Ok(rows) => {
                    this.rows.insert(board.clone(), Read::Done(rows));
                    this.read_at.insert(board.clone(), now());
                    this.stale.remove(&board);
                }
                // Read before: kept, the failure said over it.
                Err(why) if matches!(this.rows.get(&board), Some(Read::Done(_))) => {
                    this.stale.insert(board, why);
                }
                Err(why) => {
                    this.rows.insert(board, Read::Failed(why));
                }
            },
        );
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
        self.moves.insert(key.to_owned(), Read::Reading);
        let (k, put) = (key.to_owned(), key.to_owned());
        spawn_read(
            cx,
            move || model::moves(&site, &k),
            move |this, read, _| {
                this.moves_at.insert(put.clone(), now());
                this.moves.insert(put, read_of(read));
            },
        );
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
        let host = site.host().to_owned();
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
                    this.read_issue(&at.0, Some(&host), true, cx);
                    this.read_board(true, cx);
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
            _ => None,
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
            project: None,
        }
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
