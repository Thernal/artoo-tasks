//! What Tasks reads of a tracker, as plain types: a board and its columns, an issue in a list and whole, the moves
//! its workflow allows, the fields its screens hold, the news of it.
//!
//! The words are Jira's, since Jira came first: a screen's field ids and values are as Jira's REST takes them
//! (`summary`, `{"priority": {"name": "High"}}`, `duedate`), a description and a comment are Jira's wiki markup
//! (`crate::wiki` turns markdown into it and back), a state's category is `new`, `indeterminate` or `done`. Another
//! tracker maps its own onto them, as Trello's lists are states and its cards issues.

use artoo_plugin_sdk::serde::{Deserialize, Serialize};
use artoo_plugin_sdk::serde_json::Value;

/// A board as Tasks keys it: its site's host and its id there — two sites may each have a board 7.
pub type BoardId = (String, String);

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Board {
    pub id: String,
    pub name: String,
    /// The site it is on (its host); Tasks fills it in.
    pub site: String,
    /// Its project's key (`PAY`), when the board is a project's.
    pub project: Option<String>,
}

impl Board {
    pub fn key(&self) -> BoardId {
        (self.site.clone(), self.id.clone())
    }
}

/// Someone on the tracker: the id it writes them by (Jira Cloud's account id, Server's user name, Trello's user
/// name), and the name shown.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Person {
    pub id: String,
    pub name: String,
}

impl Person {
    /// The first name: `Juniper` of `Juniper Hale`.
    pub fn first(&self) -> &str {
        self.name.split_whitespace().next().unwrap_or(&self.name)
    }
}

/// A workflow's state: its id, its name, its category (`new`, `indeterminate`, `done`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct State {
    pub id: String,
    pub name: String,
    pub category: String,
}

impl State {
    pub fn done(&self) -> bool {
        self.category == "done"
    }
}

/// An epic: its key, its name, its colour on the tracker's palette (1–14; 0 none said) and whether it is done.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Epic {
    pub key: String,
    pub name: String,
    pub color: u8,
    pub done: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Sprint {
    pub id: u64,
    pub name: String,
    /// `active`, `future`, `closed`.
    pub state: String,
    /// When it ends: `2026-11-14T17:00:00.000Z`.
    pub end: Option<String>,
}

/// Another issue, as a link or a sub-task's parent names it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Brief {
    pub key: String,
    pub summary: String,
    pub kind: String,
    pub state: State,
}

/// A link to another issue: how this one relates to it (`is blocked by`), the link's type and id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Link {
    pub id: String,
    /// The type's name: `Blocks`.
    pub kind: String,
    /// The relation read from this issue: `is blocked by`, `blocks`, `relates to`.
    pub relation: String,
    pub other: Brief,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mime: String,
    pub author: Option<Person>,
    pub created: String,
    /// Where its bytes are, read with the site's token (`Tracker::file`).
    pub content: String,
}

impl Attachment {
    pub fn image(&self) -> bool {
        self.mime.starts_with("image/")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Comment {
    pub id: String,
    pub author: Option<Person>,
    pub created: String,
    pub updated: String,
    /// Jira's wiki markup.
    pub body: String,
}

/// An issue as a list shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Row {
    pub key: String,
    pub summary: String,
    pub kind: String,
    /// A sub-task's type.
    pub subtask: bool,
    pub state: State,
    pub assignee: Option<Person>,
    pub priority: Option<String>,
    /// When it last changed: `2026-11-12T10:00:00.000+0000`.
    pub updated: String,
    /// A sub-task's parent issue (not an epic).
    pub parent: Option<String>,
    pub epic: Option<Epic>,
    pub sprint: Option<Sprint>,
    /// Its sub-tasks: how many done, of how many.
    pub subtasks: (usize, usize),
    pub comments: usize,
    /// The open issues it is blocked by.
    pub blocked_by: Vec<Brief>,
    pub labels: Vec<String>,
    /// Its project's key: `PAY`.
    pub project: String,
}

/// An issue whole, as its page shows it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Issue {
    pub row: Row,
    /// The site it is on (its host); Tasks fills it in.
    pub site: String,
    /// Jira's wiki markup.
    pub description: String,
    pub reporter: Option<Person>,
    pub created: String,
    pub due: Option<String>,
    pub subtasks: Vec<Row>,
    pub comments: Vec<Comment>,
    pub links: Vec<Link>,
    pub attachments: Vec<Attachment>,
    /// Its page on the tracker.
    pub url: String,
    /// Every field by its id, as the edit screen's fields name them: what Details shows of each.
    pub fields: Value,
}

/// A move the workflow allows from an issue's state, and the fields it asks for (Done's resolution).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Move {
    pub id: String,
    pub name: String,
    pub to: State,
    pub asks: Vec<Field>,
}

/// A field of an edit or create screen: its id, its name, whether it must be filled, its kind and the values it
/// takes when they are a list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Field {
    pub id: String,
    pub name: String,
    pub required: bool,
    /// The schema's type: `string`, `array`, `priority`, `user`, `date`, `number`, `option`, `version`, `component`.
    pub kind: String,
    /// An array's items' type: `string` (labels), `component`, `version`, `option`, `json` (sprint).
    pub items: String,
    /// The values it takes, `(id, name)`; empty when any.
    pub values: Vec<(String, String)>,
    /// The value the screen fills in when none is picked, `(id, name)`: a priority's `Medium`.
    pub default: Option<(String, String)>,
}

impl Field {
    /// Many values (labels, components, fix versions) or one.
    pub fn many(&self) -> bool {
        self.kind == "array"
    }
}

/// A board's columns, each with the states it holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Column {
    pub name: String,
    /// Its states' ids.
    pub states: Vec<String>,
}

/// What a board says beside its issues: its columns, its epics, its sprints.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct BoardInfo {
    pub columns: Vec<Column>,
    pub epics: Vec<Epic>,
    pub sprints: Vec<Sprint>,
    /// The epics could not be read this time: the empty list is not the board's, and what was read before holds.
    pub epics_unread: bool,
    /// As `epics_unread`, for the sprints.
    pub sprints_unread: bool,
}

impl BoardInfo {
    /// The sprint going on.
    pub fn active(&self) -> Option<&Sprint> {
        self.sprints.iter().find(|s| s.state == "active")
    }

    /// The column state `id` is in, by its place; the last when none holds it.
    pub fn column_of(&self, state: &State) -> usize {
        (self.columns.iter())
            .position(|c| c.states.contains(&state.id))
            .unwrap_or(self.columns.len().saturating_sub(1))
    }
}

/// A type an issue is made as, in a project.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Kind {
    pub id: String,
    pub name: String,
    pub subtask: bool,
}

/// What news is about: an issue assigned to you, you mentioned, an issue of yours changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde")]
pub enum Told {
    Assigned,
    Mentioned,
    #[default]
    Changed,
}

/// Something someone else did lately on an issue you are on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct News {
    pub told: Told,
    /// The change's or the comment's id: the same news, the same id.
    pub id: String,
    pub row: Row,
    /// Who did it.
    pub who: String,
    /// What they did, them first: `Juniper Hale assigned it to you`.
    pub says: String,
    /// The start of what they wrote; empty: nothing written.
    pub text: String,
    /// When, Unix seconds.
    pub at: i64,
    /// The assignee before, when that changed.
    pub was: Option<String>,
}
