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
    /// Its kind, as Jira names it: `scrum`, `kanban`, `simple` (team-managed); empty when the tracker says none.
    pub kind: String,
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
    /// When it starts, as `end` is written; none for a future sprint not given dates.
    pub start: Option<String>,
    /// What it is for, as its team wrote it; empty when none.
    pub goal: String,
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

impl Row {
    /// Whether the filter's `words` match it: each in its key, its summary or a label; `label:x` a label alone.
    pub fn matches(&self, words: &str) -> bool {
        words.split_whitespace().all(|w| {
            let w = w.to_lowercase();
            if let Some(label) = w.strip_prefix("label:") {
                return self.labels.iter().any(|l| l.to_lowercase() == label);
            }
            self.key.to_lowercase().contains(&w)
                || self.summary.to_lowercase().contains(&w)
                || self.labels.iter().any(|l| l.to_lowercase().contains(&w))
        })
    }
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
    /// The board has no sprints (a kanban board, or a team-managed one with sprints off): it is read whole, and no
    /// sprint is offered. An older build leaves it false and is read as before.
    pub sprintless: bool,
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

/// What a board's slice is asked for: an intent, as a write is, which each tracker turns into its own query.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Ask {
    /// Which slice: `sprint:<id>`, `backlog`, `board` (a board without sprints, whole) or `epic:<key>`.
    pub slice: String,
    /// The first issue wanted, counted from 0 in the slice's order.
    pub start: usize,
    /// How many issues at most; 0 asks for the count alone.
    pub max: usize,
    /// The filter's words, as `Row::matches` reads them.
    pub words: String,
    /// Only those given to the token's person.
    pub mine: bool,
    /// Only those of this epic (its key).
    pub epic: Option<String>,
}

impl Ask {
    pub fn sprint(id: u64) -> Self {
        Self::of(format!("sprint:{id}"))
    }

    pub fn backlog() -> Self {
        Self::of("backlog".into())
    }

    pub fn board() -> Self {
        Self::of("board".into())
    }

    pub fn epic(key: &str) -> Self {
        Self::of(format!("epic:{key}"))
    }

    fn of(slice: String) -> Self {
        Self {
            slice,
            ..Default::default()
        }
    }

    /// The sprint asked for, when a sprint is.
    pub fn sprint_id(&self) -> Option<u64> {
        self.slice.strip_prefix("sprint:")?.parse().ok()
    }

    /// The epic whose issues are the slice, when an epic's are.
    pub fn epic_key(&self) -> Option<&str> {
        self.slice.strip_prefix("epic:")
    }

    /// Whether row `r` is in the slice and passes its filters, `me` the token's person.
    pub fn holds(&self, r: &Row, me: Option<&Person>) -> bool {
        let sprint = r.sprint.as_ref().map(|s| s.id);
        let epic = r.epic.as_ref().map(|e| e.key.as_str());
        let within = match self.slice.as_str() {
            "backlog" => sprint.is_none(),
            "board" => true,
            _ => match (self.sprint_id(), self.epic_key()) {
                (Some(id), _) => sprint == Some(id),
                (_, Some(key)) => epic == Some(key),
                _ => false,
            },
        };
        let mine =
            !self.mine || me.is_some_and(|me| r.assignee.as_ref().is_some_and(|p| p.id == me.id));
        within
            && mine
            && self.epic.as_deref().is_none_or(|key| epic == Some(key))
            && r.matches(&self.words)
    }
}

/// A slice's issues from `Ask::start` on, and how many it holds in all (its filters applied).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Page {
    pub rows: Vec<Row>,
    pub total: usize,
}

impl Page {
    /// The page `ask` wants of `rows`, those it holds counted and paged; `me` the token's person.
    pub fn of(rows: Vec<Row>, ask: &Ask, me: Option<&Person>) -> Self {
        let held: Vec<Row> = rows.into_iter().filter(|r| ask.holds(r, me)).collect();
        let total = held.len();
        let rows = held.into_iter().skip(ask.start).take(ask.max).collect();
        Self { rows, total }
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

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_plugin_sdk::serde_json::{self, json};

    fn row(key: &str, sprint: Option<u64>, epic: Option<&str>, who: Option<&str>) -> Row {
        Row {
            key: key.into(),
            summary: format!("Settle {key}"),
            sprint: sprint.map(|id| Sprint {
                id,
                ..Default::default()
            }),
            epic: epic.map(|key| Epic {
                key: key.into(),
                ..Default::default()
            }),
            assignee: who.map(|id| Person {
                id: id.into(),
                name: id.into(),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn an_older_build_answer_reads_without_the_new_fields() {
        let sprint: Sprint =
            serde_json::from_value(json!({ "id": 7, "name": "Sprint 7", "state": "active" }))
                .unwrap();
        assert_eq!((sprint.start, sprint.goal.as_str()), (None, ""));
        let info: BoardInfo =
            serde_json::from_value(json!({ "columns": [], "sprints": [] })).unwrap();
        assert!(!info.sprintless);
        let board: Board =
            serde_json::from_value(json!({ "id": "7", "name": "Payments" })).unwrap();
        assert_eq!(board.kind, "");
        let ask: Ask = serde_json::from_value(json!({ "slice": "backlog" })).unwrap();
        assert_eq!(ask, Ask::backlog());
        let page: Page = serde_json::from_value(json!({ "rows": [] })).unwrap();
        assert_eq!(page.total, 0);
    }

    #[test]
    fn a_slice_is_its_rows_paged_and_counted() {
        let rows = vec![
            row("PAY-1", Some(7), Some("PAY-90"), Some("kestrel")),
            row("PAY-2", Some(7), None, None),
            row("PAY-3", Some(8), Some("PAY-90"), Some("kestrel")),
            row("PAY-4", None, Some("PAY-90"), None),
            row("PAY-5", Some(7), Some("PAY-91"), Some("kestrel")),
        ];
        let keys = |p: Page| {
            (
                p.rows.iter().map(|r| r.key.clone()).collect::<Vec<_>>(),
                p.total,
            )
        };
        let sprint = Ask {
            max: 2,
            ..Ask::sprint(7)
        };
        assert_eq!(
            keys(Page::of(rows.clone(), &sprint, None)),
            (vec!["PAY-1".into(), "PAY-2".into()], 3)
        );
        let next = Ask {
            start: 2,
            ..sprint.clone()
        };
        assert_eq!(
            keys(Page::of(rows.clone(), &next, None)),
            (vec!["PAY-5".into()], 3)
        );
        let count = Ask { max: 0, ..sprint };
        assert_eq!(keys(Page::of(rows.clone(), &count, None)), (vec![], 3));
        let all = |ask: Ask| Ask { max: 50, ..ask };
        assert_eq!(Page::of(rows.clone(), &all(Ask::backlog()), None).total, 1);
        assert_eq!(Page::of(rows.clone(), &all(Ask::board()), None).total, 5);
        assert_eq!(
            Page::of(rows.clone(), &all(Ask::epic("PAY-90")), None).total,
            3
        );
        let me = Person {
            id: "kestrel".into(),
            name: "Kestrel".into(),
        };
        let mine = all(Ask {
            mine: true,
            ..Ask::sprint(7)
        });
        assert_eq!(Page::of(rows.clone(), &mine, Some(&me)).total, 2);
        assert_eq!(Page::of(rows.clone(), &mine, None).total, 0);
        let epic = all(Ask {
            epic: Some("PAY-91".into()),
            ..Ask::sprint(7)
        });
        assert_eq!(
            keys(Page::of(rows.clone(), &epic, None)),
            (vec!["PAY-5".into()], 1)
        );
        let words = all(Ask {
            words: "pay-2".into(),
            ..Ask::sprint(7)
        });
        assert_eq!(
            keys(Page::of(rows.clone(), &words, None)),
            (vec!["PAY-2".into()], 1)
        );
        assert_eq!(Page::of(rows, &all(Ask::of("odd".into())), None).total, 0);
    }

    #[test]
    fn a_slice_names_its_sprint_or_epic() {
        assert_eq!(Ask::sprint(35).sprint_id(), Some(35));
        assert_eq!(Ask::epic("PAY-90").epic_key(), Some("PAY-90"));
        assert_eq!(
            (Ask::backlog().sprint_id(), Ask::backlog().epic_key()),
            (None, None)
        );
    }

    #[test]
    fn words_match_a_key_a_summary_or_a_label() {
        let mut r = row("PAY-412", None, None, None);
        r.labels = vec!["refunds".into()];
        assert!(r.matches("412") && r.matches("settle label:refunds"));
        assert!(!r.matches("label:ledger") && !r.matches("refund ledger"));
    }
}
