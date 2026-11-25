//! The page bar of the Tasks space (`Pages::bar`): a board's name, Mine · All with their counts and the people
//! beside them, the filter, List · Board, `⋯`, Open in its tracker and New task; the drafts' count and New task; an
//! issue's board and key, `⋯` and Open in its tracker.

use crate::board::in_play;
use crate::look;
use crate::model;
use crate::page::Page;
use artoo_plugin_sdk::core::route::Route;
use artoo_plugin_sdk::gpui::{self, App, Entity, IntoElement, Stateful, div, prelude::*};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, icons, px, theme};
use artoo_plugin_sdk::shell::bar::BarAction;
use artoo_plugin_sdk::shell::{BarParts, MenuItem, PagePath, PathUp};

/// Open in its tracker (`Open in Jira`, `Open in Trello`): `url` in the browser, ⌥⌘J.
fn in_jira(url: String) -> BarAction {
    let fold = url.clone();
    let words = format!("Open in {}", crate::trackers::tracker_of(&url));
    BarAction::new(
        "tasks-jira",
        controls::bar_button("tasks-jira", "", "", false)
            .w(px(28.))
            .px_0()
            .justify_center()
            .child(
                icons::icon("external")
                    .size(px(14.))
                    .text_color(theme::muted()),
            )
            .tip(format!("{words}  ⌥⌘J"))
            .on_click(move |_, _, cx| cx.open_url(&url)),
    )
    .folds(MenuItem::new(words, move |_, cx| cx.open_url(&fold)).key("⌥⌘J"))
    .fold_order(2)
}

/// New task, ⌥⌘N.
fn new_task(page: &Entity<Page>) -> BarAction {
    let (p1, p2) = (page.clone(), page.clone());
    BarAction::new(
        "tasks-new",
        controls::bar_button("tasks-new", "New task", "", true)
            .tip("New task  ⌥⌘N")
            .on_click(move |_, _, cx| p1.update(cx, |p, cx| p.open_sheet(None, None, cx))),
    )
    .folds(
        MenuItem::new("New task", move |_, cx| {
            p2.update(cx, |p, cx| p.open_sheet(None, None, cx))
        })
        .key("⌥⌘N"),
    )
    .fold_order(9)
}

pub fn bar(page: &Entity<Page>, route: Option<&Route>, cx: &mut App) -> Option<BarParts> {
    let p = page.read(cx);
    let store = p.store.clone();
    let s = store.read(cx);
    if s.sites_in(p.project()).is_empty() {
        return Some(BarParts {
            path: Some(path(Vec::new(), "Tasks", None)),
            ..Default::default()
        });
    }
    let board = s.picked().cloned();
    // The board's name once its boards are read; before, the path is Tasks alone, never "Tasks › Tasks".
    let board_name = board.as_ref().map(|b| b.name.clone());
    if let Some(route) = route {
        let key = crate::agents::key_of(route)?;
        let url = match s.issue(&key) {
            Some(i) => i.url.clone(),
            None => s
                .site_for(&key)
                .map(|site| format!("{}/browse/{key}", site.url))
                .unwrap_or_default(),
        };
        let title = s.issue(&key).map(|i| i.row.summary.clone());
        let mut parts = BarParts {
            path: Some(PagePath {
                title,
                ..path(
                    std::iter::once("Tasks".to_owned())
                        .chain(board_name)
                        .collect(),
                    &key,
                    None,
                )
            }),
            ..Default::default()
        };
        parts.more = Page::more_items(page, cx);
        if !url.is_empty() {
            parts.actions.push(in_jira(url));
        }
        return Some(parts);
    }
    if p.drafts {
        let n = s.kept.drafts.len();
        let mut parts = BarParts {
            path: Some(path(vec!["Tasks".into()], "Drafts", Some(n.to_string()))),
            ..Default::default()
        };
        parts.actions.push(new_task(page));
        return Some(parts);
    }
    let board = board?;
    let me = s.me.get(&board.site).cloned();
    // A board read by slice: the slice's counts as the tracker counts them, else its issues as read.
    let (all_counted, mine_counted) = (s.board_slices())
        .and_then(|sl| Some(sl.counts(sl.view.as_ref()?, s.board_info()?)))
        .unwrap_or_default();
    let (mine_n, all_n) = match s.board_rows() {
        Some(rows) => {
            let play = in_play(rows);
            let top: Vec<_> = play.into_iter().filter(|r| r.parent.is_none()).collect();
            let mine = top
                .iter()
                .filter(|r| {
                    me.as_ref()
                        .is_some_and(|me| r.assignee.as_ref().is_some_and(|a| a.id == me.id))
                })
                .count();
            (mine_counted.or(Some(mine)), all_counted.or(Some(top.len())))
        }
        None => (mine_counted, all_counted),
    };
    let mine = s.mine;
    let backlog =
        (s.board_slices()).is_some_and(|sl| sl.view == Some(crate::slices::SliceId::Backlog));
    let columns = s.kept.view(&board.key()).board;
    let url = s
        .site_of(&board.site)
        .map(|site| model::board_url(&site, &board.id));
    let count = |n: Option<usize>| n.map(|n| format!(" {n}")).unwrap_or_default();
    let (m1, m2) = (page.clone(), page.clone());
    let tabs = controls::group()
        .child(
            controls::chip("tasks-mine", format!("Mine{}", count(mine_n)), mine)
                .tip("Your issues  ⌃⇥")
                .on_click(move |_, _, cx| m1.update(cx, |p, cx| p.set_mine(true, cx))),
        )
        .child(
            controls::chip("tasks-all", format!("All{}", count(all_n)), !mine)
                .tip("Everyone’s issues  ⌃⇥")
                .on_click(move |_, _, cx| m2.update(cx, |p, cx| p.set_mine(false, cx))),
        );
    let faces = page.update(cx, |p, cx| p.faces(cx));
    let view = div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(tabs)
        .children(faces)
        .into_any_element();
    let (v1, v2, v3) = (page.clone(), page.clone(), page.clone());
    let layout = controls::group()
        .h(px(28.))
        .child(
            layout_chip(
                "tasks-list",
                icons::icon("list").size(px(14.)).text_color(theme::text()),
                !columns,
            )
            .tip("List  V")
            .on_click(move |_, _, cx| v1.update(cx, |p, cx| p.set_columns(false, cx))),
        )
        .child(
            layout_chip("tasks-board", look::board_glyph(14.), columns)
                .tip("Board  V")
                .on_click(move |_, _, cx| v2.update(cx, |p, cx| p.set_columns(true, cx))),
        );
    let mut parts = BarParts {
        path: Some(path(vec!["Tasks".into()], &board.name, None)),
        view: Some(view),
        // The Backlog's words are asked of Jira, which finds by more than a row shows.
        filter: Some(if backlog {
            "Filter in Jira"
        } else {
            "Filter issues"
        }),
        ..Default::default()
    };
    parts.actions.push(
        BarAction::new("tasks-layout", layout)
            .folds(
                MenuItem::new(if columns { "List" } else { "Board" }, move |_, cx| {
                    v3.update(cx, |p, cx| p.toggle_view(cx))
                })
                .key("V"),
            )
            .fold_order(1),
    );
    parts.more = Page::more_items(page, cx);
    if let Some(url) = url {
        parts.actions.push(in_jira(url));
    }
    parts.actions.push(new_task(page));
    Some(parts)
}

fn path(up: Vec<String>, name: &str, count: Option<String>) -> PagePath {
    PagePath {
        up: up.into_iter().map(|u| PathUp::new(u, None)).collect(),
        name: name.to_owned(),
        mono: false,
        hover: None,
        count,
        title: None,
    }
}

/// One of the layout pair: a glyph alone on a 24 px square, its word the tooltip.
fn layout_chip(id: &'static str, glyph: impl IntoElement, on: bool) -> Stateful<gpui::Div> {
    controls::chip(id, "", on)
        .size(px(24.))
        .p_0()
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
}
