//! What the plugin has read of the trackers, one for the app: every site named, their boards, the board picked, its
//! issues, and each issue opened. Every read runs in the background, the sites side by side; its result is kept, and
//! the views drawn again.

use crate::jira::{self, Site};
use crate::model::{self, Board, BoardId, Issue, Row};
use artoo_plugin_sdk::gpui::{App, AppContext, Context, Entity, Global};
use std::collections::HashMap;

/// A read: on its way, done, or why not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Read<T> {
    Reading,
    Done(T),
    Failed(String),
}

pub struct Store {
    pub sites: Vec<Site>,
    /// Sites named with no token, said in the space.
    pub missing: Vec<String>,
    /// Every site's boards; failed only when no site's were read.
    pub boards: Option<Read<Vec<Board>>>,
    /// The sites whose boards were not read, and why: said under the others'.
    pub unread: Vec<String>,
    /// The board picked.
    pub board: Option<BoardId>,
    /// Only the issues assigned to you.
    pub mine: bool,
    pub rows: HashMap<(BoardId, bool), Read<Vec<Row>>>,
    pub issues: HashMap<String, Read<Issue>>,
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
    let (sites, missing) = jira::sites();
    let store = cx.new(|_| Store {
        sites,
        missing,
        boards: None,
        unread: Vec::new(),
        board: None,
        mine: false,
        rows: HashMap::new(),
        issues: HashMap::new(),
    });
    cx.set_global(Shared(store.clone()));
    store
}

impl Store {
    /// The site of `host`, when it is named.
    pub fn site_of(&self, host: &str) -> Option<Site> {
        self.sites.iter().find(|s| s.host() == host).cloned()
    }

    /// Every site's boards, read once, side by side.
    pub fn read_boards(&mut self, cx: &mut Context<Self>) {
        if self.boards.is_some() || self.sites.is_empty() {
            return;
        }
        self.boards = Some(Read::Reading);
        let reads: Vec<_> = (self.sites.iter().cloned())
            .map(|site| cx.background_spawn(async move { model::boards(&site) }))
            .collect();
        cx.spawn(async move |this, cx| {
            let (mut boards, mut unread) = (Vec::new(), Vec::new());
            for read in reads {
                match read.await {
                    Ok(more) => boards.extend(more),
                    Err(why) => unread.push(why),
                }
            }
            this.update(cx, |this, cx| {
                if this.board.is_none() {
                    this.board = boards.first().map(Board::key);
                }
                this.boards = Some(if boards.is_empty() && !unread.is_empty() {
                    Read::Failed(unread.join(" · "))
                } else {
                    Read::Done(boards)
                });
                this.unread = unread;
                this.read_rows(false, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The board picked's issues, read when they are not yet, or again with `again`.
    pub fn read_rows(&mut self, again: bool, cx: &mut Context<Self>) {
        let Some(board) = self.board.clone() else {
            return;
        };
        let Some(site) = self.site_of(&board.0) else {
            return;
        };
        let key = (board.clone(), self.mine);
        if !again && self.rows.contains_key(&key) {
            return;
        }
        if !self.rows.contains_key(&key) {
            self.rows.insert(key.clone(), Read::Reading);
        }
        let mine = self.mine;
        let task = cx.background_spawn(async move { model::board_issues(&site, board.1, mine) });
        cx.spawn(async move |this, cx| {
            let read = task.await;
            this.update(cx, |this, cx| {
                this.rows.insert(
                    key,
                    match read {
                        Ok(rows) => Read::Done(rows),
                        Err(why) => Read::Failed(why),
                    },
                );
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Issue `key`, read when it is not yet, or again with `again`: from its site when it is known, else from the first
    /// site that has it.
    pub fn read_issue(&mut self, key: &str, site: Option<&str>, again: bool, cx: &mut Context<Self>) {
        let sites = match site.and_then(|host| self.site_of(host)) {
            Some(site) => vec![site],
            None => self.sites.clone(),
        };
        if sites.is_empty() {
            return;
        }
        if !again && self.issues.contains_key(key) {
            return;
        }
        if !self.issues.contains_key(key) {
            self.issues.insert(key.to_owned(), Read::Reading);
        }
        let key = key.to_owned();
        let asked = key.clone();
        let task = cx.background_spawn(async move { model::read_issue_on(&sites, &asked) });
        cx.spawn(async move |this, cx| {
            let read = task.await;
            this.update(cx, |this, cx| {
                this.issues.insert(
                    key,
                    match read {
                        Ok(issue) => Read::Done(issue),
                        Err(why) => Read::Failed(why),
                    },
                );
                cx.notify();
            })
            .ok();
        })
        .detach();
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
        match self.issues.get(key) {
            Some(Read::Done(issue)) => Some(issue),
            _ => None,
        }
    }
}
