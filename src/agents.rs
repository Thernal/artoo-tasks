//! What agents get of the tracker: the tools they call through Artoo's MCP server (#213) and the places a note for
//! an agent is on (#212). Both read only.

use crate::{jira, model, store};
use artoo_plugin_sdk::agents::places::{self, Kind};
use artoo_plugin_sdk::core::route::Route;
use artoo_plugin_sdk::gpui::{App, AppContext, Task};
use artoo_plugin_sdk::serde_json::{self, Value, json};
use artoo_plugin_sdk::tools::{self, Tool};
use std::rc::Rc;

/// The page of an issue: `tasks.issue`, its key.
pub const ISSUE: &str = "tasks.issue";

pub fn issue_route(key: &str) -> Route {
    Route {
        kind: ISSUE.into(),
        args: json!({ "key": key }).to_string(),
    }
}

/// An issue page's key.
pub fn key_of(route: &Route) -> Option<String> {
    let v: Value = serde_json::from_str(&route.args).ok()?;
    v["key"].as_str().map(str::to_owned)
}

fn first_site() -> Result<jira::Site, String> {
    let (sites, missing) = jira::sites();
    sites.into_iter().next().ok_or_else(|| {
        missing
            .first()
            .cloned()
            .unwrap_or_else(|| "No tracker is connected in Artoo".into())
    })
}

fn arg(args: &str, name: &str) -> Option<String> {
    let v: Value = serde_json::from_str(args).ok()?;
    v[name].as_str().map(str::to_owned)
}

/// `jira.issue` and `jira.search`, for agents.
pub fn add_tools(cx: &mut App) {
    let issue = Tool {
        name: "jira.issue",
        description: "An issue of the team's Jira, whole: its facts, description, sub-tasks and comments",
        input: r#"{"type":"object","properties":{"key":{"type":"string","description":"The issue's key, PAY-412"}},"required":["key"]}"#,
        call: Rc::new(|args: String, cx: &mut App| -> Task<Result<String, String>> {
            let Some(key) = arg(&args, "key") else {
                return Task::ready(Err("key is missing".into()));
            };
            cx.background_spawn(async move {
                let site = first_site()?;
                model::read_issue(&site, &key).map(|i| model::digest(&i))
            })
        }),
    };
    let search = Tool {
        name: "jira.search",
        description: "Issues of the team's Jira a JQL query finds: each one's key, summary, state and assignee",
        input: r#"{"type":"object","properties":{"jql":{"type":"string","description":"A JQL query: project = PAY AND status != Done"},"max":{"type":"integer","description":"At most this many, 20 when left out"}},"required":["jql"]}"#,
        call: Rc::new(|args: String, cx: &mut App| -> Task<Result<String, String>> {
            let Some(jql) = arg(&args, "jql") else {
                return Task::ready(Err("jql is missing".into()));
            };
            let max = serde_json::from_str::<Value>(&args)
                .ok()
                .and_then(|v| v["max"].as_u64())
                .unwrap_or(20)
                .min(100) as usize;
            cx.background_spawn(async move {
                let site = first_site()?;
                let rows = model::search(&site, &jql, max)?;
                if rows.is_empty() {
                    return Ok(format!("Nothing matches {jql}"));
                }
                Ok(rows
                    .iter()
                    .map(|r| {
                        let who = r.assignee.as_deref().unwrap_or("unassigned");
                        format!("{} {} ({} · {} · {who})", r.key, r.summary, r.kind, r.state)
                    })
                    .collect::<Vec<_>>()
                    .join("\n"))
            })
        }),
    };
    for tool in [issue, search] {
        if let Err(why) = tools::add(cx, tool) {
            eprintln!("artoo-tasks: {why}");
        }
    }
}

/// A paragraph of an issue's description as a note's place: `{"key":"PAY-412","para":4}`.
pub fn paragraph_place(key: &str, para: usize) -> String {
    json!({ "key": key, "para": para }).to_string()
}

/// `jira.issue`: an issue, or a paragraph of its description; `jira.board`: a board as it is filtered.
pub fn add_places(cx: &mut App) {
    let issue = Kind {
        kind: "jira.issue",
        reads: Rc::new(|data: &str, _: &App| {
            let v: Value = serde_json::from_str(data).unwrap_or_default();
            let key = v["key"].as_str().unwrap_or("?");
            match v["para"].as_u64() {
                Some(n) => format!("{key} · description · ¶{n}"),
                None => key.to_owned(),
            }
        }),
        digest: Rc::new(|data: &str, cx: &App| {
            let v: Value = serde_json::from_str(data).unwrap_or_default();
            let key = v["key"].as_str().unwrap_or_default();
            let store = store::peek(cx);
            let Some(issue) = store.as_ref().and_then(|s| s.read(cx).issue(key).cloned()) else {
                return format!("Issue {key}.");
            };
            let head = format!("{} {} ({} · {})", key, issue.row.summary, issue.row.kind, issue.row.state);
            match v["para"].as_u64() {
                Some(n) => {
                    let paras = model::paragraphs(&issue.description);
                    let para = paras.get(n as usize - 1).cloned().unwrap_or_default();
                    format!("{head}, its description's paragraph {n}:\n\n{para}")
                }
                None => head,
            }
        }),
        tool: Some("jira.issue"),
        route: Rc::new(|data: &str| {
            let v: Value = serde_json::from_str(data).ok()?;
            Some(issue_route(v["key"].as_str()?))
        }),
    };
    let board = Kind {
        kind: "jira.board",
        reads: Rc::new(|data: &str, cx: &App| {
            let v: Value = serde_json::from_str(data).unwrap_or_default();
            let mine = if v["mine"].as_bool() == Some(true) { "Mine" } else { "All" };
            let name = store::peek(cx)
                .and_then(|s| s.read(cx).picked().map(|b| b.name.clone()))
                .unwrap_or_else(|| "A board".into());
            format!("{name} · {mine}")
        }),
        digest: Rc::new(|data: &str, cx: &App| {
            let v: Value = serde_json::from_str(data).unwrap_or_default();
            let (board, mine) = (v["board"].as_u64().unwrap_or_default(), v["mine"].as_bool() == Some(true));
            let store = store::peek(cx);
            let rows = store.as_ref().and_then(|s| match s.read(cx).rows.get(&(board, mine)) {
                Some(store::Read::Done(rows)) => Some(rows.clone()),
                _ => None,
            });
            match rows {
                Some(rows) => rows
                    .iter()
                    .take(40)
                    .map(|r| format!("{} {} ({})", r.key, r.summary, r.state))
                    .collect::<Vec<_>>()
                    .join("\n"),
                None => format!("Board {board}."),
            }
        }),
        tool: Some("jira.search"),
        route: Rc::new(|_: &str| None),
    };
    for kind in [issue, board] {
        if let Err(why) = places::add(cx, kind) {
            eprintln!("artoo-tasks: {why}");
        }
    }
}
