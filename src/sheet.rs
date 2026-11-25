//! New task: a sheet over the window — the project, its kind, the epic (from the group it was opened in), the state
//! (from the column it was opened in), the summary and the description, who does it, its priority, labels and sprint,
//! then what the project's create screen asks for beside them. Create ⌘↵; Save as draft ⌘S keeps it on this Mac; esc lets it go. The drafts: the side
//! panel's Drafts, a list beside a preview, a draft opening as the sheet again.

use crate::keep::Draft;
use crate::look;
use crate::model::{self, Caps, Column, Field, Kind, Move, Person, State, Write};
use crate::page::{Anchor, Page, PopKind, compose};
use crate::pop::{Act, Lead, PopRow};
use crate::store::{Read, Store, now};
use crate::trackers::SiteExt;
use crate::wiki;
use artoo_plugin_sdk::core::textedit::TextEdit;
use artoo_plugin_sdk::gpui::{
    self, AnyElement, ClickEvent, Context, Div, FontWeight, KeyDownEvent, SharedString, div,
    prelude::*,
};
use artoo_plugin_sdk::kit::composer::{Compose, ComposeAct, Look, Target, Typed as Written};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, icons, input, px, theme};
use artoo_plugin_sdk::serde_json::{Value, json};
use std::rc::Rc;

/// Which of the sheet's text has the keyboard.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum At {
    Summary,
    Description,
}

/// The sheet's description composer (`Page::compose_tool`).
const DESCRIPTION: crate::page::ComposeOf = |p| p.sheet.as_mut().map(|s| &mut s.description);

/// The fields the sheet draws itself; the create screen's others follow under "… asks for".
const OWN: [&str; 12] = [
    "project",
    "issuetype",
    "summary",
    "description",
    "assignee",
    "priority",
    "labels",
    "reporter",
    "parent",
    "customfield_10020",
    "sprint",
    "customfield_10014",
];

/// The board's column New task was opened from: the task is made in the workflow's first state, then moved into
/// the column's.
#[derive(Clone, Debug, PartialEq)]
pub struct InColumn {
    pub column: Column,
    /// The state the column is drawn with: its mark on the sheet.
    pub state: State,
    /// Every state of the board, its other columns' too.
    pub board: Vec<String>,
}

/// A task made from a column, its page opening: once it is read there, moved into the column's state when it is not
/// in one already.
pub struct Arriving {
    pub key: String,
    pub column: Column,
}

/// New task being written.
pub struct Sheet {
    pub host: String,
    /// The project's key and name.
    pub project: (String, String),
    pub kind: Option<Kind>,
    /// The epic's key, when the board has epics.
    pub epic: Option<String>,
    /// The issue it is a sub-task of, and its summary.
    pub parent: Option<(String, String)>,
    pub column: Option<InColumn>,
    pub summary: String,
    pub summary_edit: TextEdit,
    pub description: Compose,
    pub at: At,
    /// The other fields set, by id: the value as the create screen takes it, and its words.
    pub values: Vec<(String, Value, String)>,
    /// The draft it was opened from.
    pub draft: Option<u64>,
    /// Why the tracker refused it.
    pub refused: Option<String>,
    pub creating: bool,
    /// The kind whose create screen's defaults were filled in, once each.
    pub defaulted: Option<String>,
}

impl Sheet {
    fn value(&self, id: &str) -> Option<&(String, Value, String)> {
        self.values.iter().find(|(f, _, _)| f == id)
    }

    fn set(&mut self, id: &str, value: Value, words: String) {
        self.values.retain(|(f, _, _)| f != id);
        if !value.is_null() {
            self.values.push((id.to_owned(), value, words));
        }
    }

    /// A value added to or taken from a field of many.
    fn toggle(&mut self, id: &str, value: Value, words: &str) {
        let (mut list, mut names) = match self.value(id) {
            Some((_, Value::Array(l), w)) => (
                l.clone(),
                w.split(", ")
                    .filter(|x| !x.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            ),
            _ => (Vec::new(), Vec::new()),
        };
        match list.iter().position(|v| *v == value) {
            Some(ix) => {
                list.remove(ix);
                names.retain(|n| n != words);
            }
            None => {
                list.push(value);
                names.push(words.to_owned());
            }
        }
        if list.is_empty() {
            self.set(id, Value::Null, String::new());
        } else {
            self.set(id, Value::Array(list), names.join(", "));
        }
    }
}

/// The create screen's fields beyond the sheet's own, required ones first, then components and fix versions.
fn asked(screen: &[Field]) -> Vec<Field> {
    let mut out: Vec<Field> = (screen.iter())
        .filter(|f| f.required && !OWN.contains(&f.id.as_str()))
        .cloned()
        .collect();
    for want in ["components", "fixVersions", "duedate"] {
        if let Some(f) = screen.iter().find(|f| f.id == want && !f.required) {
            out.push(f.clone());
        }
    }
    out
}

/// The move from where issue `now` is into `column`'s state, the column's first state it reaches first: none when it
/// is in one already, why when the workflow has none.
fn move_into(now: &State, column: &Column, moves: &[Move]) -> Result<Option<Move>, String> {
    if column.states.contains(&now.id) {
        return Ok(None);
    }
    (column.states.iter())
        .find_map(|id| moves.iter().find(|m| &m.to.id == id))
        .cloned()
        .map(Some)
        .ok_or_else(|| {
            format!(
                "{} has no move to {}",
                look::display_state(&now.name),
                look::display_state(&column.name)
            )
        })
}

impl Page {
    /// New task, from the board picked: its project, the kind `Task` (a sub-task's under `parent`), in `epic`.
    pub fn open_sheet(
        &mut self,
        epic: Option<String>,
        parent: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        self.open_sheet_from(epic, parent, None, cx)
    }

    /// New task from the board's column named `column` (its `+`), drawn with `state`: made, then moved there.
    pub fn open_sheet_in(&mut self, column: &str, state: State, cx: &mut Context<Self>) {
        let info = self
            .store
            .read(cx)
            .board_info()
            .cloned()
            .unwrap_or_default();
        let board = (info.columns.iter())
            .flat_map(|c| c.states.iter().cloned())
            .collect();
        let column = (info.columns.iter())
            .find(|c| c.name == column)
            .cloned()
            .map(|column| InColumn {
                column,
                state,
                board,
            });
        self.open_sheet_from(None, None, column, cx)
    }

    fn open_sheet_from(
        &mut self,
        epic: Option<String>,
        parent: Option<(String, String)>,
        column: Option<InColumn>,
        cx: &mut Context<Self>,
    ) {
        let s = self.store.read(cx);
        let parent_issue = parent
            .as_ref()
            .and_then(|(k, _)| s.issue(k).map(|i| (i.site.clone(), i.row.project.clone())));
        let board = s.picked().cloned();
        let host = (parent_issue.as_ref().map(|(h, _)| h.clone()))
            .or_else(|| board.as_ref().map(|b| b.site.clone()))
            .or_else(|| s.sites.first().map(|x| x.key().to_owned()))
            .unwrap_or_default();
        let project = (parent_issue.map(|(_, p)| p))
            .or_else(|| board.as_ref().and_then(|b| b.project.clone()))
            .unwrap_or_default();
        let me = s.me.get(&host).cloned();
        let cloud = s.site_of(&host).is_some_and(|x| x.cloud());
        let sprint = s.board_info().and_then(|i| i.active().cloned());
        let name = match s.projects.get(&host) {
            Some(Read::Done(list)) => list
                .iter()
                .find(|(k, _)| *k == project)
                .map(|(_, n)| n.clone()),
            _ => None,
        };
        let name = name
            .or_else(|| board.as_ref().map(|b| b.name.clone()))
            .unwrap_or_else(|| project.clone());
        let mut sheet = Sheet {
            host: host.clone(),
            project: (project.clone(), name),
            kind: None,
            epic: if parent.is_some() { None } else { epic },
            parent: parent.clone(),
            column,
            summary: parent.as_ref().map(|(_, s)| s.clone()).unwrap_or_default(),
            summary_edit: TextEdit::new(
                parent.as_ref().map(|(_, s)| s.clone()).unwrap_or_default(),
            ),
            description: compose(Target::Request(0), ""),
            at: At::Summary,
            values: Vec::new(),
            draft: None,
            refused: None,
            creating: false,
            defaulted: None,
        };
        if let Some(me) = me {
            sheet.set("assignee", person_value(&me, cloud), "You".into());
        }
        if let (Some(sp), None) = (sprint, &parent) {
            sheet.set("sprint", json!(sp.id), sp.name.clone());
        }
        self.sheet = Some(sheet);
        self.sheet_kinds(cx);
        self.sheet_shown.set(true);
        cx.notify();
    }

    /// The project's kinds read, the kind picked when none is (`Task`, a sub-task's kind under a parent), its create
    /// screen read.
    fn sheet_kinds(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.sheet.as_ref() else {
            return;
        };
        let (host, project) = (sheet.host.clone(), sheet.project.0.clone());
        self.store
            .update(cx, |s, cx| s.read_kinds(&host, &project, cx));
        let kinds = match self
            .store
            .read(cx)
            .kinds
            .get(&(host.clone(), project.clone()))
        {
            Some(Read::Done(k)) => k.clone(),
            _ => Vec::new(),
        };
        let Some(sheet) = self.sheet.as_mut() else {
            return;
        };
        if sheet.kind.is_none() {
            let sub = sheet.parent.is_some();
            sheet.kind = (kinds
                .iter()
                .find(|k| k.subtask == sub && k.name == "Task")
                .cloned())
            .or_else(|| kinds.iter().find(|k| k.subtask == sub).cloned());
        }
        if let Some(kind) = sheet.kind.clone() {
            self.store.update(cx, |s, cx| {
                s.read_create_screen(&host, &project, &kind.id, cx)
            });
        }
        self.sheet_defaults(cx);
    }

    /// A read arrived: the kind picked once the project's kinds are read, the screen's defaults once it is; a task
    /// made from a column moved once its page has it.
    pub fn sheet_heard(&mut self, cx: &mut Context<Self>) {
        self.arrival(cx);
        let Some(sheet) = self.sheet.as_ref() else {
            return;
        };
        let key = (sheet.host.clone(), sheet.project.0.clone());
        if sheet.kind.is_none() {
            if matches!(self.store.read(cx).kinds.get(&key), Some(Read::Done(_))) {
                self.sheet_kinds(cx);
            }
        } else {
            self.sheet_defaults(cx);
        }
    }

    /// A task made from a column, its page on screen and read there: moved into the column's state as Move to moves
    /// it (a move that asks, asking on its page), or why the workflow has no such move said under its state.
    fn arrival(&mut self, cx: &mut Context<Self>) {
        let Some(a) = self.arriving.as_ref() else {
            return;
        };
        let key = a.key.clone();
        if self.issue_key().as_deref() != Some(key.as_str()) {
            return;
        }
        let s = self.store.read(cx);
        let Some(now) = s.issue(&key).map(|i| i.row.state.clone()) else {
            return;
        };
        let picked = if a.column.states.contains(&now.id) {
            Ok(None)
        } else {
            match s.moves.get(&key) {
                Some(Read::Done(moves)) => move_into(&now, &a.column, moves),
                Some(Read::Failed(why)) => Err(why.clone()),
                Some(Read::Reading) => return,
                None => {
                    self.store.update(cx, |s, cx| s.read_moves(&key, cx));
                    return;
                }
            }
        };
        let Some(a) = self.arriving.take() else {
            return;
        };
        match picked {
            Ok(None) => {}
            Ok(Some(m)) if m.asks.is_empty() => {
                self.make_move(&key, &m, json!({}), Anchor::Issue, cx)
            }
            Ok(Some(m)) => {
                self.open_pop(PopKind::Ask, Anchor::Issue, &key, cx);
                if let Some(p) = self.pop.as_mut() {
                    p.asked = vec![0; m.asks.len()];
                    p.ask = Some(m);
                }
            }
            Err(why) => {
                self.refused_to = Some(look::display_state(&a.column.name));
                self.refusal = Some((Anchor::Issue, key.clone()));
                self.store.update(cx, |s, cx| {
                    s.refused.insert((key, "state".into()), why);
                    cx.notify();
                });
            }
        }
    }

    /// The create screen's defaults (a priority's `Medium`) filled in where nothing is set, once for each kind.
    fn sheet_defaults(&mut self, cx: &mut Context<Self>) {
        let screen = self.sheet_screen(cx);
        let Some(sheet) = self.sheet.as_mut() else {
            return;
        };
        let Some(kind) = sheet.kind.as_ref().map(|k| k.id.clone()) else {
            return;
        };
        if screen.is_empty() || sheet.defaulted.as_ref() == Some(&kind) {
            return;
        }
        sheet.defaulted = Some(kind);
        for f in &screen {
            if sheet.value(&f.id).is_some() || f.many() {
                continue;
            }
            // A field whose values are the board's states (Trello's list) starts in the column's.
            let landing = (sheet.column.as_ref()).and_then(|c| column_value(f, c));
            if let Some((id, name)) = landing.or_else(|| f.default.clone()) {
                sheet.set(&f.id, json!({ "id": id }), name);
            }
        }
    }

    /// The create screen of the sheet's kind, when read.
    fn sheet_screen(&self, cx: &Context<Self>) -> Vec<Field> {
        let Some(sheet) = self.sheet.as_ref() else {
            return Vec::new();
        };
        let Some(kind) = &sheet.kind else {
            return Vec::new();
        };
        match self.store.read(cx).create_screens.get(&(
            sheet.host.clone(),
            sheet.project.0.clone(),
            kind.id.clone(),
        )) {
            Some(Read::Done(f)) => f.clone(),
            _ => Vec::new(),
        }
    }

    pub fn close_sheet(&mut self, cx: &mut Context<Self>) {
        self.sheet = None;
        self.sheet_shown.set(false);
        if self
            .pop
            .as_ref()
            .is_some_and(|p| matches!(p.anchor, Anchor::Sheet(_)))
        {
            self.close_pop(cx);
        }
        cx.notify();
    }

    /// A key while the sheet is open: its popover's first, then ⌘↵ ⌘S esc ⇥, then the text with the keyboard.
    pub fn sheet_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.pop.is_some() {
            self.pop_key(event, cx);
            return;
        }
        let key = &event.keystroke;
        let m = &key.modifiers;
        match key.key.as_str() {
            "enter" if m.platform => return self.create(cx),
            "s" if m.platform => return self.keep_draft(cx),
            "escape" => return self.close_sheet(cx),
            "tab" => {
                if let Some(s) = self.sheet.as_mut() {
                    s.at = if s.at == At::Summary {
                        At::Description
                    } else {
                        At::Summary
                    };
                }
                return cx.notify();
            }
            _ => {}
        }
        let Some(sheet) = self.sheet.as_mut() else {
            return;
        };
        match sheet.at {
            At::Summary => {
                if let input::Edit::Enter { .. } =
                    input::edit(&mut sheet.summary, &mut sheet.summary_edit, key, cx)
                {
                    sheet.at = At::Description;
                }
            }
            At::Description => {
                if self.compose_short(DESCRIPTION, key, cx) {
                    return;
                }
                let Some(sheet) = self.sheet.as_mut() else {
                    return;
                };
                if let Written::Cancel = sheet.description.edit_key(key, cx) {
                    self.close_sheet(cx);
                    return;
                }
            }
        }
        cx.notify();
    }

    /// The rows of the sheet's popover for field `id`.
    pub fn sheet_rows(
        &self,
        id: &str,
        filter: &str,
        cx: &Context<Self>,
    ) -> (String, Vec<PopRow>, bool) {
        let Some(sheet) = self.sheet.as_ref() else {
            return (String::new(), Vec::new(), false);
        };
        let s = self.store.read(cx);
        let fits = |t: &str| filter.is_empty() || t.to_lowercase().contains(filter);
        let row = |lead: Lead, text: String, value: Value, words: String| PopRow {
            lead,
            text,
            sub: None,
            hint: None,
            key: None,
            checked: false,
            rule: false,
            head: None,
            under: None,
            strong: None,
            tip: None,
            act: Act::Sheet(id.to_owned(), value, words),
        };
        let current = sheet
            .value(id)
            .map(|(_, v, _)| v.clone())
            .unwrap_or(Value::Null);
        match id {
            "project" => {
                let (list, reading) = done_or_reading(s.projects.get(&sheet.host));
                let rows = (list.into_iter())
                    .filter(|(k, n)| fits(k) || fits(n))
                    .map(|(k, n)| {
                        let mut r = row(Lead::None, n.clone(), json!(k), n);
                        r.hint = Some(k.clone());
                        r.checked = k == sheet.project.0;
                        r
                    })
                    .collect();
                ("Project".into(), rows, reading)
            }
            "issuetype" => {
                let (list, reading) =
                    done_or_reading(s.kinds.get(&(sheet.host.clone(), sheet.project.0.clone())));
                let sub = sheet.parent.is_some();
                let rows = (list.into_iter())
                    .filter(|k| k.subtask == sub && fits(&k.name))
                    .map(|k| {
                        let mut r = row(
                            Lead::Kind(k.name.clone()),
                            k.name.clone(),
                            json!(k.id),
                            k.name.clone(),
                        );
                        r.checked = sheet.kind.as_ref().is_some_and(|x| x.id == k.id);
                        r
                    })
                    .collect();
                ("Type".into(), rows, reading)
            }
            "epic" => {
                let info = s.board_info().cloned().unwrap_or_default();
                let mut rows: Vec<PopRow> = (info.epics.iter())
                    .filter(|e| fits(&e.name) || fits(&e.key))
                    .map(|e| {
                        let mut r = row(
                            Lead::Epic(e.clone()),
                            e.name.clone(),
                            json!(e.key),
                            e.name.clone(),
                        );
                        r.hint = Some(e.key.clone());
                        r.checked = sheet.epic.as_deref() == Some(&e.key);
                        r
                    })
                    .collect();
                let mut none = row(Lead::None, "No epic".into(), Value::Null, String::new());
                none.rule = !rows.is_empty();
                none.checked = sheet.epic.is_none();
                rows.push(none);
                ("Epic".into(), rows, false)
            }
            "assignee" => {
                let cloud = s.site_of(&sheet.host).is_some_and(|x| x.cloud());
                let me = s.me.get(&sheet.host).cloned();
                let mut people: Vec<Person> = Vec::new();
                for r in s.board_rows().cloned().unwrap_or_default() {
                    if let Some(p) = r.assignee
                        && !people.contains(&p)
                        && me.as_ref() != Some(&p)
                    {
                        people.push(p);
                    }
                }
                people.sort_by(|a, b| a.name.cmp(&b.name));
                let mut rows = Vec::new();
                if let Some(me) = me.filter(|_| fits("you") || filter.is_empty()) {
                    let mut r = row(
                        Lead::Face("You".into(), true),
                        "You".into(),
                        person_value(&me, cloud),
                        "You".into(),
                    );
                    r.checked = current == person_value(&me, cloud);
                    rows.push(r);
                }
                if fits("unassigned") {
                    let mut r = row(Lead::NoOne, "Unassigned".into(), Value::Null, String::new());
                    r.checked = current.is_null();
                    rows.push(r);
                }
                for p in people.into_iter().filter(|p| fits(&p.name)) {
                    let mut r = row(
                        Lead::Face(p.name.clone(), false),
                        p.name.clone(),
                        person_value(&p, cloud),
                        p.name.clone(),
                    );
                    r.checked = current == person_value(&p, cloud);
                    rows.push(r);
                }
                ("Assignee".into(), rows, false)
            }
            "sprint" => {
                let info = s.board_info().cloned().unwrap_or_default();
                let mut rows: Vec<PopRow> = (info.sprints.iter())
                    .filter(|x| x.state != "closed" && fits(&x.name))
                    .map(|x| {
                        let mut r = row(Lead::None, x.name.clone(), json!(x.id), x.name.clone());
                        r.checked = current.as_u64() == Some(x.id);
                        r
                    })
                    .collect();
                let mut back = row(Lead::None, "Backlog".into(), Value::Null, String::new());
                back.rule = !rows.is_empty();
                back.checked = current.is_null();
                rows.push(back);
                ("Sprint".into(), rows, false)
            }
            "labels" => {
                let mut labels: Vec<(String, usize)> = Vec::new();
                for r in s.board_rows().cloned().unwrap_or_default() {
                    for l in r.labels {
                        match labels.iter_mut().find(|(x, _)| *x == l) {
                            Some((_, n)) => *n += 1,
                            None => labels.push((l, 1)),
                        }
                    }
                }
                labels.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let on = |l: &str| {
                    current
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(l)))
                };
                let mut rows: Vec<PopRow> = (labels.into_iter())
                    .filter(|(l, _)| fits(l))
                    .map(|(l, n)| {
                        let mut r = row(Lead::Check(on(&l)), l.clone(), json!(l), l.clone());
                        r.act = Act::SheetMany(id.to_owned(), json!(l), l.clone());
                        r.hint = Some(n.to_string());
                        r
                    })
                    .collect();
                let typed = filter.trim();
                if !typed.is_empty() && !rows.iter().any(|r| r.text.to_lowercase() == typed) {
                    let mut r = row(
                        Lead::Icon("plus"),
                        format!("Create “{typed}”"),
                        json!(typed),
                        typed.to_owned(),
                    );
                    r.act = Act::SheetMany(id.to_owned(), json!(typed), typed.to_owned());
                    rows.insert(0, r);
                }
                ("Labels".into(), rows, false)
            }
            _ => {
                let screen = self.sheet_screen(cx);
                let Some(field) = screen.iter().find(|f| f.id == id).cloned() else {
                    return (id.to_owned(), Vec::new(), true);
                };
                let has = |name: &str| match &current {
                    Value::Array(l) => l.iter().any(|v| {
                        v["name"].as_str() == Some(name) || v["value"].as_str() == Some(name)
                    }),
                    v => v["name"].as_str() == Some(name) || v["value"].as_str() == Some(name),
                };
                let mut rows: Vec<PopRow> = (field.values.iter())
                    .filter(|(_, n)| fits(n))
                    .map(|(vid, n)| {
                        let value = if vid.is_empty() {
                            json!({ "name": n })
                        } else {
                            json!({ "id": vid, "name": n })
                        };
                        let lead = if field.many() {
                            Lead::Check(has(n))
                        } else {
                            Lead::None
                        };
                        let mut r = row(lead, n.clone(), value.clone(), n.clone());
                        if field.many() {
                            r.act = Act::SheetMany(id.to_owned(), value, n.clone());
                        }
                        r.checked = !field.many() && has(n);
                        if id == "priority" {
                            r.sub = Some(look::priority_of(n).0.to_owned());
                        }
                        r
                    })
                    .collect();
                // A field of words or a number: what is typed is its value.
                let typed = filter.trim();
                if field.values.is_empty() && !typed.is_empty() {
                    let value = match field.kind.as_str() {
                        "number" => typed
                            .parse::<f64>()
                            .map(|n| json!(n))
                            .unwrap_or(json!(typed)),
                        _ => json!(typed),
                    };
                    rows.push(row(
                        Lead::Icon("pencil"),
                        format!("Set to “{typed}”"),
                        value,
                        typed.to_owned(),
                    ));
                }
                (field.name.clone(), rows, false)
            }
        }
    }

    /// A row of the sheet's popover picked: the field set (a field of many toggled).
    pub fn sheet_pick(&mut self, act: Act, cx: &mut Context<Self>) {
        let Some(sheet) = self.sheet.as_mut() else {
            return;
        };
        match act {
            Act::Sheet(id, value, words) => {
                match id.as_str() {
                    "project" => {
                        let key = value.as_str().unwrap_or_default().to_owned();
                        if key != sheet.project.0 {
                            sheet.project = (key, words);
                            sheet.kind = None;
                            sheet.values.retain(|(f, _, _)| {
                                ["assignee", "labels", "sprint"].contains(&f.as_str())
                            });
                            sheet.epic = None;
                        }
                    }
                    "issuetype" => {
                        sheet.kind = Some(Kind {
                            id: value.as_str().unwrap_or_default().to_owned(),
                            name: words,
                            subtask: sheet.parent.is_some(),
                        });
                    }
                    "epic" => sheet.epic = value.as_str().map(str::to_owned),
                    _ => sheet.set(&id, value, words),
                }
                self.close_pop(cx);
                self.sheet_kinds(cx);
            }
            Act::SheetMany(id, value, words) => {
                sheet.toggle(&id, value, &words);
                if let Some(p) = self.pop.as_mut() {
                    p.filter.clear();
                    p.edit = TextEdit::new(String::new());
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn open_sheet_pop(&mut self, field: &str, cx: &mut Context<Self>) {
        self.open_pop(
            PopKind::Sheet(field.to_owned()),
            Anchor::Sheet(field.to_owned()),
            "",
            cx,
        );
    }

    /// The fields the tracker takes for the sheet as it stands, and the epic and sprint sent after it is made.
    fn sheet_fields(&self, cx: &Context<Self>) -> Result<Value, String> {
        let sheet = self.sheet.as_ref().ok_or("No task")?;
        let summary = sheet.summary.trim();
        if summary.is_empty() {
            return Err("A summary is needed".into());
        }
        let kind = sheet.kind.as_ref().ok_or("Pick a type")?;
        let screen = self.sheet_screen(cx);
        for f in asked(&screen) {
            if f.required && sheet.value(&f.id).is_none() {
                return Err(format!("{} asks for {}", sheet.project.1, f.name));
            }
        }
        let mut fields = json!({
            "project": { "key": sheet.project.0 },
            "issuetype": { "id": kind.id },
            "summary": summary,
        });
        let text = sheet.description.edit.text().trim();
        if !text.is_empty() {
            fields["description"] = json!(wiki::from_markdown(text));
        }
        if let Some((parent, _)) = &sheet.parent {
            fields["parent"] = json!({ "key": parent });
        }
        for (id, value, _) in &sheet.values {
            if id != "sprint" {
                fields[id.as_str()] = value.clone();
            }
        }
        Ok(fields)
    }

    /// ⌘↵: made on the tracker; its page opened, put in its epic and sprint after.
    pub fn create(&mut self, cx: &mut Context<Self>) {
        let fields = match self.sheet_fields(cx) {
            Ok(f) => f,
            Err(why) => {
                if let Some(s) = self.sheet.as_mut() {
                    s.refused = Some(why);
                }
                return cx.notify();
            }
        };
        let Some(sheet) = self.sheet.as_mut() else {
            return;
        };
        if sheet.creating {
            return;
        }
        sheet.creating = true;
        sheet.refused = None;
        let Some(site) = self.store.read(cx).site_of(&sheet.host) else {
            return;
        };
        let (epic, sprint, draft, column) = (
            sheet.epic.clone(),
            sheet.value("sprint").and_then(|(_, v, _)| v.as_u64()),
            sheet.draft,
            sheet.column.clone(),
        );
        let made = cx.background_spawn(async move {
            let made = model::write(&site, &Write::created(fields))?;
            let key = made["key"]
                .as_str()
                .ok_or("The tracker gave no key")?
                .to_owned();
            // Its epic and sprint: their own calls, as a board moves an issue into them.
            let mut after = Vec::new();
            if let Some(epic) = &epic
                && let Err(why) = model::write(&site, &Write::in_epic(&key, Some(epic)))
            {
                after.push(format!("Not put in {epic}: {why}"));
            }
            if let Some(sprint) = sprint
                && let Err(why) = model::write(&site, &Write::in_sprint(&key, Some(sprint)))
            {
                after.push(format!("Not put in the sprint: {why}"));
            }
            Ok::<_, String>((key, after))
        });
        cx.spawn(async move |this, cx| {
            let made = made.await;
            this.update(cx, |p, cx| match made {
                Ok((key, after)) => {
                    if let Some(id) = draft {
                        p.store.update(cx, |s, _| {
                            s.kept.drafts.retain(|d| d.id != id);
                            s.kept.write();
                        });
                    }
                    p.close_sheet(cx);
                    p.store
                        .update(cx, |s, cx| s.read_board(crate::store::Ask::Written, cx));
                    for why in after {
                        p.frame.error(why.into(), cx);
                    }
                    // Into the column's state once its page has it: a tracker makes it in the workflow's first.
                    p.arriving = column.map(|c| Arriving {
                        key: key.clone(),
                        column: c.column,
                    });
                    p.open_issue(&key, cx);
                }
                Err(why) => {
                    if let Some(s) = p.sheet.as_mut() {
                        s.creating = false;
                        s.refused = Some(why);
                    }
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// ⌘S: kept on this Mac as a draft, the sheet closed.
    pub fn keep_draft(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.sheet.as_ref() else {
            return;
        };
        let mut words: Vec<String> = Vec::new();
        let (mut fields, mut said) = (json!({}), json!({}));
        for (id, value, w) in &sheet.values {
            fields[id.as_str()] = value.clone();
            if !w.is_empty() {
                words.push(w.clone());
                said[id.as_str()] = json!(w);
            }
        }
        if let Some(epic) = &sheet.epic {
            fields["epic"] = json!(epic);
        }
        if let Some((parent, _)) = &sheet.parent {
            fields["parent"] = json!(parent);
        }
        if let Some(c) = &sheet.column {
            fields["state"] = json!({
                "column": c.column.name, "states": c.column.states, "board": c.board,
                "id": c.state.id, "name": c.state.name, "category": c.state.category,
            });
            said["state"] = json!(look::display_state(&c.column.name));
        }
        let s = self.store.read(cx);
        let draft = Draft {
            id: sheet.draft.unwrap_or_else(|| s.kept.next_id()),
            site: sheet.host.clone(),
            project: format!("{} {}", sheet.project.0, sheet.project.1),
            kind: sheet
                .kind
                .as_ref()
                .map(|k| (k.id.clone(), k.name.clone()))
                .unwrap_or_default(),
            summary: sheet.summary.trim().to_owned(),
            description: sheet.description.edit.text().to_owned(),
            fields,
            words: words.join(" · "),
            said,
            at: now(),
            // Kept by you now, an agent's too once you saved it.
            by: String::new(),
        };
        let id = draft.id;
        self.store.update(cx, |s, cx| {
            s.kept.drafts.retain(|d| d.id != id);
            s.kept.drafts.insert(0, draft);
            s.kept.write();
            cx.notify();
        });
        self.draft_pick = Some(id);
        self.close_sheet(cx);
    }

    /// Draft `id` opened as the sheet again.
    pub fn open_draft(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(d) = self
            .store
            .read(cx)
            .kept
            .drafts
            .iter()
            .find(|d| d.id == id)
            .cloned()
        else {
            return;
        };
        let (project, name) = d
            .project
            .split_once(' ')
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .unwrap_or((d.project.clone(), d.project.clone()));
        let mut values = Vec::new();
        let words: Vec<&str> = d.words.split(" · ").collect();
        let mut w = words.iter();
        let (mut epic, mut parent, mut column) = (None, None, None);
        if let Some(o) = d.fields.as_object() {
            for (k, v) in o {
                match k.as_str() {
                    "epic" => epic = v.as_str().map(str::to_owned),
                    "parent" => parent = v.as_str().map(|p| (p.to_owned(), String::new())),
                    "state" => column = in_column(v),
                    _ => values.push((
                        k.clone(),
                        v.clone(),
                        (d.said[k.as_str()].as_str().map(str::to_owned))
                            .or_else(|| crate::issue::value_words(v))
                            .or_else(|| w.next().map(|x| (*x).to_owned()))
                            .unwrap_or_default(),
                    )),
                }
            }
        }
        self.sheet = Some(Sheet {
            host: d.site.clone(),
            project: (project, name),
            kind: (!d.kind.0.is_empty()).then(|| Kind {
                id: d.kind.0.clone(),
                name: d.kind.1.clone(),
                subtask: parent.is_some(),
            }),
            epic,
            parent,
            column,
            summary: d.summary.clone(),
            summary_edit: TextEdit::new(d.summary.clone()),
            description: compose(Target::Request(0), &d.description),
            at: At::Summary,
            values,
            draft: Some(d.id),
            refused: None,
            creating: false,
            // A draft keeps what it was left with: no default fills a field emptied on purpose.
            defaulted: (!d.kind.0.is_empty()).then(|| d.kind.0.clone()),
        });
        self.sheet_kinds(cx);
        self.sheet_shown.set(true);
        cx.notify();
    }

    fn delete_draft(&mut self, id: u64, cx: &mut Context<Self>) {
        self.store.update(cx, |s, cx| {
            s.kept.drafts.retain(|d| d.id != id);
            s.kept.write();
            cx.notify();
        });
        if self.draft_pick == Some(id) {
            self.draft_pick = None;
        }
        cx.notify();
    }

    /// A select of the sheet: its value, ▾, its popover under it.
    fn select(
        &mut self,
        field: &str,
        lead: Option<AnyElement>,
        words: Option<String>,
        empty: &str,
        wide: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let body = match words {
            Some(w) => div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(theme::text())
                .child(w),
            None => div()
                .flex_1()
                .text_color(theme::faint())
                .child(empty.to_owned()),
        };
        self.select_box(field, lead, body.into_any_element(), wide, None, cx)
    }

    /// `select` drawing `body` as its value, `tip` shown on a long hover.
    fn select_box(
        &mut self,
        field: &str,
        lead: Option<AnyElement>,
        body: AnyElement,
        wide: bool,
        tip: Option<String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let anchor = Anchor::Sheet(field.into());
        let open = self.pop_at(&anchor, PopKind::Sheet(field.into()));
        let picked = field.to_owned();
        div()
            .relative()
            .when(wide, |d| d.flex_1())
            .when(!wide, |d| d.w(px(220.)))
            .child(
                div()
                    .id(SharedString::from(format!("sheet-{field}")))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(28.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .bg(theme::base())
                    .border_1()
                    .border_color(if open {
                        theme::focus()
                    } else {
                        theme::island_border()
                    })
                    .text_size(px(13.))
                    .cursor_pointer()
                    .children(lead)
                    .child(body)
                    .when_some(tip, |d, tip| d.tip(tip))
                    .child(
                        icons::icon("chevron")
                            .size(px(11.))
                            .text_color(theme::faint()),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.open_sheet_pop(&picked, cx)
                    })),
            )
            .children(self.pop_here(&anchor, PopKind::Sheet(field.into()), cx))
    }

    /// The sheet over the window (the part's `Overlay`).
    pub fn sheet_card(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(sheet) = self.sheet.as_ref() else {
            return div().into_any_element();
        };
        let s = self.store.read(cx);
        let has_epics =
            sheet.parent.is_none() && s.board_info().is_some_and(|i| !i.epics.is_empty());
        let epic = sheet.epic.as_ref().and_then(|k| {
            s.board_info()
                .and_then(|i| i.epics.iter().find(|e| &e.key == k).cloned())
        });
        let host = sheet.host.clone();
        let me_name = s.me.get(&host).map(|m| m.name.clone());
        // What the tracker has: a Trello board is its project, its short link no key to read; its labels are set on
        // Trello.
        let caps = s.site_of(&host).map(|x| x.caps()).unwrap_or(Caps::ALL);
        let project = sheet.project.clone();
        let kind = sheet.kind.clone();
        let parent = sheet.parent.clone();
        let column = sheet.column.clone();
        let at = sheet.at;
        let summary = sheet.summary.clone();
        let refused = sheet.refused.clone();
        let creating = sheet.creating;
        let words_of = |id: &str| {
            sheet
                .value(id)
                .map(|(_, _, w)| w.clone())
                .filter(|w| !w.is_empty())
        };
        let (assignee, priority, labels, sprint) = (
            words_of("assignee"),
            words_of("priority"),
            words_of("labels"),
            words_of("sprint"),
        );
        let screen = self.sheet_screen(cx);
        let asks = asked(&screen);
        let asks_words: Vec<(Field, Option<String>)> = asks
            .into_iter()
            .map(|f| {
                let w = self
                    .sheet
                    .as_ref()
                    .and_then(|s| s.value(&f.id).map(|(_, _, w)| w.clone()));
                (f, w)
            })
            .collect();
        let has_priority = screen.is_empty() || screen.iter().any(|f| f.id == "priority");
        let label = |text: &str, top: bool| {
            div()
                .w(px(96.))
                .flex_none()
                .when(top, |d| d.pt(px(6.)))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(text.to_owned())
        };
        let line =
            |l: Div, v: AnyElement| div().flex().items_start().gap(px(12.)).child(l).child(v);
        let typing_summary = at == At::Summary;
        let edit = &self
            .sheet
            .as_ref()
            .map(|s| s.summary_edit.clone())
            .unwrap_or_else(|| TextEdit::new(String::new()));
        let summary_field = input::field(
            &summary,
            typing_summary,
            "What is to be done",
            &input::Look::default(),
            || {
                input::text_body(
                    &summary,
                    edit,
                    input::Look::default(),
                    Default::default(),
                    None,
                )
                .into_any_element()
            },
        );
        let summary_box = div()
            .id("sheet-summary")
            .flex_1()
            .h(px(30.))
            .px(px(8.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .bg(theme::base())
            .border_1()
            .border_color(if typing_summary {
                theme::focus()
            } else {
                theme::island_border()
            })
            .text_size(px(14.))
            .cursor_text()
            .child(summary_field)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                if let Some(s) = this.sheet.as_mut() {
                    s.at = At::Summary;
                }
                cx.notify();
            }));
        let page = cx.entity();
        let acts: artoo_plugin_sdk::kit::composer::ComposeActs = Rc::new(move |act, cx| {
            page.update(cx, |this, cx| {
                if let Some(s) = this.sheet.as_mut() {
                    s.at = At::Description;
                }
                if this.compose_tool(DESCRIPTION, &act, cx) {
                    return;
                }
                if let (
                    Some(s),
                    ComposeAct::Place {
                        at,
                        shift,
                        clicks,
                        drag,
                        lower,
                    },
                ) = (this.sheet.as_mut(), act)
                {
                    s.description.place(at, shift, clicks, drag, lower);
                }
                cx.notify();
            });
        });
        // The outline is the accent only while it has the keyboard, as the summary's is.
        let look = Look {
            tone: if at == At::Description {
                theme::accent()
            } else {
                theme::border()
            },
            placeholder: "What it is about, how it is done".into(),
            head: None,
            lead: None,
            tools: crate::page::tools(),
            tail: Vec::new(),
            folded: false,
            focused: at == At::Description,
            lines: None,
            completion: None,
            measure: div().into_any_element(),
            link_base: None,
            clip: false,
        };
        let description = self
            .sheet
            .as_ref()
            .map(|s| s.description.render_input(look, acts, cx));
        let project_sel = self.select(
            "project",
            None,
            Some(if caps.project_keys {
                format!("{} {}", project.1, project.0)
            } else {
                project.1.clone()
            }),
            "Project",
            false,
            cx,
        );
        let kind_sel = self.select(
            "issuetype",
            kind.as_ref()
                .map(|k| look::kind_mark(&k.name, k.subtask).into_any_element()),
            kind.as_ref().map(|k| k.name.clone()),
            "Type",
            false,
            cx,
        );
        let epic_sel = has_epics.then(|| {
            let lead = Some(look::kind_mark("Epic", false).into_any_element());
            match &epic {
                // The name ends in … at the box's width, its key kept whole; the whole on a long hover.
                Some(e) => {
                    let body = div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .gap(px(4.))
                        .text_color(theme::text())
                        .child(div().min_w_0().truncate().child(e.name.clone()))
                        .child(self.issue_link(
                            "sheet-epic",
                            &e.key,
                            div().flex_none().child(format!("· {}", e.key)),
                            cx,
                        ));
                    let tip = Some(format!("Epic {} {}", e.key, e.name));
                    self.select_box("epic", lead, body.into_any_element(), true, tip, cx)
                }
                None => self.select("epic", lead, None, "No epic", true, cx),
            }
        });
        // "You" is drawn with your own initial.
        let me_face = assignee.as_ref().map(|w| {
            let mine = w == "You";
            let name = (mine.then(|| me_name.clone()).flatten()).unwrap_or_else(|| w.clone());
            look::face(&name, mine, 16.).into_any_element()
        });
        let who_sel = self.select("assignee", me_face, assignee, "Unassigned", false, cx);
        let pri_sel = has_priority.then(|| {
            self.select(
                "priority",
                priority
                    .as_ref()
                    .map(|p| look::priority(p, false).into_any_element()),
                priority.clone(),
                "None",
                false,
                cx,
            )
        });
        let labels_sel = self.select("labels", None, labels, "None", false, cx);
        let sprint_sel = s_has_sprints(&self.store, cx)
            .then(|| self.select("sprint", None, sprint, "Backlog", false, cx));
        let mut asks_lines = Vec::new();
        for (f, w) in asks_words {
            let sel = self.select(
                &f.id,
                None,
                w,
                if f.required { "Pick one" } else { "None" },
                false,
                cx,
            );
            let name = if f.required {
                format!("{} *", f.name)
            } else {
                f.name.clone()
            };
            asks_lines.push(line(label(&name, false), sel.into_any_element()));
        }
        let mut form = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(18.))
            .child(line(
                label("Project", false),
                project_sel.into_any_element(),
            ))
            .child(line(label("Type", false), kind_sel.into_any_element()));
        if let Some((pk, ps)) = &parent {
            form = form.child(line(
                label("Parent", false),
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(28.))
                    .text_size(px(13.))
                    .child(self.issue_link("sheet-parent", pk, look::key(pk), cx))
                    .child(
                        div()
                            .text_color(theme::muted())
                            .truncate()
                            .child(ps.clone()),
                    )
                    .into_any_element(),
            ));
        }
        if let Some(e) = epic_sel {
            form = form.child(line(label("Epic", false), e.into_any_element()));
        }
        // The state it lands in, unless a field of the screen says it (Trello's List, started in the column's).
        if let Some(c) = column.filter(|c| !screen.iter().any(|f| column_value(f, c).is_some())) {
            let name = look::display_state(&c.column.name);
            form = form.child(line(
                label("State", false),
                div()
                    .id("sheet-state")
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(28.))
                    .text_size(px(13.))
                    .text_color(theme::text())
                    .child(look::state_mark(&c.state, 13.))
                    .child(name.clone())
                    .tip(format!(
                        "From its column on the board: made, then moved to {name}"
                    ))
                    .into_any_element(),
            ));
        }
        form = form
            .child(line(
                label("Summary", false),
                summary_box.into_any_element(),
            ))
            .child(line(
                label("Description", true),
                div()
                    .flex_1()
                    .min_w_0()
                    .children(description)
                    .into_any_element(),
            ))
            .child(line(label("Assignee", false), who_sel.into_any_element()));
        if let Some(p) = pri_sel {
            form = form.child(line(label("Priority", false), p.into_any_element()));
        }
        if caps.labels {
            form = form.child(line(label("Labels", false), labels_sel.into_any_element()));
        }
        if let Some(sp) = sprint_sel {
            form = form.child(line(label("Sprint", false), sp.into_any_element()));
        }
        if !asks_lines.is_empty() {
            form = form.child(
                div()
                    .pt(px(6.))
                    .pb(px(2.))
                    .border_t_1()
                    .border_color(theme::border())
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::muted())
                    .child(format!("{} asks for", project.1).to_uppercase()),
            );
            form = form.children(asks_lines);
        }
        let title = if parent.is_some() {
            "New sub-task"
        } else {
            "New task"
        };
        div()
            .id("tasks-sheet-scrim")
            .absolute()
            .size_full()
            // Nothing under it takes a press while the sheet is open.
            .occlude()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(56.))
            .bg(gpui::black().opacity(0.35))
            .child(
                div()
                    .id("tasks-sheet")
                    .occlude()
                    .w(px(600.))
                    .max_h(px(640.))
                    .flex()
                    .flex_col()
                    .rounded(px(10.))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::island_border())
                    .shadow_lg()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .h(px(44.))
                            .px(px(18.))
                            .border_b_1()
                            .border_color(theme::border())
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::faint())
                                    .child(host),
                            )
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id("sheet-x")
                                    .cursor_pointer()
                                    .child(
                                        icons::icon("x").size(px(14.)).text_color(theme::muted()),
                                    )
                                    .tip("Cancel  esc")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.close_sheet(cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("tasks-sheet-form")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(form),
                    )
                    .children(refused.map(|why| {
                        div()
                            .mx(px(18.))
                            .mb(px(8.))
                            .px(px(10.))
                            .py(px(6.))
                            .rounded(px(6.))
                            .bg(look::tint(theme::removed(), 0.12))
                            .text_size(px(12.))
                            .text_color(theme::removed())
                            .child(why)
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .h(px(52.))
                            .px(px(18.))
                            .border_t_1()
                            .border_color(theme::border())
                            .child(
                                look::small_button("sheet-draft", "Save as draft")
                                    .tip("Keep it on this Mac  ⌘S")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.keep_draft(cx)
                                    })),
                            )
                            .child(div().flex_1())
                            .child(
                                look::small_button("sheet-cancel", "Cancel")
                                    .tip("esc")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.close_sheet(cx)
                                    })),
                            )
                            .child(
                                controls::primary_button(
                                    "sheet-create",
                                    if creating { "Creating…" } else { "Create" },
                                    "⌘↵",
                                )
                                .on_click(
                                    cx.listener(|this, _: &ClickEvent, _, cx| this.create(cx)),
                                ),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Drafts: kept on this Mac until created, a list beside the one picked.
    pub fn drafts_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        // Whose a draft is, and when it was kept: `Your agent · 2m`; yours, when alone.
        let by_when = |d: &Draft| {
            let when = look::short_ago(d.at);
            match d.by.is_empty() {
                true => when,
                false => format!("{} · {when}", d.by),
            }
        };
        let drafts = self.store.read(cx).kept.drafts.clone();
        if drafts.is_empty() {
            return artoo_plugin_sdk::kit::island()
                .flex_1()
                .flex()
                .flex_col()
                .bg(theme::editor())
                .child(controls::empty_state(
                    "pencil",
                    "No drafts".into(),
                    "New task's Save as draft ⌘S keeps one here, and so does your agent".into(),
                    None,
                ))
                .into_any_element();
        }
        let pick = self
            .draft_pick
            .filter(|id| drafts.iter().any(|d| d.id == *id))
            .unwrap_or(drafts[0].id);
        let mut list = div()
            .id("tasks-drafts")
            .size_full()
            .flex()
            .flex_col()
            .overflow_y_scroll();
        for d in &drafts {
            let id = d.id;
            let on = id == pick;
            list = list.child(
                div()
                    .id(SharedString::from(format!("draft-{id}")))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .px(px(14.))
                    .py(px(10.))
                    .border_b_1()
                    .border_color(theme::border())
                    .when(on, |x| x.bg(theme::selected()))
                    .hover(|x| x.bg(theme::hover()))
                    .cursor_pointer()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child(if d.summary.is_empty() {
                                        "No summary".to_owned()
                                    } else {
                                        d.summary.clone()
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(by_when(d)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(look::kind_mark(&d.kind.1, false))
                            .child(d.project.clone())
                            .when(!d.words.is_empty(), |x| {
                                x.child(look::sep()).child(d.words.clone())
                            }),
                    )
                    .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                        this.draft_pick = Some(id);
                        if e.click_count() > 1 {
                            this.open_draft(id, cx);
                        }
                        cx.notify();
                    })),
            );
        }
        let d = drafts
            .iter()
            .find(|d| d.id == pick)
            .cloned()
            .unwrap_or_default();
        let kv = |k: &str, v: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .text_size(px(12.))
                .child(
                    div()
                        .w(px(84.))
                        .text_color(theme::faint())
                        .child(k.to_owned()),
                )
                .child(v)
        };
        let id = d.id;
        let mut rows = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(kv(
                "Where",
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().child(d.project.clone()))
                    .child(
                        div()
                            .id("draft-kept")
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(theme::faint())
                            .child(
                                icons::icon("pencil")
                                    .size(px(12.))
                                    .text_color(theme::muted()),
                            )
                            .child(look::short_ago(d.at))
                            .tip(format!(
                                "Draft  ·  kept on this Mac until it is created  ·  saved {}",
                                artoo_plugin_sdk::kit::format::ago(d.at)
                            )),
                    )
                    .into_any_element(),
            ))
            .child(kv(
                "Type",
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(look::kind_mark(&d.kind.1, false))
                    .child(d.kind.1.clone())
                    .into_any_element(),
            ));
        if let Some(o) = d.fields.as_object() {
            for (k, v) in o {
                let said = d.said[k.as_str()].as_str().map(str::to_owned);
                if let Some(w) = said.or_else(|| crate::issue::value_words(v)) {
                    rows = rows.child(kv(&field_name(k), div().child(w).into_any_element()));
                }
            }
        }
        let preview = artoo_plugin_sdk::kit::island()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(20.))
            .bg(theme::editor())
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(d.summary.clone()),
            )
            .child(rows)
            .when(!d.description.is_empty(), |x| {
                x.child(div().text_size(px(13.)).text_color(theme::text()).child(
                    artoo_plugin_sdk::kit::markdown::body(
                        &d.description,
                        true,
                        "tasks-draft-desc",
                        None,
                        None,
                        cx,
                    ),
                ))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        controls::primary_button("draft-open", "Open", "↵").on_click(
                            cx.listener(move |this, _: &ClickEvent, _, cx| this.open_draft(id, cx)),
                        ),
                    )
                    .child(
                        look::small_button("draft-delete", "Delete")
                            .child(
                                icons::icon("trash")
                                    .size(px(12.))
                                    .text_color(theme::muted()),
                            )
                            .tip("Delete the draft  ⌘⌫")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.delete_draft(id, cx)
                            })),
                    ),
            );
        let list = artoo_plugin_sdk::kit::island()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme::editor())
            .child(list);
        self.frame
            .list_page(list.into_any_element(), preview.into_any_element(), cx)
    }

    /// ↑ ↓ through the drafts.
    pub fn step_draft(&mut self, down: bool, cx: &mut Context<Self>) {
        let ids: Vec<u64> = self
            .store
            .read(cx)
            .kept
            .drafts
            .iter()
            .map(|d| d.id)
            .collect();
        if ids.is_empty() {
            return;
        }
        let at = self
            .draft_pick
            .and_then(|id| ids.iter().position(|x| *x == id))
            .unwrap_or(0);
        let next = if down {
            (at + 1).min(ids.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        self.draft_pick = Some(ids[next]);
        cx.notify();
    }
}

fn s_has_sprints(store: &gpui::Entity<Store>, cx: &Context<Page>) -> bool {
    store
        .read(cx)
        .board_info()
        .is_some_and(|i| !i.sprints.is_empty())
}

/// A person as the create screen takes them: by account id on Cloud, by user name on Server.
fn person_value(p: &Person, cloud: bool) -> Value {
    if cloud {
        json!({ "accountId": p.id })
    } else {
        json!({ "name": p.id })
    }
}

fn done_or_reading<T: Clone>(read: Option<&Read<Vec<T>>>) -> (Vec<T>, bool) {
    match read {
        Some(Read::Done(v)) => (v.clone(), false),
        Some(Read::Reading) | None => (Vec::new(), true),
        Some(Read::Failed(_)) => (Vec::new(), false),
    }
}

/// A field's id as its row says it.
/// A draft's column, as `keep_draft` writes it.
fn in_column(v: &Value) -> Option<InColumn> {
    let text = |k: &str| v[k].as_str().unwrap_or_default().to_owned();
    let ids = |k: &str| -> Vec<String> {
        (v[k].as_array().into_iter().flatten())
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect()
    };
    v["states"].as_array()?;
    Some(InColumn {
        column: Column {
            name: text("column"),
            states: ids("states"),
        },
        state: State {
            id: text("id"),
            name: text("name"),
            category: text("category"),
        },
        board: ids("board"),
    })
}

/// When field `f`'s values are the board's states (Trello's `List`, which a card is made in), its value that is one
/// of the column's, the column's first first.
fn column_value(f: &Field, c: &InColumn) -> Option<(String, String)> {
    if f.values.is_empty() || !f.values.iter().all(|(id, _)| c.board.contains(id)) {
        return None;
    }
    (c.column.states.iter()).find_map(|s| f.values.iter().find(|(id, _)| id == s).cloned())
}

fn field_name(id: &str) -> String {
    match id {
        "assignee" => "Assignee".into(),
        "priority" => "Priority".into(),
        "labels" => "Labels".into(),
        "sprint" => "Sprint".into(),
        "components" => "Component".into(),
        "fixVersions" => "Fix version".into(),
        "duedate" => "Due".into(),
        "epic" => "Epic".into(),
        "parent" => "Parent".into(),
        "state" => "State".into(),
        other => other.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sheet_asks_what_the_screen_requires_then_components() {
        let f = |id: &str, required: bool| Field {
            id: id.into(),
            name: id.into(),
            required,
            ..Default::default()
        };
        let screen = vec![
            f("summary", true),
            f("fixVersions", false),
            f("customfield_1", true),
            f("components", false),
            f("issuetype", true),
        ];
        let ids: Vec<String> = asked(&screen).into_iter().map(|f| f.id).collect();
        assert_eq!(ids, ["customfield_1", "components", "fixVersions"]);
    }

    #[test]
    fn a_task_from_a_column_moves_into_its_first_state_it_reaches() {
        let st = |id: &str, name: &str| State {
            id: id.into(),
            name: name.into(),
            category: "indeterminate".into(),
        };
        let to = |id: &str, name: &str| Move {
            id: format!("go-{id}"),
            name: name.into(),
            to: st(id, name),
            asks: Vec::new(),
        };
        let column = Column {
            name: "In progress".into(),
            states: vec!["3".into(), "4".into()],
        };
        let todo = st("1", "To Do");
        let moves = [to("9", "Done"), to("4", "Blocked"), to("3", "In Progress")];
        assert_eq!(
            move_into(&todo, &column, &moves),
            Ok(Some(moves[2].clone()))
        );
        assert_eq!(move_into(&st("4", "Blocked"), &column, &moves), Ok(None));
        assert_eq!(
            move_into(&todo, &column, &moves[..1]),
            Err("To do has no move to In progress".to_owned())
        );
        // Trello's List starts in the column's list; a field whose ids only look like states (a priority's) does not.
        let c = InColumn {
            column,
            state: st("3", "In Progress"),
            board: vec!["1".into(), "3".into(), "4".into(), "9".into()],
        };
        let field = |values: &[(&str, &str)]| Field {
            values: (values.iter())
                .map(|(id, name)| (id.to_string(), name.to_string()))
                .collect(),
            ..Default::default()
        };
        let list = field(&[("1", "Next up"), ("4", "Doing")]);
        assert_eq!(column_value(&list, &c), Some(("4".into(), "Doing".into())));
        let priority = field(&[("1", "Highest"), ("2", "High"), ("3", "Medium")]);
        assert_eq!(column_value(&priority, &c), None);
    }

    #[test]
    fn a_drafts_column_reads_back_as_kept() {
        let kept = json!({
            "column": "In review", "states": ["10001"], "board": ["10000", "10001"],
            "id": "10001", "name": "In Review", "category": "indeterminate",
        });
        let back = in_column(&kept).unwrap();
        assert_eq!(back.column.name, "In review");
        assert_eq!(back.column.states, ["10001"]);
        assert_eq!(back.state.name, "In Review");
        assert_eq!(back.board, ["10000", "10001"]);
        assert!(in_column(&json!("10001")).is_none());
    }

    #[test]
    fn a_field_of_many_toggles_its_values() {
        let mut s = Sheet {
            host: String::new(),
            project: Default::default(),
            kind: None,
            epic: None,
            parent: None,
            column: None,
            summary: String::new(),
            summary_edit: TextEdit::new(String::new()),
            description: compose(Target::Request(0), ""),
            at: At::Summary,
            values: Vec::new(),
            draft: None,
            refused: None,
            creating: false,
            defaulted: None,
        };
        s.toggle("labels", json!("ledger"), "ledger");
        s.toggle("labels", json!("refunds"), "refunds");
        assert_eq!(s.value("labels").unwrap().2, "ledger, refunds");
        s.toggle("labels", json!("ledger"), "ledger");
        assert_eq!(s.value("labels").unwrap().1, json!(["refunds"]));
        s.toggle("labels", json!("refunds"), "refunds");
        assert!(s.value("labels").is_none());
    }
}
