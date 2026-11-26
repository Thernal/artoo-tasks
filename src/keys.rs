//! Issue keys in Artoo's texts: Tasks is a tracker the shell asks (`shell::issues`). The keys of the Jira issues the
//! plugin holds — a board's rows, kept ones among them, and the issues read whole — are links in a request's
//! description, a branch's name and the like; a key's card is drawn from what is held, never read for it, and says as
//! of when when the site was not reached; a click opens the issue's page under Tasks. A key not held stays text, so
//! no link leads nowhere. Trello's cards have no key of this shape and are left out.

use crate::model::{Issue, Row};
use crate::store::{self, Store};
use crate::{OpenKey, look};
use artoo_plugin_sdk::gpui::{App, Global};
use artoo_plugin_sdk::kit::{format, theme};
use artoo_plugin_sdk::shell::issues::{self, IssueCard};
use std::ops::Range;

/// The key clicked last, for the tab on screen to open (`OpenKey`): the shell names no tab when it asks.
#[derive(Default)]
struct Clicked(Option<String>);

impl Global for Clicked {}

/// The key clicked, taken by the tab that opens it.
pub fn take_clicked(cx: &mut App) -> Option<String> {
    cx.default_global::<Clicked>().0.take()
}

/// What is held of issue `key`: its site, its row as last changed, the issue when it was read whole, and when its site
/// is not reached the time of what is held.
struct Held<'a> {
    host: &'a str,
    row: &'a Row,
    issue: Option<&'a Issue>,
    stale: Option<i64>,
}

/// Issue `key` as the store holds it from a Jira account: from the boards of the project shown first, then any other's,
/// then as read whole; the row read whole is taken when it changed later than the board's.
fn held<'a>(store: &'a Store, key: &str) -> Option<Held<'a>> {
    let jira = |host: &str| (store.sites.iter()).any(|s| s.key() == host && s.kind == "jira");
    let shown = |host: &str| {
        let here = store.here.as_ref().and_then(|p| store.links.get(p));
        here.is_some_and(|hosts| hosts.iter().any(|h| h == host))
    };
    let on_board = (store.rows.iter())
        .filter(|(board, _)| jira(&board.0))
        .filter_map(|(board, read)| {
            let row = read.done()?.iter().find(|r| r.key == key)?;
            Some((board, row))
        })
        // The project's own site, then the board read last; the same one every time.
        .max_by_key(|(board, _)| (shown(&board.0), store.read_at.get(*board), *board));
    let issue = store.issue(key).filter(|i| jira(&i.site));
    let issue = issue.filter(|i| on_board.is_none_or(|(b, _)| b.0 == i.site));
    let (host, row) = match (on_board, issue) {
        (Some((board, row)), Some(i)) if i.row.updated > row.updated => (board.0.as_str(), &i.row),
        (Some((board, row)), _) => (board.0.as_str(), row),
        (None, Some(i)) => (i.site.as_str(), &i.row),
        (None, None) => return None,
    };
    let stale = match on_board {
        Some((board, _)) if store.stale.contains_key(board) => store.read_at.get(board).copied(),
        _ if store.unread.iter().any(|(h, _)| h == host) => {
            on_board.and_then(|(b, _)| store.read_at.get(b).copied())
        }
        _ => None,
    };
    Some(Held {
        host,
        row,
        issue,
        stale,
    })
}

/// The site the held issue `key` is on: where its page reads it.
pub fn host_of(store: &Store, key: &str) -> Option<String> {
    held(store, key).map(|h| h.host.to_owned())
}

/// The keys in `text` the store holds.
fn find(store: &Store, text: &str) -> Vec<Range<usize>> {
    (issues::scan(text).into_iter())
        .filter(|r| held(store, &text[r.clone()]).is_some())
        .collect()
}

/// A kind's glyph on the card.
fn glyph(kind: &str, subtask: bool) -> &'static str {
    match kind.to_lowercase().as_str() {
        _ if subtask => "list-checks",
        "bug" => "bug",
        "story" => "bookmark",
        "task" => "check",
        "epic" => "layers",
        _ => "circle-dot",
    }
}

/// The start of a description in plain words: its first paragraph that is no heading, without markdown's marks.
fn excerpt(description: &str) -> String {
    let Some(first) = (crate::model::paragraphs(description).into_iter())
        .find(|p| !p.starts_with('#') && !p.starts_with("```"))
    else {
        return String::new();
    };
    let words: Vec<&str> = (first.lines())
        // A list's or a quote's mark at the start, a hard break's at the end.
        .map(|l| {
            l.trim_start_matches(['>', '-', '*', ' '])
                .trim_end_matches('\\')
        })
        .flat_map(str::split_whitespace)
        .collect();
    unlinked(&words.join(" ")).replace(['*', '`'], "")
}

/// `text` with each markdown link as its words alone.
fn unlinked(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('[') {
        let link = (after.split_once("]("))
            .and_then(|(words, tail)| Some((words, tail.split_once(')')?.1)));
        let Some((words, tail)) = link else {
            break;
        };
        out.push_str(before);
        out.push_str(words);
        rest = tail;
    }
    out + rest
}

/// Issue `key`'s card from what is held; none when nothing is.
fn card(store: &Store, key: &str) -> Option<IssueCard> {
    let held = held(store, key)?;
    let row = held.row;
    let site = store.site_of(held.host)?;
    let mut fields = vec![(
        "Assignee".to_owned(),
        match &row.assignee {
            Some(who) if store.is_me(held.host, Some(who)) => "You".to_owned(),
            Some(who) => who.name.clone(),
            None => "Unassigned".to_owned(),
        },
    )];
    let (done, all) = row.subtasks;
    if all > 0 {
        fields.push(("Sub-tasks".to_owned(), format!("{done}/{all}")));
    }
    // An epic read without its colour takes the one its board says.
    let epic = row.epic.as_ref().map(|epic| {
        let mut epic = epic.clone();
        if epic.color == 0
            && let Some(known) = (store.info.iter())
                .filter(|(board, _)| board.0 == held.host)
                .filter_map(|(_, read)| read.done())
                .flat_map(|i| i.epics.iter())
                .find(|e| e.key == epic.key && e.color != 0)
        {
            epic.color = known.color;
        }
        (epic.name.clone(), look::epic_color(&epic).into())
    });
    Some(IssueCard {
        key: row.key.clone(),
        icon: glyph(&row.kind, row.subtask),
        kind_tint: Some(look::kind_color(&row.kind, row.subtask).into()),
        title: row.summary.clone(),
        source: "Jira".to_owned(),
        host: site.host().to_owned(),
        // Tasks' own tab icon: where a click on the key opens it.
        space_icon: Some("list-checks"),
        state: Some((
            look::display_state(&row.state.name),
            row.state.category == "done",
        )),
        state_tint: Some(match row.state.category.as_str() {
            "done" => theme::added(),
            "indeterminate" => theme::accent(),
            _ => theme::muted(),
        }),
        epic,
        fields,
        excerpt: held
            .issue
            .map_or_else(String::new, |i| excerpt(&i.description)),
        stale: held.stale.map(format::clock),
        url: Some(match held.issue {
            Some(i) if !i.url.is_empty() => i.url.clone(),
            _ => format!("{}/browse/{}", site.url, row.key),
        }),
    })
}

struct Tracker;

impl issues::Tracker for Tracker {
    fn name(&self) -> String {
        "Jira".to_owned()
    }

    fn find(&self, text: &str, cx: &App) -> Vec<Range<usize>> {
        store::peek(cx).map_or_else(Vec::new, |s| find(s.read(cx), text))
    }

    fn card(&self, key: &str, cx: &App) -> Option<IssueCard> {
        card(store::peek(cx)?.read(cx), key)
    }

    fn open(&self, key: &str, cx: &mut App) {
        cx.default_global::<Clicked>().0 = Some(key.to_owned());
        // After the click is done with the window, which cannot be asked while it holds it.
        cx.defer(|cx| {
            // The window clicked is the active one; with none said to be (a window drawn offscreen), the first.
            let window = cx.active_window().or_else(|| cx.windows().first().copied());
            let Some(window) = window else {
                return;
            };
            window
                .update(cx, |_, window, cx| {
                    window.dispatch_action(Box::new(OpenKey), cx)
                })
                .ok();
        });
    }
}

/// Offers Tasks as the tracker of its keys (`install`).
pub fn offer(cx: &mut App) {
    issues::add(cx, std::rc::Rc::new(Tracker));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BoardId, Epic, Person, Site, State};
    use crate::store::Read;

    const HOST: &str = "acme.atlassian.net";

    fn row(key: &str) -> Row {
        Row {
            key: key.into(),
            summary: "Refund rounds down".into(),
            kind: "Bug".into(),
            state: State {
                name: "In Progress".into(),
                category: "indeterminate".into(),
                ..Default::default()
            },
            updated: "2026-11-12T10:00:00.000+0000".into(),
            ..Default::default()
        }
    }

    fn store_with(kind: &str) -> (Store, BoardId) {
        let mut store = store::test_store();
        store.sites = vec![Site {
            url: format!("https://{HOST}"),
            kind: kind.into(),
            ..Default::default()
        }];
        let board: BoardId = (HOST.into(), "7".into());
        let mut held = row("PAY-412");
        held.assignee = Some(Person {
            id: "me".into(),
            name: "Juniper Hale".into(),
        });
        held.subtasks = (1, 3);
        held.epic = Some(Epic {
            key: "PAY-400".into(),
            name: "Refunds that add up".into(),
            ..Default::default()
        });
        store.rows.insert(board.clone(), Read::Done(vec![held]));
        store.read_at.insert(board.clone(), 1_794_000_000);
        (store, board)
    }

    #[test]
    fn only_the_keys_of_jira_issues_held_are_links() {
        let (store, _) = store_with("jira");
        let text = "Fixes PAY-412, not PAY-999 or UTF-8.";
        let found: Vec<&str> = find(&store, text).into_iter().map(|r| &text[r]).collect();
        assert_eq!(found, ["PAY-412"]);
        assert!(find(&store_with("trello").0, text).is_empty());
        assert!(card(&store, "PAY-999").is_none());
    }

    #[test]
    fn a_card_says_what_is_held_and_as_of_when_once_the_site_is_not_reached() {
        let (mut store, board) = store_with("jira");
        let read = card(&store, "PAY-412").expect("held");
        assert_eq!(read.host, HOST);
        assert_eq!(read.state, Some(("In progress".into(), false)));
        assert_eq!(
            read.fields,
            [
                ("Assignee".to_owned(), "Juniper Hale".to_owned()),
                ("Sub-tasks".to_owned(), "1/3".to_owned()),
            ]
        );
        assert_eq!(
            read.epic.map(|e| e.0),
            Some("Refunds that add up".to_owned())
        );
        assert_eq!(
            read.url.as_deref(),
            Some("https://acme.atlassian.net/browse/PAY-412")
        );
        assert_eq!(read.stale, None);
        assert_eq!(read.excerpt, "");
        store.stale.insert(board, "offline".into());
        let kept = card(&store, "PAY-412").expect("held");
        assert_eq!(kept.stale, Some(format::clock(1_794_000_000)));
        assert_eq!(kept.title, "Refund rounds down");
    }

    #[test]
    fn an_issue_read_whole_gives_its_description_and_its_later_facts() {
        let (mut store, _) = store_with("jira");
        let mut whole = row("PAY-412");
        whole.summary = "Refund rounds down on partial captures".into();
        whole.updated = "2026-11-13T07:40:00.000+0000".into();
        let issue = Issue {
            row: whole,
            site: HOST.into(),
            description:
                "h2. What happens\n\nA *partial* capture refunds {{less}}\nthan [it should|https://acme.test]."
                    .into(),
            ..Default::default()
        };
        store.issues.insert("PAY-412".into(), Read::Done(issue));
        let read = card(&store, "PAY-412").expect("held");
        assert_eq!(read.title, "Refund rounds down on partial captures");
        assert_eq!(
            read.excerpt,
            "A partial capture refunds less than it should."
        );
    }
}
