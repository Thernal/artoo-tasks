//! The Tasks space in one tab: its own page, the board picked as a list of its issues grouped by state; an issue's
//! page — its facts, its description, sub-tasks and comments, its details —, with a note for an agent on a paragraph
//! of its description (#212) in the app's composer; and the side panel's boards.

use crate::agents::{self as places_of, ISSUE};
use crate::model::{self, Issue, Row};
use crate::store::{Read, Store};
use artoo_plugin_sdk::agents::places;
use artoo_plugin_sdk::core::route::Route;
use artoo_plugin_sdk::core::textedit::TextEdit;
use artoo_plugin_sdk::gpui::{
    AnyElement, App, Context, Entity, FontWeight, KeyDownEvent, SharedString, Window, div, prelude::*,
};
use artoo_plugin_sdk::kit::composer::{Compose, ComposeAct, Look, Target, Typed as Written};
use artoo_plugin_sdk::kit::{controls, icons, island, px, theme};
use artoo_plugin_sdk::shell::{Frame, Typed};
use std::rc::Rc;

/// The space's id.
pub const SPACE: &str = "tasks";
/// The note composer's input.
pub const NOTE: &str = "tasks.note";

/// A note for the agent being written on paragraph `para` of issue `key`'s description.
struct Note {
    key: String,
    para: usize,
    compose: Compose,
}

pub struct Page {
    store: Entity<Store>,
    frame: Rc<dyn Frame>,
    route: Option<Route>,
    note: Option<Note>,
}

impl Page {
    pub fn new(store: Entity<Store>, frame: Rc<dyn Frame>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self { store, frame, route: None, note: None }
    }

    /// What is on screen now; read anew when it was just opened.
    pub fn show(&mut self, route: Option<&Route>, fresh: bool, cx: &mut Context<Self>) {
        self.route = route.cloned();
        let key = route.filter(|r| r.kind == ISSUE).and_then(places_of::key_of);
        self.store.update(cx, |s, cx| match key {
            Some(key) => s.read_issue(&key, fresh, cx),
            None => {
                s.read_boards(cx);
                s.read_rows(fresh, cx);
            }
        });
        cx.notify();
    }

    pub fn note_open(&self) -> bool {
        self.note.is_some()
    }

    fn open_note(&mut self, key: String, para: usize, cx: &mut Context<Self>) {
        self.note = Some(Note {
            key,
            para,
            compose: Compose {
                path: String::new(),
                old: false,
                first: None,
                line: 0,
                code: String::new(),
                edit: TextEdit::new(String::new()),
                at: String::new(),
                target: Target::Agent,
                error: None,
                preview: false,
                pick: 0,
                closed: None,
                page: None,
                file: None,
                answers: None,
            },
        });
        self.frame.take_keyboard(NOTE, cx);
        cx.notify();
    }

    /// A key while the note has the keyboard.
    pub fn note_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let key = &event.keystroke;
        if key.modifiers.platform && key.key == "enter" {
            self.add_note(cx);
            return Typed::Leave;
        }
        let Some(note) = self.note.as_mut() else {
            return Typed::Leave;
        };
        let typed = note.compose.edit_key(key, cx);
        cx.notify();
        match typed {
            Written::Cancel => {
                self.note = None;
                Typed::Leave
            }
            Written::Leave => Typed::Leave,
            Written::Ignored | Written::Changed => Typed::Kept,
        }
    }

    /// The note added to the tab's drafts for the agent, at its paragraph.
    fn add_note(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.note.take() else {
            return;
        };
        let body = note.compose.edit.text().trim().to_owned();
        if body.is_empty() {
            return;
        }
        let data = places_of::paragraph_place(&note.key, note.para);
        let agents = self
            .frame
            .services()
            .get::<Rc<dyn artoo_plugin_sdk::agents::tab::Agents>>();
        match (places::on(cx, "jira.issue", &data), agents) {
            (Some(on), Some(agents)) => {
                agents.add_comment(places::note(on, body), cx);
                self.frame.show_tool(Some(artoo_plugin_sdk::agents::view::WINDOW), cx);
            }
            _ => self.frame.error("Notes for agents are not here".into(), cx),
        }
        self.frame.leave_keyboard(NOTE, cx);
        cx.notify();
    }

    fn page_frame(&self) -> artoo_plugin_sdk::gpui::Div {
        island().flex_1().flex().flex_col().min_h_0().min_w_0().overflow_hidden().bg(theme::editor())
    }

    fn board_page(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let store = self.store.read(cx);
        if store.sites.is_empty() {
            let text = store.missing.first().cloned().unwrap_or_else(|| {
                "Name a Jira site in the plugin's trackers.txt, its token in ~/.config/tokens".into()
            });
            return self
                .page_frame()
                .child(controls::empty_state("list-checks", "Connect a tracker".into(), text, None))
                .into_any_element();
        }
        if let Some(Read::Failed(why)) = &store.boards {
            return self
                .page_frame()
                .child(controls::empty_state("alert", "Boards not read".into(), why.clone(), None))
                .into_any_element();
        }
        let mine = store.mine;
        let rows = store.board.and_then(|b| store.rows.get(&(b, mine)));
        let body: AnyElement = match rows {
            None | Some(Read::Reading) => controls::skeleton("tasks-rows", 8),
            Some(Read::Failed(why)) => {
                controls::empty_state("alert", "Issues not read".into(), why.clone(), None).into_any_element()
            }
            Some(Read::Done(rows)) if rows.is_empty() => controls::empty_state(
                "list-checks",
                if mine { "Nothing of yours here".into() } else { "No issues".into() },
                String::new(),
                None,
            )
            .into_any_element(),
            Some(Read::Done(rows)) => self.rows(rows.clone(), cx),
        };
        let filter = ["Mine", "All"].into_iter().fold(controls::group(), |g, label| {
            let on = (label == "Mine") == mine;
            g.child(controls::chip(SharedString::from(format!("tasks-{label}")), label, on).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.store.update(cx, |s, cx| {
                        s.mine = label == "Mine";
                        s.read_rows(false, cx);
                        cx.notify();
                    });
                }),
            ))
        });
        self.page_frame()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(16.))
                    .h(px(44.))
                    .border_b_1()
                    .border_color(theme::border())
                    .child(filter),
            )
            .child(div().id("tasks-list").flex_1().min_h_0().flex().flex_col().overflow_y_scroll().child(body))
            .into_any_element()
    }

    /// The issues grouped by their state: those to do first, then in progress, then done.
    fn rows(&self, rows: Vec<Row>, cx: &mut Context<Self>) -> AnyElement {
        let rank = |c: &str| match c {
            "new" => 0,
            "indeterminate" => 1,
            "done" => 2,
            _ => 1,
        };
        let mut states: Vec<(usize, String)> = Vec::new();
        for r in &rows {
            if !states.iter().any(|(_, s)| *s == r.state) {
                states.push((rank(&r.category), r.state.clone()));
            }
        }
        states.sort_by_key(|(rank, _)| *rank);
        let mut list = div().flex().flex_col().px(px(8.)).py(px(6.));
        for (_, state) in states {
            let of: Vec<&Row> = rows.iter().filter(|r| r.state == state).collect();
            list = list.child(
                div()
                    .px(px(8.))
                    .pt(px(12.))
                    .pb(px(4.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::muted())
                    .child(format!("{} {}", state.to_uppercase(), of.len())),
            );
            for r in of {
                let key = r.key.clone();
                let frame = self.frame.clone();
                let who = r.assignee.clone().unwrap_or_else(|| "Unassigned".into());
                list = list.child(
                    artoo_plugin_sdk::kit::row(SharedString::from(format!("row-{}", r.key)), false)
                        .h(px(52.))
                        .gap(px(12.))
                        .child(
                            div()
                                .w(px(84.))
                                .flex_none()
                                .text_size(px(12.))
                                .font_family(theme::code_font())
                                .text_color(theme::muted())
                                .child(r.key.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(div().text_size(px(13.)).text_color(theme::text()).truncate().child(r.summary.clone()))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(theme::muted())
                                        .truncate()
                                        .child(format!("{} · {who} · {}", r.kind, model::day(&r.updated))),
                                ),
                        )
                        .on_click(move |_, _, cx| frame.open(places_of::issue_route(&key), cx)),
                );
            }
            let _ = &cx;
        }
        list.into_any_element()
    }

    fn issue_page(&mut self, key: String, cx: &mut Context<Self>) -> AnyElement {
        let read = self.store.read(cx).issues.get(&key).cloned();
        let issue = match read {
            None | Some(Read::Reading) => {
                return self.page_frame().p(px(16.)).child(controls::skeleton("tasks-issue", 10)).into_any_element();
            }
            Some(Read::Failed(why)) => {
                return self
                    .page_frame()
                    .child(controls::empty_state("alert", format!("{key} not read"), why, None))
                    .into_any_element();
            }
            Some(Read::Done(issue)) => issue,
        };
        let r = &issue.row;
        let fact = |label: &'static str, value: String| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .pr(px(28.))
                .child(div().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(theme::muted()).child(label))
                .child(div().text_size(px(13.)).text_color(theme::text()).child(value))
        };
        let head = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(24.))
            .pt(px(20.))
            .pb(px(16.))
            .border_b_1()
            .border_color(theme::border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(div().text_size(px(14.)).text_color(theme::accent()).child(r.key.clone()))
                    .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).text_color(theme::text()).child(r.summary.clone())),
            )
            .child(
                div()
                    .flex()
                    .child(fact("STATE", r.state.clone()))
                    .child(fact("ASSIGNEE", r.assignee.clone().unwrap_or_else(|| "Unassigned".into())))
                    .child(fact("PRIORITY", r.priority.clone().unwrap_or_else(|| "—".into())))
                    .child(fact("TYPE", r.kind.clone())),
            );
        let main = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(self.description(&issue, cx))
            .when(!issue.subtasks.is_empty(), |d| d.child(subtasks(&issue, &self.frame)))
            .child(comments(&issue));
        let details = section("Details", None).child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .p(px(16.))
                .child(detail("Labels", if issue.labels.is_empty() { "—".into() } else { issue.labels.join(", ") }))
                .child(detail("Reporter", issue.reporter.clone().unwrap_or_else(|| "—".into())))
                .child(detail("Created", model::day(&issue.created).to_owned()))
                .child(detail("Updated", model::day(&r.updated).to_owned()))
                .when_some(r.parent.clone(), |d, p| d.child(detail("Parent", p))),
        );
        self.page_frame()
            .child(
                div()
                    .id("tasks-issue")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(head)
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_start()
                            .gap(px(16.))
                            .p(px(20.))
                            .child(main)
                            .child(div().w(px(280.)).flex_none().child(details)),
                    ),
            )
            .into_any_element()
    }

    /// The description by its paragraphs, each with `+` on its edge for a note to the agent; the note being written
    /// under its paragraph.
    fn description(&mut self, issue: &Issue, cx: &mut Context<Self>) -> AnyElement {
        let paras = model::paragraphs(&issue.description);
        let mut body = div().flex().flex_col().gap(px(10.)).pl(px(12.)).pr(px(24.)).py(px(14.));
        if paras.is_empty() {
            body = body.child(div().text_size(px(13.)).text_color(theme::muted()).child("No description"));
        }
        for (ix, para) in paras.into_iter().enumerate() {
            let n = ix + 1;
            let key = issue.row.key.clone();
            let on = self.note.as_ref().is_some_and(|note| note.key == key && note.para == n);
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
                            .text_color(theme::on_accent())
                            .cursor_pointer()
                            .invisible()
                            .group_hover("para", |s| s.visible())
                            .when(on, |s| s.visible())
                            .child(icons::icon("plus").size(px(12.)).text_color(theme::on_accent()))
                            .on_click(cx.listener(move |this, _, _, cx| this.open_note(key.clone(), n, cx))),
                    )
                    .child(div().text_size(px(13.)).line_height(px(20.)).text_color(theme::text()).child(para)),
            );
            if on {
                body = body.child(self.composer(cx));
            }
        }
        section("Description", None).child(body).into_any_element()
    }

    /// The app's composer, for the agent, at its paragraph.
    fn composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(note) = &self.note else {
            return div().into_any_element();
        };
        let place = format!("{} · description · ¶{}", note.key, note.para);
        let head = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child("To:")
                    .child(div().text_color(theme::agent()).child("Your agent")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(theme::hover())
                    .text_size(px(11.))
                    .text_color(theme::muted())
                    .child(icons::icon("pin").size(px(11.)).text_color(theme::muted()))
                    .child(place),
            )
            .into_any_element();
        let page = cx.entity();
        let acts: artoo_plugin_sdk::kit::composer::ComposeActs = Rc::new(move |act, cx| {
            page.update(cx, |this, cx| {
                if let (Some(note), ComposeAct::Place { at, shift, clicks, drag, lower }) = (this.note.as_mut(), act) {
                    note.compose.place(at, shift, clicks, drag, lower);
                    cx.notify();
                }
            });
        });
        let add = controls::primary_button("tasks-note-add", "Add to drafts", "⌘↵")
            .bg(theme::agent())
            .on_click(cx.listener(|this, _, _, cx| this.add_note(cx)));
        let cancel = div()
            .id("tasks-note-cancel")
            .cursor_pointer()
            .child(icons::icon("x").size(px(14.)).text_color(theme::muted()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.note = None;
                this.frame.leave_keyboard(NOTE, cx);
                cx.notify();
            }));
        let look = Look {
            tone: theme::agent(),
            placeholder: "What should the agent do here?".into(),
            head: Some(head),
            lead: None,
            tools: Vec::new(),
            tail: vec![cancel.into_any_element(), add.into_any_element()],
            folded: false,
            focused: true,
            lines: None,
            completion: None,
            measure: div().into_any_element(),
            link_base: None,
        };
        let input = note.compose.render_input(look, acts, cx);
        artoo_plugin_sdk::shell::input_box(div().id("tasks-note-box").pl(px(28.)).child(input), NOTE, &self.frame)
            .into_any_element()
    }
}

fn section(title: &'static str, count: Option<usize>) -> artoo_plugin_sdk::gpui::Div {
    div()
        .flex()
        .flex_col()
        .rounded(px(8.))
        .border_1()
        .border_color(theme::border())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(16.))
                .h(px(40.))
                .border_b_1()
                .border_color(theme::border())
                .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(theme::text()).child(title))
                .when_some(count, |d, n| d.child(div().text_size(px(12.)).text_color(theme::muted()).child(n.to_string()))),
        )
}

fn detail(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .gap(px(12.))
        .text_size(px(13.))
        .child(div().w(px(80.)).flex_none().text_color(theme::muted()).child(label))
        .child(div().flex_1().min_w_0().text_color(theme::text()).child(value))
}

fn subtasks(issue: &Issue, frame: &Rc<dyn Frame>) -> impl IntoElement {
    let done = issue.subtasks.iter().filter(|s| s.category == "done").count();
    let mut list = div().flex().flex_col().py(px(4.));
    for s in &issue.subtasks {
        let (key, frame) = (s.key.clone(), frame.clone());
        list = list.child(
            artoo_plugin_sdk::kit::row(SharedString::from(format!("sub-{}", s.key)), false)
                .h(px(36.))
                .gap(px(10.))
                .px(px(16.))
                .child(div().w(px(84.)).flex_none().text_size(px(12.)).font_family(theme::code_font()).text_color(theme::muted()).child(s.key.clone()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.))
                        .text_color(if s.category == "done" { theme::muted() } else { theme::text() })
                        .when(s.category == "done", |d| d.line_through())
                        .child(s.summary.clone()),
                )
                .child(div().text_size(px(12.)).text_color(theme::muted()).child(s.state.clone()))
                .on_click(move |_, _, cx| frame.open(places_of::issue_route(&key), cx)),
        );
    }
    section("Sub-tasks", None)
        .child(div().px(px(16.)).pt(px(8.)).text_size(px(12.)).text_color(theme::muted()).child(format!("{done} of {}", issue.subtasks.len())))
        .child(list)
}

fn comments(issue: &Issue) -> impl IntoElement {
    let mut list = div().flex().flex_col().gap(px(14.)).px(px(16.)).py(px(12.));
    if issue.comments.is_empty() {
        list = list.child(div().text_size(px(13.)).text_color(theme::muted()).child("No comments"));
    }
    for c in &issue.comments {
        list = list.child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .text_size(px(13.))
                        .child(div().font_weight(FontWeight::SEMIBOLD).text_color(theme::text()).child(c.author.clone()))
                        .child(div().text_color(theme::muted()).child(model::day(&c.created).to_owned())),
                )
                .child(div().text_size(px(13.)).line_height(px(20.)).text_color(theme::text()).child(c.body.trim().to_owned())),
        );
    }
    section("Comments", Some(issue.comments.len())).child(list)
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let key = self.route.as_ref().filter(|r| r.kind == ISSUE).and_then(places_of::key_of);
        let page = match key {
            Some(key) => self.issue_page(key, cx),
            None => self.board_page(cx),
        };
        div().size_full().min_w_0().flex().flex_col().child(page)
    }
}

/// The side panel: the boards by site, the picked one filled.
pub fn side(store: &Entity<Store>, frame: &Rc<dyn Frame>, cx: &mut App) -> AnyElement {
    let s = store.read(cx);
    let mut list = div().flex().flex_col().gap(px(2.)).px(px(8.)).py(px(8.));
    list = list.child(
        div()
            .px(px(8.))
            .pb(px(6.))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::muted())
            .child("BOARDS"),
    );
    match &s.boards {
        Some(Read::Done(boards)) => {
            let mut site = String::new();
            for b in boards {
                if b.site != site {
                    site = b.site.clone();
                    list = list.child(div().px(px(8.)).pt(px(4.)).text_size(px(11.)).text_color(theme::faint()).child(site.clone()));
                }
                let (id, store, frame) = (b.id, store.clone(), frame.clone());
                list = list.child(
                    artoo_plugin_sdk::kit::row(SharedString::from(format!("board-{}", b.id)), s.board == Some(b.id))
                        .h(px(30.))
                        .gap(px(8.))
                        .child(icons::icon("layers").size(px(14.)).text_color(theme::muted()))
                        .child(div().text_size(px(13.)).text_color(theme::text()).truncate().child(b.name.clone()))
                        .on_click(move |_, _, cx| {
                            store.update(cx, |s, cx| {
                                s.board = Some(id);
                                s.read_rows(false, cx);
                                cx.notify();
                            });
                            frame.go_space(SPACE, cx);
                        }),
                );
            }
        }
        Some(Read::Failed(why)) => list = list.child(div().px(px(8.)).text_size(px(12.)).text_color(theme::muted()).child(why.clone())),
        _ => list = list.child(controls::skeleton("tasks-boards", 3)),
    }
    list.into_any_element()
}
