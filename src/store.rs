//! What the plugin has read of the trackers, one for the app: the sites, their boards, the board picked, its issues,
//! and each issue opened. Every read runs in the background; its result is kept, and the views drawn again.

use crate::jira::{self, Site};
use crate::model::{self, Board, Issue, Row};
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
    pub boards: Option<Read<Vec<Board>>>,
    /// The board picked, by its id.
    pub board: Option<u64>,
    /// Only the issues assigned to you.
    pub mine: bool,
    pub rows: HashMap<(u64, bool), Read<Vec<Row>>>,
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
        board: None,
        mine: false,
        rows: HashMap::new(),
        issues: HashMap::new(),
    });
    cx.set_global(Shared(store.clone()));
    store
}

impl Store {
    /// The site issues are read from: the first one named.
    pub fn site(&self) -> Option<Site> {
        self.sites.first().cloned()
    }

    /// The boards, read once.
    pub fn read_boards(&mut self, cx: &mut Context<Self>) {
        if self.boards.is_some() {
            return;
        }
        let Some(site) = self.site() else {
            return;
        };
        self.boards = Some(Read::Reading);
        let task = cx.background_spawn(async move { model::boards(&site) });
        cx.spawn(async move |this, cx| {
            let read = task.await;
            this.update(cx, |this, cx| {
                if this.board.is_none() {
                    this.board = read.as_ref().ok().and_then(|b| b.first()).map(|b| b.id);
                }
                this.boards = Some(match read {
                    Ok(boards) => Read::Done(boards),
                    Err(why) => Read::Failed(why),
                });
                this.read_rows(false, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The board picked's issues, read when they are not yet, or again with `again`.
    pub fn read_rows(&mut self, again: bool, cx: &mut Context<Self>) {
        let (Some(site), Some(board)) = (self.site(), self.board) else {
            return;
        };
        let key = (board, self.mine);
        if !again && self.rows.contains_key(&key) {
            return;
        }
        if !self.rows.contains_key(&key) {
            self.rows.insert(key, Read::Reading);
        }
        let mine = self.mine;
        let task = cx.background_spawn(async move { model::board_issues(&site, board, mine) });
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

    /// Issue `key`, read when it is not yet, or again with `again`.
    pub fn read_issue(&mut self, key: &str, again: bool, cx: &mut Context<Self>) {
        let Some(site) = self.site() else {
            return;
        };
        if !again && self.issues.contains_key(key) {
            return;
        }
        if !self.issues.contains_key(key) {
            self.issues.insert(key.to_owned(), Read::Reading);
        }
        let key = key.to_owned();
        let asked = key.clone();
        let task = cx.background_spawn(async move { model::read_issue(&site, &asked) });
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
            Some(Read::Done(boards)) => boards.iter().find(|b| Some(b.id) == self.board),
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
