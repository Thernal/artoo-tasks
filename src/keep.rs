//! What the plugin keeps on the Mac, in its folder's `kept.json`: the board opened last and those opened lately,
//! how each board is shown (List or Board, grouped by what), the drafts of issues not created yet, an agent's comments
//! waiting for Post, a description's edit left open. Written as it changes; nothing of it reaches the tracker.

use crate::model::BoardId;
use artoo_plugin_sdk::serde_json::{self, Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

/// What a board's issues are grouped by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Group {
    /// By the board's columns.
    #[default]
    State,
    /// A story with its sub-tasks under it.
    Story,
    /// By their epic, `No epic` last.
    Epic,
}

impl Group {
    pub fn word(self) -> &'static str {
        match self {
            Group::State => "State",
            Group::Story => "Story",
            Group::Epic => "Epic",
        }
    }

    fn read(word: &str) -> Self {
        match word {
            "Story" => Group::Story,
            "Epic" => Group::Epic,
            _ => Group::State,
        }
    }
}

/// How a board is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// Columns side by side, not the list.
    pub board: bool,
    pub group: Group,
}

/// An issue not created yet: what New task had when it was kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    pub id: u64,
    pub site: String,
    pub project: String,
    /// The kind's id and name.
    pub kind: (String, String),
    pub summary: String,
    pub description: String,
    /// The other fields as the create screen takes them, by id.
    pub fields: Value,
    /// Their words, for the draft's row: `High · Juniper Hale`.
    pub words: String,
    /// Each field's words by its id, `{"priority": "High"}`: what the preview and the sheet show of it again.
    pub said: Value,
    pub at: i64,
    /// Who kept it: empty for you, else the agent that wrote it (`Your agent`).
    pub by: String,
}

/// An agent's comment for an issue, seen by you alone until you post it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    pub id: u64,
    pub key: String,
    pub site: String,
    pub body: String,
    /// Who wrote it: `Claude`.
    pub agent: String,
    pub at: i64,
}

#[derive(Default)]
pub struct Kept {
    pub last_board: Option<BoardId>,
    pub recent: Vec<BoardId>,
    /// The board each project's space was last on, by the project's folder: two projects reading one account
    /// each keep their own.
    pub boards: HashMap<String, BoardId>,
    pub views: HashMap<BoardId, View>,
    pub drafts: Vec<Draft>,
    pub pending: Vec<Pending>,
    /// A description being edited, by issue: its text.
    pub edits: HashMap<String, String>,
}

/// The plugin's folder: `<data>/plugins/artoo-tasks`.
pub fn folder() -> Option<PathBuf> {
    Some(
        artoo_plugin_sdk::core::platform::data_dir()?
            .join("plugins")
            .join("artoo-tasks"),
    )
}

fn file() -> Option<PathBuf> {
    Some(folder()?.join("kept.json"))
}

fn board_id(v: &Value) -> Option<BoardId> {
    // A number as it was kept before ids were text.
    let id = (v["board"].as_str().map(str::to_owned))
        .or_else(|| v["board"].as_u64().map(|n| n.to_string()))?;
    Some((v["site"].as_str()?.to_owned(), id))
}

fn board_json(b: &BoardId) -> Value {
    json!({ "site": b.0, "board": b.1 })
}

impl Kept {
    pub fn read() -> Self {
        let v: Value = file()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self::from_json(&v)
    }

    fn from_json(v: &Value) -> Self {
        let list = |name: &str| v[name].as_array().cloned().unwrap_or_default();
        Kept {
            last_board: board_id(&v["last_board"]),
            recent: list("recent").iter().filter_map(board_id).collect(),
            boards: (list("projects").iter())
                .filter_map(|x| Some((x["project"].as_str()?.to_owned(), board_id(x)?)))
                .collect(),
            views: (list("views").iter())
                .filter_map(|x| {
                    let view = View {
                        board: x["board_view"].as_bool() == Some(true),
                        group: Group::read(x["group"].as_str()?),
                    };
                    Some((board_id(x)?, view))
                })
                .collect(),
            drafts: (list("drafts").iter())
                .map(|d| Draft {
                    id: d["id"].as_u64().unwrap_or_default(),
                    site: d["site"].as_str().unwrap_or_default().to_owned(),
                    project: d["project"].as_str().unwrap_or_default().to_owned(),
                    kind: (
                        d["kind_id"].as_str().unwrap_or_default().to_owned(),
                        d["kind"].as_str().unwrap_or_default().to_owned(),
                    ),
                    summary: d["summary"].as_str().unwrap_or_default().to_owned(),
                    description: d["description"].as_str().unwrap_or_default().to_owned(),
                    fields: d["fields"].clone(),
                    words: d["words"].as_str().unwrap_or_default().to_owned(),
                    said: d["said"].clone(),
                    at: d["at"].as_i64().unwrap_or_default(),
                    by: d["by"].as_str().unwrap_or_default().to_owned(),
                })
                .collect(),
            pending: (list("pending").iter())
                .map(|p| Pending {
                    id: p["id"].as_u64().unwrap_or_default(),
                    key: p["key"].as_str().unwrap_or_default().to_owned(),
                    site: p["site"].as_str().unwrap_or_default().to_owned(),
                    body: p["body"].as_str().unwrap_or_default().to_owned(),
                    agent: p["agent"].as_str().unwrap_or("Your agent").to_owned(),
                    at: p["at"].as_i64().unwrap_or_default(),
                })
                .collect(),
            edits: (v["edits"].as_object().into_iter().flatten())
                .filter_map(|(k, t)| Some((k.clone(), t.as_str()?.to_owned())))
                .collect(),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "last_board": self.last_board.as_ref().map(board_json),
            "recent": self.recent.iter().map(board_json).collect::<Vec<_>>(),
            "projects": self.boards.iter().map(|(p, b)| {
                let mut x = board_json(b);
                x["project"] = json!(p);
                x
            }).collect::<Vec<_>>(),
            "views": self.views.iter().map(|(b, v)| {
                let mut x = board_json(b);
                x["board_view"] = json!(v.board);
                x["group"] = json!(v.group.word());
                x
            }).collect::<Vec<_>>(),
            "drafts": self.drafts.iter().map(|d| json!({
                "id": d.id, "site": d.site, "project": d.project, "kind_id": d.kind.0, "kind": d.kind.1,
                "summary": d.summary, "description": d.description, "fields": d.fields, "words": d.words, "said": d.said, "at": d.at,
                "by": d.by,
            })).collect::<Vec<_>>(),
            "pending": self.pending.iter().map(|p| json!({
                "id": p.id, "key": p.key, "site": p.site, "body": p.body, "agent": p.agent, "at": p.at,
            })).collect::<Vec<_>>(),
            "edits": self.edits,
        })
    }

    /// Written to the plugin's folder.
    pub fn write(&self) {
        let Some(file) = file() else {
            return;
        };
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(
            file,
            serde_json::to_string_pretty(&self.to_json()).unwrap_or_default(),
        );
    }

    /// Board `id` opened, in project `project`'s space when one is shown: the last one, first among those opened
    /// lately, and that project's.
    pub fn opened_board(&mut self, id: BoardId, project: Option<&str>) {
        if let Some(project) = project {
            self.boards.insert(project.to_owned(), id.clone());
        }
        self.recent.retain(|b| *b != id);
        self.recent.insert(0, id.clone());
        self.recent.truncate(20);
        self.last_board = Some(id);
        self.write();
    }

    pub fn view(&self, id: &BoardId) -> View {
        self.views.get(id).copied().unwrap_or_default()
    }

    pub fn set_view(&mut self, id: BoardId, view: View) {
        self.views.insert(id, view);
        self.write();
    }

    /// A new id for a draft or a pending comment.
    pub fn next_id(&self) -> u64 {
        let used = self
            .drafts
            .iter()
            .map(|d| d.id)
            .chain(self.pending.iter().map(|p| p.id));
        used.max().unwrap_or(0) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_kept_reads_back_the_same() {
        let mut kept = Kept {
            recent: vec![("acme.atlassian.net".into(), "7".into())],
            last_board: Some(("acme.atlassian.net".into(), "7".into())),
            ..Default::default()
        };
        kept.boards
            .insert("/w/pay".into(), ("acme.atlassian.net".into(), "7".into()));
        kept.views.insert(
            ("acme.atlassian.net".into(), "7".into()),
            View {
                board: true,
                group: Group::Epic,
            },
        );
        kept.drafts.push(Draft {
            id: 1,
            site: "acme.atlassian.net".into(),
            project: "PAY".into(),
            kind: ("10002".into(), "Task".into()),
            summary: "Clear the remainders".into(),
            fields: json!({"priority": {"id": "2"}}),
            said: json!({"priority": "High"}),
            by: "Your agent".into(),
            ..Default::default()
        });
        kept.pending.push(Pending {
            id: 2,
            key: "PAY-412".into(),
            body: "Fixed in !412".into(),
            ..Default::default()
        });
        kept.edits.insert("PAY-412".into(), "A new text".into());
        let back = Kept::from_json(&kept.to_json());
        assert_eq!(back.recent, kept.recent);
        assert_eq!(back.last_board, kept.last_board);
        assert_eq!(back.boards, kept.boards);
        assert_eq!(
            back.view(&("acme.atlassian.net".into(), "7".into())),
            View {
                board: true,
                group: Group::Epic
            }
        );
        assert_eq!(back.drafts, kept.drafts);
        assert_eq!(back.pending[0].body, "Fixed in !412");
        assert_eq!(back.edits["PAY-412"], "A new text");
        assert_eq!(back.next_id(), 3);
    }
}
