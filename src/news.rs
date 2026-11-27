//! News in the Inbox: what others did lately on your issues — assigned one to you, mentioned you, changed one of
//! yours — listed under Jira, each opening its issue; one assigned to you also a toast while Artoo is in front.

use crate::agents::issue_route;
use crate::model::{News, Told};
use crate::store::{Store, news_key};
use artoo_plugin_sdk::gpui::{App, Entity};
use artoo_plugin_sdk::shell::notices::{self, Kind, Listed, Lister, Listers, Source, Why};
use artoo_plugin_sdk::shell::{Frame, Toast};
use std::rc::Rc;

pub const ASSIGNED: &str = "artoo-tasks.assigned";
pub const MENTIONED: &str = "artoo-tasks.mentioned";
pub const CHANGED: &str = "artoo-tasks.changed";

/// The news' kinds, offered as the plugin installs: Settings → Notifications switches each.
pub fn offer(cx: &mut App) {
    notices::add_source(
        cx,
        Source::new(
            env!("CARGO_PKG_NAME"),
            crate::jira::trackers_named(),
            vec![
                Kind::listed(ASSIGNED, "Assigned to you").toast(),
                Kind::listed(MENTIONED, "Mentions you").why(Why::Mentions),
                Kind::listed(CHANGED, "Your issue changed"),
            ],
        )
        // Its rows wear its tab's icon, not a letter (the owner, 2026-11-27).
        .icon(crate::look::TASKS_ICON),
    );
}

fn kind_of(told: Told) -> &'static str {
    match told {
        Told::Assigned => ASSIGNED,
        Told::Mentioned => MENTIONED,
        Told::Changed => CHANGED,
    }
}

/// A news as the Inbox lists it: what happened, the issue, its state and who has it.
// Artoo after 2.0.0 adds fields to `Listed` (`who`, `badge`); the update fills them there and is needless on 2.0.0.
#[allow(clippy::needless_update)]
fn listed(host: &str, n: &News, s: &Store) -> Listed {
    let me = s.me.get(host);
    let assignee = match &n.row.assignee {
        Some(a) if me.is_some_and(|m| m.id == a.id) => "You".to_owned(),
        Some(a) => a.name.clone(),
        None => "Unassigned".to_owned(),
    };
    let assignee = match &n.was {
        Some(was) => format!("{assignee} · was {was}"),
        None => assignee,
    };
    let place = match s.projects.get(host).and_then(|p| p.done()) {
        Some(list) => (list.iter())
            .find(|(k, _)| *k == n.row.project)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| n.row.project.clone()),
        None => n.row.project.clone(),
    };
    Listed {
        kind: kind_of(n.told),
        key: news_key(host, n),
        says: n.says.clone(),
        about: format!("{} {}", n.row.key, n.row.summary),
        text: n.text.clone(),
        at: n.at,
        tag: n.row.key.clone(),
        place,
        facts: vec![
            ("State".into(), n.row.state.name.clone()),
            ("Assignee".into(), assignee),
        ],
        open: format!("Open {}", n.row.key),
        ..Default::default()
    }
}

/// A tab's part of the news: its Inbox rows, opening their issues; the toasts of news new since the last read; the
/// news read each minute from the first tab on.
pub fn install_in(store: &Entity<Store>, frame: &Rc<dyn Frame>, cx: &mut App) {
    let (rows_store, open_store, open_frame) = (store.clone(), store.clone(), frame.clone());
    Listers::of(&frame.services()).add(Lister {
        rows: Rc::new(move |cx| {
            let s = rows_store.read(cx);
            (s.news.iter()).map(|(h, n)| listed(h, n, s)).collect()
        }),
        open: Rc::new(move |key, cx| {
            let found = (open_store.read(cx).news.iter())
                .find(|(h, n)| news_key(h, n) == key)
                .map(|(h, n)| (h.clone(), n.row.key.clone()));
            if let Some((host, issue)) = found {
                open_frame.open(issue_route(&issue, Some(&host)), cx);
            }
        }),
        done: None,
    });
    let toasts = frame.clone();
    cx.observe(store, move |store, cx| {
        if store.read(cx).news_fresh.is_empty() {
            return;
        }
        let fresh = store.update(cx, |s, _| std::mem::take(&mut s.news_fresh));
        for (host, n) in fresh {
            store.update(cx, |s, _| {
                s.news_last = Some((host.clone(), n.row.key.clone()))
            });
            let says = n.says.clone();
            toasts.toast(
                Toast {
                    kind: Some(kind_of(n.told)),
                    about: Some(format!("{} {}", n.row.key, n.row.summary)),
                    button: Some(("Open", Box::new(crate::OpenNews))),
                    ..Toast::new(says)
                },
                cx,
            );
        }
    })
    .detach();
    store.update(cx, |s, cx| s.watch_news(cx));
}
