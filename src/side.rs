//! The space's side panel: the boards of every account the project reads, under its host, the one picked marked, a
//! site not reached saying so with when its boards were read; `+` connecting another, a host's menu leaving it;
//! Drafts at the foot, with how many.

use crate::page::{Page, SPACE};
use crate::store::Read;
use crate::trackers::SiteExt;
use artoo_plugin_sdk::gpui::{
    self, AnyElement, App, Entity, FontWeight, SharedString, div, prelude::*,
};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, icons, px, theme};
use artoo_plugin_sdk::shell::MenuItem;

/// The fold of the side panel's Boards.
const BOARDS: &str = "side-boards";

/// `None` while the project reads no tracker: the page alone, as wide as the window, says how to connect one.
pub fn side(page: &Entity<Page>, width: f32, cx: &mut App) -> Option<AnyElement> {
    let p = page.read(cx);
    let project = p.project().to_owned();
    let (store, frame, drafts_on) = (
        p.store.clone(),
        p.frame.clone(),
        p.drafts && p.issue_key().is_none(),
    );
    let s = store.read(cx);
    let sites = s.sites_in(&project);
    let unread = s.unread_in(&project);
    let mut list = div()
        .w(px(width))
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(px(8.))
        .py(px(8.));
    // No tracker yet: no panel at all, not an empty one keeping its room.
    if sites.is_empty() {
        return None;
    }
    let (connects, folds) = (page.clone(), page.clone());
    let folded = p.boards_folded;
    // Boards folds and opens, moving, as the app's sections do; `+` beside it connects another account.
    list = list.child(
        div()
            .id("tasks-boards-head")
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .pb(px(6.))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::muted())
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                folds.update(cx, |p, cx| {
                    p.boards_folded = !p.boards_folded;
                    p.moved(BOARDS, cx);
                })
            })
            .child(crate::look::chevron("tasks-boards-turn", !folded, 11.))
            .child(div().flex_1().child("BOARDS"))
            .child(
                div()
                    .id("tasks-connect-more")
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::hover()))
                    .child(icons::icon("plus").size(px(12.)).text_color(theme::muted()))
                    .tip("Connect a tracker")
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        connects.update(cx, |p, cx| p.connect(cx))
                    }),
            ),
    );
    // What folds under Boards: each account's host and its boards.
    let mut body = div().flex().flex_col().gap(px(2.));
    match s.boards_in(&project) {
        Some(Read::Done(boards)) => {
            for site in &sites {
                let host = site.host().to_owned();
                let keys = site.caps().project_keys;
                let bad = unread
                    .iter()
                    .find(|(h, _)| *h == host)
                    .map(|(_, why)| why.clone());
                // A board of it read before and not again: when it was read, as the board's line says.
                let since = (s.stale.keys())
                    .filter(|b| b.0 == host)
                    .filter_map(|b| s.read_at.get(b).copied())
                    .min()
                    .map(artoo_plugin_sdk::kit::format::clock);
                let unreached = bad.is_some() || since.is_some();
                let mine: Vec<_> = boards.iter().filter(|b| b.site == host).collect();
                let (leave_store, leave_project, leave_host, leave_frame) =
                    (store.clone(), project.clone(), host.clone(), frame.clone());
                body = body.child(
                    div()
                        .id(SharedString::from(format!("site-{host}")))
                        .on_mouse_down(gpui::MouseButton::Right, move |e, _, cx| {
                            let (store, project, host) = (
                                leave_store.clone(),
                                leave_project.clone(),
                                leave_host.clone(),
                            );
                            let words = format!("Stop reading {host} in this project");
                            let item = MenuItem::new(words, move |_, cx| {
                                store
                                    .update(cx, |s, cx| s.link(&project, &host, false, cx))
                                    .ok();
                            });
                            leave_frame.menu(e.position, vec![vec![item]], cx);
                        })
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(8.))
                        .pt(px(6.))
                        .pb(px(2.))
                        .text_size(px(11.))
                        .text_color(if unreached {
                            theme::warning()
                        } else {
                            theme::faint()
                        })
                        .children(
                            unreached
                                .then(|| div().size(px(6.)).rounded_full().bg(theme::warning())),
                        )
                        .child(div().flex_1().truncate().child(host.clone()))
                        .children(since.clone().map(|at| div().flex_none().child(at)))
                        .tip(match (&since, &bad) {
                            (Some(at), _) => format!("Can’t reach {host} · boards from {at}"),
                            (None, Some(why)) => format!("Can’t reach {host} · {why}"),
                            (None, None) => {
                                if site.cloud() {
                                    "Jira Cloud".to_owned()
                                } else {
                                    site.tracker()
                                }
                            }
                        }),
                );
                if mine.is_empty() && bad.is_none() {
                    body = body.child(
                        div()
                            .px(px(22.))
                            .py(px(4.))
                            .text_size(px(12.))
                            .text_color(theme::faint())
                            .child("No boards"),
                    );
                }
                for b in mine {
                    let (id, store, frame, page) =
                        (b.key(), store.clone(), frame.clone(), page.clone());
                    let on = !drafts_on && s.board.as_ref() == Some(&id);
                    body =
                        body.child(
                            artoo_plugin_sdk::kit::row(
                                SharedString::from(format!("board-{}-{}", b.site, b.id)),
                                on,
                            )
                            .h(px(28.))
                            .gap(px(8.))
                            .child(
                                icons::icon("layers")
                                    .size(px(14.))
                                    .text_color(theme::muted()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(13.))
                                    .text_color(theme::text())
                                    .truncate()
                                    .child(b.name.clone()),
                            )
                            // A tracker without project keys (Trello: the board's own short link) shows none.
                            .children(b.project.clone().filter(|_| keys).map(|k| {
                                div().text_size(px(11.)).text_color(theme::faint()).child(k)
                            }))
                            .tip(format!(
                                "{}{}",
                                b.name,
                                b.project
                                    .as_ref()
                                    .filter(|_| keys)
                                    .map(|k| format!(" · {k}"))
                                    .unwrap_or_default()
                            ))
                            .on_click(move |_, _, cx| {
                                page.update(cx, |p, cx| {
                                    p.drafts = false;
                                    cx.notify();
                                });
                                store.update(cx, |s, cx| s.pick_board(id.clone(), cx));
                                frame.go_space(SPACE, cx);
                            }),
                        );
                }
            }
        }
        Some(Read::Failed(why)) => {
            body = body.child(
                div()
                    .px(px(8.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(why.clone()),
            );
        }
        None if sites.is_empty() => {}
        _ => {
            body = body.child(
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .child(controls::skeleton("tasks-boards", 3)),
            )
        }
    }
    list = list.children(page.read(cx).folding(BOARDS, !folded, body));
    let n = s.kept.drafts.len();
    if !sites.is_empty() {
        let (page, frame) = (page.clone(), frame.clone());
        list = list
            .child(
                div()
                    .mt(px(8.))
                    .pt(px(6.))
                    .border_t_1()
                    .border_color(theme::border()),
            )
            .child(
                artoo_plugin_sdk::kit::row("tasks-drafts-row", drafts_on)
                    .h(px(28.))
                    .gap(px(8.))
                    .child(
                        icons::icon("pencil")
                            .size(px(14.))
                            .text_color(theme::muted()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(theme::text())
                            .child("Drafts"),
                    )
                    .when(n > 0, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::muted())
                                .child(n.to_string()),
                        )
                    })
                    .tip("Drafts: kept on this Mac until created")
                    .on_click(move |_, _, cx| {
                        page.update(cx, |p, cx| {
                            p.drafts = true;
                            cx.notify();
                        });
                        frame.go_space(SPACE, cx);
                    }),
            );
    }
    Some(list.into_any_element())
}
