//! ⌘K's Tasks tab: the issues read so far — every board's, every issue opened — found by their key or their words,
//! each opening its page; All shows the first few.

use crate::agents::issue_route;
use crate::model::Row;
use crate::store::{Read, Store};
use artoo_plugin_sdk::gpui::Entity;
use artoo_plugin_sdk::shell::search::{AllOf, Ask, Find, Opens, Search};
use std::rc::Rc;

/// The scope's id: the action `tasks.search` opens ⌘K on it.
pub const SCOPE: &str = "tasks";

pub fn scope(store: Entity<Store>) -> Search {
    Search {
        id: SCOPE,
        label: "Tasks",
        placeholder: "Issues — by key or words",
        go: "tasks.search",
        order: 45,
        all: AllOf {
            few: 3,
            order: 45,
            prefix: None,
            alone: false,
        },
        find: Rc::new(move |ask, cx| find(ask, store.read(cx))),
        opened: None,
    }
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

fn find(ask: Ask, s: &Store) -> Vec<Find> {
    let text = ask.text.trim();
    let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
    // All finds nothing until something is typed; the tab lists every issue.
    if ask.all && words.is_empty() {
        return Vec::new();
    }
    let key = text.to_uppercase();
    let mut found: Vec<(u8, String, Row)> = known(s)
        .into_iter()
        .filter_map(|(site, row)| {
            let hay = format!("{} {}", row.key, row.summary).to_lowercase();
            let rank = if row.key == key {
                0
            } else if row.key.starts_with(&key) && !key.is_empty() {
                1
            } else if words.iter().all(|w| hay.contains(w.as_str())) {
                2
            } else {
                return None;
            };
            Some((rank, site, row))
        })
        .collect();
    // The best first; among equals, the latest changed.
    found.sort_by(|a, b| a.0.cmp(&b.0).then(b.2.updated.cmp(&a.2.updated)));
    found
        .into_iter()
        .take(60)
        .map(|(_, site, row)| {
            let who = (row.assignee.as_ref())
                .map(|a| {
                    if s.is_me(&site, Some(a)) {
                        "You".to_owned()
                    } else {
                        a.name.clone()
                    }
                })
                .unwrap_or_else(|| "Unassigned".into());
            let detail = format!("{} · {who}", row.state.name);
            let open = Opens::Route(issue_route(&row.key, Some(&site)));
            Find::new(
                "Issues",
                format!("{} {}", row.key, row.summary),
                detail,
                open,
            )
            .glyph("list-checks")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_finds_its_issue_first_then_words() {
        let mut s = crate::store::test_store();
        let row = |key: &str, summary: &str, updated: &str| Row {
            key: key.into(),
            summary: summary.into(),
            updated: updated.into(),
            ..Default::default()
        };
        s.rows.insert(
            ("acme.atlassian.net".into(), 7),
            Read::Done(vec![
                row("PAY-4120", "Refunds in bulk", "2026-11-12"),
                row("PAY-412", "Refund rounds down", "2026-11-10"),
                row("PAY-418", "Rounding rule in the preview", "2026-11-13"),
            ]),
        );
        let labels = |text: &str| -> Vec<String> {
            find(Ask { text, all: false }, &s)
                .into_iter()
                .map(|f| f.label)
                .collect()
        };
        assert_eq!(labels("pay-412")[0], "PAY-412 Refund rounds down");
        assert_eq!(
            labels("rounding"),
            vec!["PAY-418 Rounding rule in the preview"]
        );
        assert!(
            find(
                Ask {
                    text: "",
                    all: true
                },
                &s
            )
            .is_empty()
        );
    }
}
