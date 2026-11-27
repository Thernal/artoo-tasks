//! ⌘K's Tasks tab: the issues read so far — every board's, every issue opened — found by their key or their words,
//! and the boards by name, each opening its page. A key typed that is not read yet is read (one issue, on the site
//! whose board has its project) and is the top hit meanwhile. All shows the first few, first among what is found.

use crate::agents::issue_route;
use crate::model::{Board, Row};
use crate::page::{Page, SPACE};
use crate::store::{Read, Store};
use crate::trackers::SiteExt;
use artoo_plugin_sdk::gpui::{App, Entity};
use artoo_plugin_sdk::shell::search::{AllOf, Ask, Find, Opens, Search, SearchWindow};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// The scope's id: the action `tasks.search` opens ⌘K on it.
pub const SCOPE: &str = "tasks";

/// How long a key typed stays before it is read: typing on reads only the key it ends at.
const SETTLE: Duration = Duration::from_millis(300);

/// A key typed that is not read yet: the text asked last, and the key waiting to be read.
#[derive(Default)]
struct Looking {
    asked: String,
    waiting: Option<String>,
}

pub fn scope(store: Entity<Store>, page: Entity<Page>) -> Search {
    let looking = Rc::new(RefCell::new(Looking::default()));
    Search {
        id: SCOPE,
        label: "Tasks",
        placeholder: "Issues and boards — by key or words",
        go: "tasks.search",
        order: 45,
        // First among what All finds, after the commands that match.
        all: AllOf {
            few: 3,
            order: 5,
            prefix: None,
            alone: false,
        },
        find: Rc::new(move |ask, cx| {
            let project = page.read(cx).project().to_owned();
            let unread = look_up(ask.text, &project, &store, &page, &looking, cx);
            let boards = match store.read(cx).boards_in(&project) {
                Some(Read::Done(boards)) => boards,
                _ => Vec::new(),
            };
            // A board opens as the side panel's row does: picked, in the space.
            let open_board = |b: &Board| {
                let (id, store, page) = (b.key(), store.clone(), page.clone());
                Opens::Run(Rc::new(move |cx: &mut App| {
                    let frame = page.read(cx).frame.clone();
                    page.update(cx, |p, cx| {
                        p.drafts = false;
                        cx.notify();
                    });
                    store.update(cx, |s, cx| s.pick_board(id.clone(), cx));
                    frame.go_space(SPACE, cx);
                }))
            };
            let mut found = find(ask, store.read(cx), &boards, &open_board);
            if let Some((key, host)) = unread {
                let open = Opens::Route(issue_route(&key, Some(&host)));
                found.insert(
                    0,
                    Find::new("Tasks", key, "Reading…".into(), open).glyph(crate::look::TASKS_ICON),
                );
            }
            found
        }),
        opened: None,
    }
}

/// Text `text` when it is a key of a project's (`PAY-427`) not read yet, upper case, with the site to read it on:
/// read once it has stayed `SETTLE`, the open window found again once it is answered. None for any other text, and
/// for a key read already or refused.
fn look_up(
    text: &str,
    project: &str,
    store: &Entity<Store>,
    page: &Entity<Page>,
    looking: &Rc<RefCell<Looking>>,
    cx: &App,
) -> Option<(String, String)> {
    let text = text.trim();
    looking.borrow_mut().asked = text.to_owned();
    let key = text.to_uppercase();
    let s = store.read(cx);
    let host = is_key(&key)
        .then(|| site_of_key(&key, project, s))
        .flatten();
    let Some(host) = host else {
        looking.borrow_mut().waiting = None;
        return None;
    };
    let known = (s.rows.values())
        .filter_map(|r| r.done())
        .flatten()
        .any(|r| r.key.to_uppercase() == key);
    match s.issues.get(&key) {
        _ if known => return None,
        Some(Read::Done(_) | Read::Failed(_)) => return None,
        Some(Read::Reading) => return Some((key, host)),
        None => {}
    }
    if looking.borrow().waiting.as_deref() == Some(key.as_str()) {
        return Some((key, host));
    }
    looking.borrow_mut().waiting = Some(key.clone());
    let (store, page, looking, wanted, on) = (
        store.clone(),
        page.clone(),
        looking.clone(),
        key.clone(),
        host.clone(),
    );
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SETTLE).await;
        if looking.borrow().waiting.as_deref() != Some(wanted.as_str()) {
            return;
        }
        cx.update(|cx| {
            let window = page.read(cx).frame.services().get::<Rc<dyn SearchWindow>>();
            let key = wanted.clone();
            store.update(cx, |s, cx| {
                s.look_up(
                    &key,
                    &on,
                    move |_, cx| {
                        let text = {
                            let mut l = looking.borrow_mut();
                            if l.waiting.as_deref() == Some(wanted.as_str()) {
                                l.waiting = None;
                            }
                            l.asked.clone()
                        };
                        // Found again for the text in the field, while it is still this key.
                        if let Some(window) = window
                            && text.to_uppercase() == wanted
                        {
                            cx.defer(move |cx| window.again(cx));
                        }
                    },
                    cx,
                )
            });
        });
    })
    .detach();
    Some((key, host))
}

/// Whether `text` is a project's issue key: letters, digits or `_` from a letter, `-`, a number (`PAY-427`).
fn is_key(text: &str) -> bool {
    let Some((project, n)) = text.split_once('-') else {
        return false;
    };
    project.starts_with(|c: char| c.is_ascii_alphabetic())
        && project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !n.is_empty()
        && n.chars().all(|c| c.is_ascii_digit())
}

/// The site key `key` is read on: the one whose board is its project's, else the first of project `project`'s sites
/// with project keys; None when none has.
fn site_of_key(key: &str, project: &str, s: &Store) -> Option<String> {
    let prefix = key.split_once('-')?.0;
    let boards = match s.boards_in(project) {
        Some(Read::Done(boards)) => boards,
        _ => Vec::new(),
    };
    let sites = s.sites_in(project);
    (boards.iter())
        .find(|b| {
            b.project
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(prefix))
        })
        .map(|b| b.site.clone())
        .or_else(|| {
            (sites.iter())
                .find(|site| site.caps().project_keys)
                .map(|site| site.key().to_owned())
        })
}

/// Every issue read, once, with its site: the boards' first, then those opened alone.
fn known(s: &Store) -> Vec<(String, Row)> {
    let mut out: Vec<(String, Row)> = Vec::new();
    let boards = (s.rows.iter()).filter_map(|((site, _), read)| match read {
        Read::Done(rows) => Some(rows.iter().map(move |r| (site.clone(), r.clone()))),
        _ => None,
    });
    let opened = (s.issues.values()).filter_map(|read| match read {
        Read::Done(i) => Some((i.site.clone(), i.row.clone())),
        _ => None,
    });
    for (site, row) in boards.flatten().chain(opened) {
        if !out.iter().any(|(_, r)| r.key == row.key) {
            out.push((site, row));
        }
    }
    out
}

/// What `ask` finds among the issues read and `boards`: a key itself first, then a board its name or key starts
/// with, the keys it starts, a board with every word in its name, the issues with every word; among equals the
/// latest changed. A board's find opens as `open_board` says.
fn find(ask: Ask, s: &Store, boards: &[Board], open_board: &dyn Fn(&Board) -> Opens) -> Vec<Find> {
    let text = ask.text.trim();
    let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
    // All finds nothing until something is typed; the tab lists every issue.
    if ask.all && words.is_empty() {
        return Vec::new();
    }
    let key = text.to_uppercase();
    let mut found: Vec<(u8, String, Find)> = known(s)
        .into_iter()
        .filter_map(|(site, row)| {
            let hay = format!("{} {}", row.key, row.summary).to_lowercase();
            // A Jira key is upper case; Trello's short link mixed, found either way.
            let upper = row.key.to_uppercase();
            let rank = if upper == key {
                0
            } else if upper.starts_with(&key) && !key.is_empty() {
                2
            } else if words.iter().all(|w| hay.contains(w.as_str())) {
                4
            } else {
                return None;
            };
            Some((rank, row.updated.clone(), issue_find(s, &site, &row)))
        })
        .collect();
    let lower = text.to_lowercase();
    found.extend(boards.iter().filter_map(|b| {
        let name = b.name.to_lowercase();
        let project = b.project.as_deref().unwrap_or_default().to_lowercase();
        let rank = if !lower.is_empty() && (name.starts_with(&lower) || project == lower) {
            1
        } else if words.iter().all(|w| name.contains(w.as_str())) {
            3
        } else {
            return None;
        };
        let detail = match &b.project {
            Some(p) => format!("Board · {p} · {}", b.site),
            None => format!("Board · {}", b.site),
        };
        let find = Find::new("Tasks", b.name.clone(), detail, open_board(b));
        Some((rank, String::new(), find.glyph("layers")))
    }));
    // The best first; among equals, the latest changed.
    found.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    found.into_iter().take(60).map(|(_, _, f)| f).collect()
}

/// Issue `row`'s find, on site `site`: its key and summary, its state and who has it.
fn issue_find(s: &Store, site: &str, row: &Row) -> Find {
    let who = (row.assignee.as_ref())
        .map(|a| {
            if s.is_me(site, Some(a)) {
                "You".to_owned()
            } else {
                a.name.clone()
            }
        })
        .unwrap_or_else(|| "Unassigned".into());
    let detail = format!("{} · {who}", row.state.name);
    let open = Opens::Route(issue_route(&row.key, Some(site)));
    Find::new(
        "Tasks",
        format!("{} {}", row.key, row.summary),
        detail,
        open,
    )
    .glyph(crate::look::TASKS_ICON)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_finds_its_issue_first_then_boards_then_words() {
        let mut s = crate::store::test_store();
        let row = |key: &str, summary: &str, updated: &str| Row {
            key: key.into(),
            summary: summary.into(),
            updated: updated.into(),
            ..Default::default()
        };
        s.rows.insert(
            ("acme.atlassian.net".into(), "7".into()),
            Read::Done(vec![
                row("PAY-4120", "Refunds in bulk", "2026-11-12"),
                row("PAY-412", "Refund rounds down", "2026-11-10"),
                row("PAY-418", "Rounding rule in the preview", "2026-11-13"),
                row("LED-3", "Payments ledger export", "2026-11-09"),
            ]),
        );
        let board = |id: &str, name: &str, project: &str| Board {
            id: id.into(),
            name: name.into(),
            site: "acme.atlassian.net".into(),
            project: Some(project.into()),
            ..Default::default()
        };
        let boards = [board("7", "Payments", "PAY"), board("9", "Ledger", "LED")];
        let nowhere = |_: &Board| Opens::Run(Rc::new(|_: &mut App| {}));
        let labels = |text: &str| -> Vec<String> {
            find(Ask { text, all: false }, &s, &boards, &nowhere)
                .into_iter()
                .map(|f| match f.glyph {
                    Some("layers") => format!("board {}", f.label),
                    _ => f.label,
                })
                .collect()
        };
        assert_eq!(labels("pay-412")[0], "PAY-412 Refund rounds down");
        assert_eq!(
            labels("rounding"),
            vec!["PAY-418 Rounding rule in the preview"]
        );
        // A board by its name or its key, before the issues its words find.
        assert_eq!(
            labels("pay")[..2],
            ["board Payments", "PAY-418 Rounding rule in the preview"]
        );
        assert_eq!(
            labels("ledger"),
            ["board Ledger", "LED-3 Payments ledger export"]
        );
        assert!(
            find(
                Ask {
                    text: "",
                    all: true
                },
                &s,
                &boards,
                &nowhere
            )
            .is_empty()
        );
    }

    #[test]
    fn a_key_is_letters_a_dash_and_a_number() {
        for key in ["PAY-427", "A1_B-3"] {
            assert!(is_key(key), "{key}");
        }
        for text in [
            "PAY",
            "PAY-",
            "PAY-4a",
            "1PAY-4",
            "refund rounds",
            "hP4sK7vL",
        ] {
            assert!(!is_key(text), "{text}");
        }
    }
}
