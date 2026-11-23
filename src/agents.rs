//! What agents get of the tracker: the tools they call through Artoo's MCP server (#213) and the places a note for
//! an agent is on (#212). They read; what an agent writes — a comment, a draft of a new issue — is kept in Artoo for
//! the developer, and reaches the tracker only when they send it.

use crate::{jira, model, store};
use artoo_plugin_sdk::agents::places::{self, Kind};
use artoo_plugin_sdk::core::route::Route;
use artoo_plugin_sdk::gpui::{App, AppContext, Task};
use artoo_plugin_sdk::serde_json::{self, Value, json};
use artoo_plugin_sdk::tools::{self, Tool};
use std::rc::Rc;

/// The page of an issue: `tasks.issue`, its key and the host of its site when known.
pub const ISSUE: &str = "tasks.issue";

pub fn issue_route(key: &str, site: Option<&str>) -> Route {
    let args = match site {
        Some(site) => json!({ "key": key, "site": site }),
        None => json!({ "key": key }),
    };
    Route {
        kind: ISSUE.into(),
        args: args.to_string(),
    }
}

/// An issue page's site, when it says.
pub fn site_of(route: &Route) -> Option<String> {
    let v: Value = serde_json::from_str(&route.args).ok()?;
    v["site"].as_str().map(str::to_owned)
}

/// An issue page's key.
pub fn key_of(route: &Route) -> Option<String> {
    let v: Value = serde_json::from_str(&route.args).ok()?;
    v["key"].as_str().map(str::to_owned)
}

/// The sites a tool reads: the one of host `site` when it names one, else every site.
fn sites_for(site: Option<String>) -> Result<Vec<jira::Site>, String> {
    let (sites, missing) = jira::sites();
    if sites.is_empty() {
        return Err(missing
            .first()
            .cloned()
            .unwrap_or_else(|| "No tracker is connected in Artoo".into()));
    }
    match site {
        Some(host) => (sites.into_iter())
            .find(|s| s.key() == host)
            .map(|s| vec![s])
            .ok_or_else(|| format!("No tracker at {host} is connected in Artoo")),
        None => Ok(sites),
    }
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
        input: r#"{"type":"object","properties":{"key":{"type":"string","description":"The issue's key, PAY-412"},"site":{"type":"string","description":"The tracker's host, jira.example.com; every tracker connected when left out"}},"required":["key"]}"#,
        call: Rc::new(
            |args: String, cx: &mut App| -> Task<Result<String, String>> {
                let Some(key) = arg(&args, "key") else {
                    return Task::ready(Err("key is missing".into()));
                };
                let site = arg(&args, "site");
                // The kinds other plugins offer, read here: the Tasks space may not have been opened.
                crate::trackers::read_offered(cx);
                cx.background_spawn(async move {
                    let sites = sites_for(site)?;
                    model::read_issue_on(&sites, &key).map(|i| model::digest(&i))
                })
            },
        ),
    };
    let search = Tool {
        name: "jira.search",
        description: "Issues of the team's Jira a JQL query finds: each one's key, summary, state and assignee",
        input: r#"{"type":"object","properties":{"jql":{"type":"string","description":"A JQL query: project = PAY AND status != Done"},"max":{"type":"integer","description":"At most this many from each tracker, 20 when left out"},"site":{"type":"string","description":"The tracker's host, jira.example.com; every tracker connected when left out"}},"required":["jql"]}"#,
        call: Rc::new(
            |args: String, cx: &mut App| -> Task<Result<String, String>> {
                let Some(jql) = arg(&args, "jql") else {
                    return Task::ready(Err("jql is missing".into()));
                };
                let max = serde_json::from_str::<Value>(&args)
                    .ok()
                    .and_then(|v| v["max"].as_u64())
                    .unwrap_or(20)
                    .min(100) as usize;
                let site = arg(&args, "site");
                crate::trackers::read_offered(cx);
                cx.background_spawn(async move {
                    let sites = sites_for(site)?;
                    let many = sites.len() > 1;
                    let (mut lines, mut unread) = (Vec::new(), Vec::new());
                    for site in &sites {
                        match model::search(site, &jql, max) {
                            Ok(rows) => lines.extend(rows.iter().map(|r| {
                                let who = r
                                    .assignee
                                    .as_ref()
                                    .map_or("unassigned", |p| p.name.as_str());
                                let at = if many {
                                    format!(" · {}", site.key())
                                } else {
                                    String::new()
                                };
                                format!(
                                    "{} {} ({} · {} · {who}{at})",
                                    r.key, r.summary, r.kind, r.state.name
                                )
                            })),
                            Err(why) => unread.push(why),
                        }
                    }
                    if lines.is_empty() && !unread.is_empty() {
                        return Err(unread.join("\n"));
                    }
                    if lines.is_empty() {
                        lines.push(format!("Nothing matches {jql}"));
                    }
                    lines.extend(unread.into_iter().map(|why| format!("Not searched: {why}")));
                    Ok(lines.join("\n"))
                })
            },
        ),
    };
    let comment = Tool {
        name: "jira.comment",
        description: "A comment for an issue of the team's Jira, kept in Artoo for the developer to read, edit and post; nothing reaches Jira until they post it",
        input: r#"{"type":"object","properties":{"key":{"type":"string","description":"The issue's key, PAY-412"},"body":{"type":"string","description":"The comment, in markdown"},"site":{"type":"string","description":"The tracker's host, jira.example.com, when more than one is connected"}},"required":["key","body"]}"#,
        call: Rc::new(
            |args: String, cx: &mut App| -> Task<Result<String, String>> {
                let (Some(key), Some(body)) = (arg(&args, "key"), arg(&args, "body")) else {
                    return Task::ready(Err("key and body are needed".into()));
                };
                let store = store::get(cx);
                let said = store.update(cx, |s, cx| {
                let site = arg(&args, "site").or_else(|| s.site_for(&key).map(|x| x.key().to_owned())).unwrap_or_default();
                let id = s.kept.next_id();
                s.kept.pending.push(crate::keep::Pending { id, key: key.clone(), site, body, agent: "Your agent".into(), at: store::now() });
                s.kept.write();
                cx.notify();
                format!("Kept for the developer on {key}: they read it on the issue's page and post it, edit it or discard it.")
            });
                Task::ready(Ok(said))
            },
        ),
    };
    let draft = Tool {
        name: "jira.draft",
        description: "A new issue for the team's Jira, kept as a draft in Artoo (Tasks › Drafts) for the developer to open, check and create; nothing reaches Jira until they create it",
        input: r#"{"type":"object","properties":{"project":{"type":"string","description":"The project's key, PAY"},"summary":{"type":"string","description":"The issue's summary, one line"},"description":{"type":"string","description":"Its description, in markdown"},"type":{"type":"string","description":"Its issue type by name, Task or Bug; the project's first when left out"},"site":{"type":"string","description":"The tracker's host, jira.example.com; every tracker connected when left out"}},"required":["project","summary"]}"#,
        call: Rc::new(
            |args: String, cx: &mut App| -> Task<Result<String, String>> {
                let (Some(project), Some(summary)) = (arg(&args, "project"), arg(&args, "summary"))
                else {
                    return Task::ready(Err("project and summary are needed".into()));
                };
                let (description, kind, site) = (
                    arg(&args, "description").unwrap_or_default(),
                    arg(&args, "type"),
                    arg(&args, "site"),
                );
                crate::trackers::read_offered(cx);
                let store = store::get(cx);
                // Where it goes, read as New task reads it: the site that has the project, the project's name, the
                // type's id.
                let place = cx.background_spawn(async move {
                    let sites = sites_for(site)?;
                    for site in &sites {
                        let Ok(projects) = model::projects(site) else {
                            continue;
                        };
                        let Some((key, name)) =
                            (projects.into_iter()).find(|(k, _)| k.eq_ignore_ascii_case(&project))
                        else {
                            continue;
                        };
                        let kinds = model::kinds(site, &key).unwrap_or_default();
                        let kind = kind_of(&kinds, kind.as_deref(), &key)?;
                        return Ok((site.key().to_owned(), format!("{key} {name}"), kind));
                    }
                    Err(format!(
                        "No tracker connected in Artoo has a project {project}"
                    ))
                });
                cx.spawn(async move |cx| {
                    let (host, project, kind) = place.await?;
                    let said = format!(
                        "Kept as a draft for the developer: {summary}, in {project}. They open it under Tasks › Drafts, check it and create it; nothing reaches the tracker before."
                    );
                    cx.update(|cx| {
                        store.update(cx, |s, cx| {
                            let id = s.kept.next_id();
                            let draft = agent_draft(id, host, project, kind, summary, description);
                            s.kept.drafts.insert(0, draft);
                            s.kept.write();
                            cx.notify();
                        })
                    });
                    Ok(said)
                })
            },
        ),
    };
    for tool in [issue, search, comment, draft] {
        if let Err(why) = tools::add(cx, tool) {
            eprintln!("artoo-tasks: {why}");
        }
    }
}

/// The type named `name` among a project's `kinds` (any case), else its first that is not a sub-task; none when the
/// project's types were not read (the sheet picks one as the draft opens). Named and not among them: why.
fn kind_of(
    kinds: &[model::Kind],
    name: Option<&str>,
    project: &str,
) -> Result<Option<model::Kind>, String> {
    let tops: Vec<&model::Kind> = kinds.iter().filter(|k| !k.subtask).collect();
    match name {
        Some(name) if !kinds.is_empty() => (tops.iter())
            .find(|k| k.name.eq_ignore_ascii_case(name))
            .map(|k| Some((*k).clone()))
            .ok_or_else(|| {
                let names: Vec<&str> = tops.iter().map(|k| k.name.as_str()).collect();
                format!(
                    "{project} has no issue type {name}; it has {}",
                    names.join(", ")
                )
            }),
        _ => Ok(tops.first().map(|k| (*k).clone())),
    }
}

/// A draft an agent wrote: issue `summary` in project `project` (`PAY Payments`) on site `host`, of type `kind`,
/// kept as New task's Save as draft keeps one, saying it is the agent's.
fn agent_draft(
    id: u64,
    host: String,
    project: String,
    kind: Option<model::Kind>,
    summary: String,
    description: String,
) -> crate::keep::Draft {
    crate::keep::Draft {
        id,
        site: host,
        project,
        kind: kind.map(|k| (k.id, k.name)).unwrap_or_default(),
        summary: summary.trim().to_owned(),
        description,
        fields: json!({}),
        words: String::new(),
        said: json!({}),
        at: store::now(),
        by: "Your agent".into(),
    }
}

/// A paragraph of an issue's description as a note's place: `{"key":"PAY-412","site":"jira.example.com","para":4}`.
pub fn paragraph_place(key: &str, site: &str, para: usize) -> String {
    json!({ "key": key, "site": site, "para": para }).to_string()
}

/// The board picked, as it is filtered, as a note's place:
/// `{"site":"jira.example.com","board":7,"name":"Payments","mine":true,"count":3}`.
pub fn board_place(board: &model::BoardId, name: &str, mine: bool, count: Option<usize>) -> String {
    let mut v = json!({ "site": board.0, "board": board.1, "name": name, "mine": mine });
    if let Some(n) = count {
        v["count"] = json!(n);
    }
    v.to_string()
}

/// A board's place in words: `Payments · Mine · 3 issues`.
pub fn board_words(name: &str, mine: bool, count: Option<usize>) -> String {
    let mut words = format!("{name} · {}", if mine { "Mine" } else { "All" });
    if let Some(n) = count {
        words.push_str(&format!(
            " · {n} {}",
            if n == 1 { "issue" } else { "issues" }
        ));
    }
    words
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
            let head = format!(
                "{} {} ({} · {})",
                key, issue.row.summary, issue.row.kind, issue.row.state.name
            );
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
            Some(issue_route(v["key"].as_str()?, v["site"].as_str()))
        }),
    };
    let board = Kind {
        kind: "jira.board",
        reads: Rc::new(|data: &str, _: &App| {
            let v: Value = serde_json::from_str(data).unwrap_or_default();
            let name = v["name"].as_str().unwrap_or("A board");
            board_words(
                name,
                v["mine"].as_bool() == Some(true),
                v["count"].as_u64().map(|n| n as usize),
            )
        }),
        digest: Rc::new(|data: &str, cx: &App| {
            let v: Value = serde_json::from_str(data).unwrap_or_default();
            let site = v["site"].as_str().unwrap_or_default().to_owned();
            let (board, mine) = (
                (
                    site,
                    (v["board"].as_str().map(str::to_owned))
                        .or_else(|| v["board"].as_u64().map(|n| n.to_string()))
                        .unwrap_or_default(),
                ),
                v["mine"].as_bool() == Some(true),
            );
            let store = store::peek(cx);
            let rows = store.as_ref().and_then(|s| {
                let s = s.read(cx);
                match s.rows.get(&board) {
                    Some(store::Read::Done(rows)) => Some(
                        rows.iter()
                            .filter(|r| !mine || s.is_me(&board.0, r.assignee.as_ref()))
                            .cloned()
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                }
            });
            let name = v["name"].as_str().unwrap_or("A board");
            let head = format!("The board {} on {}", board_words(name, mine, None), board.0);
            match rows {
                Some(rows) => {
                    let list = (rows.iter().take(40))
                        .map(|r| format!("{} {} ({})", r.key, r.summary, r.state.name))
                        .collect::<Vec<_>>()
                        .join("\n");
                    format!("{head}, its issues:\n\n{list}")
                }
                None => format!("{head}."),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_note_reads_as_the_board_is_filtered() {
        assert_eq!(
            board_words("Payments", true, Some(3)),
            "Payments · Mine · 3 issues"
        );
        assert_eq!(
            board_words("Payments", false, Some(1)),
            "Payments · All · 1 issue"
        );
        assert_eq!(board_words("Payments", false, None), "Payments · All");
        let place: Value = serde_json::from_str(&board_place(
            &("jira.example.com".into(), "7".into()),
            "Payments",
            true,
            Some(3),
        ))
        .unwrap();
        assert_eq!(
            place,
            json!({"site": "jira.example.com", "board": "7", "name": "Payments", "mine": true, "count": 3})
        );
    }

    #[test]
    fn an_agents_draft_is_kept_with_the_type_it_names_and_whose_it_is() {
        let kind = |id: &str, name: &str, subtask: bool| model::Kind {
            id: id.into(),
            name: name.into(),
            subtask,
        };
        let kinds = [
            kind("10003", "Sub-task", true),
            kind("10002", "Task", false),
            kind("10004", "Bug", false),
        ];
        assert_eq!(
            kind_of(&kinds, Some("bug"), "PAY").unwrap().unwrap().id,
            "10004"
        );
        assert_eq!(kind_of(&kinds, None, "PAY").unwrap().unwrap().id, "10002");
        assert_eq!(
            kind_of(&kinds, Some("Epic"), "PAY").unwrap_err(),
            "PAY has no issue type Epic; it has Task, Bug"
        );
        assert_eq!(kind_of(&[], Some("Bug"), "PAY"), Ok(None));
        let d = agent_draft(
            4,
            "acme.atlassian.net".into(),
            "PAY Payments".into(),
            Some(kinds[1].clone()),
            " Clear the remainders already booked ".into(),
            "The 0.01 remainders.".into(),
        );
        assert_eq!(d.kind, ("10002".to_owned(), "Task".to_owned()));
        assert_eq!(d.summary, "Clear the remainders already booked");
        assert_eq!(d.by, "Your agent");
    }
}
