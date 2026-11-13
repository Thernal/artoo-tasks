//! An issue's page: what someone else changed over its title, its kind, key and summary (a click edits it), its facts
//! row — State, Assignee, Priority, Type, Epic, Sprint, each a control —, then Description (a note for an agent on a
//! paragraph, Edit in place), Sub-tasks (an empty row always last, typed into), Linked issues, Attachments and
//! Comments (yours edited or deleted, an agent's waiting for Post, the composer, ⇥ to the agent) as sections of their
//! own, Details on the right.

use crate::keep::Pending;
use crate::look;
use crate::model::{self, Attachment, Comment, Field, Issue, Link, Person, Row, Write};
use crate::page::{
    Anchor, INPUT_COMMENT, INPUT_DESC, INPUT_SUB, INPUT_TITLE, Page, PopKind, Spot, compose,
};
use crate::store::Read;
use crate::wiki::{self, handle};
use artoo_plugin_sdk::core::textedit::TextEdit;
use artoo_plugin_sdk::gpui::{
    self, AnyElement, App, Bounds, ClickEvent, Context, Div, FontWeight, KeyDownEvent, ObjectFit,
    Pixels, SharedString, canvas, div, img, point, prelude::*,
};
use artoo_plugin_sdk::kit::composer::{Compose, ComposeAct, Look, Target, Typed as Written};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, icons, input, markdown, px, theme};
use artoo_plugin_sdk::serde_json::{Value, json};
use artoo_plugin_sdk::shell::Typed;
use std::cell::Cell;
use std::rc::Rc;

/// The issue page's own state: what is being written or edited on it.
#[derive(Default)]
pub struct IssueState {
    /// The paragraph under the pointer: C writes a note on it.
    pub hovered_para: Option<usize>,
    /// The comment being written, and whether it goes to the agent (⇥) rather than everyone.
    pub comment: Option<Compose>,
    pub to_agent: bool,
    /// One of your comments being edited: its id, in the composer.
    pub editing: Option<String>,
    /// The description being edited, and the text it was when the editing began.
    pub desc: Option<(Compose, String)>,
    /// A save that would overwrite: who changed it and their text; the diff shown.
    pub conflict: Option<(String, String)>,
    pub compare: bool,
    /// The sub-tasks' empty row: what is typed.
    pub sub: String,
    pub sub_edit: TextEdit,
    /// The summary being edited.
    pub title: Option<(String, TextEdit)>,
    /// An agent's comment waiting, being edited: its id.
    pub pending_edit: Option<u64>,
    /// The Link popover: the relation picked (`(type, words, outward)`), what is typed, the issues found.
    pub link: Option<LinkPick>,
    /// The image open over the window: the attachment's id.
    pub viewing: Option<String>,
    /// Drawings left that go to the page's foot: the composer opened there grows as its text is laid out.
    pub to_foot: u8,
    /// Where the Link button was drawn, and the drawings left that bring it into view: L opens its card under it,
    /// wherever the page was scrolled.
    pub link_spot: Rc<Cell<Option<Bounds<Pixels>>>>,
    pub to_link: u8,
}

pub struct LinkPick {
    pub relation: usize,
    pub words: String,
    pub edit: TextEdit,
    pub found: Read<Vec<Row>>,
    pub at: usize,
}

fn section(title: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(8.))
        .border_1()
        .border_color(theme::border())
        .bg(theme::panel())
        .overflow_hidden()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(16.))
                .h(px(40.))
                .border_b_1()
                .border_color(theme::border())
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(title.to_owned()),
        )
}

/// A section whose head holds more than its title.
fn section_with(head: Div) -> Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(8.))
        .border_1()
        .border_color(theme::border())
        .bg(theme::panel())
        .overflow_hidden()
        .child(
            head.flex()
                .items_center()
                .gap(px(8.))
                .px(px(16.))
                .h(px(40.))
                .border_b_1()
                .border_color(theme::border())
                .text_size(px(13.))
                .text_color(theme::text()),
        )
}

/// A fact of the facts row: its label over its value.
fn fact(label: &'static str, value: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::muted())
                .child(label.to_uppercase()),
        )
        .child(value)
}

/// A control: its value and ▾, ringed while its list is open.
fn ctl(id: impl Into<SharedString>, on: bool, tip: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id.into())
        .relative()
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(24.))
        .px(px(6.))
        .ml(px(-6.))
        .rounded(px(5.))
        .text_size(px(13.))
        .text_color(theme::text())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .when(on, |d| d.border_1().border_color(theme::accent()))
        .tip(tip)
}

fn chevron() -> impl IntoElement {
    icons::icon("chevron")
        .size(px(11.))
        .text_color(theme::faint())
}

/// A changed field's dot, until it is seen.
fn dot(on: bool) -> Option<Div> {
    on.then(|| div().size(px(6.)).rounded_full().bg(theme::warning()))
}

/// A value of a field as words: a name, a value, a list of them, a number, a day.
/// The people a mention names, `(as the tracker writes them, as written here)`: `accountid:5b10…` (a username on
/// Jira Server) and `Juniper.Hale`.
pub type Named = Vec<(String, String)>;

/// `md` with the mentions of people known by their names: `@accountid:5b10…` reads `@Juniper.Hale`.
fn named(md: &str, people: &Named) -> String {
    wiki::mentions_as(md, |n| {
        (people.iter())
            .find(|(t, _)| t == n)
            .map(|(_, h)| h.clone())
    })
}

/// `md` with the names mentioned as the tracker writes them, for what goes to it.
fn unnamed(md: &str, people: &Named) -> String {
    wiki::mentions_as(md, |n| {
        (people.iter())
            .find(|(_, h)| h.eq_ignore_ascii_case(n))
            .map(|(t, _)| t.clone())
    })
}

pub fn value_words(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) if s.is_empty() => None,
        Value::String(s) => Some(match model::stamp(s) {
            Some(_) if s.len() == 10 => model::day_words(s),
            _ => s.clone(),
        }),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Array(list) => {
            let words: Vec<String> = list.iter().filter_map(value_words).collect();
            (!words.is_empty()).then(|| words.join(", "))
        }
        Value::Object(o) => ["displayName", "name", "value", "key"]
            .iter()
            .find_map(|k| o.get(*k).and_then(|x| x.as_str()).map(str::to_owned)),
    }
}

impl Page {
    pub fn issue_page(&mut self, key: String, cx: &mut Context<Self>) -> AnyElement {
        let frame = || {
            artoo_plugin_sdk::kit::island()
                .flex_1()
                .flex()
                .flex_col()
                .min_h_0()
                .min_w_0()
                .overflow_hidden()
                .bg(theme::editor())
        };
        if self.issue.to_foot > 0 {
            self.issue.to_foot -= 1;
            self.issue_scroll.scroll_to_bottom();
            cx.notify();
        }
        if self.issue.to_link > 0 {
            self.issue.to_link -= 1;
            self.link_into_view();
            cx.notify();
        }
        let read = self.store.read(cx).issues.get(&key).cloned();
        let issue = match read {
            None | Some(Read::Reading) => {
                return frame()
                    .p(px(16.))
                    .child(controls::skeleton("tasks-issue", 10))
                    .into_any_element();
            }
            Some(Read::Failed(why)) => {
                return frame()
                    .child(controls::empty_state(
                        "alert",
                        format!("{key} not read"),
                        why,
                        None,
                    ))
                    .into_any_element();
            }
            Some(Read::Done(issue)) => issue,
        };
        let changed = self.changed_line(&issue, cx);
        let head = self.issue_head(&issue, cx);
        let main = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(self.description(&issue, cx))
            .child(self.subtasks(&issue, cx))
            .when(!issue.links.is_empty() || self.issue.link.is_some(), |d| {
                d.child(self.links(&issue, cx))
            })
            .when(!issue.attachments.is_empty(), |d| {
                d.child(self.attachments(&issue, cx))
            })
            .child(self.comments(&issue, cx));
        let details = self.details(&issue, cx);
        let viewer = self
            .issue
            .viewing
            .clone()
            .and_then(|id| self.viewer(&issue, &id, cx));
        frame()
            .child(
                div()
                    .id("tasks-issue")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.issue_scroll)
                    .children(changed)
                    .child(head)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_start()
                            .gap(px(16.))
                            .px(px(20.))
                            .pb(px(24.))
                            .child(main)
                            .child(div().w(px(260.)).flex_none().child(details)),
                    ),
            )
            .children(viewer)
            .into_any_element()
    }

    /// Who changed what since the issue was opened, over its title; Dismiss.
    fn changed_line(&mut self, issue: &Issue, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.store.read(cx);
        let c = s.changed.get(&issue.row.key)?.clone();
        if c.unseen.is_empty() {
            return None;
        }
        let key = issue.row.key.clone();
        let fields = match c.fields.as_slice() {
            [one] => one.clone(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
            [] => "it".into(),
        };
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .mx(px(20.))
                .mt(px(14.))
                .px(px(12.))
                .h(px(36.))
                .rounded(px(6.))
                .bg(theme::warning_bg())
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(div().size(px(6.)).rounded_full().bg(theme::warning()))
                .child(look::face(&c.who, false, 16.))
                .child(
                    div().child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(c.who.clone()),
                    ),
                )
                .child(format!("changed {fields}"))
                .child(
                    div()
                        .text_color(theme::faint())
                        .child(artoo_plugin_sdk::kit::format::ago(c.at)),
                )
                .child(div().flex_1())
                .child(
                    look::small_button("tasks-dismiss", "Dismiss")
                        .tip("Mark as seen  esc")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let k = key.clone();
                            this.store.update(cx, |s, cx| {
                                s.changed.remove(&k);
                                cx.notify();
                            });
                        })),
                )
                .into_any_element(),
        )
    }

    /// Its kind, key and summary; the facts row.
    fn issue_head(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let r = issue.row.clone();
        let s = self.store.read(cx);
        let unseen = s
            .changed
            .get(&r.key)
            .map(|c| c.unseen.clone())
            .unwrap_or_default();
        let me = s.is_me(&issue.site, r.assignee.as_ref());
        let epic = r.epic.as_ref().map(|e| s.epic(e));
        let has_epics = epic.is_some() || s.board_info().is_some_and(|i| !i.epics.is_empty());
        let key = r.key.clone();
        let title_editing = self.issue.title.is_some();
        let typing = self.frame.typing(cx) == Some(INPUT_TITLE);
        let title: AnyElement = match &self.issue.title {
            Some((text, edit)) => {
                let shown = text.clone();
                let field =
                    input::field(&shown, typing, "Summary", &input::Look::default(), || {
                        input::text_body(
                            &shown,
                            edit,
                            input::Look::default(),
                            Default::default(),
                            None,
                        )
                        .into_any_element()
                    });
                artoo_plugin_sdk::shell::input_box(
                    div()
                        .id("tasks-title-edit")
                        .flex_1()
                        .h(px(30.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(theme::focus())
                        .text_size(px(18.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(field),
                    INPUT_TITLE,
                    &self.frame,
                )
                .into_any_element()
            }
            None => div()
                .id("tasks-title")
                .flex_1()
                .min_w_0()
                .text_size(px(18.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .cursor_text()
                .child(r.summary.clone())
                .tip("Click to edit")
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.edit_title(cx)))
                .into_any_element(),
        };
        let _ = title_editing;
        let anchor_of = |name: &str| Anchor::Detail(name.to_owned());
        // State.
        let state = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(self.state_control(&r, Anchor::Issue, false, cx))
            .children(dot(unseen.contains("State")));
        // Assignee.
        let who_open = self.pop_at(&Anchor::Issue, PopKind::Assign);
        let k = key.clone();
        let assignee = ctl("tasks-ctl-who", who_open, "Assign  A · to me  I")
            .child(match &r.assignee {
                Some(p) => div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(look::face(&p.name, me, 16.))
                    .child(if me { "You".to_owned() } else { p.name.clone() }),
                None => div().text_color(theme::faint()).child("Unassigned"),
            })
            .child(chevron())
            .children(dot(unseen.contains("Assignee")))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.open_pop(PopKind::Assign, Anchor::Issue, &k, cx)
            }))
            .children(self.pop_here(&Anchor::Issue, PopKind::Assign, cx));
        // Priority, Type, Sprint, Epic: their lists.
        let field_ctl = |this: &mut Page,
                         id: &'static str,
                         field: &'static str,
                         body: AnyElement,
                         cx: &mut Context<Page>| {
            let anchor = anchor_of(field);
            let open = this.pop_at(&anchor, PopKind::Field(field.into()));
            let k = key.clone();
            let a = anchor.clone();
            ctl(id, open, "Change")
                .child(body)
                .child(chevron())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.open_pop(PopKind::Field(field.into()), a.clone(), &k, cx)
                }))
                .children(this.pop_here(&anchor, PopKind::Field(field.into()), cx))
        };
        let priority = field_ctl(
            self,
            "tasks-ctl-priority",
            "priority",
            match &r.priority {
                Some(p) => look::priority(p, true).into_any_element(),
                None => div()
                    .text_color(theme::faint())
                    .child("None")
                    .into_any_element(),
            },
            cx,
        )
        .children(dot(unseen.contains("Priority")));
        let kind = field_ctl(
            self,
            "tasks-ctl-type",
            "issuetype",
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(look::kind_mark(&r.kind, r.subtask))
                .child(r.kind.clone())
                .into_any_element(),
            cx,
        );
        let sprint_field = if issue.fields.get("customfield_10020").is_some() {
            "customfield_10020"
        } else {
            "sprint"
        };
        let sprint = field_ctl(
            self,
            "tasks-ctl-sprint",
            sprint_field,
            match &r.sprint {
                Some(sp) => div().child(sp.name.clone()).into_any_element(),
                None => div()
                    .text_color(theme::faint())
                    .child("Backlog")
                    .into_any_element(),
            },
            cx,
        )
        .children(dot(unseen.contains("Sprint")));
        let epic_ctl = has_epics.then(|| {
            let anchor = anchor_of("epic");
            let open = self.pop_at(&anchor, PopKind::Epic);
            let k = key.clone();
            let a = anchor.clone();
            ctl("tasks-ctl-epic", open, "Change the epic  ⇧E")
                .child(match &epic {
                    Some(e) => div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(look::kind_mark("Epic", false))
                        .child(look::epic_lozenge(e))
                        .into_any_element(),
                    None => div()
                        .text_color(theme::faint())
                        .child("No epic")
                        .into_any_element(),
                })
                .child(chevron())
                .children(dot(unseen.contains("Epic")))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.open_pop(PopKind::Epic, a.clone(), &k, cx)
                }))
                .children(self.pop_here(&anchor, PopKind::Epic, cx))
        });
        let mut facts = div()
            .flex()
            .flex_wrap()
            .gap(px(28.))
            .child(fact("State", state))
            .child(fact("Assignee", assignee))
            .child(fact("Priority", priority))
            .child(fact("Type", kind));
        if let Some(e) = epic_ctl {
            facts = facts.child(fact("Epic", e));
        }
        facts = facts.child(fact("Sprint", sprint));
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(20.))
            .pt(px(18.))
            .pb(px(18.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(look::kind_mark(&r.kind, r.subtask))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(theme::accent())
                            .child(r.key.clone()),
                    )
                    .child(title)
                    .children(dot(unseen.contains("Summary"))),
            )
            .child(facts)
            .into_any_element()
    }

    /// Click on the title: its summary in a field, ↵ saves, esc leaves it.
    pub fn edit_title(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.issue_key() else {
            return;
        };
        let text = self
            .store
            .read(cx)
            .row(&key)
            .map(|r| r.summary.clone())
            .unwrap_or_default();
        self.issue.title = Some((text.clone(), TextEdit::new(text)));
        self.frame.take_keyboard(INPUT_TITLE, cx);
        cx.notify();
    }

    pub fn title_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let Some((text, edit)) = self.issue.title.as_mut() else {
            return Typed::Leave;
        };
        match input::edit(text, edit, &event.keystroke, cx) {
            input::Edit::Escape => {
                self.issue.title = None;
                cx.notify();
                return Typed::Leave;
            }
            input::Edit::Enter { .. } => {
                let summary = text.trim().to_owned();
                self.issue.title = None;
                if let Some(key) = self.issue_key()
                    && !summary.is_empty()
                {
                    self.set_field(&key, "summary", json!(summary), cx);
                }
                cx.notify();
                return Typed::Leave;
            }
            _ => {}
        }
        cx.notify();
        Typed::Kept
    }

    /// The description by its paragraphs, each with `+` on its edge for a note to the agent, a picture where one
    /// stands; Edit on the head (or E, or a double click) edits it in place.
    /// The people a mention on issue `key` names: its reporter, assignee and commenters, the board's assignees, you.
    fn mentionable(&self, key: &str, cx: &App) -> Named {
        let s = self.store.read(cx);
        let Some(issue) = s.issue(key) else {
            return Vec::new();
        };
        let cloud = s.site_of(&issue.site).is_some_and(|x| x.email.is_some());
        let rows = s.board_rows().into_iter().flatten();
        let people = (issue.reporter.iter())
            .chain(issue.row.assignee.iter())
            .chain(issue.comments.iter().filter_map(|c| c.author.as_ref()))
            .chain(rows.filter_map(|r| r.assignee.as_ref()))
            .chain(s.me.get(&issue.site));
        let mut out: Named = Vec::new();
        for p in people {
            let tracker = if cloud {
                format!("accountid:{}", p.id)
            } else {
                p.id.clone()
            };
            if !out.iter().any(|(t, _)| *t == tracker) {
                out.push((tracker, handle(&p.name)));
            }
        }
        out
    }

    /// `text` as markdown is written here, the issue's people named.
    fn readable(&self, key: &str, md: &str, cx: &App) -> String {
        named(md, &self.mentionable(key, cx))
    }

    /// `text` written here as the tracker takes it: wiki, the names mentioned as accounts.
    fn as_wiki(&self, key: &str, text: &str, cx: &App) -> String {
        wiki::from_markdown(&unnamed(text, &self.mentionable(key, cx)))
    }

    fn description(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        if self.issue.desc.is_some() {
            return self.description_editor(issue, cx);
        }
        let unseen = self
            .store
            .read(cx)
            .changed
            .get(&issue.row.key)
            .is_some_and(|c| c.unseen.contains("Description"));
        let head = div()
            .group("desc-head")
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Description"))
            .children(dot(unseen))
            .child(div().flex_1())
            .child(
                look::small_button("tasks-desc-edit", "Edit")
                    .child(
                        icons::icon("pencil")
                            .size(px(12.))
                            .text_color(theme::muted()),
                    )
                    .tip("Edit the description  E · or a double-click in it")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.edit_description(cx))),
            );
        let people = self.mentionable(&issue.row.key, cx);
        let paras: Vec<String> = (model::paragraphs(&issue.description).iter())
            .map(|p| named(p, &people))
            .collect();
        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .pl(px(12.))
            .pr(px(24.))
            .py(px(14.));
        if paras.is_empty() {
            body = body.child(
                div()
                    .pl(px(16.))
                    .text_size(px(13.))
                    .text_color(theme::faint())
                    .child("No description"),
            );
        }
        for (ix, para) in paras.into_iter().enumerate() {
            let n = ix + 1;
            let (key, site) = (issue.row.key.clone(), issue.site.clone());
            let on = self.note.as_ref().is_some_and(|note| matches!(&note.spot, Spot::Para { key: k, para, .. } if *k == key && *para == n));
            // A picture standing alone: drawn where it stands.
            let picture = picture_of(&para, &issue.attachments);
            let content: AnyElement = match picture {
                Some(a) => self.picture(&a, cx),
                None => div()
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .text_color(theme::text())
                    .child(markdown::body(
                        &para,
                        true,
                        format!("tasks-para-{n}"),
                        None,
                        None,
                        cx,
                    ))
                    .into_any_element(),
            };
            body = body.child(
                div()
                    .id(SharedString::from(format!("para-{n}")))
                    .group("para")
                    .relative()
                    .rounded(px(4.))
                    .pl(px(28.))
                    .pr(px(6.))
                    .py(px(2.))
                    .when(on, |d| d.bg(theme::agent_bg()))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.issue.hovered_para = Some(n);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                        if e.click_count() > 1 {
                            this.edit_description(cx);
                        }
                    }))
                    .child(
                        div()
                            .id(SharedString::from(format!("para-add-{n}")))
                            .absolute()
                            .left(px(4.))
                            .top(px(3.))
                            .size(px(18.))
                            .rounded(px(4.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(theme::agent())
                            .cursor_pointer()
                            .invisible()
                            .group_hover("para", |s| s.visible())
                            .when(on, |s| s.visible())
                            .child(
                                icons::icon("plus")
                                    .size(px(12.))
                                    .text_color(theme::on_accent()),
                            )
                            .tip("Note for an agent on this paragraph  C")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.open_note(
                                    Spot::Para {
                                        key: key.clone(),
                                        site: site.clone(),
                                        para: n,
                                    },
                                    cx,
                                )
                            })),
                    )
                    .child(content),
            );
            if on {
                body = body.child(div().pl(px(28.)).child(self.composer(cx)));
            }
        }
        section_with(head).child(body).into_any_element()
    }

    /// E: the description in the composer's editor; kept on this Mac while it is open.
    pub fn edit_description(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.issue_key() else {
            return;
        };
        let s = self.store.read(cx);
        let Some(issue) = s.issue(&key) else {
            return;
        };
        let base = issue.description.clone();
        let kept = s.kept.edits.get(&key).cloned();
        let text = kept.unwrap_or_else(|| self.readable(&key, &wiki::to_markdown(&base), cx));
        self.issue.desc = Some((compose(Target::Request(0), &text), base));
        self.issue.conflict = None;
        self.frame.take_keyboard(INPUT_DESC, cx);
        cx.notify();
    }

    fn description_editor(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let Some((desc, _)) = &self.issue.desc else {
            return div().into_any_element();
        };
        let page = cx.entity();
        let acts: artoo_plugin_sdk::kit::composer::ComposeActs = Rc::new(move |act, cx| {
            page.update(cx, |this, cx| {
                if let (
                    Some((c, _)),
                    ComposeAct::Place {
                        at,
                        shift,
                        clicks,
                        drag,
                        lower,
                    },
                ) = (this.issue.desc.as_mut(), act)
                {
                    c.place(at, shift, clicks, drag, lower);
                    cx.notify();
                }
            });
        });
        let conflict = self.issue.conflict.clone();
        let tail = vec![
            div()
                .text_size(px(11.))
                .text_color(theme::faint())
                .child("Kept on this Mac")
                .into_any_element(),
            look::small_button("tasks-desc-cancel", "Cancel")
                .tip("Cancel  esc")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_description(cx)))
                .into_any_element(),
            controls::primary_button(
                "tasks-desc-save",
                if conflict.is_some() {
                    "Save mine"
                } else {
                    "Save"
                },
                "⌘↵",
            )
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save_description(cx)))
            .into_any_element(),
        ];
        let look = Look {
            tone: theme::accent(),
            placeholder: "What this issue is about".into(),
            head: None,
            lead: None,
            tools: Vec::new(),
            tail,
            folded: false,
            focused: true,
            lines: None,
            completion: None,
            measure: div().into_any_element(),
            link_base: None,
        };
        let input = desc.render_input(look, acts, cx);
        let editor = artoo_plugin_sdk::shell::input_box(
            div().id("tasks-desc-box").child(input),
            INPUT_DESC,
            &self.frame,
        );
        let warn =
            conflict.map(|(who, theirs)| {
                let mine = desc.edit.text().to_owned();
                let compare = self.issue.compare;
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .p(px(10.))
                    .rounded(px(6.))
                    .bg(theme::warning_bg())
                    .text_size(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().size(px(6.)).rounded_full().bg(theme::warning()))
                            .child(div().flex_1().text_color(theme::text()).child(format!(
                                "{who} changed the description while you edited it"
                            )))
                            .child(
                                look::small_button("tasks-desc-compare", "Compare").on_click(
                                    cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.issue.compare = !this.issue.compare;
                                        cx.notify();
                                    }),
                                ),
                            )
                            .child(
                                look::small_button("tasks-desc-theirs", "Keep theirs").on_click(
                                    cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.cancel_description(cx)
                                    }),
                                ),
                            ),
                    )
                    .when(compare, |d| {
                        d.child(diff_lines(&wiki::to_markdown(&theirs), &mine))
                    })
            });
        let _ = issue;
        section("Description")
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .p(px(12.))
                    .children(warn)
                    .child(editor),
            )
            .into_any_element()
    }

    pub fn cancel_description(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.issue_key() {
            self.store.update(cx, |s, _| {
                s.kept.edits.remove(&key);
                s.kept.write();
            });
        }
        self.issue.desc = None;
        self.issue.conflict = None;
        self.frame.leave_keyboard(INPUT_DESC, cx);
        cx.notify();
    }

    /// Save: unless someone else changed the description since the editing began (read again just now), which
    /// says so and waits for Save mine.
    pub fn save_description(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.issue_key() else {
            return;
        };
        let Some((desc, base)) = &self.issue.desc else {
            return;
        };
        let text = desc.edit.text().trim().to_owned();
        let s = self.store.read(cx);
        let now = s
            .issue(&key)
            .map(|i| i.description.clone())
            .unwrap_or_default();
        if self.issue.conflict.is_none() && now != *base {
            let who = s
                .changed
                .get(&key)
                .map(|c| c.who.clone())
                .unwrap_or_else(|| "Someone".into());
            self.issue.conflict = Some((who, now));
            cx.notify();
            return;
        }
        let wiki_text = self.as_wiki(&key, &text, cx);
        let k = key.clone();
        let v = wiki_text.clone();
        self.store.update(cx, |s, cx| {
            s.kept.edits.remove(&k);
            s.kept.write();
            s.change(
                &k.clone(),
                "description",
                Write::edited(&k, json!({ "description": wiki_text })),
                move |s| s.edit_issue(&k, |i| i.description = v.clone()),
                |_, _, _| {},
                cx,
            )
        });
        self.issue.desc = None;
        self.issue.conflict = None;
        self.frame.leave_keyboard(INPUT_DESC, cx);
        cx.notify();
    }

    pub fn desc_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let key = &event.keystroke;
        if key.modifiers.platform && key.key == "enter" {
            self.save_description(cx);
            return if self.issue.desc.is_some() {
                Typed::Kept
            } else {
                Typed::Leave
            };
        }
        let Some((desc, _)) = self.issue.desc.as_mut() else {
            return Typed::Leave;
        };
        let typed = desc.edit_key(key, cx);
        let text = desc.edit.text().to_owned();
        if let Some(k) = self.issue_key() {
            self.store.update(cx, |s, _| {
                s.kept.edits.insert(k, text);
                s.kept.write();
            });
        }
        cx.notify();
        match typed {
            Written::Cancel => {
                self.cancel_description(cx);
                Typed::Leave
            }
            Written::Leave => Typed::Leave,
            Written::Ignored | Written::Changed => Typed::Kept,
        }
    }

    /// A picture from the attachments, read with the site's token, at its own size up to the column's; a click
    /// opens it whole.
    fn picture(&mut self, a: &Attachment, cx: &mut Context<Self>) -> AnyElement {
        let image = self.store.read(cx).images.get(&a.id).cloned();
        let id = a.id.clone();
        let body: AnyElement = match image {
            Some(Read::Done(image)) => img(image)
                .max_w_full()
                .max_h(px(320.))
                .object_fit(ObjectFit::Contain)
                .rounded(px(6.))
                .into_any_element(),
            Some(Read::Failed(why)) => div()
                .text_size(px(12.))
                .text_color(theme::faint())
                .child(why)
                .into_any_element(),
            _ => div()
                .w(px(240.))
                .h(px(140.))
                .rounded(px(6.))
                .bg(theme::hover())
                .into_any_element(),
        };
        div()
            .id(SharedString::from(format!("pic-{}", a.id)))
            .flex()
            .flex_col()
            .gap(px(4.))
            .cursor_pointer()
            .child(body)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child(
                        icons::icon("image")
                            .size(px(11.))
                            .text_color(theme::faint()),
                    )
                    .child(a.name.clone()),
            )
            .tip(format!("{} · click to open", a.name))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.issue.viewing = Some(id.clone());
                cx.notify();
            }))
            .into_any_element()
    }

    /// Sub-tasks: how many done, each with its state; the empty row last, always there.
    fn subtasks(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let done = issue.subtasks.iter().filter(|s| s.state.done()).count();
        let all = issue.subtasks.len();
        let head = div()
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Sub-tasks"))
            .when(all > 0, |d| {
                d.child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(format!("{done} of {all}")),
                )
                .child(look::progress(done, all))
            });
        let mut list = div().flex().flex_col();
        let site = issue.site.clone();
        let me_of = |p: Option<&Person>| self.store.read(cx).is_me(&site, p);
        let rows: Vec<(Row, bool)> = issue
            .subtasks
            .iter()
            .map(|s| (s.clone(), me_of(s.assignee.as_ref())))
            .collect();
        let refused_key = self.refusal.as_ref().map(|(_, k)| k.clone());
        for (s, me) in rows {
            let key = s.key.clone();
            let bad = refused_key.as_deref() == Some(&s.key);
            let open_key = key.clone();
            let face = match &s.assignee {
                Some(p) => look::face(&p.name, me, 16.).into_any_element(),
                None => icons::icon("user")
                    .size(px(13.))
                    .text_color(theme::faint())
                    .into_any_element(),
            };
            list = list.child(
                div()
                    .id(SharedString::from(format!("sub-{key}")))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(36.))
                    .px(px(16.))
                    .border_b_1()
                    .border_color(theme::border())
                    .when(bad, |d| d.bg(theme::danger_bg()))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child(look::kind_mark(&s.kind, true))
                    .child(look::key(&s.key))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .text_color(if s.state.done() {
                                theme::muted()
                            } else {
                                theme::text()
                            })
                            .when(s.state.done(), |d| d.line_through())
                            .child(s.summary.clone()),
                    )
                    .child(face)
                    .child(
                        div()
                            .w(px(110.))
                            .flex()
                            .justify_end()
                            .child(self.state_control(&s, Anchor::Sub(key.clone()), true, cx)),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.open_issue(&open_key, cx)
                    })),
            );
        }
        list = list.child(self.new_sub_row(issue, cx));
        section_with(head).child(list).into_any_element()
    }

    /// The empty row: typed into at once (N puts the caret there); ↵ creates and keeps the caret for the next, ⇧↵
    /// opens New task with the parent, esc lets it go.
    fn new_sub_row(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let typing = self.frame.typing(cx) == Some(INPUT_SUB);
        let shown = self.issue.sub.clone();
        let project = issue.row.project.clone();
        if !typing && shown.is_empty() {
            return artoo_plugin_sdk::shell::input_box(
                div()
                    .id("tasks-sub-new")
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(36.))
                    .px(px(16.))
                    .text_size(px(13.))
                    .text_color(theme::faint())
                    .cursor_text()
                    .hover(|s| s.bg(theme::hover()))
                    .child(icons::icon("plus").size(px(13.)).text_color(theme::faint()))
                    .child(div().flex_1().child("Add a sub-task…"))
                    .child(div().text_size(px(11.)).child("N"))
                    .tip("Add a sub-task  N")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_sub(cx))),
                INPUT_SUB,
                &self.frame,
            )
            .into_any_element();
        }
        let edit = &self.issue.sub_edit;
        let field = input::field(
            &shown,
            typing,
            "Summary of the sub-task",
            &input::Look::default(),
            || {
                input::text_body(
                    &shown,
                    edit,
                    input::Look::default(),
                    Default::default(),
                    None,
                )
                .into_any_element()
            },
        );
        artoo_plugin_sdk::shell::input_box(
            div()
                .id("tasks-sub-typed")
                .flex()
                .items_center()
                .gap(px(10.))
                .h(px(36.))
                .px(px(16.))
                .text_size(px(13.))
                .bg(theme::selected())
                .child(look::kind_mark("Sub-task", true))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_family(theme::code_font())
                        .text_color(theme::faint())
                        .child(format!("{project}-…")),
                )
                .child(field.flex_1())
                .child(
                    div()
                        .flex()
                        .gap(px(10.))
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child("↵ create")
                        .child(
                            div()
                                .id("tasks-sub-more")
                                .child("⇧↵ more fields")
                                .tip(format!("New task with Parent {}", issue.row.key)),
                        )
                        .child("esc"),
                ),
            INPUT_SUB,
            &self.frame,
        )
        .into_any_element()
    }

    pub fn start_sub(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.issue_key() {
            let host = self
                .store
                .read(cx)
                .issue(&key)
                .map(|i| (i.site.clone(), i.row.project.clone()));
            if let Some((host, project)) = host {
                self.store
                    .update(cx, |s, cx| s.read_kinds(&host, &project, cx));
            }
        }
        self.frame.take_keyboard(INPUT_SUB, cx);
        cx.notify();
    }

    pub fn sub_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let key = &event.keystroke;
        let edit = input::edit(&mut self.issue.sub, &mut self.issue.sub_edit, key, cx);
        match edit {
            input::Edit::Escape => {
                self.issue.sub.clear();
                self.issue.sub_edit = TextEdit::new(String::new());
                cx.notify();
                return Typed::Leave;
            }
            input::Edit::Enter { shift } => {
                let summary = self.issue.sub.trim().to_owned();
                let Some(parent) = self.issue_key() else {
                    return Typed::Leave;
                };
                if shift {
                    self.issue.sub.clear();
                    self.issue.sub_edit = TextEdit::new(String::new());
                    self.open_sheet(None, Some((parent, summary)), cx);
                    return Typed::Leave;
                }
                if !summary.is_empty() {
                    self.create_sub(&parent, &summary, cx);
                    self.issue.sub.clear();
                    self.issue.sub_edit = TextEdit::new(String::new());
                }
            }
            _ => {}
        }
        cx.notify();
        Typed::Kept
    }

    /// A sub-task of `parent` made, its summary `summary`, of the project's sub-task kind.
    fn create_sub(&mut self, parent: &str, summary: &str, cx: &mut Context<Self>) {
        let s = self.store.read(cx);
        let Some(issue) = s.issue(parent) else {
            return;
        };
        let (host, project) = (issue.site.clone(), issue.row.project.clone());
        let kind = match s.kinds.get(&(host.clone(), project.clone())) {
            Some(Read::Done(kinds)) => kinds.iter().find(|k| k.subtask).cloned(),
            _ => None,
        };
        let Some(kind) = kind else {
            self.frame
                .error(format!("{project} has no sub-task type").into(), cx);
            return;
        };
        let fields = json!({
            "project": { "key": project },
            "parent": { "key": parent },
            "summary": summary,
            "issuetype": { "id": kind.id },
        });
        let p = parent.to_owned();
        let placeholder = Row {
            key: format!("{project}-…"),
            summary: summary.to_owned(),
            kind: kind.name.clone(),
            subtask: true,
            state: crate::model::State {
                id: String::new(),
                name: "To Do".into(),
                category: "new".into(),
            },
            parent: Some(p.clone()),
            project: project.clone(),
            ..Default::default()
        };
        let frame = self.frame.clone();
        self.store.update(cx, |s, cx| {
            s.change(
                &p,
                "subtasks",
                Write::created(fields),
                |s| s.edit_issue(&p, |i| i.subtasks.push(placeholder.clone())),
                move |_, read, cx| {
                    if let Err(why) = read {
                        frame.error(why.into(), cx);
                    }
                },
                cx,
            )
        });
    }

    /// Linked issues by how they relate, each opening; Link: the relation and a search by key or words.
    fn links(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let mut groups: Vec<(String, Vec<Link>)> = Vec::new();
        for l in &issue.links {
            match groups.iter_mut().find(|(r, _)| *r == l.relation) {
                Some((_, v)) => v.push(l.clone()),
                None => groups.push((l.relation.clone(), vec![l.clone()])),
            }
        }
        let open = self.issue.link.is_some();
        let head = div()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Linked issues"),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(issue.links.len().to_string()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .relative()
                    .child(
                        look::small_button("tasks-link-add", "Link")
                            .child(icons::icon("link").size(px(12.)).text_color(theme::muted()))
                            .when(open, |d| d.border_color(theme::accent()))
                            .tip("Link an issue  L")
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.open_link(cx)),
                            ),
                    )
                    .child({
                        let spot = self.issue.link_spot.clone();
                        canvas(move |b, _, _| spot.set(Some(b)), |_, _, _, _| {})
                            .absolute()
                            .size_full()
                    })
                    .children(open.then(|| crate::pop::anchored_under(self.link_card(cx)))),
            );
        let mut body = div().flex().flex_col();
        for (relation, links) in groups {
            body = body.child(
                div()
                    .px(px(16.))
                    .pt(px(10.))
                    .pb(px(4.))
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child(relation),
            );
            for l in links {
                let key = l.other.key.clone();
                let blocks = l.relation == "is blocked by" && !l.other.state.done();
                body = body.child(
                    div()
                        .id(SharedString::from(format!("link-{}", l.id)))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .h(px(34.))
                        .px(px(16.))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()))
                        .when(blocks, |d| d.child(look::blocked_mark()))
                        .child(look::kind_mark(&l.other.kind, false))
                        .child(look::key(&l.other.key))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(13.))
                                .text_color(theme::text())
                                .child(l.other.summary.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(look::state_color(&l.other.state))
                                .child(look::display_state(&l.other.state.name)),
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.open_issue(&key, cx)
                        })),
                );
            }
        }
        section_with(head).child(body.pb(px(6.))).into_any_element()
    }

    /// L: the Link popover.
    /// The Link button scrolled near the page's top when it is out of view or its card has no room under it.
    fn link_into_view(&self) {
        let Some(b) = self.issue.link_spot.get() else {
            return;
        };
        let view = self.issue_scroll.bounds();
        if b.top() >= view.top() && b.bottom() + px(280.) <= view.bottom() {
            return;
        }
        let off = self.issue_scroll.offset();
        let y = (off.y - (b.top() - view.top() - px(16.)))
            .min(px(0.))
            .max(-self.issue_scroll.max_offset().y);
        self.issue_scroll.set_offset(point(off.x, y));
    }

    pub fn open_link(&mut self, cx: &mut Context<Self>) {
        if self.issue.link.take().is_some() {
            self.frame.leave_keyboard(crate::page::INPUT_POP, cx);
            cx.notify();
            return;
        }
        let host = self
            .issue_key()
            .and_then(|k| self.store.read(cx).issue(&k).map(|i| i.site.clone()));
        if let Some(host) = host {
            self.store.update(cx, |s, cx| s.read_link_kinds(&host, cx));
        }
        self.issue.link = Some(LinkPick {
            relation: 0,
            words: String::new(),
            edit: TextEdit::new(String::new()),
            found: Read::Done(Vec::new()),
            at: 0,
        });
        self.frame.take_keyboard(crate::page::INPUT_POP, cx);
        self.issue.to_link = 3;
        cx.notify();
    }

    /// The relations to pick from: each link type's two ways, `(type, words, outward)`.
    fn relations(&self, cx: &Context<Self>) -> Vec<(String, String, bool)> {
        let s = self.store.read(cx);
        let host = self
            .issue_key()
            .and_then(|k| s.issue(&k).map(|i| i.site.clone()))
            .unwrap_or_default();
        let kinds = match s.link_kinds.get(&host) {
            Some(Read::Done(k)) => k.clone(),
            _ => Vec::new(),
        };
        let mut out = Vec::new();
        for (name, inward, outward) in kinds {
            out.push((name.clone(), inward.clone(), false));
            if outward != inward {
                out.push((name, outward, true));
            }
        }
        out
    }

    fn link_card(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let relations = self.relations(cx);
        let Some(link) = self.issue.link.as_ref() else {
            return div().into_any_element();
        };
        let typing = self.frame.typing(cx) == Some(crate::page::INPUT_POP);
        let shown = link.words.clone();
        let field = input::field(
            &shown,
            typing,
            "Key or words",
            &input::Look::default(),
            || {
                input::text_body(
                    &shown,
                    &link.edit,
                    input::Look::default(),
                    Default::default(),
                    None,
                )
                .into_any_element()
            },
        );
        let mut rel = div().flex().flex_wrap().gap(px(4.));
        for (ix, (_, words, _)) in relations.iter().enumerate() {
            rel = rel.child(
                controls::chip(
                    SharedString::from(format!("rel-{ix}")),
                    words.clone(),
                    ix == link.relation,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(l) = this.issue.link.as_mut() {
                        l.relation = ix;
                    }
                    cx.notify();
                })),
            );
        }
        let mut found = div().flex().flex_col();
        match &link.found {
            Read::Reading => {
                found = found.child(
                    div()
                        .px(px(8.))
                        .py(px(6.))
                        .text_size(px(12.))
                        .text_color(theme::faint())
                        .child("Searching…"),
                )
            }
            Read::Failed(why) => {
                found = found.child(
                    div()
                        .px(px(8.))
                        .py(px(6.))
                        .text_size(px(12.))
                        .text_color(theme::faint())
                        .child(why.clone()),
                )
            }
            Read::Done(rows) => {
                for (ix, r) in rows.iter().enumerate() {
                    let other = r.key.clone();
                    found = found.child(
                        div()
                            .id(SharedString::from(format!("link-found-{ix}")))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .h(px(28.))
                            .px(px(8.))
                            .rounded(px(5.))
                            .cursor_pointer()
                            .when(ix == link.at, |d| d.bg(theme::selected()))
                            .hover(|s| s.bg(theme::hover()))
                            .child(look::kind_mark(&r.kind, r.subtask))
                            .child(look::key(&r.key))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.))
                                    .child(r.summary.clone()),
                            )
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.make_link(&other, cx)
                            })),
                    );
                }
            }
        }
        div()
            .id("tasks-link")
            .w(px(360.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(10.))
            .rounded(px(8.))
            .bg(theme::panel())
            .border_1()
            .border_color(theme::island_border())
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.issue.link = None;
                this.frame.leave_keyboard(crate::page::INPUT_POP, cx);
                cx.notify();
            }))
            .child(rel)
            .child(artoo_plugin_sdk::shell::input_box(
                div()
                    .id("tasks-link-field")
                    .h(px(28.))
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
                    .text_size(px(12.))
                    .child(
                        icons::icon("search")
                            .size(px(12.))
                            .text_color(theme::faint()),
                    )
                    .child(field),
                crate::page::INPUT_POP,
                &self.frame,
            ))
            .child(found)
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child("↵ links · esc"),
            )
            .into_any_element()
    }

    /// A key while the Link popover has the keyboard.
    pub fn link_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let key = &event.keystroke;
        let Some(link) = self.issue.link.as_mut() else {
            return Typed::Leave;
        };
        match key.key.as_str() {
            "up" | "down" => {
                let n = match &link.found {
                    Read::Done(r) => r.len(),
                    _ => 0,
                };
                link.at = if key.key == "down" {
                    (link.at + 1).min(n.saturating_sub(1))
                } else {
                    link.at.saturating_sub(1)
                };
                cx.notify();
                return Typed::Kept;
            }
            "tab" => {
                let n = self.relations(cx).len().max(1);
                if let Some(l) = self.issue.link.as_mut() {
                    l.relation = if key.modifiers.shift {
                        (l.relation + n - 1) % n
                    } else {
                        (l.relation + 1) % n
                    };
                }
                cx.notify();
                return Typed::Kept;
            }
            _ => {}
        }
        match input::edit(&mut link.words, &mut link.edit, key, cx) {
            input::Edit::Escape => {
                self.issue.link = None;
                cx.notify();
                return Typed::Leave;
            }
            input::Edit::Enter { .. } => {
                let picked = match &link.found {
                    Read::Done(rows) => rows.get(link.at).map(|r| r.key.clone()),
                    _ => None,
                };
                let typed = link.words.trim().to_uppercase();
                let other = picked.or_else(|| typed.split_once('-').is_some().then_some(typed));
                if let Some(other) = other {
                    self.make_link(&other, cx);
                }
                return Typed::Leave;
            }
            input::Edit::Changed => self.search_links(cx),
            _ => {}
        }
        cx.notify();
        Typed::Kept
    }

    /// What is typed in Link searched: a key alone, else the words in summaries.
    fn search_links(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.issue.link.as_mut() else {
            return;
        };
        let words = link.words.trim().to_owned();
        if words.len() < 2 {
            link.found = Read::Done(Vec::new());
            return;
        }
        link.found = Read::Reading;
        link.at = 0;
        let Some(site) = self
            .issue_key()
            .and_then(|k| self.store.read(cx).site_for(&k))
        else {
            return;
        };
        let jql = if words.contains('-')
            && words.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            format!("key = \"{}\"", words.to_uppercase())
        } else {
            format!(
                "text ~ \"{}\" ORDER BY updated DESC",
                words.replace('"', "")
            )
        };
        let task = cx.background_spawn(async move { model::search_for(&site, &jql, 8, "find") });
        cx.spawn(async move |this, cx| {
            let found = task.await;
            this.update(cx, |p, cx| {
                // Not the issue itself, nor one it is linked to already.
                let here = p.issue_key();
                let linked: Vec<String> = (here.as_ref())
                    .and_then(|k| p.store.read(cx).issue(k))
                    .map(|i| i.links.iter().map(|l| l.other.key.clone()).collect())
                    .unwrap_or_default();
                if let Some(l) = p.issue.link.as_mut()
                    && l.words.trim() == words
                {
                    l.found = match found {
                        Ok(rows) => Read::Done(
                            rows.into_iter()
                                .filter(|r| {
                                    Some(&r.key) != here.as_ref() && !linked.contains(&r.key)
                                })
                                .collect(),
                        ),
                        Err(why) => Read::Failed(why),
                    };
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The issue on screen linked to `other` as the relation picked says.
    fn make_link(&mut self, other: &str, cx: &mut Context<Self>) {
        let relations = self.relations(cx);
        let Some(link) = self.issue.link.take() else {
            return;
        };
        self.frame.leave_keyboard(crate::page::INPUT_POP, cx);
        let Some(key) = self.issue_key() else {
            return;
        };
        let Some((kind, words, outward)) = relations.get(link.relation).cloned() else {
            return;
        };
        // "from blocks to": this issue is the outward one when the relation read from it is the outward words.
        let write = if outward {
            Write::linked(&kind, &key, other)
        } else {
            Write::linked(&kind, other, &key)
        };
        let brief = self.store.read(cx).row(other).map(|r| model::Brief {
            key: r.key.clone(),
            summary: r.summary.clone(),
            kind: r.kind.clone(),
            state: r.state.clone(),
        });
        let brief = brief.unwrap_or(model::Brief {
            key: other.to_owned(),
            ..Default::default()
        });
        let k = key.clone();
        self.store.update(cx, |s, cx| {
            s.change(
                &k.clone(),
                "links",
                write,
                |s| {
                    s.edit_issue(&k, |i| {
                        i.links.push(Link {
                            id: String::new(),
                            kind: kind.clone(),
                            relation: words.clone(),
                            other: brief.clone(),
                        })
                    })
                },
                |_, _, _| {},
                cx,
            )
        });
        cx.notify();
    }

    /// Attachments: pictures as tiles, the rest as rows; + Add.
    fn attachments(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let head = div()
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Attachments"))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(issue.attachments.len().to_string()),
            )
            .child(div().flex_1())
            .child(
                look::small_button("tasks-attach", "Add")
                    .child(icons::icon("plus").size(px(12.)).text_color(theme::muted()))
                    .tip("Add a file  ⇧⌘A; or drop one on the page")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.pick_attachment(cx))),
            );
        let (pictures, files): (Vec<&Attachment>, Vec<&Attachment>) =
            issue.attachments.iter().partition(|a| a.image());
        let mut tiles = div().flex().flex_wrap().gap(px(10.)).p(px(12.));
        for a in pictures {
            let image = self.store.read(cx).images.get(&a.id).cloned();
            let id = a.id.clone();
            tiles = tiles.child(
                div()
                    .id(SharedString::from(format!("tile-{}", a.id)))
                    .w(px(150.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .cursor_pointer()
                    .child(
                        div()
                            .h(px(96.))
                            .w_full()
                            .rounded(px(6.))
                            .overflow_hidden()
                            .bg(theme::base())
                            .border_1()
                            .border_color(theme::border())
                            .children(
                                image
                                    .and_then(|i| i.done().cloned())
                                    .map(|i| img(i).size_full().object_fit(ObjectFit::Cover)),
                            ),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(theme::muted())
                            .child(a.name.clone()),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.issue.viewing = Some(id.clone());
                        cx.notify();
                    })),
            );
        }
        let mut rows = div().flex().flex_col();
        for a in files {
            let url = a.content.clone();
            rows = rows.child(
                div()
                    .id(SharedString::from(format!("file-{}", a.id)))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .h(px(34.))
                    .px(px(16.))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child(icons::icon("file").size(px(14.)).text_color(theme::muted()))
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(13.))
                            .child(a.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme::faint())
                            .child(artoo_plugin_sdk::kit::format::bytes(a.size as usize)),
                    )
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            );
        }
        section_with(head)
            .child(tiles)
            .child(rows)
            .into_any_element()
    }

    /// + Add: a file picked and attached.
    pub fn pick_attachment(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.issue_key() else {
            return;
        };
        let Some(site) = self.store.read(cx).site_for(&key) else {
            return;
        };
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let frame = self.frame.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(file) = paths.into_iter().next() else {
                return;
            };
            let path = format!("/rest/api/2/issue/{key}/attachments");
            let sent = cx
                .background_spawn(async move { crate::jira::attach(&site, &path, &file) })
                .await;
            this.update(cx, |p, cx| {
                match sent {
                    Ok(_) => p.read_again(cx),
                    Err(why) => frame.error(why.into(), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The picture open over the window: its name, size, who and when; ← → through the issue's pictures; esc.
    fn viewer(&mut self, issue: &Issue, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pictures: Vec<&Attachment> = issue.attachments.iter().filter(|a| a.image()).collect();
        let at = pictures.iter().position(|a| a.id == id)?;
        let a = pictures[at];
        let image = self
            .store
            .read(cx)
            .images
            .get(&a.id)
            .and_then(|r| r.done().cloned());
        let step = |by: isize| {
            let n = pictures.len() as isize;
            pictures[((at as isize + by).rem_euclid(n)) as usize]
                .id
                .clone()
        };
        let (prev, next) = (step(-1), step(1));
        let url = a.content.clone();
        let who = a
            .author
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default();
        Some(
            div()
                .id("tasks-viewer")
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .bg(gpui::black().opacity(0.85))
                .occlude()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .h(px(44.))
                        .px(px(16.))
                        .text_size(px(12.))
                        .text_color(gpui::white())
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(a.name.clone()),
                        )
                        .child(div().opacity(0.7).child(format!(
                            "{} · {who} · {}",
                            artoo_plugin_sdk::kit::format::bytes(a.size as usize),
                            model::ago(&a.created)
                        )))
                        .child(div().flex_1())
                        .child(div().opacity(0.7).child(format!(
                            "{} of {}",
                            at + 1,
                            pictures.len()
                        )))
                        .child(
                            look::small_button("viewer-prev", "←")
                                .tip("Previous  ←")
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.issue.viewing = Some(prev.clone());
                                    cx.notify();
                                })),
                        )
                        .child(
                            look::small_button("viewer-next", "→")
                                .tip("Next  →")
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.issue.viewing = Some(next.clone());
                                    cx.notify();
                                })),
                        )
                        .child(
                            look::small_button("viewer-jira", "Open in Jira")
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                        )
                        .child(
                            look::small_button("viewer-close", "Close")
                                .tip("Close  esc")
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.issue.viewing = None;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .p(px(24.))
                        .children(image.map(|i| {
                            img(i)
                                .max_w_full()
                                .max_h_full()
                                .object_fit(ObjectFit::Contain)
                        })),
                )
                .into_any_element(),
        )
    }

    /// Comments: each with who and when, yours with Edit and Delete, others' with Reply; an agent's waiting for
    /// Post; the composer last, to everyone or (⇥) to the agent.
    fn comments(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let pending: Vec<Pending> = self
            .store
            .read(cx)
            .kept
            .pending
            .iter()
            .filter(|p| p.key == issue.row.key)
            .cloned()
            .collect();
        let head = div()
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Comments"))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(issue.comments.len().to_string()),
            )
            .when(!pending.is_empty(), |d| {
                d.child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::agent())
                        .child(format!("· {} pending", pending.len())),
                )
            });
        let mut list = div().flex().flex_col().gap(px(14.)).px(px(16.)).py(px(12.));
        if issue.comments.is_empty() && pending.is_empty() {
            list = list.child(
                div()
                    .text_size(px(13.))
                    .text_color(theme::faint())
                    .child("No comments"),
            );
        }
        for c in &issue.comments {
            let editing = self.issue.editing.as_deref() == Some(&c.id);
            if editing {
                list = list.child(self.comment_composer(issue, cx));
                continue;
            }
            list = list.child(self.comment(issue, c, cx));
        }
        for p in pending {
            list = list.child(self.pending(&p, cx));
        }
        if self.issue.editing.is_none() {
            list = list.child(self.comment_composer(issue, cx));
        }
        section_with(head).child(list).into_any_element()
    }

    fn comment(&mut self, issue: &Issue, c: &Comment, cx: &mut Context<Self>) -> AnyElement {
        let s = self.store.read(cx);
        let mine = s.is_me(&issue.site, c.author.as_ref());
        let who = c
            .author
            .as_ref()
            .map_or("Someone".to_owned(), |p| p.name.clone());
        let edited = !c.updated.is_empty() && c.updated != c.created;
        let body = self.readable(&issue.row.key, &wiki::to_markdown(c.body.trim()), cx);
        let (id, id2) = (c.id.clone(), c.id.clone());
        let mention = c.author.clone();
        let acts = if mine {
            div()
                .flex()
                .gap(px(6.))
                .invisible()
                .group_hover("cmt", |s| s.visible())
                .child(
                    div()
                        .id(SharedString::from(format!("cmt-edit-{}", c.id)))
                        .cursor_pointer()
                        .child(
                            icons::icon("pencil")
                                .size(px(13.))
                                .text_color(theme::muted()),
                        )
                        .tip("Edit  E")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.edit_comment(&id, cx)
                        })),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("cmt-del-{}", c.id)))
                        .cursor_pointer()
                        .child(
                            icons::icon("trash")
                                .size(px(13.))
                                .text_color(theme::muted()),
                        )
                        .tip("Delete")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.delete_comment(&id2, cx)
                        })),
                )
        } else {
            div()
                .flex()
                .invisible()
                .group_hover("cmt", |s| s.visible())
                .child(
                    div()
                        .id(SharedString::from(format!("cmt-reply-{}", c.id)))
                        .cursor_pointer()
                        .child(
                            icons::icon("corner-left-up")
                                .size(px(13.))
                                .text_color(theme::muted()),
                        )
                        .tip("Reply  R")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.reply_to(mention.clone(), cx)
                        })),
                )
        };
        div()
            .group("cmt")
            .flex()
            .gap(px(10.))
            .child(look::face(&who, mine, 24.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child(if mine { "You".to_owned() } else { who.clone() }),
                            )
                            .child(
                                div()
                                    .text_color(theme::faint())
                                    .child(model::ago(&c.created)),
                            )
                            .when(edited, |d| {
                                d.child(div().text_color(theme::faint()).child("· edited"))
                            })
                            .child(div().flex_1())
                            .child(acts),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .text_color(theme::text())
                            .child(markdown::body(
                                &body,
                                true,
                                format!("tasks-comment-{}", c.id),
                                None,
                                None,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    /// An agent's comment for the issue: pending, seen by you alone until you post it; Edit, Discard, Post ⌘↵.
    fn pending(&mut self, p: &Pending, cx: &mut Context<Self>) -> AnyElement {
        let (id, id2, id3) = (p.id, p.id, p.id);
        div()
            .flex()
            .gap(px(10.))
            .p(px(10.))
            .rounded(px(8.))
            .bg(theme::agent_bg())
            .border_1()
            .border_color(look::tint(theme::agent(), 0.4))
            .child(
                div()
                    .size(px(24.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme::agent())
                    .child(
                        icons::icon("bot")
                            .size(px(13.))
                            .text_color(theme::on_accent()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child(p.agent.clone()),
                            )
                            .child(
                                div()
                                    .text_color(theme::faint())
                                    .child(artoo_plugin_sdk::kit::format::ago(p.at)),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("pend-tag-{}", p.id)))
                                    .px(px(6.))
                                    .rounded_full()
                                    .bg(look::tint(theme::agent(), 0.2))
                                    .text_color(theme::agent())
                                    .text_size(px(11.))
                                    .child("pending")
                                    .tip(format!(
                                        "Written by {}. Only you see it until you post it.",
                                        p.agent
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .text_color(theme::text())
                            .child(markdown::body(
                                &self.readable(&p.key, &p.body, cx),
                                true,
                                format!("tasks-pending-{}", p.id),
                                None,
                                None,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                look::small_button(
                                    SharedString::from(format!("pend-edit-{id}")),
                                    "Edit",
                                )
                                .tip("Edit  E")
                                .on_click(cx.listener(
                                    move |this, _: &ClickEvent, _, cx| this.edit_pending(id, cx),
                                )),
                            )
                            .child(
                                look::small_button(
                                    SharedString::from(format!("pend-discard-{id2}")),
                                    "Discard",
                                )
                                .tip("Discard  ⌫")
                                .on_click(cx.listener(
                                    move |this, _: &ClickEvent, _, cx| {
                                        this.discard_pending(id2, cx)
                                    },
                                )),
                            )
                            .child(div().flex_1())
                            .child(
                                controls::primary_button("pend-post", "Post", "⌘↵")
                                    .tip("Post to Jira  ⌘↵")
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.post_pending(id3, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn edit_pending(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(p) = self
            .store
            .read(cx)
            .kept
            .pending
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return;
        };
        self.issue.pending_edit = Some(id);
        self.issue.to_agent = false;
        let body = self.readable(&p.key, &p.body, cx);
        self.issue.comment = Some(compose(Target::Request(0), &body));
        self.frame.take_keyboard(INPUT_COMMENT, cx);
        self.issue.to_foot = 3;
        cx.notify();
    }

    fn discard_pending(&mut self, id: u64, cx: &mut Context<Self>) {
        self.store.update(cx, |s, cx| {
            s.kept.pending.retain(|p| p.id != id);
            s.kept.write();
            cx.notify();
        });
    }

    /// An agent's comment posted to the tracker as it stands.
    fn post_pending(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(p) = self
            .store
            .read(cx)
            .kept
            .pending
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return;
        };
        self.discard_pending(id, cx);
        self.post_comment(&p.key, &p.body, cx);
    }

    /// Body `text` posted on issue `key` as you.
    fn post_comment(&mut self, key: &str, text: &str, cx: &mut Context<Self>) {
        let body = self.as_wiki(key, text, cx);
        let k = key.to_owned();
        let s = self.store.read(cx);
        let host = s.issue(key).map(|i| i.site.clone()).unwrap_or_default();
        let me = s.me.get(&host).cloned();
        let b = body.clone();
        let frame = self.frame.clone();
        self.store.update(cx, |s, cx| {
            s.change(
                &k.clone(),
                "comments",
                Write::commented(&k, &body),
                move |s| {
                    let now = chrono_like_now();
                    s.edit_issue(&k, |i| {
                        i.comments.push(Comment {
                            id: String::new(),
                            author: me.clone(),
                            created: now.clone(),
                            updated: now.clone(),
                            body: b.clone(),
                        })
                    });
                    s.edit_row(&k, |r| r.comments += 1);
                },
                move |_, read, cx| {
                    if let Err(why) = read {
                        frame.error(why.into(), cx);
                    }
                },
                cx,
            )
        });
    }

    fn edit_comment(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(key) = self.issue_key() else {
            return;
        };
        let body = self.store.read(cx).issue(&key).and_then(|i| {
            i.comments
                .iter()
                .find(|c| c.id == id)
                .map(|c| wiki::to_markdown(&c.body))
        });
        let body = body.map(|b| self.readable(&key, &b, cx));
        self.issue.editing = Some(id.to_owned());
        self.issue.to_agent = false;
        self.issue.comment = Some(compose(Target::Request(0), &body.unwrap_or_default()));
        self.frame.take_keyboard(INPUT_COMMENT, cx);
        self.issue.to_foot = 3;
        cx.notify();
    }

    fn delete_comment(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(key) = self.issue_key() else {
            return;
        };
        let (k, cid) = (key.clone(), id.to_owned());
        self.store.update(cx, |s, cx| {
            s.change(
                &k.clone(),
                "comments",
                Write::comment_deleted(&k, &cid),
                |s| {
                    s.edit_issue(&k, |i| i.comments.retain(|c| c.id != cid));
                    s.edit_row(&k, |r| r.comments = r.comments.saturating_sub(1));
                },
                |_, _, _| {},
                cx,
            )
        });
    }

    /// R: a reply to the last comment someone else wrote.
    pub fn key_reply(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.issue_key() else {
            return;
        };
        let s = self.store.read(cx);
        let Some(issue) = s.issue(&key) else {
            return;
        };
        let who = (issue.comments.iter().rev())
            .find(|c| !s.is_me(&issue.site, c.author.as_ref()))
            .and_then(|c| c.author.clone());
        self.reply_to(who, cx);
    }

    /// R on someone's comment: the composer, mentioning them.
    fn reply_to(&mut self, who: Option<Person>, cx: &mut Context<Self>) {
        let mention = who
            .map(|p| format!("@{} ", handle(&p.name)))
            .unwrap_or_default();
        self.issue.editing = None;
        self.issue.to_agent = false;
        self.issue.comment = Some(compose(Target::Request(0), &mention));
        self.frame.take_keyboard(INPUT_COMMENT, cx);
        // The composer is at the comments' foot: the page goes there.
        self.issue.to_foot = 3;
        cx.notify();
    }

    /// The composer at the comments' foot: To: Everyone on the issue, or (⇥) your agent; Comment ⌘↵.
    fn comment_composer(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let agent = self.issue.to_agent;
        let editing = self.issue.editing.is_some() || self.issue.pending_edit.is_some();
        let key = issue.row.key.clone();
        if self.issue.comment.is_none() {
            return artoo_plugin_sdk::shell::input_box(
                div()
                    .id("tasks-comment-rest")
                    .h(px(36.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme::island_border())
                    .text_size(px(13.))
                    .text_color(theme::faint())
                    .cursor_text()
                    .child(format!("Comment on {key}…"))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.issue.comment = Some(compose(Target::Request(0), ""));
                        this.frame.take_keyboard(INPUT_COMMENT, cx);
                        cx.notify();
                    })),
                INPUT_COMMENT,
                &self.frame,
            )
            .into_any_element();
        }
        let tone = if agent {
            theme::agent()
        } else {
            theme::accent()
        };
        let to = div()
            .id("tasks-comment-to")
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(theme::muted())
            .cursor_pointer()
            .child("To:")
            .child(div().text_color(tone).child(if agent {
                "Your agent".to_owned()
            } else {
                format!("Everyone on {key}")
            }))
            .tip(format!(
                "Who reads it  ⇥ {}",
                if agent {
                    format!("Everyone on {key}")
                } else {
                    "your agent".into()
                }
            ))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.issue.to_agent = !this.issue.to_agent;
                cx.notify();
            }));
        let page = cx.entity();
        let acts: artoo_plugin_sdk::kit::composer::ComposeActs = Rc::new(move |act, cx| {
            page.update(cx, |this, cx| {
                if let (
                    Some(c),
                    ComposeAct::Place {
                        at,
                        shift,
                        clicks,
                        drag,
                        lower,
                    },
                ) = (this.issue.comment.as_mut(), act)
                {
                    c.place(at, shift, clicks, drag, lower);
                    cx.notify();
                }
            });
        });
        let words = if agent {
            "Add to drafts"
        } else if editing {
            "Save"
        } else {
            "Comment"
        };
        let send = controls::primary_button("tasks-comment-send", words, "⌘↵")
            .when(agent, |d| d.bg(theme::agent()))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.send_comment(cx)));
        let cancel = div()
            .id("tasks-comment-cancel")
            .cursor_pointer()
            .child(icons::icon("x").size(px(14.)).text_color(theme::muted()))
            .tip("Cancel  esc")
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_comment(cx)));
        let look = Look {
            tone,
            placeholder: if agent {
                "What should the agent do with this issue?".into()
            } else {
                format!("Comment on {key}").into()
            },
            head: Some(to.into_any_element()),
            lead: None,
            tools: Vec::new(),
            tail: vec![cancel.into_any_element(), send.into_any_element()],
            folded: false,
            focused: true,
            lines: None,
            completion: None,
            measure: div().into_any_element(),
            link_base: None,
        };
        let Some(c) = self.issue.comment.as_ref() else {
            return div().into_any_element();
        };
        let input = c.render_input(look, acts, cx);
        artoo_plugin_sdk::shell::input_box(
            div().id("tasks-comment-box").child(input),
            INPUT_COMMENT,
            &self.frame,
        )
        .into_any_element()
    }

    fn close_comment(&mut self, cx: &mut Context<Self>) {
        self.issue.comment = None;
        self.issue.editing = None;
        self.issue.pending_edit = None;
        self.frame.leave_keyboard(INPUT_COMMENT, cx);
        cx.notify();
    }

    /// ⌘↵ in the composer: posted, saved, or to the agent's drafts.
    fn send_comment(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.issue_key() else {
            return;
        };
        let Some(c) = self.issue.comment.as_ref() else {
            return;
        };
        let text = c.edit.text().trim().to_owned();
        if text.is_empty() {
            return;
        }
        if self.issue.to_agent {
            let site = self
                .store
                .read(cx)
                .issue(&key)
                .map(|i| i.site.clone())
                .unwrap_or_default();
            let data = json!({ "key": key, "site": site }).to_string();
            self.send_to_agent("jira.issue", &data, text, cx);
        } else if let Some(id) = self.issue.pending_edit {
            // Kept as the agent wrote it, the names mentioned as accounts.
            let mine = unnamed(&text, &self.mentionable(&key, cx));
            self.store.update(cx, |s, _| {
                if let Some(p) = s.kept.pending.iter_mut().find(|p| p.id == id) {
                    p.body = mine.clone();
                }
                s.kept.write();
            });
        } else if let Some(id) = self.issue.editing.clone() {
            let body = self.as_wiki(&key, &text, cx);
            let (k, b) = (key.clone(), body.clone());
            self.store.update(cx, |s, cx| {
                s.change(
                    &k.clone(),
                    "comments",
                    Write::comment_edited(&k, &id, &body),
                    move |s| {
                        s.edit_issue(&k, |i| {
                            if let Some(c) = i.comments.iter_mut().find(|c| c.id == id) {
                                c.body = b.clone();
                                c.updated = chrono_like_now();
                            }
                        })
                    },
                    |_, _, _| {},
                    cx,
                )
            });
        } else {
            self.post_comment(&key, &text, cx);
        }
        self.close_comment(cx);
    }

    pub fn comment_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let key = &event.keystroke;
        if key.modifiers.platform && key.key == "enter" {
            self.send_comment(cx);
            return Typed::Leave;
        }
        if key.key == "tab"
            && !key.modifiers.platform
            && !key.modifiers.control
            && self.issue.editing.is_none()
            && self.issue.pending_edit.is_none()
        {
            self.issue.to_agent = !self.issue.to_agent;
            cx.notify();
            return Typed::Kept;
        }
        let Some(c) = self.issue.comment.as_mut() else {
            return Typed::Leave;
        };
        let typed = c.edit_key(key, cx);
        cx.notify();
        match typed {
            Written::Cancel => {
                self.close_comment(cx);
                Typed::Leave
            }
            Written::Leave => Typed::Leave,
            Written::Ignored | Written::Changed => Typed::Kept,
        }
    }

    /// Details: the edit screen's other fields, each a control; who reported it; when it was made and changed.
    fn details(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let s = self.store.read(cx);
        let screen: Vec<Field> = match s.screens.get(&issue.row.key) {
            Some(Read::Done(f)) => f.clone(),
            _ => Vec::new(),
        };
        let unseen = s
            .changed
            .get(&issue.row.key)
            .map(|c| c.unseen.clone())
            .unwrap_or_default();
        let reporter = issue.reporter.clone();
        let me_rep = s.is_me(&issue.site, reporter.as_ref());
        // The facts row's and the page's own are not repeated here.
        const SHOWN: [&str; 14] = [
            "summary",
            "description",
            "issuetype",
            "priority",
            "assignee",
            "reporter",
            "project",
            "parent",
            "customfield_10020",
            "sprint",
            "comment",
            "attachment",
            "issuelinks",
            "epic",
        ];
        let mut ordered: Vec<Field> = Vec::new();
        for want in [
            "labels",
            "duedate",
            "components",
            "customfield_10016",
            "fixVersions",
        ] {
            if let Some(f) = screen.iter().find(|f| f.id == want) {
                ordered.push(f.clone());
            }
        }
        for f in &screen {
            if !SHOWN.contains(&f.id.as_str())
                && !ordered.iter().any(|o| o.id == f.id)
                && !f.id.starts_with("customfield_1001")
            {
                ordered.push(f.clone());
            }
        }
        if screen.is_empty() && !issue.row.labels.is_empty() {
            ordered.push(Field {
                id: "labels".into(),
                name: "Labels".into(),
                kind: "array".into(),
                items: "string".into(),
                ..Default::default()
            });
        }
        let key = issue.row.key.clone();
        let mut dl = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(14.))
            .text_size(px(12.));
        for f in ordered {
            let anchor = Anchor::Detail(f.id.clone());
            let open = self.pop_at(&anchor, PopKind::Field(f.id.clone()));
            let value: AnyElement = if f.id == "labels" {
                let labels = issue.row.labels.clone();
                if labels.is_empty() {
                    div()
                        .text_color(theme::faint())
                        .child("None")
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(4.))
                        .children(labels.iter().map(|l| look::pill(l)))
                        .into_any_element()
                }
            } else {
                match value_words(&issue.fields[f.id.as_str()]) {
                    Some(w) => div().child(w).into_any_element(),
                    None => div()
                        .text_color(theme::faint())
                        .child("None")
                        .into_any_element(),
                }
            };
            let (k, a, id) = (key.clone(), anchor.clone(), f.id.clone());
            let changed = unseen.contains(&f.name);
            dl = dl.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.))
                    .child(
                        div()
                            .w(px(84.))
                            .flex_none()
                            .pt(px(3.))
                            .text_color(theme::faint())
                            .child(f.name.clone()),
                    )
                    .child(
                        ctl(
                            SharedString::from(format!("detail-{}", f.id)),
                            open,
                            "Change",
                        )
                        .flex_1()
                        .min_w_0()
                        .h_auto()
                        .min_h(px(24.))
                        .text_size(px(12.))
                        .child(value)
                        .children(dot(changed))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.open_pop(PopKind::Field(id.clone()), a.clone(), &k, cx)
                        }))
                        .children(self.pop_here(
                            &anchor,
                            PopKind::Field(f.id.clone()),
                            cx,
                        )),
                    ),
            );
        }
        if let Some(rep) = &reporter {
            dl = dl.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .w(px(84.))
                            .flex_none()
                            .text_color(theme::faint())
                            .child("Reporter"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(look::face(&rep.name, me_rep, 16.))
                            .child(rep.name.clone()),
                    ),
            );
        }
        let when = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(14.))
            .border_t_1()
            .border_color(theme::border())
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(div().w(px(84.)).text_color(theme::faint()).child("Created"))
                    .child(
                        div()
                            .text_color(theme::faint())
                            .child(model::day_words(&issue.created)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(10.))
                    .child(div().w(px(84.)).text_color(theme::faint()).child("Updated"))
                    .child(
                        div()
                            .text_color(theme::faint())
                            .child(model::ago(&issue.row.updated)),
                    ),
            );
        section("Details").child(dl).child(when).into_any_element()
    }

    /// An issue's workflow over the page: the board's states in order, the one it is in ringed, the moves from it
    /// numbered as Move to numbers them.
    pub fn workflow_card(&mut self, key: &str, cx: &mut Context<Self>) -> AnyElement {
        let s = self.store.read(cx);
        let info = s.board_info().cloned().unwrap_or_default();
        let row = s.row(key).cloned();
        let moves = match s.moves.get(key) {
            Some(Read::Done(m)) => m.clone(),
            _ => Vec::new(),
        };
        let board = s.picked().map(|b| b.name.clone()).unwrap_or_default();
        let url = s.site_for(key).map(|site| site.url.clone());
        let here = row.as_ref().map(|r| info.column_of(&r.state));
        let current = row.as_ref().map(|r| r.state.id.clone()).unwrap_or_default();
        let numbered: Vec<&model::Move> = moves.iter().filter(|m| m.to.id != current).collect();
        let mut boxes = div().flex().items_center().gap(px(10.));
        for (ix, c) in info.columns.iter().enumerate() {
            let n = numbered
                .iter()
                .position(|m| c.states.contains(&m.to.id))
                .map(|i| i + 1);
            let asks = numbered
                .iter()
                .find(|m| c.states.contains(&m.to.id))
                .is_some_and(|m| !m.asks.is_empty());
            if ix > 0 {
                boxes = boxes.child(div().text_color(theme::faint()).child("→"));
            }
            boxes = boxes.child(
                div()
                    .w(px(130.))
                    .h(px(52.))
                    .px(px(10.))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if Some(ix) == here {
                        theme::accent()
                    } else {
                        theme::island_border()
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(13.))
                            .text_color(theme::text())
                            .child(c.name.clone())
                            .children(n.map(|n| {
                                div()
                                    .ml_auto()
                                    .size(px(18.))
                                    .rounded_full()
                                    .bg(theme::accent())
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(11.))
                                    .text_color(theme::on_accent())
                                    .child(n.to_string())
                            })),
                    )
                    .when(asks, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child("asks Resolution"),
                        )
                    })
                    .when(Some(ix) == here, |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::accent())
                                .child(key.to_owned()),
                        )
                    }),
            );
        }
        div()
            .id("tasks-workflow")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.45))
            .occlude()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .p(px(18.))
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
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child("Workflow"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::faint())
                                    .child(format!("{board} · Jira")),
                            )
                            .child(div().flex_1()),
                    )
                    .child(boxes)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .children(url.map(|u| {
                                look::small_button("wf-jira", "Open in Jira")
                                    .on_click(move |_, _, cx| cx.open_url(&u))
                            }))
                            .child(div().flex_1())
                            .child(
                                controls::primary_button("wf-close", "Close", "esc").on_click(
                                    cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.workflow = None;
                                        cx.notify();
                                    }),
                                ),
                            ),
                    ),
            )
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.workflow = None;
                cx.notify();
            }))
            .into_any_element()
    }
}

/// A paragraph that is a picture alone (`*name.png*`, as wiki's `!name.png|thumbnail!` reads), and its attachment.
fn picture_of(para: &str, attachments: &[Attachment]) -> Option<Attachment> {
    let name = para.trim().strip_prefix('*')?.strip_suffix('*')?;
    attachments
        .iter()
        .find(|a| a.image() && a.name == name)
        .cloned()
}

/// Two texts line by line: what is only theirs struck red, what is only yours green.
fn diff_lines(theirs: &str, mine: &str) -> Div {
    let a: Vec<&str> = theirs.lines().collect();
    let b: Vec<&str> = mine.lines().collect();
    let mut out = div()
        .flex()
        .flex_col()
        .font_family(theme::code_font())
        .text_size(px(12.));
    for l in &a {
        if !b.contains(l) {
            out = out.child(
                div()
                    .px(px(6.))
                    .bg(theme::removed_line())
                    .child(format!("- {l}")),
            );
        }
    }
    for l in &b {
        if !a.contains(l) {
            out = out.child(
                div()
                    .px(px(6.))
                    .bg(theme::added_line())
                    .child(format!("+ {l}")),
            );
        }
    }
    out
}

/// Now as the tracker writes a time, for a change shown before the tracker answers.
fn chrono_like_now() -> String {
    let secs = crate::store::now();
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Back from days to a civil date (Howard Hinnant's `civil_from_days`).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000+0000",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_reads_back_as_the_same_time() {
        let text = chrono_like_now();
        let back = model::stamp(&text).unwrap();
        assert!((back - crate::store::now()).abs() < 5, "{text}");
    }

    #[test]
    fn a_fields_value_reads_as_words() {
        assert_eq!(value_words(&json!({"name": "2.5"})), Some("2.5".into()));
        assert_eq!(
            value_words(&json!([{"name": "Refunds API"}, {"name": "Ledger"}])),
            Some("Refunds API, Ledger".into())
        );
        assert_eq!(value_words(&json!(3.0)), Some("3.0".into()));
        assert_eq!(value_words(&json!("2026-11-13")), Some("Fri 13 Nov".into()));
        assert_eq!(value_words(&Value::Null), None);
    }
}
