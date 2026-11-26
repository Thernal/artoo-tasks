//! The space's side panel: the boards of every account the project reads, under its host, the one picked marked, a
//! site not reached saying so with when its boards were read; `+` connecting another, a host's menu leaving it; a
//! filter over them (⌘P); Drafts at the foot, with how many.

use crate::keep::KeptSlice;
use crate::model::Board;
use crate::page::{INPUT_BOARDS, Page, SPACE};
use crate::store::{Read, Store};
use crate::trackers::SiteExt;
use artoo_plugin_sdk::core::textedit::TextEdit;
use artoo_plugin_sdk::gpui::{
    self, AnyElement, App, Context, Entity, FontWeight, KeyDownEvent, SharedString, div, prelude::*,
};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, icons, input, keys, px, theme};
use artoo_plugin_sdk::shell::{MenuItem, Typed};

/// The fold of the side panel's Boards.
const BOARDS: &str = "side-boards";

/// `None` while the project reads no tracker: the page alone, as wide as the window, says how to connect one.
pub fn side(page: &Entity<Page>, width: f32, cx: &mut App) -> Option<AnyElement> {
    let p = page.read(cx);
    let project = p.project().to_owned();
    let filter = filter_field(p, p.typing == Some(INPUT_BOARDS));
    let typed = p.boards_filter.clone();
    let (store, frame, drafts_on) = (
        p.store.clone(),
        p.frame.clone(),
        p.drafts && p.issue_key().is_none(),
    );
    let s = store.read(cx);
    let sites = s.sites_in(&project);
    let unread = s.unread_in(&project);
    // An island of its own, as Code's tree and the list beside it are (the owner, 2026-11-26, asked twice).
    let mut list = artoo_plugin_sdk::kit::island()
        .w(px(width))
        .flex_none()
        .min_w_0()
        .h_full()
        .min_h_0()
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
    // What folds under Boards: the filter, each account's host and its boards.
    let mut body = div().flex().flex_col().gap(px(2.)).child(filter);
    match s.boards_in(&project) {
        Some(Read::Done(boards)) => {
            // As the tracker lists them: a board picked stays where it is (the owner, 2026-11-26).
            let boards: Vec<Board> = boards.into_iter().filter(|b| matches(b, &typed)).collect();
            if boards.is_empty() && !typed.trim().is_empty() {
                body = body.child(
                    div()
                        .px(px(8.))
                        .py(px(4.))
                        .text_size(px(12.))
                        .text_color(theme::faint())
                        .child("No board matches"),
                );
            }
            for site in &sites {
                let host = site.key().to_owned();
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
                // Filtered, a site none of whose boards is left is left out.
                if mine.is_empty() && !typed.trim().is_empty() {
                    continue;
                }
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
                            // Each with its icon, as the design draws the menu (the owner, 2026-11-26).
                            let item = MenuItem::new(words, move |_, cx| {
                                store
                                    .update(cx, |s, cx| s.link(&project, &host, false, cx))
                                    .ok();
                            })
                            .icon("x");
                            let frame = leave_frame.clone();
                            let accounts = MenuItem::new("Settings → Accounts", move |_, cx| {
                                frame.open_settings(artoo_plugin_sdk::hosts::connect::SETTINGS, cx)
                            })
                            .icon("settings");
                            leave_frame.menu(e.position, vec![vec![item], vec![accounts]], cx);
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
                        .child(div().flex_1().truncate().child(site.named()))
                        .children(since.clone().map(|at| div().flex_none().child(at)))
                        .map(|d| {
                            let unreached = || format!("Can’t reach {host}");
                            match (&since, &bad) {
                                (Some(at), _) => {
                                    d.tip_two(unreached(), format!("Boards from {at}"))
                                }
                                (None, Some(why)) => d.tip_two(unreached(), why.clone()),
                                (None, None) if site.cloud() => d.tip("Jira Cloud"),
                                (None, None) => d.tip(site.tracker()),
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
                    let key = b.project.clone().filter(|_| keys);
                    let left = s.kept.slices.get(&id).and_then(left_on);
                    let mut tip = b.name.clone();
                    if let Some(k) = &key {
                        tip = format!("{tip} · {k}");
                    }
                    if let Some(left) = &left {
                        tip = format!("{tip} · {}", left.words());
                    }
                    let name = div()
                        .text_size(px(13.))
                        .text_color(theme::text())
                        .truncate()
                        .child(b.name.clone());
                    let row = artoo_plugin_sdk::kit::row(
                        SharedString::from(format!("board-{}-{}", b.site, b.id)),
                        on,
                    )
                    .gap(px(8.))
                    .child(crate::look::board_glyph_in(14., theme::muted()));
                    // One line, as the design has it (the owner, 2026-11-26): where the board was left is its tip's.
                    let row = row.h(px(28.)).child(name.flex_1().min_w_0());
                    body = body.child(
                        row
                            // A tracker without project keys (Trello: the board's own short link) shows none.
                            .children(key.map(|k| {
                                div().text_size(px(11.)).text_color(theme::faint()).child(k)
                            }))
                            .tip(tip)
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
    // Only the boards scroll: as tall as they are while they fit, the panel's head and Drafts staying in sight.
    let p = page.read(cx);
    list = list.child(
        div()
            .id("tasks-side-boards")
            .flex_shrink(1.)
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&p.boards_scroll)
            .children(p.folding(BOARDS, !folded, body)),
    );
    let n = s.kept.drafts.len();
    if !sites.is_empty() {
        let (page, frame) = (page.clone(), frame.clone());
        // Drafts at the panel's foot, as the design has it (the owner, 2026-11-26).
        list = list
            .child(div().flex_1())
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
                    .tip_two("Drafts", "Kept on this Mac until created")
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

/// The filter over the boards, as Design draws it at the head of Boards: `Filter boards ⌘P` until it is typed in.
fn filter_field(p: &Page, typing: bool) -> AnyElement {
    let shown = p.boards_filter.clone();
    let row = div()
        .id("tasks-boards-filter")
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(28.))
        .mx(px(2.))
        .mb(px(6.))
        .px(px(8.))
        .rounded(px(6.))
        .bg(theme::base())
        .text_size(px(13.))
        .text_color(theme::faint())
        .cursor_text()
        .when(typing, |d| d.border_1().border_color(theme::focus()))
        .child(
            icons::icon("search")
                .size(px(13.))
                .text_color(theme::faint()),
        );
    let row = if !typing && shown.is_empty() {
        row.child(div().flex_1().child("Filter boards"))
            .child(keys::caps("⌘P", keys::On::Plain))
            .tip("Filter boards  ⌘P")
    } else {
        let edit = &p.boards_filter_edit;
        let field = input::field(
            &shown,
            typing,
            "Filter boards",
            &input::Look::default(),
            || {
                input::text_body(
                    &shown,
                    edit,
                    input::Look::default(),
                    Default::default(),
                    None,
                )
                .into_any_element()
            },
        );
        row.text_color(theme::text()).child(field.flex_1())
    };
    artoo_plugin_sdk::shell::input_box(row, INPUT_BOARDS, &p.frame).into_any_element()
}

/// Whether board `b` is one `typed` finds: every word of it in its name or its project's key, any case.
fn matches(b: &Board, typed: &str) -> bool {
    let hay = format!("{} {}", b.name, b.project.as_deref().unwrap_or_default()).to_lowercase();
    typed
        .split_whitespace()
        .all(|w| hay.contains(&w.to_lowercase()))
}

/// Where a board was left, as its row in Boards says it.
#[derive(Debug, PartialEq)]
struct Left {
    /// The slice's name as the tracker names it; empty for a board without sprints, read whole.
    name: String,
    /// `78 issues`, as of the slice's last read.
    count: Option<String>,
}

impl Left {
    /// The tooltip's part: the slice's whole name and its count.
    fn words(&self) -> String {
        let at = (!self.name.is_empty()).then(|| format!("opens on {}", self.name));
        at.into_iter()
            .chain(self.count.clone())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// `None` for a board never opened, or opened and never counted on a slice without a name: its name alone shows.
fn left_on(kept: &KeptSlice) -> Option<Left> {
    let count = kept
        .count
        .map(|n| format!("{n} {}", if n == 1 { "issue" } else { "issues" }));
    (!kept.name.is_empty() || count.is_some()).then(|| Left {
        name: kept.name.clone(),
        count,
    })
}

/// The boards the side panel shows for project `project` with `typed` in its filter, in its order: by site, as the
/// sites are kept, each site's as the tracker lists them.
fn shown_boards(s: &Store, project: &str, typed: &str) -> Vec<Board> {
    let Some(Read::Done(boards)) = s.boards_in(project) else {
        return Vec::new();
    };
    let boards: Vec<Board> = boards.into_iter().filter(|b| matches(b, typed)).collect();
    (s.sites_in(project).iter())
        .flat_map(|site| (boards.iter()).filter(move |b| b.site == site.key()))
        .cloned()
        .collect()
}

impl Page {
    /// ⌘P: the side panel's filter of the boards given the keyboard, Boards opened when folded. False where no side
    /// panel is beside the page (an issue's page, no tracker yet): ⌘P goes on to Search files.
    pub fn filter_boards(&mut self, cx: &mut Context<Self>) -> bool {
        let project = self.project().to_owned();
        if self.issue_key().is_some() || self.store.read(cx).sites_in(&project).is_empty() {
            return false;
        }
        if self.boards_folded {
            self.boards_folded = false;
            self.moved(BOARDS, cx);
        }
        // Hidden with ⌘J or folded into the menu, the panel comes back for the filter it holds.
        if !self.frame.side_shown(cx) {
            self.frame.show_side(cx);
        }
        // The filter heads the boards, so it is scrolled away with them.
        self.boards_scroll.set_offset(gpui::point(px(0.), px(0.)));
        self.frame.take_keyboard(INPUT_BOARDS, cx);
        cx.notify();
        true
    }

    /// A key in the boards' filter: ↵ opens the first board it leaves, esc empties it; either lets the keyboard go.
    pub fn boards_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let edit = input::edit(
            &mut self.boards_filter,
            &mut self.boards_filter_edit,
            &event.keystroke,
            cx,
        );
        let left = match edit {
            input::Edit::Escape => None,
            input::Edit::Enter { .. } => {
                let project = self.project().to_owned();
                let first = shown_boards(self.store.read(cx), &project, &self.boards_filter);
                first.into_iter().next()
            }
            _ => {
                cx.notify();
                return Typed::Kept;
            }
        };
        self.boards_filter.clear();
        self.boards_filter_edit = TextEdit::new(String::new());
        if let Some(b) = left {
            self.drafts = false;
            self.store.update(cx, |s, cx| s.pick_board(b.key(), cx));
            self.frame.go_space(SPACE, cx);
        }
        cx.notify();
        Typed::Leave
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_is_found_by_every_word_in_its_name_or_its_key() {
        let b = Board {
            name: "Payments".into(),
            project: Some("PAY".into()),
            ..Default::default()
        };
        for typed in ["", "pay", "PAYM", "ments pay", "  "] {
            assert!(matches(&b, typed), "{typed}");
        }
        for typed in ["ledger", "pay ledger"] {
            assert!(!matches(&b, typed), "{typed}");
        }
    }

    #[test]
    fn a_board_shows_where_it_was_left_and_never_opened_its_name_alone() {
        let kept = |name: &str, count| KeptSlice {
            slice: "sprint:34".into(),
            name: name.into(),
            count,
        };
        let sprint = left_on(&kept("Sprint 34", Some(78))).unwrap();
        assert_eq!(sprint.count.as_deref(), Some("78 issues"));
        assert_eq!(sprint.words(), "opens on Sprint 34 · 78 issues");
        let whole = left_on(&kept("", Some(1))).unwrap();
        assert_eq!(
            (whole.name.as_str(), whole.words().as_str()),
            ("", "1 issue")
        );
        assert_eq!(
            left_on(&kept("Backlog", None)).unwrap().words(),
            "opens on Backlog"
        );
        assert_eq!(left_on(&kept("", None)), None);
    }
}
