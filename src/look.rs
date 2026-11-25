//! The small marks every view of an issue draws, as the design's (`tasks.css`): its kind's square, its priority, its
//! state's chip, a person, an epic's lozenge, a label's pill, how far its sub-tasks are, a group's state mark.

use crate::model::{Epic, Person, State};
use artoo_plugin_sdk::gpui::{
    self, Div, FontWeight, Hsla, SharedString, Stateful, div, prelude::*,
};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{discussion, icons, motion, px, theme};

/// A colour as the marks mix it: `c` at `alpha`.
pub fn tint(c: impl Into<Hsla>, alpha: f32) -> Hsla {
    let c: Hsla = c.into();
    c.opacity(alpha)
}

/// The blue the tracker's In progress wears (the design's `--info`).
pub fn info() -> Hsla {
    theme::accent().into()
}

/// A kind's colour: a bug red, a story green, a task blue, a sub-task a quieter blue, an epic the first code colour.
pub fn kind_color(kind: &str, subtask: bool) -> Hsla {
    match kind.to_lowercase().as_str() {
        _ if subtask => tint(theme::accent(), 0.7),
        "bug" => theme::removed().into(),
        "story" => theme::added().into(),
        "task" => info(),
        "epic" => theme::code(1),
        _ => theme::muted().into(),
    }
}

/// A kind's square, 14 px, its glyph in the base colour: what kind of issue it is, said on hover.
pub fn kind_mark(kind: &str, subtask: bool) -> Div {
    let glyph = match kind.to_lowercase().as_str() {
        _ if subtask => div()
            .size(px(7.))
            .border_1()
            .border_color(theme::base())
            .into_any_element(),
        "bug" => div()
            .size(px(6.))
            .rounded_full()
            .bg(theme::base())
            .into_any_element(),
        "story" => icons::icon("bookmark")
            .size(px(10.))
            .text_color(theme::base())
            .into_any_element(),
        "task" => icons::icon("check")
            .size(px(10.))
            .text_color(theme::base())
            .into_any_element(),
        "epic" => icons::icon("layers")
            .size(px(10.))
            .text_color(theme::base())
            .into_any_element(),
        _ => div()
            .size(px(5.))
            .rounded_full()
            .bg(theme::base())
            .into_any_element(),
    };
    div()
        .flex_none()
        .size(px(14.))
        .rounded(px(3.))
        .flex()
        .items_center()
        .justify_center()
        .bg(kind_color(kind, subtask))
        .child(glyph)
}

/// A priority's arrow and colour: `↑` red for High and over, `=` amber for Medium, `↓` blue for Low and under.
pub fn priority_of(name: &str) -> (&'static str, Hsla) {
    match name.to_lowercase().as_str() {
        "highest" | "blocker" | "critical" | "high" | "major" => ("↑", theme::removed().into()),
        "low" | "lowest" | "minor" | "trivial" => ("↓", info()),
        _ => ("=", theme::warning().into()),
    }
}

/// A priority: its arrow, then its name unless `word` is false.
pub fn priority(name: &str, word: bool) -> Div {
    let (arrow, color) = priority_of(name);
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(2.))
        .child(
            div()
                .font_weight(FontWeight::BOLD)
                .text_color(color)
                .child(arrow),
        )
        .when(word, |d| d.child(name.to_owned()))
}

/// A state's colour by its category: to do the text's second shade, in progress blue, done green.
pub fn state_color(state: &State) -> Hsla {
    match state.category.as_str() {
        "done" => theme::added().into(),
        "indeterminate" => info(),
        _ => theme::muted().into(),
    }
}

/// A state's chip: its name and ▾ on a tint of its colour, ringed while its menu is open (`on`) or a move was
/// refused (`bad`).
pub fn state_chip(
    id: impl Into<SharedString>,
    state: &State,
    on: bool,
    bad: bool,
    small: bool,
) -> gpui::Stateful<Div> {
    let c = state_color(state);
    div()
        .id(id.into())
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.))
        .h(px(if small { 18. } else { 22. }))
        .pl(px(8.))
        .pr(px(4.))
        .rounded(px(4.))
        .bg(tint(c, 0.16))
        .text_color(c)
        .text_size(px(if small { 11. } else { 12. }))
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .when(on, |d| d.border_1().border_color(c))
        .when(bad, |d| d.border_1().border_color(theme::removed()))
        .hover(move |s| s.border_1().border_color(c))
        .child(display_state(&state.name))
        .child(icons::icon("chevron").size(px(11.)).text_color(c))
}

/// A state's name as the board says it: `In Progress` as `In progress`.
pub fn display_state(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, word) in name.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
            out.push_str(&word.to_lowercase());
        } else {
            out.push_str(word);
        }
    }
    out
}

/// Someone's face: their initial on their colour, the accent for you.
pub fn face(name: &str, me: bool, size: f32) -> Div {
    discussion::avatar(name, me, size)
}

/// An epic's colour from the tracker's palette (1–14) on the code colours; the first when none is said.
pub fn epic_color(epic: &Epic) -> Hsla {
    match epic.color {
        0 => theme::code(1),
        n => theme::code(u32::from((n - 1) % 8 + 1)),
    }
}

/// An epic's lozenge: its name on a tint of its colour, ending in … past 200 px or where its row ends; the whole on a
/// long hover.
pub fn epic_lozenge(epic: &Epic) -> Stateful<Div> {
    let c = epic_color(epic);
    div()
        .id(SharedString::from(format!("epic-{}", epic.key)))
        .tip(format!("Epic {} {}", epic.key, epic.name))
        .min_w_0()
        .max_w(px(200.))
        .h(px(18.))
        .px(px(6.))
        .flex()
        .items_center()
        .rounded(px(4.))
        .bg(tint(c, 0.16))
        .text_color(c)
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .overflow_hidden()
        .child(div().truncate().child(epic.name.clone()))
}

/// A label's pill.
pub fn pill(text: &str) -> Div {
    div()
        .flex_none()
        .h(px(20.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded_full()
        .bg(theme::hover())
        .text_color(theme::muted())
        .text_size(px(11.))
        .whitespace_nowrap()
        .child(text.to_owned())
}

/// How far the sub-tasks are: a 64 px bar, green to `done` of `all`.
pub fn progress(done: usize, all: usize) -> Div {
    let part = if all == 0 {
        0.
    } else {
        done as f32 / all as f32
    };
    div()
        .flex_none()
        .w(px(64.))
        .h(px(4.))
        .rounded_full()
        .bg(theme::hover())
        .overflow_hidden()
        .child(div().h_full().w(px(64. * part)).bg(theme::added()))
}

/// The faint dot between facts.
pub fn sep() -> Div {
    div().flex_none().text_color(theme::faint()).child("·")
}

/// An issue's key, in code's font.
pub fn key(text: &str) -> Div {
    div()
        .flex_none()
        .font_family(theme::code_font())
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(text.to_owned())
}

/// A group's state mark, drawn: an empty ring to do, a half-filled one in progress, an eye in review, a check done.
pub fn state_mark(state: &State, size: f32) -> Div {
    let c = state_color(state);
    let review = state.name.to_lowercase().contains("review");
    match state.category.as_str() {
        "done" => div()
            .flex_none()
            .child(icons::icon("check").size(px(size)).text_color(c)),
        _ if review => div()
            .flex_none()
            .child(icons::icon("eye").size(px(size)).text_color(c)),
        "indeterminate" => div()
            .flex_none()
            .size(px(size - 2.))
            .rounded_full()
            .border_1()
            .border_color(c)
            .overflow_hidden()
            .flex()
            .child(div().w_1_2().h_full().bg(c)),
        _ => div()
            .flex_none()
            .size(px(size - 2.))
            .rounded_full()
            .border_1()
            .border_color(c),
    }
}

/// The mark of a blocked issue: a ring struck through, red.
pub fn blocked_mark() -> Div {
    div()
        .flex_none()
        .text_color(theme::removed())
        .text_size(px(12.))
        .child("⊘")
}

/// A small button drawn as the design's `.button` in a line of facts: words on the panel's colour, a hairline round.
pub fn small_button(
    id: impl Into<SharedString>,
    words: impl Into<SharedString>,
) -> gpui::Stateful<Div> {
    div()
        .id(id.into())
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.))
        .h(px(24.))
        .px(px(10.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme::island_border())
        .text_size(px(12.))
        .text_color(theme::text())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(words.into())
}

/// A fold's chevron: down while open, turning a quarter to the right as it folds (#56, 1), as the app's sections'.
pub fn chevron(id: impl Into<gpui::ElementId>, open: bool, size: f32) -> impl IntoElement {
    motion::toward(
        icons::icon("chevron")
            .size(px(size))
            .text_color(theme::muted()),
        id,
        open,
        false,
        motion::FAST,
        |c, v| {
            c.with_transformation(gpui::Transformation::rotate(gpui::radians(
                -std::f32::consts::FRAC_PI_2 * (1. - v),
            )))
        },
    )
}

/// A time as the design's lists say it, short: `now`, `2m`, `1h`, `1d`, then the date.
pub fn short_ago(time: i64) -> String {
    let secs = (artoo_plugin_sdk::kit::format::now() - time).max(0);
    match secs {
        0..60 => "now".to_owned(),
        60..3600 => format!("{}m", secs / 60),
        3600..86400 => format!("{}h", secs / 3600),
        86400..2_592_000 => format!("{}d", secs / 86400),
        _ => artoo_plugin_sdk::kit::format::date(time),
    }
}

/// A 24 px square button with one icon and its tooltip: the design's icon button in a row of facts.
pub fn icon_button(
    id: impl Into<SharedString>,
    icon: &'static str,
    tip: impl Into<SharedString>,
) -> gpui::Stateful<Div> {
    square_button(id, icon).tip(tip)
}

/// `icon_button` before its tooltip, for one whose tooltip is a title and a quieter line.
pub fn square_button(id: impl Into<SharedString>, icon: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id.into())
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(24.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme::island_border())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(icons::icon(icon).size(px(14.)).text_color(theme::muted()))
}

/// A fact of a row or a card: a glyph and, where it has one, its figure; its sentence is the tooltip.
fn fact(id: String, glyph: impl IntoElement, figure: Option<String>, tip: String) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(3.))
        .child(glyph)
        .children(figure)
        .tip(tip)
}

/// A priority as its arrow alone, its name the tooltip.
pub fn priority_fact(key: &str, name: &str) -> Stateful<Div> {
    fact(
        format!("prio-{key}"),
        priority(name, false),
        None,
        format!("{name} priority"),
    )
}

/// How many sub-tasks are done of all: `▤ 1/3`.
pub fn subtasks_fact(key: &str, done: usize, all: usize) -> Stateful<Div> {
    fact(
        format!("subs-{key}"),
        icons::icon("list").size(px(12.)).text_color(theme::muted()),
        Some(format!("{done}/{all}")),
        format!("{done} of {all} sub-tasks done"),
    )
}

/// How many comments an issue has.
pub fn comments_fact(key: &str, n: usize) -> Stateful<Div> {
    fact(
        format!("cmts-{key}"),
        icons::icon("message")
            .size(px(12.))
            .text_color(theme::muted()),
        Some(n.to_string()),
        format!("{n} comment{}", if n == 1 { "" } else { "s" }),
    )
}

/// What blocks an issue: the struck ring alone, who blocks it and its state the tooltip.
pub fn blocked_fact(key: &str, by: &crate::model::Brief) -> Stateful<Div> {
    fact(
        format!("blk-{key}"),
        blocked_mark(),
        None,
        format!(
            "Blocked by {} {} · {}",
            by.key,
            by.summary,
            display_state(&by.state.name)
        ),
    )
}

/// The board layout's glyph: a frame in columns, drawn since the kit has no icon of it.
pub fn board_glyph(size: f32) -> Div {
    let line = theme::text();
    div()
        .flex_none()
        .size(px(size - 2.))
        .rounded(px(3.))
        .border_1()
        .border_color(line)
        .flex()
        .child(div().w_1_3().h_full().border_r_1().border_color(line))
        .child(div().w_1_3().h_full().border_r_1().border_color(line))
}

/// Nobody: a dashed ring with an outline of a person, as the design draws an unassigned issue.
pub fn nobody(size: f32) -> Div {
    div()
        .flex_none()
        .size(px(size))
        .rounded_full()
        .border_1()
        .border_dashed()
        .border_color(theme::faint())
        .flex()
        .items_center()
        .justify_center()
        .child(
            icons::icon("user")
                .size(px(size - 6.))
                .text_color(theme::faint()),
        )
}

/// A row's or card's end: the assignee's face alone, a dashed one for nobody; who it is the tooltip.
pub fn assignee(person: Option<&Person>, me: bool, size: f32) -> Div {
    match person {
        Some(p) => face(&p.name, me, size),
        None => nobody(size),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_reads_as_the_board_says_it() {
        assert_eq!(display_state("In Progress"), "In progress");
        assert_eq!(display_state("To Do"), "To do");
        assert_eq!(display_state("Done"), "Done");
    }
}
