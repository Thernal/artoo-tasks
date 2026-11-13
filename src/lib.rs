//! Task management for Artoo, its first stage: a tracker's boards and issues as the Tasks space, read from Jira,
//! Trello, or a tracker another plugin adds through the contract (`api/`, `trackers`); a
//! note for your agent on a paragraph of an issue or on a board as it is filtered; tools agents call for an issue or
//! a search; issues moved, assigned, edited, commented on, linked and made, a write at a time, on your act.
//!
//! The trackers are named in `<data>/plugins/artoo-tasks/trackers.txt` (`https://jira.example.com`, an email after
//! it for Jira Cloud), a line each, every one read; each one's token in `~/.config/tokens/<host>`.

mod accounts;
mod agents;
mod bar;
mod board;
mod issue;
mod jira;
mod keep;
mod look;
mod model;
mod news;
mod page;
mod pop;
mod search;
mod sheet;
mod side;
mod store;
mod trackers;
mod trello;

/// The model's text: Jira's wiki markup, and markdown.
use artoo_tasks_api::wiki;

use artoo_plugin_sdk::core::route::Route;
use artoo_plugin_sdk::gpui::{self, App, AppContext, ClipboardItem, Context, Entity, prelude::*};
use artoo_plugin_sdk::kit::{icons, px, theme};
use artoo_plugin_sdk::shell::spaces::{self, Space, SpaceInfo};
use artoo_plugin_sdk::shell::{
    Action, Input, ItemRow, Link, ListKeys, Menu, Overlay, PagePath, Pages, Parts, PathUp, Screen,
    Typed, add_action, add_parts, when,
};
use artoo_plugin_sdk::{self as sdk, Meta};
use page::Page;
use std::rc::Rc;

gpui::actions!(
    artoo_tasks,
    [
        GoToTasks,
        NoteOnBoard,
        NewTask,
        MoveIssue,
        AssignIssue,
        AssignToMe,
        ChangeEpic,
        EditIssue,
        LinkIssue,
        AddSubtask,
        ToggleLayout,
        GroupBy,
        OpenInJira,
        ReadAgain,
        NoteHere,
        Reply,
        CopyKey,
        CopyBranch,
        OpenNews,
        SearchTasks,
    ]
);

/// The space's own keys: bare letters while nothing is typed, in the Tasks space alone.
const HERE: &str = "Workspace && !Typing && space == tasks";
/// ⌘ and ⌥ keys in the Tasks space, typing or not.
const SPACE_KEYS: &str = "Workspace && space == tasks";

sdk::plugin!(
    Meta {
        id: env!("CARGO_PKG_NAME"),
        name: "Tasks",
        version: env!("CARGO_PKG_VERSION"),
        repository: env!("CARGO_PKG_REPOSITORY"),
        provides: &[],
        requires: &[],
    },
    install
);

/// The space's commands: `(id, act, words, key, when)`, in ⌘K's order.
fn commands() -> Vec<Action> {
    fn a(
        id: &'static str,
        act: Box<dyn gpui::Action>,
        words: &'static str,
        key: &'static str,
        when: &'static str,
    ) -> Action {
        Action {
            act,
            ..Action::new(id, GoToTasks)
        }
        .words(words)
        .key(key, when)
    }
    vec![
        a(
            "tasks.new",
            Box::new(NewTask),
            "New task…",
            "alt-cmd-n",
            SPACE_KEYS,
        )
        .order(37),
        a(
            "tasks.move",
            Box::new(MoveIssue),
            "Move to…",
            "alt-s",
            SPACE_KEYS,
        )
        .here("Move to…")
        .order(38),
        a("tasks.assign", Box::new(AssignIssue), "Assign…", "a", HERE)
            .here("Assign…")
            .order(39),
        a(
            "tasks.assign-me",
            Box::new(AssignToMe),
            "Assign to me",
            "i",
            HERE,
        )
        .here("Assign to me")
        .order(40),
        a(
            "tasks.epic",
            Box::new(ChangeEpic),
            "Change the epic…",
            "shift-e",
            HERE,
        )
        .here("Change the epic…")
        .order(41),
        a(
            "tasks.edit",
            Box::new(EditIssue),
            "Edit the description",
            "e",
            HERE,
        )
        .here("Edit the description")
        .order(42),
        a(
            "tasks.link",
            Box::new(LinkIssue),
            "Link an issue…",
            "l",
            HERE,
        )
        .here("Link an issue…")
        .order(43),
        a(
            "tasks.subtask",
            Box::new(AddSubtask),
            "Add a sub-task",
            "n",
            HERE,
        )
        .here("Add a sub-task")
        .order(44),
        a(
            "tasks.layout",
            Box::new(ToggleLayout),
            "List or Board",
            "v",
            HERE,
        )
        .here("List or Board")
        .order(45),
        a("tasks.group", Box::new(GroupBy), "Group by…", "g", HERE)
            .here("Group by…")
            .order(46),
        a(
            "tasks.jira",
            Box::new(OpenInJira),
            "Open in Jira",
            "alt-cmd-j",
            SPACE_KEYS,
        )
        .here("Open in Jira")
        .order(47),
        a(
            "tasks.read",
            Box::new(ReadAgain),
            "Read the tracker again",
            "cmd-r",
            SPACE_KEYS,
        )
        .order(48),
        a(
            "tasks.note",
            Box::new(NoteHere),
            "Note for an agent…",
            "c",
            HERE,
        )
        .here("Note for an agent…")
        .order(49),
        a(
            "tasks.reply",
            Box::new(Reply),
            "Reply to the last comment",
            "r",
            HERE,
        )
        .here("Reply")
        .order(50),
        a(
            "tasks.copy-key",
            Box::new(CopyKey),
            "Copy the issue’s key",
            "shift-cmd-c",
            SPACE_KEYS,
        )
        .order(51),
        a(
            "tasks.copy-branch",
            Box::new(CopyBranch),
            "Copy a branch name for the issue",
            "shift-cmd-b",
            SPACE_KEYS,
        )
        .order(52),
    ]
}

/// Whether `view`'s input is open, as the shell asks it: also while `view` draws (`Frame::typing`), when `view` cannot
/// be read. The answer is noted each time `view` changes.
pub(crate) fn kept_open<T: 'static>(
    view: &Entity<T>,
    open: fn(&T) -> bool,
    cx: &mut App,
) -> Rc<dyn Fn(&App) -> bool> {
    let kept = Rc::new(std::cell::Cell::new(open(view.read(cx))));
    let noted = kept.clone();
    cx.observe(view, move |v, cx| noted.set(open(v.read(cx))))
        .detach();
    Rc::new(move |_| kept.get())
}

fn install(cx: &mut App) {
    spaces::add(
        cx,
        SpaceInfo {
            space: Space(page::SPACE),
            label: "Tasks",
            icon: "list-checks",
            order: 50,
            go: "tasks.space",
        },
    );
    add_action(
        cx,
        Action::new("tasks.space", GoToTasks)
            .words("Tasks")
            .order(35)
            .key("ctrl-cmd-5", when::TAB)
            .menu(Menu::Go, "Tasks"),
    );
    add_action(
        cx,
        Action::new("tasks.board-note", NoteOnBoard)
            .words("Note for an agent on the board")
            .order(36),
    );
    for action in commands() {
        add_action(cx, action);
    }
    add_action(
        cx,
        Action::new("tasks.search", SearchTasks)
            .words("Search issues")
            .heading(artoo_plugin_sdk::shell::Heading::Find),
    );
    agents::add_tools(cx);
    agents::add_places(cx);
    news::offer(cx);
    add_parts(cx, |frame, _window, cx| {
        let store = store::get(cx);
        let view = cx.new(|cx| Page::new(store.clone(), frame.clone(), cx));
        let goes = frame.clone();
        let (path_store, row_store, link_store) = (store.clone(), store.clone(), store.clone());
        let (side_view, shows, bar_view, here_view) =
            (view.clone(), view.clone(), view.clone(), view.clone());
        let (note_frame, note_view) = (frame.clone(), view.clone());
        let (step_view, open_view) = (view.clone(), view.clone());
        let (accounts_rows, accounts_input) = accounts::parts(store.clone(), frame.clone(), cx);
        news::install_in(&store, &frame, cx);
        let (news_store, news_frame) = (store.clone(), frame.clone());
        let searching = frame.clone();
        let (sheet_key, sheet_close) = (view.clone(), view.clone());
        let overlay = Overlay {
            view: artoo_plugin_sdk::shell::settings::row_view(view.clone(), Page::sheet_card, cx),
            open: view.read(cx).sheet_shown.clone(),
            key: Rc::new(move |event, _, cx| sheet_key.update(cx, |p, cx| p.sheet_key(event, cx))),
            close: Rc::new(move |cx| sheet_close.update(cx, |p, cx| p.close_sheet(cx))),
            poll: None,
        };
        let on = |act: fn(&mut Page, &mut Context<Page>)| -> artoo_plugin_sdk::shell::Run {
            let page = view.clone();
            Rc::new(move |_, cx| page.update(cx, act))
        };
        let mut actions: Vec<(Box<dyn gpui::Action>, artoo_plugin_sdk::shell::Run)> = vec![
            (
                Box::new(GoToTasks),
                Rc::new(move |_, cx| goes.go_space(page::SPACE, cx)),
            ),
            (
                Box::new(SearchTasks),
                Rc::new(move |_, cx| searching.search(search::SCOPE, cx)),
            ),
            (
                Box::new(OpenNews),
                Rc::new(move |_, cx| {
                    let last = news_store.read(cx).news_last.clone();
                    if let Some((host, key)) = last {
                        news_frame.open(agents::issue_route(&key, Some(&host)), cx);
                    }
                }),
            ),
            (
                Box::new(NoteOnBoard),
                Rc::new(move |_, cx| {
                    note_frame.open_space(page::SPACE, cx);
                    note_view.update(cx, |p, cx| p.open_board_note(cx));
                }),
            ),
        ];
        actions.extend([
            (
                Box::new(NewTask) as Box<dyn gpui::Action>,
                on(|p, cx| p.open_sheet(None, None, cx)),
            ),
            (Box::new(MoveIssue), on(Page::key_move)),
            (Box::new(AssignIssue), on(Page::key_assign)),
            (Box::new(AssignToMe), on(Page::key_assign_me)),
            (Box::new(ChangeEpic), on(Page::key_epic)),
            (Box::new(EditIssue), on(Page::key_edit)),
            (Box::new(LinkIssue), on(Page::key_link)),
            (Box::new(AddSubtask), on(Page::key_subtask)),
            (Box::new(ToggleLayout), on(Page::toggle_view)),
            (Box::new(GroupBy), on(|p, cx| p.group_menu(None, cx))),
            (Box::new(OpenInJira), on(Page::open_in_jira)),
            (Box::new(ReadAgain), on(Page::read_again)),
            (Box::new(NoteHere), on(Page::key_note)),
            (Box::new(Reply), on(Page::key_reply)),
            (Box::new(CopyKey), on(Page::copy_key)),
            (Box::new(CopyBranch), on(Page::copy_branch)),
        ]);
        let input = |id: &'static str,
                     open: fn(&Page) -> bool,
                     key: fn(&mut Page, &gpui::KeyDownEvent, &mut Context<Page>) -> Typed,
                     caret: bool,
                     cx: &mut App| {
            let (k, l) = (view.clone(), view.clone());
            Input {
                id,
                open: kept_open(&view, open, cx),
                key: Rc::new(move |event, _, cx| k.update(cx, |p, cx| key(p, event, cx))),
                left: Rc::new(move |cx| l.update(cx, |_, cx| cx.notify())),
                selected: Rc::new(|_| None),
                caret: Rc::new(move |_| caret),
            }
        };
        let inputs = vec![
            input(page::NOTE, Page::note_open, Page::note_key, true, cx),
            input(
                page::INPUT_POP,
                |p| p.pop.is_some() || p.issue.link.is_some(),
                Page::pop_or_link_key,
                true,
                cx,
            ),
            input(
                page::INPUT_COMMENT,
                |p| p.issue.comment.is_some(),
                Page::comment_key,
                true,
                cx,
            ),
            input(
                page::INPUT_DESC,
                |p| p.issue.desc.is_some(),
                Page::desc_key,
                true,
                cx,
            ),
            input(page::INPUT_SUB, |_| true, Page::sub_key, false, cx),
            input(
                page::INPUT_TITLE,
                |p| p.issue.title.is_some(),
                Page::title_key,
                false,
                cx,
            ),
            accounts_input,
        ];
        Parts {
            actions,
            inputs,
            overlay: Some(overlay),
            settings: vec![accounts_rows],
            searches: vec![search::scope(store.clone())],
            pages: Some(Pages {
                space: page::SPACE,
                kinds: &[agents::ISSUE],
                parent: |_| None,
                item: |route| Some(route.clone()),
                path: Rc::new(move |route: &Route, _, cx: &App| {
                    let key = agents::key_of(route)?;
                    let s = path_store.read(cx);
                    let board = s
                        .picked()
                        .map(|b| b.name.clone())
                        .unwrap_or_else(|| "Tasks".into());
                    Some(PagePath {
                        up: vec![PathUp::new("Tasks", None), PathUp::new(board, None)],
                        name: key.clone(),
                        mono: false,
                        hover: None,
                        count: None,
                        title: s.issue(&key).map(|i| i.row.summary.clone()),
                    })
                }),
                screen: |route| {
                    if route.is_none() {
                        Screen::List
                    } else {
                        Screen::Own
                    }
                },
                row: Rc::new(move |item: &Route, _: &Route, cx: &App| {
                    let key = agents::key_of(item)?;
                    let s = row_store.read(cx);
                    let summary = s
                        .issue(&key)
                        .map(|i| i.row.summary.clone())
                        .unwrap_or_default();
                    Some(ItemRow {
                        icon: icons::icon("list-checks")
                            .size(px(14.))
                            .text_color(theme::muted())
                            .into_any_element(),
                        label: format!("{key} {summary}"),
                        mono: false,
                        tip: format!("{key} {summary}"),
                        news: false,
                    })
                }),
                called: |route| {
                    if route.is_some() {
                        "this issue"
                    } else {
                        "this board"
                    }
                },
                link: Rc::new(move |route: Option<&Route>, _, _, cx: &App| {
                    let s = link_store.read(cx);
                    let Some(route) = route else {
                        let board = s.picked().ok_or("No board is open")?;
                        let site = s.site_of(&board.site).ok_or("No tracker is connected")?;
                        return Ok(Link {
                            url: model::board_url(&site, &board.id),
                            what: board.name.clone(),
                        });
                    };
                    let key = agents::key_of(route).ok_or("This page has no link")?;
                    let url = match s.issue(&key) {
                        Some(issue) => issue.url.clone(),
                        None => {
                            let site = (agents::site_of(route).and_then(|host| s.site_of(&host)))
                                .or_else(|| s.sites.first().cloned())
                                .ok_or("No tracker is connected")?;
                            format!("{}/browse/{key}", site.url)
                        }
                    };
                    Ok(Link { url, what: key })
                }),
                view: view.clone().into(),
                side: Some(Rc::new(
                    move |route: Option<&Route>, width: f32, cx: &mut App| {
                        // The boards beside the board; an issue's page has the window's width.
                        route.is_none().then(|| side::side(&side_view, width, cx))
                    },
                )),
                show: Rc::new(move |route: Option<&Route>, fresh: bool, cx: &mut App| {
                    shows.update(cx, |p, cx| p.show(route, fresh, cx))
                }),
                keys: Some(ListKeys {
                    step: Rc::new(move |down, cx| {
                        step_view.update(cx, |p, cx| p.list_step(down, cx))
                    }),
                    open: Rc::new(move |cx| open_view.update(cx, |p, cx| p.list_open(cx))),
                }),
                here: Some(Rc::new(move |route: Option<&Route>, cx: &App| {
                    here_of(&here_view, route, cx)
                })),
                bar: Some(Rc::new(move |route: Option<&Route>, cx: &mut App| {
                    bar::bar(&bar_view, route, cx)
                })),
            }),
            ..Default::default()
        }
    });
}

/// ⌘K's Here on the space's pages: an issue's acts on its page, a board's on the board.
fn here_of(page: &Entity<Page>, route: Option<&Route>, cx: &App) -> Vec<&'static str> {
    let p = page.read(cx);
    if p.store.read(cx).sites.is_empty() {
        return vec!["tasks.new"];
    }
    match route {
        Some(_) => vec![
            "tasks.move",
            "tasks.assign",
            "tasks.assign-me",
            "tasks.edit",
            "tasks.subtask",
            "tasks.link",
            "tasks.note",
            "tasks.jira",
            "tasks.copy-branch",
        ],
        None if p.drafts => vec!["tasks.new"],
        None => vec![
            "tasks.new",
            "tasks.move",
            "tasks.assign",
            "tasks.layout",
            "tasks.group",
            "tasks.board-note",
            "tasks.jira",
        ],
    }
}

impl Page {
    /// ↑ ↓ on the space's own page: the board's rows, or the drafts.
    fn list_step(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.drafts {
            self.step_draft(down, cx)
        } else {
            self.step(down, cx)
        }
    }

    /// ↵: the issue picked opened, or the draft as New task.
    fn list_open(&mut self, cx: &mut Context<Self>) {
        self.open_picked(cx)
    }

    fn pop_or_link_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        if self.pop.is_none() && self.issue.link.is_some() {
            return self.link_key(event, cx);
        }
        self.pop_key(event, cx)
    }

    /// E: the description edited on an issue's page; on the board, the issue picked opened to edit it.
    fn key_edit(&mut self, cx: &mut Context<Self>) {
        if self.issue_key().is_some() {
            self.edit_description(cx);
        }
    }

    /// L: Link on an issue's page.
    fn key_link(&mut self, cx: &mut Context<Self>) {
        if self.issue_key().is_some() {
            self.open_link(cx);
        }
    }

    /// N: the sub-tasks' empty row on an issue's page; New task on the board.
    fn key_subtask(&mut self, cx: &mut Context<Self>) {
        if self.issue_key().is_some() {
            self.start_sub(cx);
        } else {
            self.open_sheet(None, None, cx);
        }
    }

    /// C: a note for the agent on the paragraph under the pointer (the first), or on the board.
    fn key_note(&mut self, cx: &mut Context<Self>) {
        if self.issue_key().is_some() {
            self.note_on_first_paragraph(cx);
        } else {
            self.open_board_note(cx);
        }
    }

    /// ⌥⌘J: the issue on screen or picked, or the board, in Jira.
    fn open_in_jira(&mut self, cx: &mut Context<Self>) {
        let s = self.store.read(cx);
        let url = match self.acted_on() {
            Some(key) => match s.issue(&key) {
                Some(i) => Some(i.url.clone()),
                None => s
                    .site_for(&key)
                    .map(|site| format!("{}/browse/{key}", site.url)),
            },
            None => s
                .picked()
                .and_then(|b| Some(model::board_url(&s.site_of(&b.site)?, &b.id))),
        };
        if let Some(url) = url {
            cx.open_url(&url);
        }
    }

    /// ⇧⌘C: the issue's key.
    fn copy_key(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.acted_on() {
            cx.write_to_clipboard(ClipboardItem::new_string(key));
        }
    }

    /// ⇧⌘B: a branch's name for the issue.
    fn copy_branch(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.acted_on() else {
            return;
        };
        let summary = self
            .store
            .read(cx)
            .row(&key)
            .map(|r| r.summary.clone())
            .unwrap_or_default();
        cx.write_to_clipboard(ClipboardItem::new_string(page::branch_name(&key, &summary)));
    }
}
