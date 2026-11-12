//! Task management for Artoo, its first stage: a tracker's boards and issues as the Tasks space, read from Jira; a
//! note for your agent on a paragraph of an issue; tools agents call for an issue or a search. Reads only.
//!
//! The trackers are named in `<data>/plugins/artoo-tasks/trackers.txt` (`https://jira.example.com`, an email after
//! it for Jira Cloud), each one's token in `~/.config/tokens/<host>`.

mod agents;
mod jira;
mod model;
mod page;
mod store;
mod wiki;

use artoo_plugin_sdk::core::route::Route;
use artoo_plugin_sdk::gpui::{self, App, AppContext, prelude::*};
use artoo_plugin_sdk::kit::{icons, px, theme};
use artoo_plugin_sdk::shell::spaces::{self, Space, SpaceInfo};
use artoo_plugin_sdk::shell::{
    Action, Input, ItemRow, Link, Menu, PagePath, Pages, Parts, PathUp, Screen, add_action, add_parts, when,
};
use artoo_plugin_sdk::{self as sdk, Meta};
use std::rc::Rc;

gpui::actions!(artoo_tasks, [GoToTasks]);

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
    agents::add_tools(cx);
    agents::add_places(cx);
    add_parts(cx, |frame, _window, cx| {
        let store = store::get(cx);
        let view = cx.new(|cx| page::Page::new(store.clone(), frame.clone(), cx));
        let goes = frame.clone();
        let (side_store, side_frame) = (store.clone(), frame.clone());
        let shows = view.clone();
        let (row_store, path_store, link_store) = (store.clone(), store.clone(), store.clone());
        let (open_view, key_view, left_view) = (view.clone(), view.clone(), view.clone());
        Parts {
            actions: vec![(
                Box::new(GoToTasks),
                Rc::new(move |_, cx| goes.go_space(page::SPACE, cx)),
            )],
            inputs: vec![Input {
                id: page::NOTE,
                open: Rc::new(move |cx| open_view.read(cx).note_open()),
                key: Rc::new(move |event, _, cx| key_view.update(cx, |p, cx| p.note_key(event, cx))),
                left: Rc::new(move |cx| left_view.update(cx, |_, cx| cx.notify())),
                selected: Rc::new(|_| None),
                caret: Rc::new(|_| true),
            }],
            pages: Some(Pages {
                space: page::SPACE,
                kinds: &[agents::ISSUE],
                parent: |_| None,
                item: |route| Some(route.clone()),
                path: Rc::new(move |route: &Route, _, cx: &App| {
                    let key = agents::key_of(route)?;
                    let s = path_store.read(cx);
                    let board = s.picked().map(|b| b.name.clone()).unwrap_or_else(|| "Tasks".into());
                    Some(PagePath {
                        up: vec![PathUp::new(board, None)],
                        name: key.clone(),
                        mono: false,
                        hover: None,
                        count: None,
                        title: s.issue(&key).map(|i| i.row.summary.clone()),
                    })
                }),
                screen: |_| Screen::Own,
                row: Rc::new(move |item: &Route, _: &Route, cx: &App| {
                    let key = agents::key_of(item)?;
                    let summary = row_store.read(cx).issue(&key).map(|i| i.row.summary.clone()).unwrap_or_default();
                    Some(ItemRow {
                        icon: icons::icon("list-checks").size(px(14.)).text_color(theme::muted()).into_any_element(),
                        label: format!("{key} {summary}"),
                        mono: false,
                        tip: format!("{key} {summary}"),
                        news: false,
                    })
                }),
                called: |route| if route.is_some() { "this issue" } else { "this board" },
                link: Rc::new(move |route: Option<&Route>, _, _, cx: &App| {
                    let key = route.and_then(agents::key_of).ok_or("A board has no link of its own yet")?;
                    let s = link_store.read(cx);
                    let site = s.site().ok_or("No tracker is connected")?;
                    Ok(Link {
                        url: format!("{}/browse/{key}", site.url),
                        what: key,
                    })
                }),
                view: view.clone().into(),
                side: Some(Rc::new(move |_route: Option<&Route>, _width: f32, cx: &mut App| {
                    Some(page::side(&side_store, &side_frame, cx))
                })),
                show: Rc::new(move |route: Option<&Route>, fresh: bool, cx: &mut App| {
                    shows.update(cx, |p, cx| p.show(route, fresh, cx))
                }),
                keys: None,
            }),
            ..Default::default()
        }
    });
}
