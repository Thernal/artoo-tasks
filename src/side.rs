//! The space's side panel: every site's boards under its host, the one picked marked, a site not reached saying so
//! with when its boards were read; Drafts at the foot, with how many.

use crate::page::{Page, SPACE};
use crate::store::Read;
use artoo_plugin_sdk::gpui::{AnyElement, App, Entity, FontWeight, SharedString, div, prelude::*};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, icons, px, theme};

pub fn side(page: &Entity<Page>, width: f32, cx: &mut App) -> AnyElement {
    let p = page.read(cx);
    let (store, frame, drafts_on) = (
        p.store.clone(),
        p.frame.clone(),
        p.drafts && p.issue_key().is_none(),
    );
    let s = store.read(cx);
    let mut list = div()
        .w(px(width))
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .px(px(8.))
        .py(px(8.));
    // No tracker yet: the page says how to connect one, the panel nothing.
    if s.sites.is_empty() {
        return list.into_any_element();
    }
    list = list.child(
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .pb(px(6.))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::muted())
            .child(
                icons::icon("chevron")
                    .size(px(11.))
                    .text_color(theme::faint()),
            )
            .child("BOARDS"),
    );
    match &s.boards {
        Some(Read::Done(boards)) => {
            for site in &s.sites {
                let host = site.host().to_owned();
                let bad = s
                    .unread
                    .iter()
                    .find(|(h, _)| *h == host)
                    .map(|(_, why)| why.clone());
                let mine: Vec<_> = boards.iter().filter(|b| b.site == host).collect();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("site-{host}")))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .px(px(8.))
                        .pt(px(6.))
                        .pb(px(2.))
                        .text_size(px(11.))
                        .text_color(if bad.is_some() {
                            theme::removed()
                        } else {
                            theme::faint()
                        })
                        .children(
                            bad.as_ref()
                                .map(|_| div().size(px(6.)).rounded_full().bg(theme::removed())),
                        )
                        .child(div().flex_1().truncate().child(host.clone()))
                        .tip(match &bad {
                            Some(why) => format!("Can’t reach {host} · {why}"),
                            None => {
                                if site.cloud() {
                                    "Jira Cloud".to_owned()
                                } else {
                                    site.tracker().to_owned()
                                }
                            }
                        }),
                );
                if mine.is_empty() && bad.is_none() {
                    list = list.child(
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
                    list =
                        list.child(
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
                            // A Trello board's project is its own short link: not shown.
                            .children(b.project.clone().filter(|_| !site.trello()).map(|k| {
                                div().text_size(px(11.)).text_color(theme::faint()).child(k)
                            }))
                            .tip(format!(
                                "{}{}",
                                b.name,
                                b.project
                                    .as_ref()
                                    .filter(|_| !site.trello())
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
            list = list.child(
                div()
                    .px(px(8.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(why.clone()),
            );
        }
        None if s.sites.is_empty() => {}
        _ => list = list.child(controls::skeleton("tasks-boards", 3)),
    }
    let n = s.kept.drafts.len();
    if !s.sites.is_empty() {
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
    list.into_any_element()
}
