//! A tracker of its own, added to Tasks by this plugin: Sample Desk, on `desk.example.org`, a token alone signing in.
//! Its one project's issues live in memory, so a move, an assignee, an edit, a comment or an issue made is seen at
//! once, and gone when Artoo quits. A company's plugin has this shape, its desk a server it asks instead.
//!
//! Tasks shows its sites once one is named: Settings → Accounts → Add a tracker → `desk.example.org` and any token
//! (or a line `https://desk.example.org` in a fixture's `trackers.txt`).

use artoo_plugin_sdk::gpui::App;
use artoo_plugin_sdk::serde_json::{Value, json};
use artoo_plugin_sdk::{self as sdk, Meta};
use artoo_tasks_api::{
    self as tasks, Board, BoardInfo, Caps, Column, Comment, Field, Issue, Kind, Move, Person, Row,
    Site, State, Tracker, TrackerKind, Write,
};
use std::sync::Mutex;

sdk::plugin!(
    Meta {
        id: env!("CARGO_PKG_NAME"),
        name: "Sample Desk",
        version: env!("CARGO_PKG_VERSION"),
        repository: env!("CARGO_PKG_REPOSITORY"),
        provides: &["tasks.tracker.sample@1"],
        requires: &[],
    },
    install
);

/// Offers the kind: Tasks reads it when its space opens, before or after this plugin installs.
fn install(cx: &mut App) {
    let kind = TrackerKind {
        id: "sample".into(),
        name: "Sample Desk".into(),
        hosts: vec!["desk.example.org".into()],
        user: None,
        token_page: Some((
            "Any token will do".into(),
            "https://desk.example.org/tokens".into(),
        )),
        unit: "project".into(),
        caps: Caps {
            priority: true,
            project_keys: true,
            ..Caps::NONE
        },
    };
    tasks::offer(cx, kind, |site| Box::new(Desk { site }) as Box<dyn Tracker>);
}

const PROJECT: &str = "DESK";

fn state(id: &str) -> State {
    let (name, category) = match id {
        "open" => ("Open", "new"),
        "doing" => ("In progress", "indeterminate"),
        _ => ("Solved", "done"),
    };
    State {
        id: id.into(),
        name: name.into(),
        category: category.into(),
    }
}

fn person(id: &str) -> Person {
    let name = match id {
        "rmora" => "Rowan Mora",
        "tbeck" => "Tamsin Beck",
        _ => "You",
    };
    Person {
        id: id.into(),
        name: name.into(),
    }
}

/// An issue in memory: its row, what it says, its comments.
#[derive(Clone)]
struct Ticket {
    row: Row,
    description: String,
    comments: Vec<Comment>,
}

/// The desk: its tickets, the next number.
struct Tickets {
    all: Vec<Ticket>,
    next: usize,
}

static TICKETS: Mutex<Option<Tickets>> = Mutex::new(None);

/// The desk as it starts: five tickets, in each state.
fn seed() -> Tickets {
    let t = |n: usize,
             summary: &str,
             at: &str,
             who: Option<&str>,
             priority: &str,
             updated: &str| Ticket {
        row: Row {
            key: format!("{PROJECT}-{n}"),
            summary: summary.into(),
            kind: "Ticket".into(),
            state: state(at),
            assignee: who.map(person),
            priority: Some(priority.into()),
            updated: updated.into(),
            project: PROJECT.into(),
            ..Default::default()
        },
        description: format!("Reported through the desk.\n\n*{summary}*: seen by two customers."),
        comments: Vec::new(),
    };
    Tickets {
        all: vec![
            t(
                1,
                "Invoice PDF shows the old address",
                "open",
                None,
                "High",
                "2026-11-13T09:10:00.000Z",
            ),
            t(
                2,
                "Password reset mail arrives late",
                "doing",
                Some("rmora"),
                "Medium",
                "2026-11-13T08:40:00.000Z",
            ),
            t(
                3,
                "Export to CSV drops the last row",
                "doing",
                Some("me"),
                "High",
                "2026-11-12T16:05:00.000Z",
            ),
            t(
                4,
                "Dark mode logo is unreadable",
                "open",
                Some("tbeck"),
                "Low",
                "2026-11-12T11:30:00.000Z",
            ),
            t(
                5,
                "Refund note missing on receipts",
                "solved",
                Some("me"),
                "Medium",
                "2026-11-11T14:00:00.000Z",
            ),
        ],
        next: 6,
    }
}

fn with<T>(f: impl FnOnce(&mut Tickets) -> T) -> T {
    let mut held = TICKETS.lock().unwrap_or_else(|e| e.into_inner());
    f(held.get_or_insert_with(seed))
}

/// Sample Desk opened on a site: every site reads the one desk in memory.
struct Desk {
    site: Site,
}

impl Desk {
    fn ticket(&self, key: &str) -> Result<Ticket, String> {
        with(|d| d.all.iter().find(|t| t.row.key == key).cloned())
            .ok_or_else(|| format!("{} has no {key}", self.site.host()))
    }

    fn change(&self, key: &str, f: impl FnOnce(&mut Ticket)) -> Result<Value, String> {
        with(|d| {
            let t = (d.all.iter_mut())
                .find(|t| t.row.key == key)
                .ok_or_else(|| format!("{} has no {key}", self.site.host()))?;
            f(t);
            t.row.updated = "2026-11-13T12:00:00.000Z".into();
            Ok(Value::Null)
        })
    }
}

impl Tracker for Desk {
    fn myself(&self) -> Result<Person, String> {
        Ok(person("me"))
    }

    fn boards(&self) -> Result<Vec<Board>, String> {
        Ok(vec![Board {
            id: "support".into(),
            name: "Support desk".into(),
            project: Some(PROJECT.into()),
            ..Default::default()
        }])
    }

    fn board_info(&self, _: &str) -> Result<BoardInfo, String> {
        let column = |name: &str, id: &str| Column {
            name: name.into(),
            states: vec![id.into()],
        };
        Ok(BoardInfo {
            columns: vec![
                column("Open", "open"),
                column("In progress", "doing"),
                column("Solved", "solved"),
            ],
            ..Default::default()
        })
    }

    fn board_issues(&self, _: &str) -> Result<Vec<Row>, String> {
        let mut rows: Vec<Row> = with(|d| d.all.iter().map(|t| t.row.clone()).collect());
        rows.sort_by(|a, b| b.updated.cmp(&a.updated));
        Ok(rows)
    }

    fn board_url(&self, board: &str) -> String {
        format!("{}/boards/{board}", self.site.url)
    }

    fn issue(&self, key: &str) -> Result<Issue, String> {
        let t = self.ticket(key)?;
        Ok(Issue {
            url: format!("{}/tickets/{key}", self.site.url),
            reporter: Some(person("tbeck")),
            created: "2026-11-10T10:00:00.000Z".into(),
            description: t.description,
            fields: json!({ "priority": { "name": t.row.priority } }),
            row: Row {
                comments: t.comments.len(),
                ..t.row
            },
            comments: t.comments,
            ..Default::default()
        })
    }

    fn find(&self, words: &str, max: usize) -> Result<Vec<Row>, String> {
        let words = words.to_lowercase();
        Ok(with(|d| {
            (d.all.iter())
                .filter(|t| {
                    format!("{} {}", t.row.key, t.row.summary)
                        .to_lowercase()
                        .contains(&words)
                })
                .take(max)
                .map(|t| t.row.clone())
                .collect()
        }))
    }

    fn moves(&self, key: &str) -> Result<Vec<Move>, String> {
        let here = self.ticket(key)?.row.state.id;
        Ok(["open", "doing", "solved"]
            .into_iter()
            .filter(|id| *id != here)
            .map(|id| Move {
                id: id.into(),
                name: state(id).name,
                to: state(id),
                asks: Vec::new(),
            })
            .collect())
    }

    fn edit_screen(&self, _: &str) -> Result<Vec<Field>, String> {
        Ok(vec![Field {
            id: "priority".into(),
            name: "Priority".into(),
            kind: "priority".into(),
            values: ["High", "Medium", "Low"]
                .map(|p| (p.to_owned(), p.to_owned()))
                .to_vec(),
            ..Default::default()
        }])
    }

    fn assignable(&self, _: &str, query: &str) -> Result<Vec<Person>, String> {
        let query = query.to_lowercase();
        Ok(["me", "rmora", "tbeck"]
            .map(person)
            .into_iter()
            .filter(|p| p.name.to_lowercase().contains(&query))
            .collect())
    }

    fn projects(&self) -> Result<Vec<(String, String)>, String> {
        Ok(vec![(PROJECT.into(), "Support desk".into())])
    }

    fn kinds(&self, _: &str) -> Result<Vec<Kind>, String> {
        Ok(vec![Kind {
            id: "ticket".into(),
            name: "Ticket".into(),
            subtask: false,
        }])
    }

    fn create_screen(&self, _: &str, _: &str) -> Result<Vec<Field>, String> {
        self.edit_screen("")
    }

    fn file(&self, url: &str) -> Result<Vec<u8>, String> {
        Err(format!("{} keeps no files: {url}", self.site.host()))
    }

    fn write(&self, w: &Write) -> Result<Value, String> {
        match w {
            Write::Move { key, step, .. } => self.change(key, |t| t.row.state = state(step)),
            Write::Assign { key, who } => {
                self.change(key, |t| t.row.assignee = who.as_deref().map(person))
            }
            Write::Edit { key, fields } => {
                let fields = fields.clone();
                self.change(key, move |t| {
                    if let Some(s) = fields["summary"].as_str() {
                        t.row.summary = s.into();
                    }
                    if let Some(d) = fields["description"].as_str() {
                        t.description = d.into();
                    }
                    if let Some(p) = fields["priority"]["name"]
                        .as_str()
                        .or(fields["priority"]["id"].as_str())
                    {
                        t.row.priority = Some(p.into());
                    }
                })
            }
            Write::Comment { key, body } => {
                let body = body.clone();
                self.change(key, move |t| {
                    let id = (t.comments.len() + 1).to_string();
                    t.comments.push(Comment {
                        id,
                        author: Some(person("me")),
                        created: "2026-11-13T12:00:00.000Z".into(),
                        updated: "2026-11-13T12:00:00.000Z".into(),
                        body,
                    });
                })
            }
            Write::EditComment { key, id, body } => {
                let body = body.clone();
                self.change(key, move |t| {
                    if let Some(c) = t.comments.iter_mut().find(|c| &c.id == id) {
                        c.body = body;
                    }
                })
            }
            Write::DeleteComment { key, id } => {
                self.change(key, |t| t.comments.retain(|c| &c.id != id))
            }
            Write::Create { fields } => with(|d| {
                let key = format!("{PROJECT}-{}", d.next);
                d.next += 1;
                let text = |k: &str| fields[k].as_str().unwrap_or_default().to_owned();
                d.all.push(Ticket {
                    row: Row {
                        key: key.clone(),
                        summary: text("summary"),
                        kind: "Ticket".into(),
                        state: state("open"),
                        assignee: fields["assignee"]["name"].as_str().map(person),
                        priority: fields["priority"]["name"].as_str().map(str::to_owned),
                        updated: "2026-11-13T12:00:00.000Z".into(),
                        project: PROJECT.into(),
                        ..Default::default()
                    },
                    description: text("description"),
                    comments: Vec::new(),
                });
                Ok(json!({ "key": key }))
            }),
            Write::Epic { .. } => Err("Sample Desk has no epics".into()),
            Write::Sprint { .. } => Err("Sample Desk has no sprints".into()),
            Write::Link { .. } => Err("Sample Desk's tickets are not linked".into()),
            Write::Change { .. } => Err("Sample Desk has no labels".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_tasks_api::wiki;

    #[test]
    fn a_ticket_moves_and_is_commented_on() {
        let desk = Desk {
            site: Site {
                url: "https://desk.example.org".into(),
                ..Default::default()
            },
        };
        desk.write(&Write::moved("DESK-1", "doing", json!({})))
            .unwrap();
        desk.write(&Write::commented("DESK-1", "On it")).unwrap();
        let issue = desk.issue("DESK-1").unwrap();
        assert_eq!(issue.row.state.name, "In progress");
        assert_eq!(wiki::to_markdown(&issue.comments[0].body), "On it");
        assert_eq!(
            desk.write(&Write::in_sprint("DESK-1", Some(3))),
            Err("Sample Desk has no sprints".into())
        );
    }
}
