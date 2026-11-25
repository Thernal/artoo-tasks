//! The plugin's own popovers, anchored under what opened them: Move to (its numbers pick), the people an issue goes
//! to (a filter, Assign to me `I`), its epic, a field's values, the people beside Mine · All, a move that asks for a
//! field (Done's resolution), the tracker's refusal. Their rows are one list for the keys and the drawing.

use crate::look;
use crate::model::{self, Epic, Field, Move, Person, Row, Write};
use crate::page::{Anchor, INPUT_POP, Page, Pop, PopKind};
use crate::slices::{self, SliceId};
use crate::store::{Read, Store};
use crate::trackers::SiteExt;
use artoo_plugin_sdk::core::textedit::TextEdit;
use artoo_plugin_sdk::gpui;
use artoo_plugin_sdk::gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Hsla, KeyDownEvent, SharedString, div, prelude::*,
};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{icons, input, px, theme};
use artoo_plugin_sdk::serde_json::{Value, json};
use artoo_plugin_sdk::shell::{Toast, Typed};

/// What a row of a popover shows first.
#[derive(Clone)]
pub enum Lead {
    None,
    Dot(Hsla),
    Face(String, bool),
    NoOne,
    Kind(String),
    Epic(Epic),
    Check(bool),
    Icon(&'static str),
    /// A chevron: what unfolds, turned when open.
    Turn(bool),
}

/// What picking a row does.
#[derive(Clone)]
pub enum Act {
    /// The issue moved, the fields it asks filled.
    Move(Move),
    Assign(Option<Person>),
    Epic(Option<String>),
    /// A person beside Mine · All picked or let go; `None` lets them all go.
    Face(Option<String>),
    /// Field `id` set to `value` (`{"name": "High"}`), or a value added to or taken from a field of many.
    Set(String, Value),
    Many(String, Value, bool),
    Workflow,
    Url(String),
    /// A select of New task set: its field, the value, its words; of a field of many, a value toggled.
    Sheet(String, Value, String),
    SheetMany(String, Value, String),
    /// A slice of the board shown.
    Slice(SliceId),
    /// The sprint picker's Past sprints unfolded or folded.
    Past,
    Nothing,
}

#[derive(Clone)]
pub struct PopRow {
    pub lead: Lead,
    pub text: String,
    /// Faint after the text: `· now Kestrel`.
    pub sub: Option<String>,
    /// At its end: a key (`1`, `I`) or a count (`2 open`).
    pub hint: Option<String>,
    pub key: Option<char>,
    pub checked: bool,
    /// A rule before it.
    pub rule: bool,
    /// A section's title before it: `Active`.
    pub head: Option<String>,
    /// A second line under its text, faint, and what ends that line in bold: `78 issues · ends Fri 28 Nov`, `9 yours`.
    pub under: Option<String>,
    pub strong: Option<String>,
    /// Its tooltip: where its key is said.
    pub tip: Option<String>,
    pub act: Act,
}

impl PopRow {
    fn new(lead: Lead, text: impl Into<String>, act: Act) -> Self {
        PopRow {
            lead,
            text: text.into(),
            sub: None,
            hint: None,
            key: None,
            checked: false,
            rule: false,
            head: None,
            under: None,
            strong: None,
            tip: None,
            act,
        }
    }
}

/// A popover's title, whether it has a filter, and its rows.
pub struct Body {
    pub title: String,
    pub filter: Option<String>,
    pub rows: Vec<PopRow>,
    pub foot: Option<String>,
    pub reading: bool,
}

fn open_of(rows: &[Row], who: Option<&Person>) -> usize {
    (rows.iter())
        .filter(|r| !r.state.done() && r.parent.is_none())
        .filter(|r| r.assignee.as_ref().map(|p| &p.id) == who.map(|p| &p.id))
        .count()
}

/// A menu of one value numbered as Move to is: its first nine values picked by 1–9 while nothing is typed in it.
fn numbered(rows: &mut [PopRow]) {
    let one = |r: &PopRow| {
        matches!(r.act, Act::Set(..) | Act::Epic(_) | Act::Sheet(..)) && r.key.is_none()
    };
    for (n, r) in (1..=9).zip(rows.iter_mut().filter(|r| one(r))) {
        r.key = char::from_digit(n, 10);
        r.hint.get_or_insert_with(|| n.to_string());
    }
}

/// The reassignment a toast offers to undo: the issue, and whom it was assigned to before.
pub struct Undo {
    pub key: String,
    pub before: Option<Person>,
}

/// What the toast of a reassignment says, from whom to whom, `me` as you: `Reassigned from you to Juniper Hale`.
fn reassigned(from: &Person, to: Option<&Person>, me: Option<&Person>) -> String {
    let name = |p: &Person| match me.is_some_and(|m| m.id == p.id) {
        true => "you".to_owned(),
        false => p.name.clone(),
    };
    match to {
        Some(to) => format!("Reassigned from {} to {}", name(from), name(to)),
        None => format!("Unassigned from {}", name(from)),
    }
}

impl Page {
    /// S, or the facts line's sprint name: the sprint picker of a board read by slice, its sprints counted.
    pub fn open_sprints(&mut self, cx: &mut Context<Self>) {
        let s = self.store.read(cx);
        let Some(board) = s.board.clone() else {
            return;
        };
        if s.board_slices().is_none() || s.board_info().is_none_or(|i| i.sprintless) {
            return;
        }
        self.store.update(cx, |s, cx| s.count_sprints(&board, cx));
        self.open_pop(PopKind::Sprints, Anchor::Picker, "", cx);
    }

    /// Whether popover `kind` is open at `anchor`.
    pub fn pop_at(&self, anchor: &Anchor, kind: PopKind) -> bool {
        self.pop
            .as_ref()
            .is_some_and(|p| &p.anchor == anchor && p.kind == kind)
    }

    /// Popover `kind` opened at `anchor` for issue `key`, with the keyboard; what it lists read when it is not yet.
    pub fn open_pop(&mut self, kind: PopKind, anchor: Anchor, key: &str, cx: &mut Context<Self>) {
        if self.pop_at(&anchor, kind.clone()) {
            self.close_pop(cx);
            return;
        }
        let k = key.to_owned();
        self.store.update(cx, |s, cx| match &kind {
            PopKind::Move => s.read_moves(&k, cx),
            PopKind::Assign => s.read_people(&k, cx),
            PopKind::Field(_) => s.read_screen(&k, cx),
            _ => {}
        });
        self.pop = Some(Pop {
            kind,
            anchor,
            key: key.to_owned(),
            filter: String::new(),
            edit: TextEdit::new(String::new()),
            at: 0,
            ask: None,
            asked: Vec::new(),
            find: false,
            past: false,
        });
        self.frame.take_keyboard(INPUT_POP, cx);
        cx.notify();
    }

    pub fn close_pop(&mut self, cx: &mut Context<Self>) {
        if self.pop.take().is_some() {
            self.frame.leave_keyboard(INPUT_POP, cx);
        }
        cx.notify();
    }

    /// What popover `pop` lists now.
    pub fn pop_body(&self, pop: &Pop, cx: &Context<Self>) -> Body {
        let s = self.store.read(cx);
        let row = s.row(&pop.key).cloned();
        let f = pop.filter.to_lowercase();
        let fits = |t: &str| f.is_empty() || t.to_lowercase().contains(&f);
        let board = s
            .picked()
            .map(|b| b.name.clone())
            .unwrap_or_else(|| "this project".into());
        let host = s
            .issue(&pop.key)
            .map(|i| i.site.clone())
            .or_else(|| s.board.as_ref().map(|b| b.0.clone()))
            .unwrap_or_default();
        let me = s.me.get(&host).cloned();
        match &pop.kind {
            PopKind::Move => {
                let mut rows = Vec::new();
                let reading = !matches!(s.moves.get(&pop.key), Some(Read::Done(_)));
                if let Some(Read::Done(moves)) = s.moves.get(&pop.key) {
                    let current = row.as_ref().map(|r| r.state.id.clone()).unwrap_or_default();
                    let mut n = 0;
                    for m in moves.iter().filter(|m| m.to.id != current) {
                        n += 1;
                        let mut r = PopRow::new(
                            Lead::Dot(look::state_color(&m.to)),
                            look::display_state(&m.to.name),
                            Act::Move(m.clone()),
                        );
                        if n <= 9 {
                            r.hint = Some(n.to_string());
                            r.key = char::from_digit(n, 10);
                        }
                        if m.name != m.to.name && !m.name.is_empty() {
                            r.sub = Some(format!("· {}", m.name));
                        }
                        rows.push(r);
                    }
                    if let Some(r) = &row {
                        let mut cur = PopRow::new(
                            Lead::Dot(look::state_color(&r.state)),
                            look::display_state(&r.state.name),
                            Act::Nothing,
                        );
                        cur.checked = true;
                        cur.rule = true;
                        rows.push(cur);
                    }
                    // A tracker whose every state is a move from any other (Trello's lists) has no workflow to show.
                    if s.site_for(&pop.key)
                        .is_some_and(|site| site.caps().workflow)
                    {
                        let mut wf =
                            PopRow::new(Lead::Icon("branch"), "View workflow…", Act::Workflow);
                        wf.hint = Some("W".into());
                        wf.key = Some('w');
                        wf.rule = true;
                        rows.push(wf);
                    }
                }
                if let Some(Read::Failed(why)) = s.moves.get(&pop.key) {
                    rows.push(PopRow::new(Lead::Icon("alert"), why.clone(), Act::Nothing));
                }
                Body {
                    title: format!("Move {} to", pop.key),
                    filter: None,
                    rows,
                    foot: None,
                    reading,
                }
            }
            PopKind::Assign => {
                let board_rows = s.board_rows().cloned().unwrap_or_default();
                let now = row.as_ref().and_then(|r| r.assignee.clone());
                let mut rows = Vec::new();
                if let Some(me) = &me
                    && f.is_empty()
                {
                    let mut r = PopRow::new(
                        Lead::Face(me.name.clone(), true),
                        "Assign to me",
                        Act::Assign(Some(me.clone())),
                    );
                    r.hint = Some("I".into());
                    r.key = Some('i');
                    rows.push(r);
                }
                if fits("unassigned") {
                    let mut r = PopRow::new(Lead::NoOne, "Unassigned", Act::Assign(None));
                    r.hint = Some(format!("{} open", open_of(&board_rows, None)));
                    r.checked = now.is_none();
                    r.rule = !rows.is_empty();
                    rows.push(r);
                }
                let reading = !matches!(s.people.get(&pop.key), Some(Read::Done(_)));
                if let Some(Read::Done(people)) = s.people.get(&pop.key) {
                    for p in people.iter().filter(|p| fits(&p.name)) {
                        let mine = me.as_ref().is_some_and(|m| m.id == p.id);
                        let mut r = PopRow::new(
                            Lead::Face(p.name.clone(), mine),
                            p.name.clone(),
                            Act::Assign(Some(p.clone())),
                        );
                        r.hint = Some(format!("{} open", open_of(&board_rows, Some(p))));
                        r.checked = now.as_ref().is_some_and(|n| n.id == p.id);
                        rows.push(r);
                    }
                }
                let title = match &now {
                    Some(p) => format!("Assign {} · now {}", pop.key, p.name),
                    None => format!("Assign {}", pop.key),
                };
                Body {
                    title,
                    filter: Some(format!("Filter people in {board}")),
                    rows,
                    foot: None,
                    reading,
                }
            }
            PopKind::Epic => {
                let info = s.board_info().cloned().unwrap_or_default();
                let board_rows = s.board_rows().cloned().unwrap_or_default();
                let now = row
                    .as_ref()
                    .and_then(|r| r.epic.as_ref().map(|e| e.key.clone()));
                let mut rows: Vec<PopRow> = (info.epics.iter())
                    .filter(|e| fits(&e.name) || fits(&e.key))
                    .map(|e| {
                        let all: Vec<&Row> = board_rows
                            .iter()
                            .filter(|r| {
                                r.epic.as_ref().is_some_and(|x| x.key == e.key)
                                    && r.parent.is_none()
                            })
                            .collect();
                        let done = all.iter().filter(|r| r.state.done()).count();
                        let mut r = PopRow::new(
                            Lead::Epic(e.clone()),
                            e.key.clone(),
                            Act::Epic(Some(e.key.clone())),
                        );
                        r.hint = Some(format!("{done} of {} done", all.len()));
                        r.checked = now.as_deref() == Some(&e.key);
                        r
                    })
                    .collect();
                let mut none = PopRow::new(Lead::None, "No epic", Act::Epic(None));
                none.rule = !rows.is_empty();
                none.checked = now.is_none();
                rows.push(none);
                if f.is_empty() {
                    numbered(&mut rows);
                }
                let site = s.site_of(&host);
                let foot = now.as_ref().map(|k| format!("Open {k} in Jira ↗"));
                if let (Some(k), Some(site)) = (now, site) {
                    let mut open = PopRow::new(
                        Lead::Icon("external"),
                        format!("Open {k} in Jira"),
                        Act::Url(format!("{}/browse/{k}", site.url)),
                    );
                    open.rule = true;
                    rows.push(open);
                }
                let _ = foot;
                Body {
                    title: format!("Epic of {}", pop.key),
                    filter: Some(format!("Filter epics in {board}")),
                    rows,
                    foot: Some("↵ move · esc".into()),
                    reading: false,
                }
            }
            PopKind::Sprints => sprint_rows(s, pop),
            PopKind::People => {
                let play = s
                    .board_rows()
                    .map(|r| crate::board::in_play(r))
                    .unwrap_or_default();
                let all = crate::board::team(&play, me.as_ref());
                let mut rows: Vec<PopRow> = (all.iter())
                    .filter(|(p, _)| fits(p.as_ref().map_or("Unassigned", |p| &p.name)))
                    .map(|(p, n)| {
                        let id = p
                            .as_ref()
                            .map_or(crate::board::NONE.to_owned(), |p| p.id.clone());
                        let on = s.faces.contains(&id);
                        let name = p
                            .as_ref()
                            .map_or("Unassigned".to_owned(), |p| p.name.clone());
                        let mut r = PopRow::new(Lead::Check(on), name, Act::Face(Some(id)));
                        r.hint = Some(n.to_string());
                        r
                    })
                    .collect();
                if !s.faces.is_empty() {
                    let mut clear = PopRow::new(Lead::Icon("x"), "Clear", Act::Face(None));
                    clear.rule = true;
                    rows.push(clear);
                }
                Body {
                    title: format!("People on {board} · {}", all.len()),
                    filter: Some("Filter people".into()),
                    rows,
                    foot: None,
                    reading: false,
                }
            }
            PopKind::Field(id) => {
                let screen = match s.screens.get(&pop.key) {
                    Some(Read::Done(fields)) => fields.clone(),
                    _ => Vec::new(),
                };
                let reading = !matches!(s.screens.get(&pop.key), Some(Read::Done(_)));
                let field = screen.iter().find(|f| &f.id == id).cloned();
                let issue = s.issue(&pop.key);
                let current = issue
                    .map(|i| i.fields[id.as_str()].clone())
                    .unwrap_or(Value::Null);
                let mut rows = Vec::new();
                let name = field.as_ref().map_or(id.clone(), |f| f.name.clone());
                if let Some(field) = &field {
                    rows = field_rows(field, &current, &f, s, &pop.key);
                    if f.is_empty() {
                        numbered(&mut rows);
                    }
                    // A field of free words (labels, a number, a day): what is typed is the value.
                    if field.values.is_empty() && !pop.filter.trim().is_empty() {
                        let typed = pop.filter.trim().to_owned();
                        let act = if field.many() {
                            Act::Many(id.clone(), json!(typed), true)
                        } else if field.kind == "number" {
                            Act::Set(
                                id.clone(),
                                typed
                                    .parse::<f64>()
                                    .map(|n| json!(n))
                                    .unwrap_or(json!(typed)),
                            )
                        } else {
                            Act::Set(id.clone(), json!(typed))
                        };
                        let words = if field.many() {
                            format!("Create “{typed}”")
                        } else {
                            format!("Set to “{typed}”")
                        };
                        let mut r = PopRow::new(Lead::Icon("plus"), words, act);
                        r.rule = !rows.is_empty();
                        if !rows.iter().any(|x| x.text.eq_ignore_ascii_case(&typed)) {
                            rows.push(r);
                        }
                    }
                }
                let filter = match &field {
                    Some(f) if f.values.is_empty() && f.kind == "date" => {
                        Some("A day: 2026-11-14".into())
                    }
                    Some(f) if f.values.is_empty() && f.kind == "number" => Some("A number".into()),
                    Some(f) if f.values.is_empty() && !f.many() => {
                        Some(format!("Type {}", f.name.to_lowercase()))
                    }
                    _ => Some(format!("Filter {}", name.to_lowercase())),
                };
                Body {
                    title: format!("{name} of {}", pop.key),
                    filter,
                    rows,
                    foot: None,
                    reading,
                }
            }
            PopKind::Ask => Body {
                title: String::new(),
                filter: None,
                rows: Vec::new(),
                foot: None,
                reading: false,
            },
            PopKind::Sheet(field) => {
                let (title, mut rows, reading) = self.sheet_rows(field, &f, cx);
                if f.is_empty() {
                    numbered(&mut rows);
                }
                let filter = (rows.len() > 7
                    || !matches!(field.as_str(), "issuetype" | "epic" | "sprint" | "priority"))
                .then(|| format!("Filter {}", title.to_lowercase()));
                Body {
                    title,
                    filter,
                    rows,
                    foot: None,
                    reading,
                }
            }
        }
    }

    /// The popover open at `anchor` when it is of `kind`: drawn over the window under what opened it.
    pub fn pop_here(
        &mut self,
        anchor: &Anchor,
        kind: PopKind,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.pop_at(anchor, kind.clone()) {
            return None;
        }
        let card = if kind == PopKind::Ask {
            self.ask_card(cx)
        } else {
            self.pop_card(cx)
        };
        Some(anchored_under(card))
    }

    /// The tracker's refusal of a move, under the chip: its reason, the issue in the way opened from it.
    pub fn refusal_here(
        &mut self,
        anchor: &Anchor,
        key: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self
            .refusal
            .as_ref()
            .is_none_or(|(a, k)| a != anchor || k != key)
        {
            return None;
        }
        let why = self
            .store
            .read(cx)
            .refused
            .get(&(key.to_owned(), "state".into()))
            .cloned()?;
        let to = self.refused_to.clone().unwrap_or_default();
        // An issue key in the reason opens.
        let other = why
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .find(|w| {
                w.split_once('-').is_some_and(|(p, n)| {
                    !p.is_empty()
                        && p.chars().all(|c| c.is_ascii_uppercase())
                        && n.chars().all(|c| c.is_ascii_digit())
                        && !n.is_empty()
                })
            });
        let other = other.filter(|o| *o != key).map(str::to_owned);
        let card = div()
            .id("tasks-refused")
            .w(px(280.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .p(px(12.))
            .rounded(px(8.))
            .bg(theme::panel())
            .border_1()
            .border_color(theme::removed())
            .shadow_lg()
            .text_size(px(12.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(format!("Can’t move {key} to {to}")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(theme::muted())
                    .child(div().flex_1().child(why))
                    .children(other.map(|o| {
                        let open = o.clone();
                        div()
                            .id("tasks-refused-open")
                            .text_color(theme::accent())
                            .cursor_pointer()
                            .child(format!("Open {o}"))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.refusal = None;
                                this.open_issue(&open, cx);
                            }))
                    })),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.refusal = None;
                cx.notify();
            }));
        Some(anchored_under(card.into_any_element()))
    }

    fn pop_card(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(pop) = self.pop.as_ref() else {
            return div().into_any_element();
        };
        let body = self.pop_body(pop, cx);
        let at = pop.at.min(body.rows.len().saturating_sub(1));
        let typing = self.frame.typing(cx) == Some(INPUT_POP);
        let mut card = div()
            .id("tasks-pop")
            .w(px(match pop.kind {
                PopKind::Epic => 340.,
                PopKind::Sprints => 400.,
                _ => 260.,
            }))
            .max_h(px(420.))
            .flex()
            .flex_col()
            .py(px(4.))
            .rounded(px(8.))
            .bg(theme::panel())
            .border_1()
            .border_color(theme::island_border())
            .shadow_lg()
            .text_size(px(13.))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_pop(cx)))
            .when(!body.title.is_empty(), |d| {
                d.child(
                    div()
                        .px(px(12.))
                        .pt(px(6.))
                        .pb(px(4.))
                        .text_size(px(11.))
                        .text_color(theme::muted())
                        .child(body.title.clone()),
                )
            });
        // The sprint picker's filter takes letters once `/` or a click asks for it.
        let find_key = (pop.kind == PopKind::Sprints && !pop.find).then(|| {
            div()
                .flex_none()
                .px(px(5.))
                .rounded(px(3.))
                .bg(theme::hover())
                .text_size(px(11.))
                .text_color(theme::faint())
                .child("/")
        });
        if let Some(placeholder) = &body.filter {
            let shown = pop.filter.clone();
            let field = input::field(&shown, typing, placeholder, &input::Look::default(), || {
                input::text_body(
                    &shown,
                    &pop.edit,
                    input::Look::default(),
                    Default::default(),
                    None,
                )
                .into_any_element()
            });
            card = card.child(artoo_plugin_sdk::shell::input_box(
                div()
                    .id("tasks-pop-filter")
                    .mx(px(8.))
                    .my(px(4.))
                    .h(px(26.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(6.))
                    .bg(theme::base())
                    .border_1()
                    .border_color(if typing {
                        theme::focus()
                    } else {
                        theme::island_border()
                    })
                    .child(
                        icons::icon("search")
                            .size(px(12.))
                            .text_color(theme::faint()),
                    )
                    .child(field.text_size(px(12.)))
                    .children(find_key)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        if let Some(p) = this.pop.as_mut() {
                            p.find = true;
                        }
                        cx.notify();
                    })),
                INPUT_POP,
                &self.frame,
            ));
        }
        if body.reading && body.rows.is_empty() {
            card = card.child(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .text_color(theme::faint())
                    .child("Reading…"),
            );
        }
        let mut list = div()
            .id("tasks-pop-rows")
            .flex()
            .flex_col()
            .overflow_y_scroll();
        for (ix, r) in body.rows.into_iter().enumerate() {
            if r.rule {
                list = list.child(div().my(px(4.)).h(px(1.)).bg(theme::border()));
            }
            if let Some(head) = &r.head {
                list = list.child(
                    div()
                        .px(px(12.))
                        .pt(px(6.))
                        .pb(px(2.))
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(head.clone()),
                );
            }
            let act = r.act.clone();
            let lead: AnyElement = match &r.lead {
                Lead::None => div().w(px(14.)).into_any_element(),
                Lead::Dot(c) => div()
                    .size(px(8.))
                    .mx(px(3.))
                    .rounded_full()
                    .bg(*c)
                    .into_any_element(),
                Lead::Face(name, me) => look::face(name, *me, 16.).into_any_element(),
                Lead::NoOne => div()
                    .size(px(16.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme::faint())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::icon("user").size(px(10.)).text_color(theme::muted()))
                    .into_any_element(),
                Lead::Kind(k) => look::kind_mark(k, false).into_any_element(),
                Lead::Epic(_) => look::kind_mark("Epic", false).into_any_element(),
                Lead::Check(on) => div()
                    .size(px(14.))
                    .rounded(px(3.))
                    .border_1()
                    .border_color(if *on { theme::accent() } else { theme::faint() })
                    .when(*on, |d| d.bg(theme::accent()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(*on, |d| {
                        d.child(
                            icons::icon("check")
                                .size(px(10.))
                                .text_color(theme::on_accent()),
                        )
                    })
                    .into_any_element(),
                Lead::Icon(name) => icons::icon(name)
                    .size(px(14.))
                    .text_color(theme::muted())
                    .into_any_element(),
                Lead::Turn(open) => {
                    look::chevron(SharedString::from(format!("pop-turn-{ix}")), *open, 12.)
                        .into_any_element()
                }
            };
            let text: AnyElement = match &r.lead {
                Lead::Epic(e) => div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .min_w_0()
                    .child(look::key(&e.key))
                    .child(look::epic_lozenge(&self.store.read(cx).epic(e)))
                    .into_any_element(),
                _ if r.under.is_some() || r.strong.is_some() => div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().truncate().child(r.text.clone()))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .children(r.under.clone().map(|u| div().truncate().child(u)))
                            .children(r.strong.clone().map(|s| {
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child(s)
                            })),
                    )
                    .into_any_element(),
                _ => div().truncate().child(r.text.clone()).into_any_element(),
            };
            let two = r.under.is_some() || r.strong.is_some();
            let tip = r.tip.clone();
            let enabled = !matches!(act, Act::Nothing);
            list = list.child(
                div()
                    .id(SharedString::from(format!("pop-row-{ix}")))
                    .mx(px(4.))
                    .when(two, |d| d.py(px(4.)))
                    .when(!two, |d| d.h(px(28.)))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(5.))
                    .when_some(tip, |d, tip| d.tip(tip))
                    .when(ix == at && enabled, |d| d.bg(theme::selected()))
                    .when(enabled, |d| {
                        d.cursor_pointer().hover(|s| s.bg(theme::hover()))
                    })
                    .when(!enabled && !r.checked, |d| d.text_color(theme::faint()))
                    .child(lead)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(text)
                            .children(
                                r.sub
                                    .clone()
                                    .map(|s| div().text_color(theme::faint()).child(s)),
                            ),
                    )
                    .children(r.hint.clone().map(|h| {
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(h)
                    }))
                    // A numbered row whose hint is a count (`2 of 3 done`): its number after it.
                    .children(
                        (r.key.filter(char::is_ascii_digit).map(String::from))
                            .filter(|n| r.hint.as_ref() != Some(n))
                            .map(|n| {
                                div()
                                    .flex_none()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(n)
                            }),
                    )
                    .when(r.checked, |d| {
                        d.child(
                            icons::icon("check")
                                .size(px(13.))
                                .text_color(theme::accent()),
                        )
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.pop_pick(act.clone(), cx)
                    })),
            );
        }
        card = card.child(list);
        if let Some(foot) = body.foot {
            card = card.child(
                div()
                    .mt(px(4.))
                    .px(px(12.))
                    .pt(px(6.))
                    .pb(px(4.))
                    .border_t_1()
                    .border_color(theme::border())
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child(foot),
            );
        }
        card.into_any_element()
    }

    /// A move that asks for fields (Done's resolution): its states, each field's values, Cancel and Move ↵.
    fn ask_card(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(pop) = self.pop.as_ref() else {
            return div().into_any_element();
        };
        let Some(m) = pop.ask.clone() else {
            return div().into_any_element();
        };
        let s = self.store.read(cx);
        let from = s.row(&pop.key).map(|r| r.state.clone()).unwrap_or_default();
        let mut card = div()
            .id("tasks-ask")
            .w(px(280.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .rounded(px(8.))
            .bg(theme::panel())
            .border_1()
            .border_color(theme::island_border())
            .shadow_lg()
            .text_size(px(12.))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_pop(cx)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(look::state_chip("ask-from", &from, false, false, true))
                    .child("→")
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(look::display_state(&m.to.name)),
                    ),
            );
        for (fx, field) in m.asks.iter().enumerate() {
            let picked = pop.asked.get(fx).copied().unwrap_or(0);
            let mut values = div().flex().flex_col().gap(px(2.));
            for (vx, (_, name)) in field.values.iter().enumerate() {
                values = values.child(
                    div()
                        .id(SharedString::from(format!("ask-{fx}-{vx}")))
                        .h(px(26.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .rounded(px(5.))
                        .cursor_pointer()
                        .when(vx == picked, |d| {
                            d.bg(theme::selected()).text_color(theme::text())
                        })
                        .when(vx != picked, |d| {
                            d.text_color(theme::muted()).hover(|s| s.bg(theme::hover()))
                        })
                        .child(name.clone())
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if let Some(p) = this.pop.as_mut() {
                                if p.asked.len() <= fx {
                                    p.asked.resize(fx + 1, 0);
                                }
                                p.asked[fx] = vx;
                            }
                            cx.notify();
                        })),
                );
            }
            card = card
                .child(div().text_color(theme::muted()).child(field.name.clone()))
                .child(values);
        }
        card = card.child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(
                    look::small_button("ask-cancel", "Cancel")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_pop(cx))),
                )
                .child(
                    artoo_plugin_sdk::kit::controls::primary_button("ask-move", "Move", "↵")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.ask_done(cx))),
                ),
        );
        card.into_any_element()
    }

    /// The move asked about made, with the values picked.
    fn ask_done(&mut self, cx: &mut Context<Self>) {
        let Some(pop) = self.pop.take() else {
            return;
        };
        self.frame.leave_keyboard(INPUT_POP, cx);
        let Some(m) = pop.ask else {
            return;
        };
        let mut asked = json!({});
        for (fx, field) in m.asks.iter().enumerate() {
            let vx = pop.asked.get(fx).copied().unwrap_or(0);
            if let Some((id, name)) = field.values.get(vx) {
                asked[field.id.as_str()] = if id.is_empty() {
                    json!({ "name": name })
                } else {
                    json!({ "id": id })
                };
            }
        }
        self.make_move(&pop.key, &m, asked, pop.anchor, cx);
    }

    /// Issue `key` moved by `m`: shown at once, sent; the tracker's refusal said under the chip it came from.
    pub fn make_move(
        &mut self,
        key: &str,
        m: &Move,
        asked: Value,
        anchor: Anchor,
        cx: &mut Context<Self>,
    ) {
        let to = m.to.clone();
        let write = Write::moved(key, &m.id, asked);
        let k = key.to_owned();
        self.refused_to = Some(look::display_state(&to.name));
        let page = cx.entity().downgrade();
        let refused = k.clone();
        self.store.update(cx, |s, cx| {
            s.change(
                &k.clone(),
                "state",
                write,
                |s| s.edit_row(&k, |r| r.state = to.clone()),
                move |_, read, cx| {
                    if read.is_err() {
                        page.update(cx, |p, cx| {
                            p.refusal = Some((anchor, refused.clone()));
                            cx.notify();
                        })
                        .ok();
                    }
                },
                cx,
            )
        });
    }

    /// A row of the popover picked.
    pub fn pop_pick(&mut self, act: Act, cx: &mut Context<Self>) {
        let Some(pop) = self.pop.as_ref() else {
            return;
        };
        let key = pop.key.clone();
        let anchor = pop.anchor.clone();
        match act {
            Act::Nothing => {}
            Act::Move(m) if !m.asks.is_empty() => {
                if let Some(p) = self.pop.as_mut() {
                    p.kind = PopKind::Ask;
                    p.asked = vec![0; m.asks.len()];
                    p.ask = Some(m);
                }
                cx.notify();
            }
            Act::Move(m) => {
                self.close_pop(cx);
                self.make_move(&key, &m, json!({}), anchor, cx);
            }
            Act::Assign(who) => {
                self.close_pop(cx);
                self.assign(&key, who, cx);
            }
            Act::Epic(epic) => {
                self.close_pop(cx);
                let info = self
                    .store
                    .read(cx)
                    .board_info()
                    .cloned()
                    .unwrap_or_default();
                let to = epic
                    .as_ref()
                    .and_then(|k| info.epics.iter().find(|e| &e.key == k).cloned());
                let write = Write::in_epic(&key, epic.as_deref());
                self.store.update(cx, |s, cx| {
                    s.change(
                        &key,
                        "epic",
                        write,
                        |s| s.edit_row(&key, |r| r.epic = to.clone()),
                        |_, _, _| {},
                        cx,
                    )
                });
            }
            Act::Face(id) => {
                self.store.update(cx, |s, cx| {
                    match id {
                        Some(id) => {
                            if !s.faces.contains(&id) {
                                s.faces.push(id);
                            } else {
                                s.faces.retain(|f| *f != id);
                            }
                        }
                        None => s.faces.clear(),
                    }
                    cx.notify();
                });
            }
            Act::Set(field, value) => {
                self.close_pop(cx);
                self.set_field(&key, &field, value, cx);
            }
            Act::Many(field, value, add) => {
                if let Some(p) = self.pop.as_mut() {
                    p.filter.clear();
                    p.edit = TextEdit::new(String::new());
                }
                self.change_many(&key, &field, value, add, cx);
            }
            Act::Workflow => {
                self.close_pop(cx);
                self.workflow = Some(key);
                cx.notify();
            }
            Act::Url(url) => {
                self.close_pop(cx);
                cx.open_url(&url);
            }
            act @ (Act::Sheet(..) | Act::SheetMany(..)) => self.sheet_pick(act, cx),
            Act::Slice(slice) => {
                self.close_pop(cx);
                self.store.update(cx, |s, cx| {
                    if let Some(board) = s.board.clone() {
                        s.show_slice(&board, slice, cx);
                    }
                });
            }
            Act::Past => {
                let open = self.pop.as_mut().map(|p| {
                    p.past = !p.past;
                    p.past
                });
                if open == Some(true) {
                    self.store.update(cx, |s, cx| {
                        if let Some(board) = s.board.clone() {
                            s.read_past(&board, cx);
                        }
                    });
                }
                cx.notify();
            }
        }
    }

    /// Issue `key` given to `who`, or to no one; once the tracker took it from someone, a toast says from whom to whom,
    /// with Undo.
    pub fn assign(&mut self, key: &str, who: Option<Person>, cx: &mut Context<Self>) {
        self.assign_to(key, who, true, cx)
    }

    /// The toast's Undo, or ⌘Z while it shows: the issue last reassigned given back, with no toast of its own.
    pub fn undo_assign(&mut self, cx: &mut Context<Self>) {
        let Some(undo) = self.undo_assign.take() else {
            return;
        };
        self.frame
            .drop_toasts(Box::new(|a| a.partial_eq(&crate::UndoAssign)), cx);
        self.assign_to(&undo.key, undo.before, false, cx);
    }

    /// Issue `key` given to `who`, a toast with Undo after it when `told`.
    fn assign_to(&mut self, key: &str, who: Option<Person>, told: bool, cx: &mut Context<Self>) {
        let s = self.store.read(cx);
        let before = s.row(key).and_then(|r| r.assignee.clone());
        let summary = s.row(key).map(|r| r.summary.clone()).unwrap_or_default();
        let me = (s.site_for(key)).and_then(|x| s.me.get(x.key()).cloned());
        let write = Write::assigned(key, who.as_ref());
        let k = key.to_owned();
        // Reassigned: it had someone, and has another or no one now.
        let told = told
            && before.is_some()
            && before.as_ref().map(|p| &p.id) != who.as_ref().map(|p| &p.id);
        let (frame, page) = (self.frame.clone(), cx.entity().downgrade());
        let (to, key_told) = (who.clone(), k.clone());
        self.store.update(cx, |s, cx| {
            s.change(
                &k,
                "assignee",
                write,
                |s| s.edit_row(&k, |r| r.assignee = who.clone()),
                move |_, read, cx| {
                    let (true, Some(from), Ok(_)) = (told, before.as_ref(), read) else {
                        return;
                    };
                    let says = reassigned(from, to.as_ref(), me.as_ref());
                    let about = format!("{key_told} {summary}");
                    let undo = Undo {
                        key: key_told,
                        before,
                    };
                    page.update(cx, |p, _| p.undo_assign = Some(undo)).ok();
                    frame.toast(
                        Toast {
                            about: Some(about),
                            ..Toast::new(says).button("Undo", crate::UndoAssign)
                        },
                        cx,
                    );
                },
                cx,
            )
        });
    }

    /// Field `field` of issue `key` set to `value`: the facts the row shows change at once.
    pub fn set_field(&mut self, key: &str, field: &str, value: Value, cx: &mut Context<Self>) {
        let k = key.to_owned();
        let f = field.to_owned();
        let v = value.clone();
        let write = match field {
            "sprint" | "customfield_10020" => Write::in_sprint(key, value.as_u64()),
            _ => Write::edited(key, json!({ field: value })),
        };
        let key_of = match field {
            "customfield_10020" => "sprint".to_owned(),
            other => other.to_owned(),
        };
        self.store.update(cx, |s, cx| {
            let info = s.board_info().cloned().unwrap_or_default();
            s.change(
                &k.clone(),
                &key_of,
                write,
                move |s| {
                    s.edit_row(&k, |r| match f.as_str() {
                        "priority" => r.priority = v["name"].as_str().map(str::to_owned),
                        "summary" => r.summary = v.as_str().unwrap_or_default().to_owned(),
                        "sprint" | "customfield_10020" => {
                            r.sprint = v
                                .as_u64()
                                .and_then(|id| info.sprints.iter().find(|x| x.id == id).cloned())
                        }
                        _ => {}
                    });
                    s.edit_issue(&k, |i| {
                        if f != "sprint" {
                            i.fields[f.as_str()] = v.clone();
                        }
                    });
                },
                |_, _, _| {},
                cx,
            )
        });
    }

    /// A value added to or taken from issue `key`'s field of many (labels, components, fix versions).
    pub fn change_many(
        &mut self,
        key: &str,
        field: &str,
        value: Value,
        add: bool,
        cx: &mut Context<Self>,
    ) {
        let k = key.to_owned();
        let f = field.to_owned();
        let v = value.clone();
        let write = Write::changed_many(key, field, value, add);
        self.store.update(cx, |s, cx| {
            s.change(
                &k.clone(),
                &f.clone(),
                write,
                move |s| {
                    if f == "labels" {
                        let label = v.as_str().unwrap_or_default().to_owned();
                        s.edit_row(&k, |r| {
                            if add {
                                if !r.labels.contains(&label) {
                                    r.labels.push(label.clone());
                                }
                            } else {
                                r.labels.retain(|l| *l != label);
                            }
                        });
                    }
                    s.edit_issue(&k, |i| {
                        let list = i.fields[f.as_str()].as_array().cloned().unwrap_or_default();
                        let same = |x: &Value| {
                            x == &v
                                || (x["name"].is_string() && x["name"] == v["name"])
                                || (x["id"].is_string() && x["id"] == v["id"])
                        };
                        let mut list: Vec<Value> = list.into_iter().filter(|x| !same(x)).collect();
                        if add {
                            list.push(v.clone());
                        }
                        i.fields[f.as_str()] = Value::Array(list);
                    });
                },
                |_, _, _| {},
                cx,
            )
        });
    }

    /// A key while a popover has the keyboard: ↑ ↓ ↵, its numbers and letters, esc; the rest to its filter.
    pub fn pop_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let Some(pop) = self.pop.as_ref() else {
            return Typed::Leave;
        };
        let key = &event.keystroke;
        if pop.kind == PopKind::Ask {
            match key.key.as_str() {
                "escape" => {
                    self.close_pop(cx);
                    return Typed::Leave;
                }
                "enter" => {
                    self.ask_done(cx);
                    return Typed::Leave;
                }
                "up" | "down" => {
                    if let Some(p) = self.pop.as_mut()
                        && let Some(m) = &p.ask
                        && let Some(field) = m.asks.first()
                    {
                        let n = field.values.len().max(1);
                        let at = p.asked.first().copied().unwrap_or(0);
                        let next = if key.key == "down" {
                            (at + 1).min(n - 1)
                        } else {
                            at.saturating_sub(1)
                        };
                        if p.asked.is_empty() {
                            p.asked.push(next);
                        } else {
                            p.asked[0] = next;
                        }
                    }
                    cx.notify();
                    return Typed::Kept;
                }
                _ => return Typed::Kept,
            }
        }
        let body = self.pop_body(pop, cx);
        let picks: Vec<usize> = (0..body.rows.len())
            .filter(|i| !matches!(body.rows[*i].act, Act::Nothing))
            .collect();
        let at = pop.at;
        let plain = !key.modifiers.platform && !key.modifiers.control && !key.modifiers.alt;
        match key.key.as_str() {
            "escape" => {
                self.close_pop(cx);
                return Typed::Leave;
            }
            "up" | "down" => {
                let here = picks.iter().position(|i| *i == at).unwrap_or(0);
                let next = if key.key == "down" {
                    (here + 1).min(picks.len().saturating_sub(1))
                } else {
                    here.saturating_sub(1)
                };
                if let Some(p) = self.pop.as_mut() {
                    p.at = picks.get(next).copied().unwrap_or(0);
                }
                cx.notify();
                return Typed::Kept;
            }
            "enter" => {
                let ix = if picks.contains(&at) {
                    Some(at)
                } else {
                    picks.first().copied()
                };
                if let Some(row) = ix.and_then(|i| body.rows.get(i)) {
                    self.pop_pick(row.act.clone(), cx);
                }
                return if self.pop.is_some() {
                    Typed::Kept
                } else {
                    Typed::Leave
                };
            }
            _ => {}
        }
        let picker = pop.kind == PopKind::Sprints;
        if picker && !pop.find && key.key == "/" {
            if let Some(p) = self.pop.as_mut() {
                p.find = true;
            }
            cx.notify();
            return Typed::Kept;
        }
        // A row's own key: a number, or a letter while nothing is typed in the filter.
        if plain
            && let Some(c) = key
                .key
                .chars()
                .next()
                .filter(|_| key.key.chars().count() == 1)
        {
            let ours = if picker {
                !pop.find
            } else {
                body.filter.is_none() || pop.filter.is_empty()
            };
            if ours
                && let Some(row) = body
                    .rows
                    .iter()
                    .find(|r| r.key == Some(c.to_ascii_lowercase()) && !key.modifiers.shift)
            {
                self.pop_pick(row.act.clone(), cx);
                return if self.pop.is_some() {
                    Typed::Kept
                } else {
                    Typed::Leave
                };
            }
        }
        if body.filter.is_none() || picker && !pop.find {
            return Typed::Kept;
        }
        if let Some(p) = self.pop.as_mut() {
            let edit = input::edit(&mut p.filter, &mut p.edit, key, cx);
            if matches!(edit, input::Edit::Changed) {
                p.at = 0;
            }
        }
        cx.notify();
        Typed::Kept
    }
}

/// The sprint picker's rows: All active sprints, the active and the future sprints (each with its issues, when it ends
/// or starts, and yours), the Backlog, and Past sprints folded, which open read only. While words are typed, the
/// sprints whose names hold them.
fn sprint_rows(s: &Store, pop: &Pop) -> Body {
    let info = s.board_info().cloned().unwrap_or_default();
    let host = s.board.as_ref().map(|b| b.0.clone()).unwrap_or_default();
    let unit = crate::board::item_word(&host);
    let plural = artoo_plugin_sdk::kit::format::plural;
    let mut rows = Vec::new();
    let Some(sl) = s.board_slices() else {
        return Body {
            title: String::new(),
            filter: None,
            rows,
            foot: None,
            reading: true,
        };
    };
    let view = sl.view.as_ref();
    let words = pop.filter.to_lowercase();
    let finding = !words.is_empty();
    let fits = |name: &str| name.to_lowercase().contains(&words);
    let now = crate::store::now();
    let sprint_row = |sprint: &model::Sprint| {
        let slice = SliceId::Sprint(sprint.id);
        let read = sl.reads.get(&slice);
        let mut r = PopRow::new(Lead::None, sprint.name.clone(), Act::Nothing);
        r.under = Some(slices::sprint_words(
            sprint,
            read.and_then(|r| r.count),
            unit,
            now,
        ));
        r.strong = (read.and_then(|r| r.mine))
            .filter(|n| *n > 0 && sprint.state != "closed")
            .map(|n| format!("{n} yours"));
        r.checked = view == Some(&slice);
        r.act = Act::Slice(slice);
        r
    };
    if !finding && slices::active(&info).next().is_some() {
        let mut r = PopRow::new(
            Lead::Icon("layers"),
            "All active sprints",
            Act::Slice(SliceId::All),
        );
        r.hint = (sl.counts(&SliceId::All, &info).0).map(|n| plural(n, unit));
        r.key = Some('a');
        r.tip = Some("Every active sprint, each its own group  A".into());
        r.checked = view == Some(&SliceId::All);
        rows.push(r);
    }
    for (state, title) in [("active", "Active"), ("future", "Future")] {
        let mut first = true;
        for sprint in (info.sprints.iter()).filter(|x| x.state == state && fits(&x.name)) {
            let mut r = sprint_row(sprint);
            r.head = first.then(|| title.to_owned());
            first = false;
            rows.push(r);
        }
    }
    if !finding {
        let mut r = PopRow::new(Lead::Icon("list"), "Backlog", Act::Slice(SliceId::Backlog));
        r.hint = (sl.reads.get(&SliceId::Backlog))
            .and_then(|r| r.count)
            .map(|n| plural(n, unit));
        r.key = Some('b');
        r.tip = Some("The issues in no sprint  B".into());
        r.rule = true;
        r.checked = view == Some(&SliceId::Backlog);
        rows.push(r);
    }
    let past = match &sl.past {
        Some(Read::Done(past)) => Some(past.as_slice()),
        _ => None,
    };
    let open = pop.past || finding && past.is_some();
    let mut r = PopRow::new(Lead::Turn(open), "Past sprints", Act::Past);
    r.hint = match &sl.past {
        Some(Read::Done(past)) => Some(plural(past.len(), "sprint")),
        Some(Read::Reading) => Some("Reading…".into()),
        Some(Read::Failed(_)) => Some("Not read".into()),
        None => None,
    };
    r.tip = Some("Closed sprints, the last first; they open read only".into());
    r.rule = true;
    rows.push(r);
    if open {
        for sprint in past.into_iter().flatten().filter(|x| fits(&x.name)) {
            rows.push(sprint_row(sprint));
        }
    }
    Body {
        title: String::new(),
        filter: Some("Find a sprint".into()),
        rows,
        foot: None,
        reading: false,
    }
}

/// A field's values as rows: one of a list (priority, a component), each ticked when the issue has it; a field of
/// many toggles them.
fn field_rows(field: &Field, current: &Value, filter: &str, s: &Store, key: &str) -> Vec<PopRow> {
    let fits = |t: &str| filter.is_empty() || t.to_lowercase().contains(filter);
    let has = |id: &str, name: &str| -> bool {
        let one = |v: &Value| {
            v["id"].as_str() == Some(id)
                || v["name"].as_str() == Some(name)
                || v["value"].as_str() == Some(name)
                || v.as_str() == Some(name)
        };
        match current {
            Value::Array(list) => list.iter().any(one),
            v => one(v),
        }
    };
    if field.id == "labels" {
        // The labels the board's issues wear, with how many wear each; the issue's ticked.
        let rows = s.board_rows().cloned().unwrap_or_default();
        let mut labels: Vec<(String, usize)> = Vec::new();
        for r in &rows {
            for l in &r.labels {
                match labels.iter_mut().find(|(x, _)| x == l) {
                    Some((_, n)) => *n += 1,
                    None => labels.push((l.clone(), 1)),
                }
            }
        }
        labels.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let on: Vec<String> = s.row(key).map(|r| r.labels.clone()).unwrap_or_default();
        return (labels.into_iter())
            .filter(|(l, _)| fits(l))
            .map(|(l, n)| {
                let has = on.contains(&l);
                let mut r = PopRow::new(
                    Lead::Check(has),
                    l.clone(),
                    Act::Many("labels".into(), json!(l), !has),
                );
                r.hint = Some(n.to_string());
                r
            })
            .collect();
    }
    if field.id == "customfield_10020" || field.id == "sprint" {
        let info = s.board_info().cloned().unwrap_or_default();
        let now = s.row(key).and_then(|r| r.sprint.as_ref().map(|x| x.id));
        let mut rows: Vec<PopRow> = (info.sprints.iter())
            .filter(|x| fits(&x.name))
            .map(|x| {
                let mut r = PopRow::new(
                    Lead::None,
                    x.name.clone(),
                    Act::Set("sprint".into(), json!(x.id)),
                );
                r.sub = x
                    .end
                    .as_ref()
                    .map(|e| format!("· ends {}", model::day_words(e)));
                r.checked = now == Some(x.id);
                r
            })
            .collect();
        let mut back = PopRow::new(
            Lead::None,
            "Backlog",
            Act::Set("sprint".into(), Value::Null),
        );
        back.rule = !rows.is_empty();
        back.checked = now.is_none();
        rows.push(back);
        return rows;
    }
    (field.values.iter())
        .filter(|(_, name)| fits(name))
        .map(|(id, name)| {
            let on = has(id, name);
            let value = if field.kind == "option" || field.items == "option" {
                json!({ "id": id })
            } else if id.is_empty() {
                json!({ "name": name })
            } else {
                json!({ "id": id })
            };
            let lead = match field.id.as_str() {
                "priority" => Lead::None,
                "issuetype" => Lead::Kind(name.clone()),
                _ if field.many() => Lead::Check(on),
                _ => Lead::None,
            };
            let act = if field.many() {
                Act::Many(field.id.clone(), value, !on)
            } else {
                Act::Set(field.id.clone(), value)
            };
            let mut r = PopRow::new(lead, name.clone(), act);
            r.checked = on && !field.many();
            if field.id == "priority" {
                r.sub = Some(look::priority_of(name).0.to_owned());
            }
            r
        })
        .collect()
}

/// `card` over the window under the element it is put in.
pub fn anchored_under(card: AnyElement) -> AnyElement {
    div()
        .absolute()
        .left_0()
        .bottom_0()
        .size_0()
        .child(
            gpui::deferred(
                gpui::anchored()
                    .offset(gpui::point(px(0.), px(6.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(card),
            )
            .with_priority(2),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(id: &str, name: &str) -> Person {
        Person {
            id: id.into(),
            name: name.into(),
        }
    }

    #[test]
    fn a_menu_of_one_value_is_numbered_one_to_nine_after_what_has_a_key() {
        let set = |n: usize| {
            PopRow::new(
                Lead::None,
                format!("v{n}"),
                Act::Set("priority".into(), json!(n)),
            )
        };
        let mut rows: Vec<PopRow> = (0..11).map(set).collect();
        rows[0].key = Some('i');
        rows[2].hint = Some("2 of 3 done".into());
        rows.push(PopRow::new(Lead::None, "Open", Act::Url(String::new())));
        numbered(&mut rows);
        let keys: String = rows.iter().map(|r| r.key.unwrap_or('-')).collect();
        assert_eq!(keys, "i123456789--");
        assert_eq!(rows[1].hint.as_deref(), Some("1"));
        assert_eq!(rows[2].hint.as_deref(), Some("2 of 3 done"));
    }

    #[test]
    fn a_reassignment_says_from_whom_to_whom_and_you_as_you() {
        let (me, juniper) = (person("1", "Kestrel"), person("2", "Juniper Hale"));
        let says = reassigned(&me, Some(&juniper), Some(&me));
        assert_eq!(says, "Reassigned from you to Juniper Hale");
        let says = reassigned(&juniper, None, Some(&me));
        assert_eq!(says, "Unassigned from Juniper Hale");
        let says = reassigned(&juniper, Some(&me), None);
        assert_eq!(says, "Reassigned from Juniper Hale to Kestrel");
    }
}
