//! A tracker: the site it is on and how it signs in, its kind (what it is called, the hosts it is on, what it has),
//! and what Tasks asks of it — the `Tracker` trait — and writes to it, each change an intent (`Write`).

use crate::model::{
    Ask, Board, BoardInfo, Field, Issue, Kind, Move, News, Page, Person, Row, Sprint,
};
use artoo_plugin_sdk::serde::{Deserialize, Serialize};
use artoo_plugin_sdk::serde_json::Value;
use std::path::Path;

/// A site Tasks reads: where it is, how it signs in, which kind of tracker it is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Site {
    /// `https://jira.example.com`, no slash at the end.
    pub url: String,
    /// The sign-in's first part, when the kind asks one (`TrackerKind::user`): Jira Cloud's email, Trello's API key.
    pub user: Option<String>,
    /// The token: `~/.config/tokens/<host>`.
    pub token: String,
    /// Its kind's id: `jira`, `trello`, the id a plugin's kind has.
    pub kind: String,
    /// The name the person gave the account, to tell two on one site apart; empty when none.
    pub label: String,
    /// What tells the account from another on the same site: its host for the first, `host~name` for a later one;
    /// empty reads as the host. Boards and issues carry it as their `site`.
    pub id: String,
}

impl Site {
    /// The account's key: its id, else its host.
    pub fn key(&self) -> &str {
        match self.id.is_empty() {
            true => self.host(),
            false => &self.id,
        }
    }

    /// Its host: `jira.example.com`.
    pub fn host(&self) -> &str {
        let rest = (self.url.split_once("://")).map_or(self.url.as_str(), |(_, r)| r);
        rest.split('/').next().unwrap_or(rest)
    }
}

/// What a tracker has, beyond boards, issues, states, assignees, comments and descriptions every one has: what Tasks
/// shows and offers to change. One it lacks is not drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct Caps {
    /// An issue's priority.
    pub priority: bool,
    /// An issue's type, picked when it is made (`Bug`, `Story`).
    pub kinds: bool,
    /// Sprints, and an issue's.
    pub sprints: bool,
    /// Sub-tasks.
    pub subtasks: bool,
    /// Labels, changed from Tasks.
    pub labels: bool,
    /// A project's key shown beside its name (`PAY`), as its issues' keys start with it.
    pub project_keys: bool,
    /// A workflow that allows some moves and not others, drawn over the page (View workflow…).
    pub workflow: bool,
    /// Links between issues (`blocks`, `relates to`): an issue's Linked issues, shown while it has none too.
    pub links: bool,
    /// Files attached from Tasks (`Tracker::attach`): an issue's Attachments, shown while it has none too.
    pub attachments: bool,
}

impl Caps {
    /// Jira's: everything.
    pub const ALL: Caps = Caps {
        priority: true,
        kinds: true,
        sprints: true,
        subtasks: true,
        labels: true,
        project_keys: true,
        workflow: true,
        links: true,
        attachments: true,
    };
    /// None of them.
    pub const NONE: Caps = Caps {
        priority: false,
        kinds: false,
        sprints: false,
        subtasks: false,
        labels: false,
        project_keys: false,
        workflow: false,
        links: false,
        attachments: false,
    };
}

impl Default for Caps {
    fn default() -> Self {
        Self::NONE
    }
}

/// A kind of tracker: what it is called, the hosts its sites are on, what its sign-in asks, what it has.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde", default)]
pub struct TrackerKind {
    /// Its id, a word: `acme`. The site's `kind`; the key it is offered under (`crate::key_of`).
    pub id: String,
    /// What it is called: `Acme Desk` (`Open in Acme Desk`).
    pub name: String,
    /// The hosts its sites are on: `desk.acme.example`, or `*.acme.example` for every one under it. A site on
    /// none of any kind's is Jira's.
    pub hosts: Vec<String>,
    /// The sign-in's first part, before the token, when it has one: its label and its hint (`("Email", "The email
    /// the token is of")`). None: the token alone signs in.
    pub user: Option<(String, String)>,
    /// Where a token is made: the link's words and its address.
    pub token_page: Option<(String, String)>,
    /// What a project is called on it, a word: `project`, `board`.
    pub unit: String,
    pub caps: Caps,
}

impl TrackerKind {
    /// Whether a site on `host` is of this kind.
    pub fn claims(&self, host: &str) -> bool {
        (self.hosts.iter()).any(|h| match h.strip_prefix("*.") {
            Some(domain) => host == domain || host.ends_with(&format!(".{domain}")),
            None => h == host,
        })
    }
}

/// A change to the tracker, as Tasks asks it; the tracker's answer is JSON, `null` when it has none to give. Field
/// ids and values are a screen's (`Field`), text is Jira's wiki markup.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(crate = "artoo_plugin_sdk::serde")]
pub enum Write {
    /// Issue `key` moved by `step` (a move's id), with what the move asked (`{"resolution": {"name": "Done"}}`).
    Move {
        key: String,
        step: String,
        asked: Value,
    },
    /// Issue `key` given to `who` (a person's id), or to no one.
    Assign {
        key: String,
        who: Option<String>,
    },
    /// Fields of issue `key` set: `{"summary": "…"}`, `{"description": "…"}`, `{"priority": {"name": "High"}}`,
    /// `{"duedate": "2026-11-20"}`.
    Edit {
        key: String,
        fields: Value,
    },
    /// A value added to or taken from issue `key`'s field of many (`labels`, `components` by name).
    Change {
        key: String,
        field: String,
        value: Value,
        add: bool,
    },
    /// Issue `key` put in epic `epic`, or in none.
    Epic {
        key: String,
        epic: Option<String>,
    },
    /// Issue `key` put in sprint `sprint`, or back in the backlog; ranked before issue `before` when one is named (the
    /// Backlog's top). An older build ignores `before`.
    Sprint {
        key: String,
        sprint: Option<u64>,
        #[serde(default)]
        before: Option<String>,
    },
    /// A comment on issue `key`.
    Comment {
        key: String,
        body: String,
    },
    /// Comment `id` of issue `key` given a new text.
    EditComment {
        key: String,
        id: String,
        body: String,
    },
    DeleteComment {
        key: String,
        id: String,
    },
    /// A link: `from` relates to `to` as link kind `kind`'s outward words say (`PAY-412` blocks `PAY-425`).
    Link {
        kind: String,
        from: String,
        to: String,
    },
    /// An issue made, its fields as the create screen names them (`project`, `issuetype`, `summary`, …, `parent`);
    /// the answer names it: `{"key": "PAY-431"}`.
    Create {
        fields: Value,
    },
}

impl Write {
    pub fn moved(key: &str, step: &str, asked: Value) -> Self {
        Self::Move {
            key: key.into(),
            step: step.into(),
            asked,
        }
    }

    pub fn assigned(key: &str, who: Option<&Person>) -> Self {
        Self::Assign {
            key: key.into(),
            who: who.map(|p| p.id.clone()),
        }
    }

    pub fn edited(key: &str, fields: Value) -> Self {
        Self::Edit {
            key: key.into(),
            fields,
        }
    }

    pub fn changed_many(key: &str, field: &str, value: Value, add: bool) -> Self {
        Self::Change {
            key: key.into(),
            field: field.into(),
            value,
            add,
        }
    }

    pub fn in_epic(key: &str, epic: Option<&str>) -> Self {
        Self::Epic {
            key: key.into(),
            epic: epic.map(str::to_owned),
        }
    }

    pub fn in_sprint(key: &str, sprint: Option<u64>) -> Self {
        Self::Sprint {
            key: key.into(),
            sprint,
            before: None,
        }
    }

    pub fn commented(key: &str, body: &str) -> Self {
        Self::Comment {
            key: key.into(),
            body: body.into(),
        }
    }

    pub fn comment_edited(key: &str, id: &str, body: &str) -> Self {
        Self::EditComment {
            key: key.into(),
            id: id.into(),
            body: body.into(),
        }
    }

    pub fn comment_deleted(key: &str, id: &str) -> Self {
        Self::DeleteComment {
            key: key.into(),
            id: id.into(),
        }
    }

    pub fn linked(kind: &str, from: &str, to: &str) -> Self {
        Self::Link {
            kind: kind.into(),
            from: from.into(),
            to: to.into(),
        }
    }

    pub fn created(fields: Value) -> Self {
        Self::Create { fields }
    }

    /// The issue it changes; none for one made.
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::Move { key, .. }
            | Self::Assign { key, .. }
            | Self::Edit { key, .. }
            | Self::Change { key, .. }
            | Self::Epic { key, .. }
            | Self::Sprint { key, .. }
            | Self::Comment { key, .. }
            | Self::EditComment { key, .. }
            | Self::DeleteComment { key, .. } => Some(key),
            Self::Link { from, .. } => Some(from),
            Self::Create { .. } => None,
        }
    }
}

/// A tracker, opened on one site: what Tasks reads of it and writes to it. Each call runs off the window's thread
/// and may block (a request to the tracker); an error is a sentence the person reads (`desk.acme.example refused
/// the token`). A tracker has no state Tasks relies on: one is opened for each call.
pub trait Tracker: Send + Sync {
    /// Who the token is.
    fn myself(&self) -> Result<Person, String>;
    /// Its boards.
    fn boards(&self) -> Result<Vec<Board>, String>;
    /// Board `board`'s columns, epics and sprints.
    fn board_info(&self, board: &str) -> Result<BoardInfo, String>;
    /// Board `board`'s issues, the latest changed first.
    fn board_issues(&self, board: &str) -> Result<Vec<Row>, String>;
    /// Board `board`'s slice `ask`: a page of its issues and how many it holds. By default, read out of
    /// `board_issues` and filtered here, for a tracker whose board is one list.
    fn slice(&self, board: &str, ask: &Ask) -> Result<Page, String> {
        read_slice(self, board, ask)
    }
    /// Board `board`'s closed sprints, the last closed first; none by default.
    fn past_sprints(&self, _board: &str) -> Result<Vec<Sprint>, String> {
        Ok(Vec::new())
    }
    /// Board `board`'s page on the tracker.
    fn board_url(&self, board: &str) -> String;
    /// Issue `key` whole; `<host> has no <key>` when it has none, so the next site is asked.
    fn issue(&self, key: &str) -> Result<Issue, String>;
    /// The issues `words` find, by their key or their words, the best first, at most `max`: what a link's picker
    /// offers.
    fn find(&self, words: &str, max: usize) -> Result<Vec<Row>, String>;
    /// The issues a query in the tracker's language finds (JQL on Jira), at most `max`: what an agent asks. One
    /// with no language of its own is asked the query's words.
    fn search(&self, query: &str, max: usize) -> Result<Vec<Row>, String> {
        self.find(&words_of(query), max)
    }
    /// What others did lately on the issues `me` is on, the newest first: the Inbox's rows.
    fn news(&self, _me: &Person) -> Result<Vec<News>, String> {
        Ok(Vec::new())
    }
    /// The moves issue `key`'s workflow allows from its state.
    fn moves(&self, key: &str) -> Result<Vec<Move>, String>;
    /// The fields issue `key`'s edit screen holds, in the tracker's order: what Details shows and edits.
    fn edit_screen(&self, key: &str) -> Result<Vec<Field>, String>;
    /// The people issue `key` may be given to, those `query` finds (empty: all).
    fn assignable(&self, key: &str, query: &str) -> Result<Vec<Person>, String>;
    /// The projects an issue is made in: `(key, name)`.
    fn projects(&self) -> Result<Vec<(String, String)>, String>;
    /// The types an issue of project `project` is made as.
    fn kinds(&self, project: &str) -> Result<Vec<Kind>, String>;
    /// What making an issue of type `kind` in project `project` asks, beyond its summary and description.
    fn create_screen(&self, project: &str, kind: &str) -> Result<Vec<Field>, String>;
    /// The kinds of link: `(name, inward, outward)`, `("Blocks", "is blocked by", "blocks")`; none: no links.
    fn link_kinds(&self) -> Result<Vec<(String, String, String)>, String> {
        Ok(Vec::new())
    }
    /// The bytes at `url` (an attachment's `content`), read with the site's token.
    fn file(&self, url: &str) -> Result<Vec<u8>, String>;
    /// `file` attached to issue `key`.
    fn attach(&self, key: &str, _file: &Path) -> Result<Value, String> {
        Err(format!("{key} takes no files here"))
    }
    /// A change, sent: the tracker's answer, or why it refused (`PAY-415 is still open`).
    fn write(&self, w: &Write) -> Result<Value, String>;
}

/// Board `board`'s slice `ask`, read out of `t`'s `board_issues`: what `Tracker::slice` does by default.
pub fn read_slice<T: Tracker + ?Sized>(t: &T, board: &str, ask: &Ask) -> Result<Page, String> {
    let me = if ask.mine { Some(t.myself()?) } else { None };
    Ok(Page::of(t.board_issues(board)?, ask, me.as_ref()))
}

/// A query's words, for a tracker with no query language: what is quoted in it (`text ~ "rounding"`, `key =
/// "PAY-412"`), else the query before `ORDER BY`.
pub fn words_of(query: &str) -> String {
    if let Some((_, rest)) = query.split_once('"')
        && let Some((words, _)) = rest.split_once('"')
    {
        return words.to_owned();
    }
    let upper = query.to_uppercase();
    let end = upper.find("ORDER BY").unwrap_or(query.len());
    query[..end].trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_plugin_sdk::serde_json::json;

    #[test]
    fn a_kind_claims_its_hosts_and_those_under_a_star() {
        let kind = TrackerKind {
            hosts: vec!["desk.acme.example".into(), "*.acme.test".into()],
            ..Default::default()
        };
        assert!(kind.claims("desk.acme.example"));
        assert!(kind.claims("eu.acme.test") && kind.claims("acme.test"));
        assert!(!kind.claims("acme.example") && !kind.claims("notacme.test"));
    }

    #[test]
    fn a_query_gives_its_words() {
        assert_eq!(
            words_of("text ~ \"rounding\" ORDER BY updated DESC"),
            "rounding"
        );
        assert_eq!(words_of("refund ORDER BY updated"), "refund");
        assert_eq!(words_of("key = \"kZ3fG9aB\""), "kZ3fG9aB");
    }

    #[test]
    fn a_sprint_write_reads_without_its_place() {
        let older = json!({ "Sprint": { "key": "PAY-412", "sprint": 35 } });
        let w: Write = artoo_plugin_sdk::serde_json::from_value(older).unwrap();
        assert_eq!(w, Write::in_sprint("PAY-412", Some(35)));
        let top = Write::Sprint {
            key: "PAY-412".into(),
            sprint: None,
            before: Some("PAY-300".into()),
        };
        let sent = artoo_plugin_sdk::serde_json::to_value(&top).unwrap();
        assert_eq!(sent["Sprint"]["before"], "PAY-300");
    }

    #[test]
    fn a_site_is_its_host() {
        let site = Site {
            url: "https://jira.example.com/jira".into(),
            ..Default::default()
        };
        assert_eq!(site.host(), "jira.example.com");
        assert_eq!(site.key(), "jira.example.com");
        let second = Site {
            id: "jira.example.com~ops".into(),
            ..site
        };
        assert_eq!(second.host(), "jira.example.com");
        assert_eq!(second.key(), "jira.example.com~ops");
    }
}
