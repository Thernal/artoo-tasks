//! The Tasks space in one tab: its own page — the board picked, as a list beside a preview or as columns, or the
//! drafts —, an issue's page, the popovers both open, a note for an agent (#212) on a paragraph or on the board as it
//! is filtered, in the app's composer. Drawn in `board.rs`, `issue.rs`, `pop.rs`, `sheet.rs`; the side panel and the
//! page bar in `side.rs` and `bar.rs`.

use crate::agents::{self as places_of, ISSUE};
use crate::keep::{Group, View};
use crate::model::BoardId;
use crate::store::{Read, Store};
use artoo_plugin_sdk::agents::places;
use artoo_plugin_sdk::core::markdown::Helper;
use artoo_plugin_sdk::core::route::Route;
use artoo_plugin_sdk::core::textedit::TextEdit;
use artoo_plugin_sdk::gpui::{
    self, AnyElement, App, ClipboardItem, Context, Entity, KeyDownEvent, Keystroke, Pixels, Point,
    ScrollHandle, Window, div, prelude::*,
};
use artoo_plugin_sdk::kit::composer::{Compose, ComposeAct, Look, Short, Target, Typed as Written};
use artoo_plugin_sdk::kit::{controls, icons, markdown, motion, px, room, theme};
use artoo_plugin_sdk::shell::{Frame, MenuItem, Typed};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

/// The space's id.
pub const SPACE: &str = "tasks";
/// The note composer's input.
pub const NOTE: &str = "tasks.note";
/// A popover's input: its filter, its keys.
pub const INPUT_POP: &str = "tasks.pop";
/// The comment composer's.
pub const INPUT_COMMENT: &str = "tasks.comment";
/// The description being edited.
pub const INPUT_DESC: &str = "tasks.description";
/// The sub-tasks' empty row.
pub const INPUT_SUB: &str = "tasks.subtask";
/// The summary being edited.
pub const INPUT_TITLE: &str = "tasks.title";

/// Where a popover hangs: a row of the board, the preview, the issue page's facts, a sub-task, a detail, the
/// people beside Mine · All.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Anchor {
    Row(String),
    Preview,
    Issue,
    Sub(String),
    Detail(String),
    Faces,
    /// A select of New task, by its field.
    Sheet(String),
}

impl Anchor {
    pub fn id(&self) -> String {
        match self {
            Anchor::Row(k) => format!("row-{k}"),
            Anchor::Preview => "pv".into(),
            Anchor::Issue => "issue".into(),
            Anchor::Sub(k) => format!("sub-{k}"),
            Anchor::Detail(f) => format!("detail-{f}"),
            Anchor::Faces => "faces".into(),
            Anchor::Sheet(f) => format!("sheet-{f}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PopKind {
    /// Move to: the moves the workflow allows.
    Move,
    /// A move that asks for fields.
    Ask,
    Assign,
    Epic,
    /// Everyone on the board, beside Mine · All.
    People,
    /// A field's values, by its id: priority, sprint, labels, components.
    Field(String),
    /// A select of New task, by its field.
    Sheet(String),
}

/// A popover open.
pub struct Pop {
    pub kind: PopKind,
    pub anchor: Anchor,
    /// The issue it is about.
    pub key: String,
    pub filter: String,
    pub edit: TextEdit,
    /// The row the keyboard is on.
    pub at: usize,
    /// A move that asks, and the value picked for each field it asks.
    pub ask: Option<crate::model::Move>,
    pub asked: Vec<usize>,
}

/// Where a note for the agent is being written.
pub enum Spot {
    /// Paragraph `para` of issue `key`'s description, on site `site`.
    Para {
        key: String,
        site: String,
        para: usize,
    },
    /// The board picked as it is filtered now, with how many issues it shows.
    Board {
        board: BoardId,
        name: String,
        mine: bool,
        count: Option<usize>,
    },
}

impl Spot {
    /// What the note's place says, as the Agents window says it.
    fn words(&self) -> String {
        match self {
            Spot::Para { key, para, .. } => format!("{key} · description · ¶{para}"),
            Spot::Board {
                name, mine, count, ..
            } => places_of::board_words(name, *mine, *count),
        }
    }
}

/// A note for the agent being written.
pub struct Note {
    pub spot: Spot,
    pub compose: Compose,
}

/// The tools a composer of the page shows, as the app's own boxes: bold, code, link, list; Preview after them.
pub fn tools() -> Vec<Helper> {
    vec![Helper::Bold, Helper::Code, Helper::Link, Helper::Bullet]
}

/// Where a composer of the page is: the note's, a comment's, the description's, the sheet's.
pub type ComposeOf = for<'a> fn(&'a mut Page) -> Option<&'a mut Compose>;

/// A composer of the kit's, empty, for `target`.
pub fn compose(target: Target, text: &str) -> Compose {
    Compose {
        path: String::new(),
        old: false,
        first: None,
        line: 0,
        code: String::new(),
        edit: TextEdit::new(text.to_owned()),
        at: String::new(),
        target,
        error: None,
        preview: false,
        pick: 0,
        closed: None,
        page: None,
        file: None,
        answers: None,
        attachments: Vec::new(),
    }
}

pub struct Page {
    pub store: Entity<Store>,
    pub frame: Rc<dyn Frame>,
    pub route: Option<Route>,
    pub note: Option<Note>,
    /// The issue picked on the board.
    pub pick: Option<String>,
    /// The groups folded; Done's column is folded until it is in `unfolded`.
    pub folded_ids: HashSet<String>,
    pub unfolded: HashSet<String>,
    /// The side panel's Boards folded.
    pub boards_folded: bool,
    /// The kind whose form the connect card had open when last drawn: its folds move when it changes.
    pub connect_open: Option<String>,
    /// What last folded or opened, and how tall each fold was drawn: every fold moves, in size and alpha (#110).
    pub motion: motion::Marks,
    pub folds: motion::Folds,
    pub heights: room::Heights,
    pub pop: Option<Pop>,
    /// A move the tracker refused: where its reason shows, and to what it was.
    pub refusal: Option<(Anchor, String)>,
    pub refused_to: Option<String>,
    /// The workflow of an issue shown over the page.
    pub workflow: Option<String>,
    /// The drafts on screen in place of the board, the one picked.
    pub drafts: bool,
    pub draft_pick: Option<u64>,
    pub rows_scroll: ScrollHandle,
    pub issue_scroll: ScrollHandle,
    /// The issue page's composers and fields.
    pub issue: crate::issue::IssueState,
    /// The New task sheet, when open.
    pub sheet: Option<crate::sheet::Sheet>,
    /// Whether New task is over the window: the part's `Overlay` reads it.
    pub sheet_shown: Rc<std::cell::Cell<bool>>,
    /// The tab's accounts, whose form the connect card draws.
    pub accounts: Option<Entity<crate::accounts::Accounts>>,
    /// The project this tab's space is of, as the page last drew it: what the shell's hooks read, since a hook may
    /// not read the tab (`project_now`).
    pub here: String,
    /// The connect card on screen in place of the board, `+` beside Boards pressed with this many accounts read:
    /// one more and the board is back.
    pub connecting: Option<usize>,
    /// The last reassignment, which its toast's Undo gives back.
    pub undo_assign: Option<crate::pop::Undo>,
}

impl Page {
    pub fn new(store: Entity<Store>, frame: Rc<dyn Frame>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |p: &mut Self, _, cx| {
            p.sheet_heard(cx);
            cx.notify()
        })
        .detach();
        // What is on screen is read again every minute: someone else's change shows (`Store::note_change`).
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                let Ok(()) = this.update(cx, |p, cx| p.read_again(cx)) else {
                    break;
                };
            }
        })
        .detach();
        Self {
            store,
            frame,
            route: None,
            note: None,
            pick: None,
            folded_ids: HashSet::new(),
            unfolded: HashSet::new(),
            boards_folded: false,
            connect_open: None,
            motion: motion::Marks::default(),
            folds: motion::Folds::default(),
            heights: room::Heights::default(),
            pop: None,
            refusal: None,
            refused_to: None,
            workflow: None,
            drafts: false,
            draft_pick: None,
            rows_scroll: ScrollHandle::new(),
            issue_scroll: ScrollHandle::new(),
            issue: Default::default(),
            sheet: None,
            sheet_shown: Rc::default(),
            accounts: None,
            here: String::new(),
            connecting: None,
            undo_assign: None,
        }
    }

    /// The project this tab's space is of, as last drawn.
    pub fn project(&self) -> &str {
        &self.here
    }

    /// The tab's project, read from the tab: its main working tree, whatever worktree is open; empty with none. Read
    /// in the page's own drawing alone: the shell's hooks run while the tab is being changed.
    fn project_now(&self, cx: &App) -> String {
        let places = self.frame.places(cx);
        (places.project.or(places.root))
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The connect card on screen, over the board: another tracker for the project.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.connecting = Some(self.store.read(cx).sites_in(&self.here).len());
        self.drafts = false;
        cx.notify();
    }

    /// The connect card gone: the project's boards on screen.
    pub fn connected(&mut self, cx: &mut Context<Self>) {
        self.connecting = None;
        if let Some(accounts) = self.accounts.clone() {
            accounts.update(cx, |a, cx| a.close_in_space(cx));
        }
        cx.notify();
    }

    /// The issue page's key, when one is on screen.
    pub fn issue_key(&self) -> Option<String> {
        self.route
            .as_ref()
            .filter(|r| r.kind == ISSUE)
            .and_then(places_of::key_of)
    }

    /// The issue or the board on screen read again, quietly.
    pub fn read_again(&mut self, cx: &mut Context<Self>) {
        let key = self.issue_key();
        let site = self.route.as_ref().and_then(places_of::site_of);
        self.store.update(cx, |s, cx| match key {
            Some(key) => s.read_issue(&key, site.as_deref(), true, cx),
            None => s.read_board(true, cx),
        });
    }

    /// What is on screen now; read anew when it was just opened.
    pub fn show(&mut self, route: Option<&Route>, fresh: bool, cx: &mut Context<Self>) {
        if self.route.as_ref() != route {
            self.pop = None;
            self.refusal = None;
            self.workflow = None;
        }
        self.route = route.cloned();
        let key = self.issue_key();
        let site = route.and_then(places_of::site_of);
        if key.is_some() {
            self.drafts = false;
        }
        let project = self.here.clone();
        if !project.is_empty() {
            self.store.update(cx, |s, cx| s.show_project(&project, cx));
        }
        self.store.update(cx, |s, cx| match key {
            Some(key) => {
                s.read_issue(&key, site.as_deref(), fresh, cx);
                s.read_screen(&key, cx);
            }
            None => {
                s.read_boards(cx);
                s.read_board(fresh, cx);
            }
        });
        cx.notify();
    }

    /// Issue `key` picked on the board: read whole for the preview.
    pub fn pick_row(&mut self, key: &str, cx: &mut Context<Self>) {
        if self.pick.as_deref() != Some(key) {
            self.pick = Some(key.to_owned());
            self.refusal = None;
        }
        let k = key.to_owned();
        self.store
            .update(cx, |s, cx| s.read_issue(&k, None, false, cx));
        cx.notify();
    }

    /// Issue `key`'s page opened.
    pub fn open_issue(&mut self, key: &str, cx: &mut Context<Self>) {
        let host = self
            .store
            .read(cx)
            .site_for(key)
            .map(|s| s.host().to_owned());
        self.frame
            .open(places_of::issue_route(key, host.as_deref()), cx);
    }

    /// ↑ ↓ on the board: the next row picked.
    pub fn step(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(shown) = self.board_shown(cx) else {
            return;
        };
        let order = shown.order(&|id| self.folded(id));
        let at = self
            .pick
            .as_ref()
            .and_then(|k| order.iter().position(|x| x == k));
        let next = match (at, down) {
            (Some(i), true) => (i + 1).min(order.len().saturating_sub(1)),
            (Some(i), false) => i.saturating_sub(1),
            (None, _) => 0,
        };
        if let Some(key) = order.get(next).cloned() {
            self.pick_row(&key, cx);
            self.rows_scroll.scroll_to_item(next);
        }
    }

    /// ↵ on the board: the picked issue opened.
    pub fn open_picked(&mut self, cx: &mut Context<Self>) {
        if self.drafts {
            if let Some(id) = self.draft_pick {
                self.open_draft(id, cx);
            }
            return;
        }
        if let Some(key) = self.pick.clone() {
            self.open_issue(&key, cx);
        }
    }

    /// The issue a key acts on: the page's, else the one picked on the board.
    pub fn acted_on(&self) -> Option<String> {
        self.issue_key().or_else(|| self.pick.clone())
    }

    /// Where a key's popover hangs: the issue page's facts, else the picked row.
    fn key_anchor(&self, sub: bool) -> Option<(Anchor, String)> {
        if let Some(key) = self.issue_key() {
            return Some((
                if sub {
                    Anchor::Detail(String::new())
                } else {
                    Anchor::Issue
                },
                key,
            ));
        }
        let key = self.pick.clone()?;
        Some((Anchor::Row(key.clone()), key))
    }

    /// ⌥S: Move to on the issue on screen or picked.
    pub fn key_move(&mut self, cx: &mut Context<Self>) {
        if let Some((anchor, key)) = self.key_anchor(false) {
            self.open_pop(PopKind::Move, anchor, &key, cx);
        }
    }

    /// A: its people list.
    pub fn key_assign(&mut self, cx: &mut Context<Self>) {
        if let Some((anchor, key)) = self.key_anchor(false) {
            self.open_pop(PopKind::Assign, anchor, &key, cx);
        }
    }

    /// I: the issue given to you.
    pub fn key_assign_me(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.acted_on() else {
            return;
        };
        let s = self.store.read(cx);
        let host = s
            .site_for(&key)
            .map(|x| x.host().to_owned())
            .unwrap_or_default();
        let me = s.me.get(&host).cloned();
        if me.is_some() {
            self.assign(&key, me, cx);
        }
    }

    /// ⇧E: its epic.
    pub fn key_epic(&mut self, cx: &mut Context<Self>) {
        if let Some((anchor, key)) = self.key_anchor(false) {
            let anchor = if anchor == Anchor::Issue {
                Anchor::Detail("epic".into())
            } else {
                anchor
            };
            self.open_pop(PopKind::Epic, anchor, &key, cx);
        }
    }

    /// V: List or Board.
    pub fn toggle_view(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |s, cx| {
            if let Some(b) = s.board.clone() {
                let v = s.kept.view(&b);
                s.kept.set_view(
                    b,
                    View {
                        board: !v.board,
                        ..v
                    },
                );
            }
            cx.notify();
        });
    }

    pub fn set_columns(&mut self, columns: bool, cx: &mut Context<Self>) {
        self.store.update(cx, |s, cx| {
            if let Some(b) = s.board.clone() {
                let v = s.kept.view(&b);
                s.kept.set_view(
                    b,
                    View {
                        board: columns,
                        ..v
                    },
                );
            }
            cx.notify();
        });
    }

    /// The board grouped by `group`.
    pub fn set_group(&mut self, group: Group, cx: &mut Context<Self>) {
        self.store.update(cx, |s, cx| {
            if let Some(b) = s.board.clone() {
                let v = s.kept.view(&b);
                s.kept.set_view(b, View { group, ..v });
            }
            cx.notify();
        });
    }

    /// G, or Group ▾: State, Story, Epic.
    pub fn group_menu(&mut self, at: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let now = {
            let s = self.store.read(cx);
            s.board
                .as_ref()
                .map(|b| s.kept.view(b).group)
                .unwrap_or_default()
        };
        let page = cx.entity().downgrade();
        let items = [Group::State, Group::Story, Group::Epic]
            .into_iter()
            .map(|g| {
                let page = page.clone();
                MenuItem::new(g.word(), move |_, cx| {
                    page.update(cx, |p, cx| p.set_group(g, cx)).ok();
                })
                .checked(g == now)
            })
            .collect();
        let at = at.unwrap_or_else(|| gpui::point(px(600.), px(120.)));
        self.frame.menu(at, vec![items], cx);
    }

    /// Mine or All.
    pub fn set_mine(&mut self, mine: bool, cx: &mut Context<Self>) {
        self.store.update(cx, |s, cx| {
            s.mine = mine;
            cx.notify();
        });
    }

    /// A face beside Mine · All picked, or let go.
    pub fn toggle_face(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = id.to_owned();
        self.store.update(cx, |s, cx| {
            if s.faces.contains(&id) {
                s.faces.retain(|f| *f != id);
            } else {
                s.faces.push(id);
            }
            cx.notify();
        });
    }

    pub fn note_open(&self) -> bool {
        self.note.is_some()
    }

    pub fn board_note_open(&self) -> bool {
        self.note
            .as_ref()
            .is_some_and(|n| matches!(n.spot, Spot::Board { .. }))
    }

    /// A note for the agent on the board picked, as it is filtered now (its ⋯, ⌘K).
    pub fn open_board_note(&mut self, cx: &mut Context<Self>) {
        let count = self.board_shown(cx).map(|s| s.count);
        let s = self.store.read(cx);
        let Some(board) = s.picked().cloned() else {
            self.frame
                .error("Open a board to write a note on it".into(), cx);
            return;
        };
        let spot = Spot::Board {
            board: board.key(),
            name: board.name.clone(),
            mine: s.mine,
            count,
        };
        self.open_note(spot, cx);
    }

    /// What the page bar's `⋯` holds: an issue's branch name and link to copy (its key is selectable on its page)
    /// and a note for an agent on it; a board's note and link.
    pub fn more_items(page: &Entity<Self>, cx: &App) -> Vec<MenuItem> {
        let p = page.read(cx);
        let s = p.store.read(cx);
        let weak = page.downgrade();
        let copy = |words: String, text: Option<String>| match text {
            Some(t) => MenuItem::new(words, move |_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(t.clone()))
            }),
            None => MenuItem::off(words),
        };
        match p.issue_key() {
            Some(key) => {
                let url = s.issue(&key).map(|i| i.url.clone());
                let branch = s.issue(&key).map(|i| branch_name(&key, &i.row.summary));
                vec![
                    copy("Copy branch name".into(), branch),
                    copy("Copy link".into(), url),
                    MenuItem::new("Note for an agent…", move |_, cx| {
                        weak.update(cx, |p, cx| p.note_on_first_paragraph(cx)).ok();
                    })
                    .key("C"),
                ]
            }
            None => {
                let link = (s.picked())
                    .and_then(|b| Some(crate::model::board_url(&s.site_of(&b.site)?, &b.id)));
                vec![
                    MenuItem::new("Note for an agent…", move |_, cx| {
                        weak.update(cx, |p, cx| p.open_board_note(cx)).ok();
                    })
                    .key("C"),
                    copy("Copy link".into(), link),
                ]
            }
        }
    }

    /// C on an issue's page: a note on its first paragraph.
    pub fn note_on_first_paragraph(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.issue_key() else {
            return;
        };
        let site = self
            .store
            .read(cx)
            .issue(&key)
            .map(|i| i.site.clone())
            .unwrap_or_default();
        let para = self.issue.hovered_para.unwrap_or(1);
        self.open_note(Spot::Para { key, site, para }, cx);
    }

    pub fn open_note(&mut self, spot: Spot, cx: &mut Context<Self>) {
        self.note = Some(Note {
            spot,
            compose: compose(Target::Agent, ""),
        });
        self.frame.take_keyboard(NOTE, cx);
        cx.notify();
    }

    /// A composer's toolbar pressed: a helper on its text, Preview, or the folded tools' menu; false for any other
    /// act.
    pub fn compose_tool(
        &mut self,
        of: ComposeOf,
        act: &ComposeAct,
        cx: &mut Context<Self>,
    ) -> bool {
        match act {
            ComposeAct::Helper(helper) => {
                if let Some(c) = of(self) {
                    c.apply(*helper);
                }
            }
            ComposeAct::Preview => {
                if let Some(c) = of(self) {
                    c.toggle_preview();
                }
            }
            ComposeAct::Format { at, tools } => {
                let preview = of(self).is_some_and(|c| c.preview);
                let page = cx.entity().downgrade();
                let press = move |act: ComposeAct| {
                    let page = page.clone();
                    move |_: &mut Window, cx: &mut App| {
                        page.update(cx, |p, cx| p.compose_tool(of, &act, cx)).ok();
                    }
                };
                let helpers = (tools.iter())
                    .map(|&h| {
                        // The tip's name and keys: `Link — text, then …  ⌘⇧K` as `Link` and `⌘⇧K`.
                        let (_, tip) = markdown::helper_look(h);
                        let (name, key) = tip.split_once("  ").unwrap_or((tip, ""));
                        let name = name.split(" — ").next().unwrap_or(name);
                        MenuItem::new(name, press(ComposeAct::Helper(h))).key(key)
                    })
                    .collect();
                let preview = MenuItem::new("Preview", press(ComposeAct::Preview))
                    .key("⌘⇧P")
                    .checked(preview);
                self.frame.menu(*at, vec![helpers, vec![preview]], cx);
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// ⌘B ⌘I ⇧⌘X ⇧⌘E ⇧⌘K or ⇧⌘P on a composer, done: whether the key was one of them.
    pub fn compose_short(
        &mut self,
        of: ComposeOf,
        key: &Keystroke,
        cx: &mut Context<Self>,
    ) -> bool {
        match Compose::shortcut(key) {
            Some(Short::Helper(helper)) => self.compose_tool(of, &ComposeAct::Helper(helper), cx),
            Some(Short::Preview) => self.compose_tool(of, &ComposeAct::Preview, cx),
            None => false,
        }
    }

    /// A key while the note has the keyboard.
    pub fn note_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let key = &event.keystroke;
        if key.modifiers.platform && key.key == "enter" {
            self.add_note(cx);
            return Typed::Leave;
        }
        if self.compose_short(|p| p.note.as_mut().map(|n| &mut n.compose), key, cx) {
            return Typed::Kept;
        }
        let Some(note) = self.note.as_mut() else {
            return Typed::Leave;
        };
        let typed = note.compose.edit_key(key, cx);
        cx.notify();
        match typed {
            Written::Cancel => {
                self.note = None;
                Typed::Leave
            }
            Written::Leave => Typed::Leave,
            Written::Ignored | Written::Changed => Typed::Kept,
        }
    }

    /// The note added to the tab's drafts for the agent, at its place.
    fn add_note(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.note.take() else {
            return;
        };
        let body = note.compose.edit.text().trim().to_owned();
        if body.is_empty() {
            return;
        }
        let (kind, data) = match &note.spot {
            Spot::Para { key, site, para } => {
                ("jira.issue", places_of::paragraph_place(key, site, *para))
            }
            Spot::Board {
                board,
                name,
                mine,
                count,
            } => (
                "jira.board",
                places_of::board_place(board, name, *mine, *count),
            ),
        };
        self.send_to_agent(kind, &data, body, cx);
        self.frame.leave_keyboard(NOTE, cx);
        cx.notify();
    }

    /// `body` added to the tab's drafts for the agent, on the place of `kind` `data`; the Agents window shown.
    pub fn send_to_agent(&mut self, kind: &str, data: &str, body: String, cx: &mut Context<Self>) {
        let agents = self
            .frame
            .services()
            .get::<Rc<dyn artoo_plugin_sdk::agents::tab::Agents>>();
        match (places::on(cx, kind, data), agents) {
            (Some(on), Some(agents)) => {
                agents.add_comment(places::note(on, body), cx);
                self.frame
                    .show_tool(Some(artoo_plugin_sdk::agents::view::WINDOW), cx);
            }
            _ => self.frame.error("Notes for agents are not here".into(), cx),
        }
    }

    /// The app's composer, for the agent, at its place.
    pub fn composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(note) = &self.note else {
            return div().into_any_element();
        };
        let place = note.spot.words();
        let head = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child("To:")
                    .child(div().text_color(theme::agent()).child("Your agent")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(theme::hover())
                    .text_size(px(11.))
                    .text_color(theme::muted())
                    .child(icons::icon("pin").size(px(11.)).text_color(theme::muted()))
                    .child(place),
            )
            .into_any_element();
        let page = cx.entity();
        let acts: artoo_plugin_sdk::kit::composer::ComposeActs = Rc::new(move |act, cx| {
            page.update(cx, |this, cx| {
                if this.compose_tool(|p| p.note.as_mut().map(|n| &mut n.compose), &act, cx) {
                    return;
                }
                if let (
                    Some(note),
                    ComposeAct::Place {
                        at,
                        shift,
                        clicks,
                        drag,
                        lower,
                    },
                ) = (this.note.as_mut(), act)
                {
                    note.compose.place(at, shift, clicks, drag, lower);
                    cx.notify();
                }
            });
        });
        let add = controls::primary_button("tasks-note-add", "Add to drafts", "⌘↵")
            .bg(theme::agent())
            .on_click(cx.listener(|this, _, _, cx| this.add_note(cx)));
        let cancel = div()
            .id("tasks-note-cancel")
            .cursor_pointer()
            .child(icons::icon("x").size(px(14.)).text_color(theme::muted()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.note = None;
                this.frame.leave_keyboard(NOTE, cx);
                cx.notify();
            }));
        let look = Look {
            tone: theme::agent(),
            placeholder: "What should the agent do here?".into(),
            head: Some(head),
            lead: None,
            tools: tools(),
            tail: vec![cancel.into_any_element(), add.into_any_element()],
            folded: false,
            focused: true,
            lines: None,
            completion: None,
            measure: div().into_any_element(),
            link_base: None,
            clip: false,
        };
        let input = note.compose.render_input(look, acts, cx);
        artoo_plugin_sdk::shell::input_box(
            div().id("tasks-note-box").child(input),
            NOTE,
            &self.frame,
        )
        .into_any_element()
    }

    /// The board's own page: the board, the drafts, or what is missing for them; the connect card while the project
    /// reads no tracker, or while `+` asked for another.
    fn own_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let project = self.here.clone();
        let s = self.store.read(cx);
        let sites = s.sites_in(&project);
        match self.connecting {
            Some(n) if sites.len() > n => self.connecting = None,
            Some(_) => return self.connect_page(&project, !sites.is_empty(), cx),
            None if sites.is_empty() => return self.connect_page(&project, false, cx),
            None => {}
        }
        let s = self.store.read(cx);
        let boards = s.boards_in(&project);
        // The board picked is another project's, in a window behind: this one's is picked once it is in front.
        let elsewhere = (s.board.as_ref()).is_some_and(|b| !sites.iter().any(|x| x.host() == b.0));
        if let Some(Read::Failed(why)) = boards {
            return (self.page_frame_own())
                .child(controls::empty_state(
                    "alert",
                    "Boards not read".into(),
                    why,
                    None,
                ))
                .into_any_element();
        }
        if elsewhere {
            return (self.page_frame_own())
                .child(crate::board::skeleton_in_page("tasks-board-elsewhere", 6))
                .into_any_element();
        }
        if matches!(&boards, Some(Read::Done(b)) if b.is_empty()) {
            let host = sites
                .first()
                .map(|x| x.host().to_owned())
                .unwrap_or_default();
            return (self.page_frame_own())
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_size(px(14.))
                                .text_color(theme::text())
                                .child(format!("No boards on {host}")),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(theme::muted())
                                .child("The token reaches 0 projects"),
                        )
                        .child(
                            div()
                                .id("tasks-accounts")
                                .text_size(px(12.))
                                .text_color(theme::accent())
                                .cursor_pointer()
                                .child("Settings → Accounts")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.frame.open_settings(
                                        artoo_plugin_sdk::hosts::connect::SETTINGS,
                                        cx,
                                    )
                                })),
                        ),
                )
                .into_any_element();
        }
        if self.drafts {
            return self.drafts_page(cx);
        }
        self.board(cx)
    }

    /// Fold `key` set moving: drawn on its way, then still — the shell's side panel too, which this page's own
    /// redraw does not reach.
    pub(crate) fn moved(&mut self, key: &str, cx: &mut Context<Self>) {
        self.motion.mark(key);
        let frame = self.frame.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(motion::BASE + Duration::from_millis(120))
                .await;
            this.update(cx, |_, cx| {
                cx.notify();
                frame.redraw(cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
        let frame = self.frame.clone();
        cx.defer(move |cx| frame.redraw(cx));
    }

    /// `content` folding open or shut under `key`, moving while `key`'s mark is fresh.
    pub(crate) fn folding(
        &self,
        key: &str,
        open: bool,
        content: impl IntoElement + 'static,
    ) -> Option<AnyElement> {
        motion::folding(
            &self.folds,
            &self.heights,
            self.motion.recent(key),
            key,
            open,
            content,
        )
    }

    pub(crate) fn page_frame_own(&self) -> gpui::Div {
        artoo_plugin_sdk::kit::island()
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .bg(theme::editor())
    }
}

/// A branch's name for issue `key`: `PAY-412-refund-rounds-down-on-partial-captures`.
pub fn branch_name(key: &str, summary: &str) -> String {
    let words: Vec<String> = (summary.split(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mut out = key.to_owned();
    for w in words {
        if out.len() + w.len() + 1 > 60 {
            break;
        }
        out.push('-');
        out.push_str(&w);
    }
    out
}

impl Render for Page {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The project is read here, where the tab may be read; the side panel and the bar, drawn by the tab, read it
        // as kept, so the tab is drawn again when it changes.
        let project = self.project_now(cx);
        if project != self.here {
            self.here = project.clone();
            let frame = self.frame.clone();
            cx.defer(move |cx| frame.redraw(cx));
        }
        // The store's board follows the project of the window in front; one behind waits for its turn.
        if window.is_window_active() && self.store.read(cx).here.as_deref() != Some(&project) {
            let store = self.store.clone();
            cx.defer(move |cx| store.update(cx, |s, cx| s.show_project(&project, cx)));
        }
        let page = match self.issue_key() {
            Some(key) => self.issue_page(key, cx),
            None => self.own_page(cx),
        };
        let workflow = self
            .workflow
            .clone()
            .map(|key| self.workflow_card(&key, cx));
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .child(page)
            .children(workflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_is_named_by_the_key_and_the_summary() {
        assert_eq!(
            branch_name("PAY-412", "Refund rounds down on partial captures"),
            "PAY-412-refund-rounds-down-on-partial-captures"
        );
        assert!(branch_name("PAY-1", &"word ".repeat(40)).len() <= 60);
    }
}
