//! A board as the space's own page shows it: which issues it shows (the slice picked, Mine or All, the people
//! picked, what the filter says), grouped by state, story or epic; drawn as a list beside a preview or as columns of
//! cards, a lane across them for each story or epic. What is drawn here reads plain data (`Shown`); the page's state
//! and its clicks are the page's (`page.rs`).

use crate::keep::Group;
use crate::look::{self, sep};
use crate::model::{self, BoardInfo, Epic, Issue, Person, Row, Sprint, State};
use crate::page::{Anchor, Page, PopKind};
use crate::slices::{self, Progress, SliceId, Slices};
use crate::store::Read;
use artoo_plugin_sdk::gpui::{
    AnyElement, App, ClickEvent, Context, Div, FontWeight, IntoElement, Render, SharedString,
    Window, div, prelude::*,
};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, icons, markdown, px, theme};
use std::collections::HashSet;

/// Who the people filter picks, by person id; `NONE` for Unassigned.
pub const NONE: &str = "-";

/// A card on its way between columns: its key and summary, what follows the pointer.
#[derive(Clone)]
struct Dragged {
    key: String,
    summary: String,
}

impl Render for Dragged {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(220.))
            .px(px(10.))
            .py(px(8.))
            .rounded(px(6.))
            .bg(theme::panel())
            .border_1()
            .border_color(theme::accent())
            .shadow_lg()
            .text_size(px(12.))
            .text_color(theme::text())
            .opacity(0.9)
            .child(format!("{} {}", self.key, self.summary))
    }
}

/// What a tracker calls the things on its boards: Trello's cards, everyone else's issues.
pub(crate) fn item_word(host: &str) -> &'static str {
    if crate::trackers::kind_of(host) == crate::trackers::TRELLO {
        "card"
    } else {
        "issue"
    }
}

/// A time as the preview says it at a row's end: `3h`, `11d`.
fn short_ago(at: &str) -> String {
    let ago = model::ago(at);
    match ago.strip_suffix(" ago") {
        Some(short) => short.replace(' ', ""),
        None => ago,
    }
}

/// How a board is drawn while its site is not reached: as last read, dimmed.
const STALE: f32 = 0.72;

/// What the board shows of its issues, as the bar's filters and the board's own say.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// Only yours.
    pub mine: bool,
    /// The people picked beside Mine · All: their issues alone (`NONE`, Unassigned).
    pub faces: Vec<String>,
    /// The filter's words: each must be in the key, the summary or a label; `label:x` a label alone.
    pub words: String,
}

/// A group of the list: its head and the rows shown under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Shelf {
    /// Kept by it: what folds it.
    pub id: String,
    pub head: Head,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Head {
    /// A column: its name, its first state (its mark), how many it holds.
    State { name: String, state: State },
    /// A story with sub-tasks: itself, its sub-tasks done of all.
    Story {
        row: Box<Row>,
        done: usize,
        all: usize,
    },
    /// Under the stories: the issues with no sub-tasks.
    Other,
    /// An epic: itself, its issues done of all; none for `No epic`.
    Epic {
        epic: Option<Epic>,
        done: usize,
        all: usize,
    },
}

/// Whether row `r` is someone's in `faces` (`NONE` Unassigned).
fn picked(r: &Row, faces: &[String]) -> bool {
    let id = r.assignee.as_ref().map_or(NONE, |p| p.id.as_str());
    faces.iter().any(|f| f == id)
}

/// Whether row `r` matches the filter's words.
pub fn matches(r: &Row, words: &str) -> bool {
    r.matches(words)
}

/// The board's issues in play: those of the slice shown, no epics among them.
pub fn in_play(rows: &[Row]) -> Vec<&Row> {
    rows.iter().filter(|r| r.kind != "Epic").collect()
}

/// The rows the filter keeps, `me` your id.
pub fn shown<'a>(rows: &[&'a Row], filter: &Filter, me: Option<&Person>) -> Vec<&'a Row> {
    (rows.iter().copied())
        .filter(|r| {
            !filter.mine || me.is_some_and(|me| r.assignee.as_ref().is_some_and(|p| p.id == me.id))
        })
        .filter(|r| filter.faces.is_empty() || picked(r, &filter.faces))
        .filter(|r| matches(r, &filter.words))
        .collect()
}

/// The list's groups of `rows` (those in play), as `group` says; the rows `filter` keeps under each.
pub fn shelves(
    rows: &[&Row],
    info: &BoardInfo,
    group: Group,
    filter: &Filter,
    me: Option<&Person>,
) -> Vec<Shelf> {
    let keep = shown(rows, filter, me);
    let col = |r: &Row| info.column_of(&r.state);
    let by_state = |mut v: Vec<Row>| {
        v.sort_by_key(|r| col(r));
        v
    };
    match group {
        Group::State => (info.columns.iter().enumerate())
            .map(|(ix, c)| {
                let state = column_state(info, rows.iter().copied(), ix);
                Shelf {
                    id: format!("state:{}", c.name),
                    head: Head::State {
                        name: c.name.clone(),
                        state,
                    },
                    rows: (keep.iter())
                        .filter(|r| r.parent.is_none() && col(r) == ix)
                        .map(|r| (*r).clone())
                        .collect(),
                }
            })
            .collect(),
        Group::Story => {
            let mut out = Vec::new();
            for p in rows
                .iter()
                .filter(|r| r.parent.is_none() && r.subtasks.1 > 0)
            {
                let subs: Vec<Row> = (keep.iter())
                    .filter(|r| r.parent.as_deref() == Some(&p.key))
                    .map(|r| (*r).clone())
                    .collect();
                if subs.is_empty() {
                    continue;
                }
                out.push(Shelf {
                    id: format!("story:{}", p.key),
                    head: Head::Story {
                        row: Box::new((*p).clone()),
                        done: p.subtasks.0,
                        all: p.subtasks.1,
                    },
                    rows: by_state(subs),
                });
            }
            let rest = (keep.iter())
                .filter(|r| r.parent.is_none() && r.subtasks.1 == 0)
                .map(|r| (*r).clone())
                .collect();
            out.push(Shelf {
                id: "other".into(),
                head: Head::Other,
                rows: by_state(rest),
            });
            out
        }
        Group::Epic => {
            let mut epics: Vec<Epic> = info.epics.clone();
            for r in rows {
                if let Some(e) = &r.epic
                    && !epics.iter().any(|x| x.key == e.key)
                {
                    epics.push(e.clone());
                }
            }
            let of = |r: &Row, e: Option<&str>| {
                r.parent.is_none() && r.epic.as_ref().map(|x| x.key.as_str()) == e
            };
            let mut out: Vec<Shelf> = (epics.iter())
                .filter_map(|e| {
                    let all: Vec<&&Row> = rows.iter().filter(|r| of(r, Some(&e.key))).collect();
                    let shown: Vec<Row> = keep
                        .iter()
                        .filter(|r| of(r, Some(&e.key)))
                        .map(|r| (*r).clone())
                        .collect();
                    (!all.is_empty()).then(|| Shelf {
                        id: format!("epic:{}", e.key),
                        head: Head::Epic {
                            epic: Some(e.clone()),
                            done: all.iter().filter(|r| r.state.done()).count(),
                            all: all.len(),
                        },
                        rows: by_state(shown),
                    })
                })
                .collect();
            let none: Vec<Row> = keep
                .iter()
                .filter(|r| of(r, None))
                .map(|r| (*r).clone())
                .collect();
            out.push(Shelf {
                id: "epic:".into(),
                head: Head::Epic {
                    epic: None,
                    done: 0,
                    all: none.len(),
                },
                rows: by_state(none),
            });
            out
        }
    }
}

/// Whether group `id` is Done's column, of a sprint of All active sprints or not.
fn done_group(id: &str) -> bool {
    id.rsplit('/')
        .next()
        .is_some_and(|own| own.eq_ignore_ascii_case("state:done"))
}

/// One of All active sprints' groups: the sprint, how many issues it holds, its issues in play and its own groups.
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub sprint: Sprint,
    pub count: Option<usize>,
    pub rows: Vec<Row>,
    pub shelves: Vec<Shelf>,
}

/// What folds sprint `id`'s part of All active sprints, and holds its groups' ids: `sprint:34`.
pub fn part_id(id: u64) -> String {
    format!("sprint:{id}")
}

/// All active sprints as the board lists them, each with its issues as read and its groups as `group` says, the
/// rows `filter` keeps under each; a group's id is under its sprint's (`sprint:34/state:To do`).
pub fn parts(
    info: &BoardInfo,
    slices: &Slices,
    group: Group,
    filter: &Filter,
    me: Option<&Person>,
) -> Vec<Part> {
    slices::active(info)
        .map(|sprint| {
            let read = slices.reads.get(&SliceId::Sprint(sprint.id));
            let rows = read.map(|r| in_play(&r.rows)).unwrap_or_default();
            let mut groups = shelves(&rows, info, group, filter, me);
            for g in &mut groups {
                g.id = format!("{}/{}", part_id(sprint.id), g.id);
            }
            Part {
                sprint: sprint.clone(),
                count: read.and_then(|r| r.count),
                rows: rows.into_iter().cloned().collect(),
                shelves: groups,
            }
        })
        .collect()
}

/// A sprint's dates: `Mon 17 – Fri 28 Nov`, the month said once when both are in it; its end alone when it has no
/// start.
pub fn span_words(start: Option<&str>, end: &str) -> String {
    let end = model::day_words(end);
    let Some(start) = start.map(model::day_words) else {
        return end;
    };
    let month = |d: &str| d.rsplit(' ').next().map(str::to_owned);
    match start.rsplit_once(' ') {
        Some((day, _)) if month(&start) == month(&end) => format!("{day} – {end}"),
        _ => format!("{start} – {end}"),
    }
}

/// The state column `ix` of `info` is drawn with: the first of its states one of `rows` is in, else one named for
/// the column, its category by its place (the first new, the last done).
pub fn column_state<'a>(
    info: &BoardInfo,
    mut rows: impl Iterator<Item = &'a Row>,
    ix: usize,
) -> State {
    let Some(c) = info.columns.get(ix) else {
        return State::default();
    };
    (rows.find(|r| c.states.contains(&r.state.id)))
        .map(|r| r.state.clone())
        .unwrap_or_else(|| State {
            id: c.states.first().cloned().unwrap_or_default(),
            name: c.name.clone(),
            category: if ix + 1 == info.columns.len() {
                "done".into()
            } else if ix == 0 {
                "new".into()
            } else {
                "indeterminate".into()
            },
        })
}

/// The people on the board's issues in play: you first, then by how many issues, Unassigned (`None`) last; each
/// with how many.
pub fn team(rows: &[&Row], me: Option<&Person>) -> Vec<(Option<Person>, usize)> {
    let mut out: Vec<(Option<Person>, usize)> = Vec::new();
    for r in rows.iter().filter(|r| r.parent.is_none()) {
        match out
            .iter_mut()
            .find(|(p, _)| p.as_ref().map(|p| &p.id) == r.assignee.as_ref().map(|p| &p.id))
        {
            Some((_, n)) => *n += 1,
            None => out.push((r.assignee.clone(), 1)),
        }
    }
    let is_me = |p: &Option<Person>| me.is_some_and(|me| p.as_ref().is_some_and(|p| p.id == me.id));
    out.sort_by(|a, b| {
        (is_me(&b.0).cmp(&is_me(&a.0)))
            .then(a.0.is_none().cmp(&b.0.is_none()))
            .then(b.1.cmp(&a.1))
    });
    out
}

/// Everything the board draws, read from the store once per drawing.
pub struct Shown {
    pub host: String,
    /// The slice shown, when the board is read by slice.
    pub slice: Option<SliceShown>,
    /// All active sprints, each its own group; empty for any other slice.
    pub parts: Vec<Part>,
    pub board_name: String,
    pub info: BoardInfo,
    pub shelves: Vec<Shelf>,
    /// Every row in play, for the preview and the counts.
    pub all: Vec<Row>,
    pub me: Option<Person>,
    pub group: Group,
    pub columns: bool,
    /// How many of the issues in play the filters keep.
    pub count: usize,
    pub subtasks: usize,
    pub people_picked: Vec<String>,
}

/// The slice a board read by slice shows.
#[derive(Clone, Debug)]
pub struct SliceShown {
    pub id: SliceId,
    /// As the tracker names it.
    pub name: String,
    /// The sprint shown, when it is one.
    pub sprint: Option<Sprint>,
    /// How many issues it holds, sub-tasks left out, as the tracker counts them.
    pub count: Option<usize>,
}

impl Shown {
    /// The rows in the order the list draws them, folded groups left out: what ↑ ↓ walk.
    pub fn order(&self, folded: &dyn Fn(&str) -> bool) -> Vec<String> {
        if !self.parts.is_empty() {
            return (self.parts.iter())
                .filter(|p| !folded(&part_id(p.sprint.id)))
                .flat_map(|p| order_of(&p.shelves, folded))
                .collect();
        }
        order_of(&self.shelves, folded)
    }
}

/// The rows of `shelves` in the order the list draws them, folded groups left out.
fn order_of(shelves: &[Shelf], folded: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    for s in shelves {
        if let Head::Story { row, .. } = &s.head {
            out.push(row.key.clone());
        }
        if !folded(&s.id) {
            out.extend(s.rows.iter().map(|r| r.key.clone()));
        }
    }
    out
}

impl Page {
    /// What the board shows now, or why it shows nothing.
    pub fn board_shown(&self, cx: &Context<Self>) -> Option<Shown> {
        let s = self.store.read(cx);
        let board = s.picked()?.clone();
        let rows = s.board_rows()?;
        let info = s.board_info().cloned().unwrap_or_default();
        let view = s.kept.view(&board.key());
        let me = s.me.get(&board.site).cloned();
        let play = in_play(rows);
        let filter = Filter {
            mine: s.mine,
            faces: s.faces.clone(),
            words: self.frame.filter(cx),
        };
        let kept = shown(&play, &filter, me.as_ref());
        let slices = s.board_slices();
        let slice = slices.and_then(|sl| {
            let id = sl.view.clone()?;
            let past = match &sl.past {
                Some(Read::Done(past)) => past.as_slice(),
                _ => &[],
            };
            let (count, _) = sl.counts(&id, &info);
            Some(SliceShown {
                name: slices::name(&id, Some(&info), past),
                sprint: (id.sprint())
                    .and_then(|n| info.sprints.iter().chain(past).find(|s| s.id == n).cloned()),
                id,
                count,
            })
        });
        let parts = match (slices, &slice) {
            (Some(sl), Some(shown)) if shown.id == SliceId::All => {
                parts(&info, sl, view.group, &filter, me.as_ref())
            }
            _ => Vec::new(),
        };
        Some(Shown {
            host: board.site.clone(),
            slice,
            parts,
            board_name: board.name.clone(),
            shelves: shelves(&play, &info, view.group, &filter, me.as_ref()),
            count: kept.iter().filter(|r| r.parent.is_none()).count(),
            subtasks: kept.iter().filter(|r| r.parent.is_some()).count(),
            all: play.iter().map(|r| (*r).clone()).collect(),
            info,
            me,
            group: view.group,
            columns: view.board,
            people_picked: s.faces.clone(),
        })
    }

    /// Whether group `id` is folded: Done's column is until unfolded.
    pub fn folded(&self, id: &str) -> bool {
        if done_group(id) {
            return !self.unfolded.contains(id);
        }
        self.folded_ids.contains(id)
    }

    fn fold(&mut self, id: &str, cx: &mut Context<Self>) {
        if done_group(id) {
            if !self.unfolded.remove(id) {
                self.unfolded.insert(id.to_owned());
            }
        } else if !self.folded_ids.remove(id) {
            self.folded_ids.insert(id.to_owned());
        }
        // The list's group and the board's lane are one fold, each drawn moving.
        self.motion.mark(format!("lane-{id}"));
        self.moved(&format!("shelf-{id}"), cx);
    }

    /// The board's line of facts: its sprint and when it ends, how many issues; the people picked; Group ▾.
    fn facts_line(
        &mut self,
        shown: &Shown,
        stale: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) -> Div {
        let line = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .min_h(px(36.))
            .px(px(16.))
            .border_b_1()
            .border_color(theme::border())
            .text_size(px(12.))
            .text_color(theme::muted())
            .whitespace_nowrap();
        if let Some((host, at)) = stale {
            return line
                .text_color(theme::warning())
                .child(div().size(px(8.)).rounded_full().bg(theme::warning()))
                .child(format!("Can’t reach {host} · boards from {at}"))
                .child(div().flex_1())
                .child(
                    look::small_button("tasks-retry", "Retry")
                        .tip("Read again  ⌘R")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.store
                                .update(cx, |s, cx| s.read_board(crate::store::Ask::Reload, cx))
                        })),
                );
        }
        let picker = (shown.slice.as_ref()).filter(|_| !shown.info.sprintless);
        let mut line = match picker {
            Some(slice) => line.child(self.sprint_picker(slice, cx)),
            None => line.child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(shown.board_name.clone()),
            ),
        };
        let unit = item_word(&shown.host);
        let all = shown.all.iter().filter(|r| r.parent.is_none()).count();
        let nothing = shown.count == 0 && all > 0;
        // The count is the bar's All; here only when the filters keep none of it.
        if nothing {
            let mut tip = artoo_plugin_sdk::kit::format::plural(shown.count, unit);
            if shown.group == Group::Story {
                tip.push_str(&format!(
                    ", {}",
                    artoo_plugin_sdk::kit::format::plural(shown.subtasks, "sub-task")
                ));
            }
            line = line.child(
                div()
                    .id("tasks-count")
                    .child(format!(
                        "0 of {}",
                        artoo_plugin_sdk::kit::format::plural(all, unit)
                    ))
                    .tip(tip),
            );
        }
        if !shown.people_picked.is_empty() {
            let names: Vec<String> = (shown.people_picked.iter())
                .map(|id| match id.as_str() {
                    NONE => "Unassigned".to_owned(),
                    id => (shown.all.iter())
                        .find_map(|r| {
                            r.assignee
                                .as_ref()
                                .filter(|p| p.id == id)
                                .map(|p| p.name.clone())
                        })
                        .unwrap_or_else(|| id.to_owned()),
                })
                .collect();
            let words = if names.len() == 1 {
                names[0].clone()
            } else {
                format!("{} people", names.len())
            };
            line = line
                .child(sep())
                .child(div().id("tasks-picked").child(words).tip(names.join(", ")));
        }
        if nothing {
            return line.child(div().flex_1()).child(shown.host.clone());
        }
        let group = shown.group;
        line.child(div().flex_1())
            // Painted as the Mac kept it, until the tracker answers.
            .children(
                self.kept_at(cx)
                    .map(|at| div().text_color(theme::faint()).child(format!("from {at}"))),
            )
            .child(
                look::small_button("tasks-group", "")
                    .child("Group:")
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(group.word()),
                    )
                    .child(
                        icons::icon("chevron")
                            .size(px(12.))
                            .text_color(theme::muted()),
                    )
                    .tip("Group by state, story or epic  G")
                    .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                        this.group_menu(Some(e.position()), cx)
                    })),
            )
    }

    /// The facts line's sprint picker: the slice shown and ▾, its menu the board's slices.
    fn sprint_picker(&mut self, slice: &SliceShown, cx: &mut Context<Self>) -> AnyElement {
        let open = self.pop_at(&Anchor::Picker, PopKind::Sprints);
        div()
            .id("tasks-picker")
            .relative()
            .flex()
            .items_center()
            .gap(px(4.))
            .h(px(24.))
            .min_w_0()
            .px(px(8.))
            .ml(px(-8.))
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover()))
            .when(open, |d| d.bg(theme::hover()))
            .child(
                div()
                    .max_w(px(320.))
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(slice.name.clone()),
            )
            .child(
                icons::icon("chevron")
                    .size(px(12.))
                    .text_color(theme::muted()),
            )
            .tip(format!("{}\nPick a sprint  S", slice.name))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_sprints(cx)))
            .children(self.pop_here(&Anchor::Picker, PopKind::Sprints, cx))
            .into_any_element()
    }

    /// A sprint's head: its dates and days left (orange from three), its goal on one line, how far its issues have got
    /// (`24 / 78`, the words in the bar's tooltip). In All active sprints its name leads and it folds its part (`fold`:
    /// whether it is folded).
    fn sprint_head(
        &mut self,
        sprint: &Sprint,
        rows: &[Row],
        count: Option<usize>,
        unit: &str,
        fold: Option<bool>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = sprint.id;
        let progress = Progress::of(rows);
        let all = progress.all(count).max(1) as f32;
        let tint = |category: &str| {
            look::state_color(&State {
                category: category.into(),
                ..Default::default()
            })
        };
        const BAR: f32 = 72.;
        let bar = div()
            .id(SharedString::from(format!("sprint-bar-{id}")))
            .flex_none()
            .flex()
            .w(px(BAR))
            .h(px(4.))
            .rounded_full()
            .overflow_hidden()
            .bg(theme::border())
            .child(
                div()
                    .h_full()
                    .w(px(BAR * progress.done.min(all as usize) as f32 / all))
                    .bg(tint("done")),
            )
            .child(
                div()
                    .h_full()
                    .w(px(BAR * progress.doing as f32 / all))
                    .bg(tint("indeterminate")),
            )
            .tip(progress.words(count, unit, &sprint.name));
        let how_far = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(bar)
            .child(div().text_color(theme::text()).child(progress.short(count)));
        let when = sprint.end.as_deref().map(|end| {
            let left = slices::days_left(end, crate::store::now());
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(
                            icons::icon("clock")
                                .size(px(12.))
                                .text_color(theme::muted()),
                        )
                        .child(span_words(sprint.start.as_deref(), end)),
                )
                .children(left.map(|(words, few)| {
                    div()
                        .when(few, |d| d.text_color(theme::warning()))
                        .child(words)
                }))
        });
        let goal = (!sprint.goal.is_empty()).then(|| {
            div()
                .id(SharedString::from(format!("sprint-goal-{id}")))
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(10.))
                        .text_color(theme::faint())
                        .child("GOAL"),
                )
                .child(div().min_w_0().truncate().child(sprint.goal.clone()))
                .tip(sprint.goal.clone())
        });
        let head = div()
            .id(SharedString::from(format!("sprint-head-{id}")))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(4.))
            .px(px(16.))
            .py(px(8.))
            .border_b_1()
            .border_color(theme::border())
            .text_size(px(12.))
            .text_color(theme::muted())
            .whitespace_nowrap();
        let line = || div().flex().items_center().gap(px(8.)).min_w_0();
        let Some(folded) = fold else {
            return head
                .child(line().children(when).child(div().flex_1()).child(how_far))
                .children(goal.map(|g| line().child(g)))
                .into_any_element();
        };
        let fold_id = part_id(id);
        head.bg(theme::base())
            .cursor_pointer()
            .child(
                line()
                    .child(look::chevron(
                        SharedString::from(format!("sprint-turn-{id}")),
                        !folded,
                        12.,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(sprint.name.clone()),
                    )
                    .children(when),
            )
            .child(
                line()
                    .pl(px(20.))
                    .children(goal)
                    .when(sprint.goal.is_empty(), |d| d.child(div().flex_1()))
                    .child(how_far),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.fold(&fold_id, cx)))
            .into_any_element()
    }

    /// The board as a list beside the preview, or its columns of cards.
    pub fn board(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let s = self.store.read(cx);
        let board = s.picked().cloned();
        let rows_read = s.board.as_ref().and_then(|b| s.rows.get(b)).cloned();
        let stale = self.stale(cx);
        let Some(board) = board else {
            return self
                .page_frame()
                .child(skeleton_in_page("tasks-rows", 8))
                .into_any_element();
        };
        match rows_read {
            None | Some(Read::Reading) => {
                return self
                    .page_frame()
                    .child(skeleton_in_page("tasks-rows", 8))
                    .into_any_element();
            }
            Some(Read::Failed(why)) => {
                return (self.page_frame())
                    .child(controls::empty_state(
                        "alert",
                        format!("{} not read", board.name),
                        why,
                        None,
                    ))
                    .into_any_element();
            }
            Some(Read::Done(_)) => {}
        }
        let Some(shown) = self.board_shown(cx) else {
            return self.page_frame().into_any_element();
        };
        // The pick stays among the rows shown, the first when it is not.
        let order = shown.order(&|id| self.folded(id));
        if self
            .pick
            .as_ref()
            .is_none_or(|k| !order.contains(k) && !shown.all.iter().any(|r| &r.key == k))
        {
            self.pick = order.first().cloned();
        }
        let mine_empty = shown.count == 0 && self.store.read(cx).mine;
        if mine_empty {
            let all = shown.all.iter().filter(|r| r.parent.is_none()).count();
            let empty = div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(format!("Nothing of yours on {}", board.name)),
                )
                .child(
                    div()
                        .id("tasks-all")
                        .text_size(px(13.))
                        .text_color(theme::accent())
                        .cursor_pointer()
                        .child(format!("All {all}"))
                        .on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.set_mine(false, cx)),
                        ),
                );
            return (self.page_frame())
                .child(self.facts_line(&shown, stale, cx))
                .child(empty)
                .into_any_element();
        }
        // One sprint shown: its head over its issues. All active sprints: each sprint's over its part.
        let head = (shown.slice.as_ref())
            .filter(|_| shown.parts.is_empty())
            .and_then(|sl| Some((sl.sprint.clone()?, sl.count)))
            .map(|(sprint, count)| {
                let unit = item_word(&shown.host);
                self.sprint_head(&sprint, &shown.all, count, unit, None, cx)
            });
        let note = self.board_note_open().then(|| {
            div()
                .flex_none()
                .px(px(12.))
                .py(px(10.))
                .border_b_1()
                .border_color(theme::border())
                .child(self.composer(cx))
        });
        if shown.columns {
            return (self.page_frame())
                .when(stale.is_some(), |d| d.opacity(STALE))
                .child(self.facts_line(&shown, stale, cx))
                .children(head)
                .children(note)
                .child(self.columns(&shown, cx))
                .into_any_element();
        }
        let list = (self.list_frame())
            .when(stale.is_some(), |d| d.opacity(STALE))
            .child(self.facts_line(&shown, stale.clone(), cx))
            .children(head)
            .children(note)
            .child(
                div()
                    .id("tasks-rows")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.rows_scroll)
                    .child(self.shelves(&shown, cx)),
            )
            .into_any_element();
        let preview = self.preview(&shown, cx);
        self.frame.list_page(list, preview, cx)
    }

    fn list_frame(&self) -> Div {
        artoo_plugin_sdk::kit::island()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme::editor())
    }

    /// The list's groups, each with its head and rows.
    fn shelves(&mut self, shown: &Shown, cx: &mut Context<Self>) -> Div {
        if shown.parts.is_empty() {
            return self.shelf_list(&shown.shelves, shown, cx);
        }
        let mut out = div().flex().flex_col();
        let unit = item_word(&shown.host);
        for part in &shown.parts {
            let id = part_id(part.sprint.id);
            let folded = self.folded(&id);
            out = out.child(self.sprint_head(
                &part.sprint,
                &part.rows,
                part.count,
                unit,
                Some(folded),
                cx,
            ));
            let key = format!("shelf-{id}");
            if folded && !self.motion.recent(&key) {
                continue;
            }
            let groups = self.shelf_list(&part.shelves, shown, cx);
            out = out.children(self.folding(&key, !folded, groups));
        }
        out
    }

    /// Groups `shelves`, each with its head and rows.
    fn shelf_list(&mut self, shelves: &[Shelf], shown: &Shown, cx: &mut Context<Self>) -> Div {
        let mut out = div().flex().flex_col();
        for shelf in shelves {
            let folded = self.folded(&shelf.id);
            out = out.child(self.shelf_head(shelf, folded, cx));
            // Its rows fold in and out moving; folded and still, they are not drawn.
            let key = format!("shelf-{}", shelf.id);
            if folded && !self.motion.recent(&key) {
                continue;
            }
            let mut rows = div().flex().flex_col();
            for r in &shelf.rows {
                rows = rows.child(self.row(r, shown, cx));
            }
            out = out.children(self.folding(&key, !folded, rows));
        }
        out
    }

    fn shelf_head(&mut self, shelf: &Shelf, folded: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = shelf.id.clone();
        let head = div()
            .id(SharedString::from(format!("shelf-{id}")))
            .group("shelf")
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(32.))
            .px(px(14.))
            .bg(theme::base())
            .border_b_1()
            .border_color(theme::border())
            .text_size(px(12.))
            .cursor_pointer();
        let count = |n: String| div().text_size(px(11.)).text_color(theme::muted()).child(n);
        // New task in the group: a column's state, an epic.
        let add =
            |cx: &mut Context<Self>, words: String, epic: Option<String>, state: Option<State>| {
                div()
                    .id(SharedString::from(format!("shelf-add-{}", words)))
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .hover(|s| s.bg(theme::hover()))
                    .child(icons::icon("plus").size(px(12.)).text_color(theme::muted()))
                    .tip(format!("New task in {words}"))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        match &state {
                            Some(state) => this.open_sheet_in(&words, state.clone(), cx),
                            None => this.open_sheet(epic.clone(), None, cx),
                        }
                    }))
            };
        let chevron = look::chevron(SharedString::from(format!("shelf-turn-{id}")), !folded, 12.);
        let head = match &shelf.head {
            Head::State { name, state } => head
                .child(look::state_mark(state, 14.))
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(name.clone()),
                )
                .child(div().flex_1())
                .child(count(shelf.rows.len().to_string()))
                .child(add(cx, name.clone(), None, Some(state.clone())))
                .child(chevron),
            Head::Story { row, done, all } => {
                // The story is a row of its own: a click picks it, its chevron folds it.
                let key = row.key.clone();
                let picked = self.pick.as_deref() == Some(&row.key);
                let fold_id = id.clone();
                return head
                    .when(picked, |d| d.bg(theme::selected()))
                    .child(look::kind_mark(&row.kind, row.subtask))
                    .child(look::key(&row.key))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(row.summary.clone()),
                    )
                    .child(count(format!("{done} of {all}")))
                    .child(self.state_control(row, Anchor::Row(row.key.clone()), true, cx))
                    .child(
                        div()
                            .id(SharedString::from(format!("fold-{id}")))
                            .child(chevron)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.fold(&fold_id, cx)
                            })),
                    )
                    .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                        this.pick_row(&key, cx);
                        if e.click_count() > 1 {
                            this.open_issue(&key, cx);
                        }
                    }))
                    .into_any_element();
            }
            Head::Other => head
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Other issues"),
                )
                .child(div().flex_1())
                .child(count(shelf.rows.len().to_string()))
                .child(chevron),
            Head::Epic {
                epic: Some(e),
                done,
                all,
            } => {
                let e = self.store.read(cx).epic(e);
                head.child(look::kind_mark("Epic", false))
                    .child(look::key(&e.key))
                    .child(look::epic_lozenge(&e))
                    .child(div().flex_1())
                    .child(count(format!("{done} of {all}")))
                    .child(add(cx, e.name.clone(), Some(e.key.clone()), None))
                    .child(chevron)
            }
            Head::Epic {
                epic: None, all, ..
            } => head
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("No epic"),
                )
                .child(div().flex_1())
                .child(count(all.to_string()))
                .child(chevron),
        };
        head.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.fold(&id, cx)))
            .into_any_element()
    }

    /// A row: its summary and who has it, its state on the picked row or under the pointer (always, grouped by
    /// story or epic); under it the kind and key, priority, sub-tasks, comments, what blocks it, its epic.
    fn row(&mut self, r: &Row, shown: &Shown, cx: &mut Context<Self>) -> AnyElement {
        let picked = self.pick.as_deref() == Some(&r.key);
        let me = shown
            .me
            .as_ref()
            .is_some_and(|me| r.assignee.as_ref().is_some_and(|p| p.id == me.id));
        let always = shown.group != Group::State;
        let s = self.store.read(cx);
        let refused = s.refused.contains_key(&(r.key.clone(), "state".into()));
        let saving = s.saving.iter().any(|(k, _)| *k == r.key);
        let epic = r.epic.as_ref().map(|e| s.epic(e));
        let key = r.key.clone();
        let anchor = Anchor::Row(r.key.clone());
        let who_open = self.pop_at(&anchor, PopKind::Assign);
        let state_open = self.pop_at(&anchor, PopKind::Move);
        let mut facts = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .min_w_0()
            .text_size(px(12.))
            .text_color(theme::muted())
            .whitespace_nowrap()
            .child(look::kind_mark(&r.kind, r.subtask))
            .child(look::key(&r.key));
        if let Some(p) = &r.priority {
            facts = facts.child(look::priority_fact(&r.key, p));
        }
        if r.subtasks.1 > 0 {
            facts = facts.child(look::subtasks_fact(&r.key, r.subtasks.0, r.subtasks.1));
        }
        if r.comments > 0 {
            facts = facts.child(look::comments_fact(&r.key, r.comments));
        }
        if let Some(b) = r.blocked_by.first() {
            facts = facts.child(look::blocked_fact(&r.key, b));
        }
        if shown.group != Group::Epic
            && let Some(e) = &epic
        {
            facts = facts.child(look::epic_lozenge(e));
        }
        let who = self.who_control(r, me, anchor.clone(), who_open, cx);
        let chip = self.state_control(r, anchor, true, cx);
        let chip = if always || picked || state_open || refused {
            chip.into_any_element()
        } else {
            // Beside the face, over the title's end: its room is not kept, so the faces line up at the row's end.
            // A hover that changes `display` lays out and paints apart, which GPUI refuses; visibility is safe.
            div()
                .absolute()
                .right(px(28.))
                .invisible()
                .group_hover("tasks-row", |s| s.visible())
                .child(chip)
                .into_any_element()
        };
        let indent = shown.group == Group::Story && r.parent.is_some();
        div()
            .id(SharedString::from(format!("row-{}", r.key)))
            .group("tasks-row")
            .flex_none()
            .flex()
            .flex_col()
            .justify_center()
            // Design's 60 px: 10 above and below, lines of 20 and 18 with 2 between; the text 34 in, as its rows
            // keep 16 and an 8 px dot's room and 10 before it.
            .gap(px(2.))
            .h(px(60.))
            .pl(px(if indent { 48. } else { 34. }))
            .pr(px(16.))
            .border_b_1()
            .border_color(theme::border())
            .when(picked, |d| d.bg(theme::selected()))
            .when(!picked, |d| d.hover(|s| s.bg(theme::hover())))
            .when(saving, |d| d.opacity(0.7))
            .child(
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .h(px(20.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::text())
                            .child(r.summary.clone()),
                    )
                    .child(who)
                    .child(chip),
            )
            .child(facts.h(px(18.)))
            .tip(format!("{} {}", r.key, r.summary))
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                this.pick_row(&key, cx);
                if e.click_count() > 1 {
                    this.open_issue(&key, cx);
                }
            }))
            .into_any_element()
    }

    /// Who has issue `r`, a click giving it to someone else (its people list).
    pub fn who_control(
        &mut self,
        r: &Row,
        me: bool,
        anchor: Anchor,
        open: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = r.key.clone();
        let a = anchor.clone();
        let refused = self
            .store
            .read(cx)
            .refused
            .get(&(r.key.clone(), "assignee".into()))
            .cloned();
        div()
            .id(SharedString::from(format!("who-{}-{}", anchor.id(), r.key)))
            .relative()
            .flex_none()
            .px(px(4.))
            .py(px(1.))
            .rounded(px(4.))
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover()))
            .when(open, |d| d.border_1().border_color(theme::accent()))
            .when(refused.is_some(), |d| {
                d.border_1().border_color(theme::removed())
            })
            .child(look::assignee(r.assignee.as_ref(), me, 16.))
            .tip(refused.unwrap_or_else(|| {
                let who = r
                    .assignee
                    .as_ref()
                    .map_or("Unassigned", |p| p.name.as_str());
                format!("{who}  ·  Assignee  A · to me  I")
            }))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.pick_row(&key, cx);
                this.open_pop(PopKind::Assign, a.clone(), &key, cx);
            }))
            .children(self.pop_here(&anchor, PopKind::Assign, cx))
            .into_any_element()
    }

    /// Issue `r`'s state chip, a click opening Move to; the tracker's refusal said under it.
    pub fn state_control(
        &mut self,
        r: &Row,
        anchor: Anchor,
        small: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = r.key.clone();
        let open = self.pop_at(&anchor, PopKind::Move) || self.pop_at(&anchor, PopKind::Ask);
        let refused = self
            .store
            .read(cx)
            .refused
            .contains_key(&(r.key.clone(), "state".into()));
        let a = anchor.clone();
        let chip = look::state_chip(
            format!("st-{}-{}", anchor.id(), r.key),
            &r.state,
            open,
            refused,
            small,
        )
        .tip("Move  ⌥S")
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            this.pick_row(&key, cx);
            this.open_pop(PopKind::Move, a.clone(), &key, cx);
        }));
        let mut pops: Vec<AnyElement> = self
            .pop_here(&anchor, PopKind::Move, cx)
            .into_iter()
            .collect();
        pops.extend(self.pop_here(&anchor, PopKind::Ask, cx));
        pops.extend(self.refusal_here(&anchor, &r.key, cx));
        div()
            .relative()
            .flex_none()
            .child(chip)
            .children(pops)
            .into_any_element()
    }

    /// The board's columns side by side, each with its cards; grouped by story or epic, a lane for each across
    /// them (`lanes`).
    fn columns(&mut self, shown: &Shown, cx: &mut Context<Self>) -> AnyElement {
        if shown.group != Group::State {
            return self.lanes(shown, cx);
        }
        let mut cols = div()
            .id("tasks-cols")
            .flex_1()
            .min_h_0()
            .flex()
            .gap(px(8.))
            .p(px(10.))
            .overflow_x_scroll();
        for (ix, shelf) in shown.shelves.iter().enumerate() {
            let Head::State { name, state } = &shelf.head else {
                continue;
            };
            let mut col = div()
                .id(SharedString::from(format!("col-{ix}")))
                .flex_1()
                .min_w(px(180.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .p(px(6.))
                .rounded(px(8.))
                .bg(theme::base())
                .overflow_y_scroll()
                .drag_over::<Dragged>(|d, _, _, _| d.bg(theme::hover()))
                .on_drop(
                    cx.listener(move |this, d: &Dragged, _, cx| this.drop_card(&d.key, ix, cx)),
                )
                .child(self.column_head(ix, name, state, shelf.rows.len(), cx));
            for r in &shelf.rows {
                col = col.child(self.card(r, state.done(), shown, cx));
            }
            cols = cols.child(col);
        }
        cols.into_any_element()
    }

    /// A column's head: its state's mark, its name, how many cards it holds, `+` for a new task in its state.
    fn column_head(
        &mut self,
        ix: usize,
        name: &str,
        state: &State,
        count: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(26.))
            .px(px(6.))
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::text())
            .whitespace_nowrap()
            .child(look::state_mark(state, 13.))
            .child(name.to_owned())
            .child(
                div()
                    .px(px(6.))
                    .rounded_full()
                    .bg(theme::hover())
                    .text_size(px(11.))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme::muted())
                    .child(count.to_string()),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id(SharedString::from(format!("col-add-{ix}")))
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .hover(|s| s.bg(theme::hover()))
                    .child(icons::icon("plus").size(px(12.)).text_color(theme::muted()))
                    .tip(format!("New task in {name}"))
                    .on_click(cx.listener({
                        let (name, state) = (name.to_owned(), state.clone());
                        move |this, _: &ClickEvent, _, cx| {
                            this.open_sheet_in(&name, state.clone(), cx)
                        }
                    })),
            )
    }

    /// Grouped by story or epic, as Jira's swimlanes: the columns' heads across the top, then a lane for each group
    /// of the list — its head across the columns, its cards in each, folding as the list's group does —, Other
    /// issues or No epic last.
    fn lanes(&mut self, shown: &Shown, cx: &mut Context<Self>) -> AnyElement {
        let columns = shown.info.columns.clone();
        let states: Vec<State> = (0..columns.len())
            .map(|ix| column_state(&shown.info, shown.all.iter(), ix))
            .collect();
        let col = |r: &Row| shown.info.column_of(&r.state);
        // The columns hold their 180 px each and the gaps between them, the lanes' edges besides; past the page's
        // width the lanes scroll sideways under their heads.
        let wide = columns.len() as f32 * 188. + 4.;
        let mut heads = div().flex_none().flex().gap(px(8.)).px(px(6.)).pb(px(6.));
        for (ix, c) in columns.iter().enumerate() {
            let count = (shown.shelves.iter().flat_map(|s| &s.rows))
                .filter(|r| col(r) == ix)
                .count();
            heads = heads.child(div().flex_1().min_w(px(180.)).child(self.column_head(
                ix,
                &c.name,
                &states[ix],
                count,
                cx,
            )));
        }
        let mut body = div()
            .id("tasks-lanes")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .pb(px(10.))
            .overflow_y_scroll();
        for shelf in &shown.shelves {
            let folded = self.folded(&shelf.id);
            let key = format!("lane-{}", shelf.id);
            let mut lane = div()
                .flex_none()
                .flex()
                .flex_col()
                .px(px(6.))
                .pb(px(6.))
                .rounded(px(8.))
                .bg(theme::base())
                .child(self.lane_head(shelf, folded, shown, cx));
            // Its cards fold in and out moving; folded and still, they are not drawn.
            if !folded || self.motion.recent(&key) {
                let mut cells = div().flex().gap(px(8.));
                for (ix, state) in states.iter().enumerate() {
                    let mut cell = div().flex_1().min_w(px(180.)).flex().flex_col().gap(px(6.));
                    for r in shelf.rows.iter().filter(|r| col(r) == ix) {
                        cell = cell.child(self.card(r, state.done(), shown, cx));
                    }
                    cells = cells.child(cell);
                }
                lane = lane.children(self.folding(&key, !folded, cells));
            }
            body = body.child(lane);
        }
        div()
            .id("tasks-lanes-across")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .pt(px(10.))
            .px(px(10.))
            .overflow_x_scroll()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .min_w(px(wide))
                    .flex()
                    .flex_col()
                    .child(heads)
                    .child(body),
            )
            .into_any_element()
    }

    /// A lane's head across the columns, its chevron first: a story's kind, key and summary, its sub-tasks done of
    /// all, who has it and its state, picked and opened as the list's story is; an epic's key and name, its issues
    /// done of all, `+`; or Other issues, No epic, with how many. A click on it, or on a story's chevron, folds it.
    fn lane_head(
        &mut self,
        shelf: &Shelf,
        folded: bool,
        shown: &Shown,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = shelf.id.clone();
        let head = div()
            .id(SharedString::from(format!("lane-{id}")))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(34.))
            .px(px(4.))
            .text_size(px(12.))
            .text_color(theme::muted())
            .whitespace_nowrap()
            .cursor_pointer();
        let chevron = look::chevron(SharedString::from(format!("lane-turn-{id}")), !folded, 12.);
        let faint = |words: String| div().flex_none().text_color(theme::faint()).child(words);
        let title = |words: String| {
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(words)
        };
        let head = match &shelf.head {
            Head::Story { row, done, all } => {
                let key = row.key.clone();
                let picked = self.pick.as_deref() == Some(&row.key);
                let me = (shown.me.as_ref())
                    .is_some_and(|me| row.assignee.as_ref().is_some_and(|p| p.id == me.id));
                let fold_id = id.clone();
                return head
                    .rounded(px(6.))
                    .when(picked, |d| d.bg(theme::selected()))
                    .child(
                        div()
                            .id(SharedString::from(format!("lane-fold-{id}")))
                            .flex_none()
                            .child(chevron)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.fold(&fold_id, cx)
                            })),
                    )
                    .child(look::kind_mark(&row.kind, row.subtask))
                    .child(look::key(&row.key))
                    .child(title(row.summary.clone()))
                    .child(faint(format!("{done} of {all}")))
                    .child(div().flex_1())
                    .children(row.assignee.as_ref().map(|p| look::face(&p.name, me, 16.)))
                    .child(self.state_control(row, Anchor::Row(row.key.clone()), true, cx))
                    .tip(format!("{} {}", row.key, row.summary))
                    .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                        this.pick_row(&key, cx);
                        if e.click_count() > 1 {
                            this.open_issue(&key, cx);
                        }
                    }))
                    .into_any_element();
            }
            Head::Epic {
                epic: Some(e),
                done,
                all,
            } => {
                let e = self.store.read(cx).epic(e);
                let (name, key) = (e.name.clone(), e.key.clone());
                head.child(chevron)
                    .child(look::kind_mark("Epic", false))
                    .child(look::key(&e.key))
                    .child(look::epic_lozenge(&e))
                    .child(faint(format!("{done} of {all}")))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id(SharedString::from(format!("lane-add-{id}")))
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .hover(|s| s.bg(theme::hover()))
                            .child(icons::icon("plus").size(px(12.)).text_color(theme::muted()))
                            .tip(format!("New task in {name}"))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.open_sheet(Some(key.clone()), None, cx);
                            })),
                    )
            }
            Head::Epic {
                epic: None, all, ..
            } => head
                .child(chevron)
                .child(title("No epic".into()))
                .child(faint(all.to_string())),
            Head::Other => head
                .child(chevron)
                .child(title("Other issues".into()))
                .child(faint(shelf.rows.len().to_string())),
            Head::State { name, .. } => head
                .child(chevron)
                .child(title(name.clone()))
                .child(faint(shelf.rows.len().to_string())),
        };
        head.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.fold(&id, cx)))
            .into_any_element()
    }

    fn card(&mut self, r: &Row, done: bool, shown: &Shown, cx: &mut Context<Self>) -> AnyElement {
        let picked = self.pick.as_deref() == Some(&r.key);
        let s = self.store.read(cx);
        let me = shown
            .me
            .as_ref()
            .is_some_and(|me| r.assignee.as_ref().is_some_and(|p| p.id == me.id));
        let epic = r.epic.as_ref().map(|e| s.epic(e));
        let key = r.key.clone();
        let anchor = Anchor::Row(r.key.clone());
        let drag = Dragged {
            key: r.key.clone(),
            summary: r.summary.clone(),
        };
        let store = self.store.clone();
        // clip: in a narrow column the last facts give way, never the face at the end.
        let mut meta = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(5.))
            .whitespace_nowrap()
            .overflow_hidden()
            .child(look::kind_mark(&r.kind, r.subtask))
            .child(look::key(&r.key));
        if let Some(p) = &r.priority {
            meta = meta.child(look::priority_fact(&r.key, p));
        }
        if r.subtasks.1 > 0 {
            meta = meta.child(look::subtasks_fact(&r.key, r.subtasks.0, r.subtasks.1));
        }
        if r.comments > 0 {
            meta = meta.child(look::comments_fact(&r.key, r.comments));
        }
        if let Some(b) = r.blocked_by.first() {
            meta = meta.child(look::blocked_fact(&r.key, b));
        }
        let meta = div()
            .flex()
            .items_center()
            .gap(px(5.))
            .text_size(px(12.))
            .text_color(theme::muted())
            .child(meta)
            .child(
                div()
                    .id(SharedString::from(format!("face-{}", r.key)))
                    .flex_none()
                    .child(look::assignee(r.assignee.as_ref(), me, 16.))
                    .tip(match &r.assignee {
                        Some(p) => p.name.clone(),
                        None => "Unassigned".to_owned(),
                    }),
            );
        let lozenge = (shown.group != Group::Epic)
            .then(|| epic.as_ref().map(look::epic_lozenge))
            .flatten();
        div()
            .id(SharedString::from(format!("card-{}", r.key)))
            .relative()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .px(px(10.))
            .pt(px(10.))
            .pb(px(8.))
            .rounded(px(6.))
            .bg(theme::panel())
            .border_1()
            .border_color(if picked {
                theme::accent()
            } else {
                theme::border()
            })
            .hover(|s| s.bg(theme::hover()))
            .cursor_pointer()
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .line_clamp(2)
                    .text_color(if done { theme::muted() } else { theme::text() })
                    .child(r.summary.clone()),
            )
            .children(lozenge)
            .child(meta)
            .children(self.pop_here(&anchor, PopKind::Ask, cx))
            .children(self.refusal_here(&anchor, &r.key, cx))
            .on_drag(drag, move |d, _, _, cx| {
                // Its moves are read as it is lifted, so they are known where it lands.
                store.update(cx, |s, cx| s.read_moves(&d.key, cx));
                cx.new(|_| d.clone())
            })
            .tip(format!("{} {}", r.key, r.summary))
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                this.pick_row(&key, cx);
                if e.click_count() > 1 {
                    this.open_issue(&key, cx);
                }
            }))
            .into_any_element()
    }

    /// Card `key` dropped on column `ix`: the move the tracker offers into it, made; one that asks for values asks
    /// them where the card is; no way there said in a sentence.
    pub(crate) fn drop_card(&mut self, key: &str, ix: usize, cx: &mut Context<Self>) {
        let s = self.store.read(cx);
        let Some(column) = s.board_info().and_then(|i| i.columns.get(ix)).cloned() else {
            return;
        };
        let Some(row) = s
            .board_rows()
            .and_then(|rows| rows.iter().find(|r| r.key == key))
        else {
            return;
        };
        if column.states.contains(&row.state.id) {
            return;
        }
        let from = look::display_state(&row.state.name);
        let anchor = Anchor::Row(key.to_owned());
        let found = match s.moves.get(key) {
            Some(Read::Done(moves)) => moves
                .iter()
                .find(|m| column.states.contains(&m.to.id))
                .cloned()
                .ok_or(format!(
                    "{from} has no way to {} in its workflow",
                    column.name
                )),
            _ => Err("Its moves were still being read; try again".to_owned()),
        };
        match found {
            Ok(m) if m.asks.is_empty() => {
                self.make_move(key, &m, artoo_plugin_sdk::serde_json::json!({}), anchor, cx)
            }
            Ok(m) => {
                self.refused_to = Some(look::display_state(&m.to.name));
                self.open_pop(PopKind::Move, anchor, key, cx);
                if let Some(p) = self.pop.as_mut() {
                    p.kind = PopKind::Ask;
                    p.asked = vec![0; m.asks.len()];
                    p.ask = Some(m);
                }
                cx.notify();
            }
            Err(why) => {
                self.refused_to = Some(look::display_state(&column.name));
                self.store.update(cx, |s, cx| {
                    s.refused.insert((key.to_owned(), "state".into()), why);
                    cx.notify();
                });
                self.refusal = Some((anchor, key.to_owned()));
                cx.notify();
            }
        }
    }

    /// The picked issue beside the list: its kind and key, its title, its facts, its description's first part,
    /// Open and Open in Jira.
    fn preview(&mut self, shown: &Shown, cx: &mut Context<Self>) -> AnyElement {
        let stale = self.stale(cx).is_some();
        let frame = || {
            artoo_plugin_sdk::kit::island()
                .size_full()
                .min_w_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .bg(theme::editor())
                .when(stale, |d| d.opacity(STALE))
        };
        let Some(r) = self
            .pick
            .as_ref()
            .and_then(|k| shown.all.iter().find(|r| &r.key == k))
            .cloned()
        else {
            return frame()
                .child(controls::empty_state(
                    "list-checks",
                    "No issue picked".into(),
                    String::new(),
                    None,
                ))
                .into_any_element();
        };
        let s = self.store.read(cx);
        let issue: Option<Issue> = s.issue(&r.key).cloned();
        let me = shown
            .me
            .as_ref()
            .is_some_and(|me| r.assignee.as_ref().is_some_and(|p| p.id == me.id));
        let epic = r.epic.as_ref().map(|e| s.epic(e));
        let parent = r
            .parent
            .as_ref()
            .and_then(|p| shown.all.iter().find(|x| &x.key == p))
            .cloned();
        let url = issue.as_ref().map(|i| i.url.clone()).or_else(|| {
            s.site_of(&shown.host)
                .map(|site| format!("{}/browse/{}", site.url, r.key))
        });
        let kv = |k: &'static str, v: AnyElement| {
            div()
                .flex()
                .gap(px(12.))
                .min_h(px(20.))
                .items_center()
                .child(
                    div()
                        .w(px(84.))
                        .flex_none()
                        .text_color(theme::faint())
                        .child(k),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.))
                        .overflow_hidden()
                        .text_color(theme::text())
                        .child(v),
                )
        };
        let anchor = Anchor::Preview;
        let mut rows = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .mt(px(16.))
            .py(px(14.))
            .border_t_1()
            .border_b_1()
            .border_color(theme::border())
            .text_size(px(12.));
        if let Some(p) = &parent {
            rows = rows.child(kv(
                "Parent",
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .min_w_0()
                    .child(look::kind_mark(&p.kind, false))
                    .child(look::key(&p.key))
                    .child(div().truncate().child(p.summary.clone()))
                    .into_any_element(),
            ));
        }
        if let Some(e) = &epic {
            rows = rows.child(kv(
                "Epic",
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(look::kind_mark("Epic", false))
                    .child(look::epic_lozenge(e))
                    .child(look::key(&e.key))
                    .into_any_element(),
            ));
        }
        rows = rows.child(kv(
            "State",
            self.state_control(&r, anchor.clone(), false, cx),
        ));
        let assignee = match &r.assignee {
            Some(p) => div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(look::face(&p.name, me, 16.))
                .child(if me { "You".to_owned() } else { p.name.clone() }),
            None => div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_color(theme::faint())
                .child(look::nobody(16.))
                .child("Unassigned"),
        };
        let key = r.key.clone();
        let who_open = self.pop_at(&anchor, PopKind::Assign);
        rows = rows.child(kv(
            "Assignee",
            div()
                .id("pv-who")
                .relative()
                .cursor_pointer()
                .rounded(px(4.))
                .when(who_open, |d| d.border_1().border_color(theme::accent()))
                .child(assignee)
                .tip("Assignee  A · to me  I")
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.open_pop(PopKind::Assign, Anchor::Preview, &key, cx)
                }))
                .children(self.pop_here(&Anchor::Preview, PopKind::Assign, cx))
                .into_any_element(),
        ));
        if let Some(rep) = issue.as_ref().and_then(|i| i.reporter.clone()) {
            let mine = shown.me.as_ref().is_some_and(|m| m.id == rep.id);
            rows = rows.child(kv(
                "Reporter",
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(look::face(&rep.name, mine, 16.))
                    .child(rep.name.clone())
                    .into_any_element(),
            ));
        }
        if let Some(p) = &r.priority {
            rows = rows.child(kv("Priority", look::priority(p, true).into_any_element()));
        }
        if let Some(sp) = &r.sprint {
            let ends = sp.end.as_ref().map(|end| {
                let day = model::day_words(end);
                div()
                    .id("pv-ends")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_color(theme::muted())
                    .child(
                        icons::icon("clock")
                            .size(px(12.))
                            .text_color(theme::muted()),
                    )
                    .child(day.clone())
                    .tip(format!("Ends {day}"))
            });
            rows = rows.child(kv(
                "Sprint",
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(sp.name.clone())
                    .children(ends)
                    .into_any_element(),
            ));
        }
        if !r.labels.is_empty() {
            rows = rows.child(kv(
                "Labels",
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.))
                    .children(r.labels.iter().map(|l| look::pill(l)))
                    .into_any_element(),
            ));
        }
        if r.subtasks.1 > 0 {
            rows = rows.child(kv(
                "Sub-tasks",
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(look::progress(r.subtasks.0, r.subtasks.1))
                    .child(format!("{}/{}", r.subtasks.0, r.subtasks.1))
                    .into_any_element(),
            ));
        }
        if r.comments > 0 {
            let last = issue.as_ref().and_then(|i| i.comments.last()).map(|c| {
                let name = c.author.as_ref().map_or("someone", |p| p.name.as_str());
                let face = look::face(name, false, 14.);
                div()
                    .id("pv-last")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_color(theme::muted())
                    .child(face)
                    .child(short_ago(&c.created))
                    .tip(format!("Last from {name}, {}", model::ago(&c.created)))
            });
            rows = rows.child(kv(
                "Comments",
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(r.comments.to_string())
                    .children(last)
                    .into_any_element(),
            ));
        }
        if let Some(b) = r.blocked_by.first() {
            rows = rows.child(kv(
                "Blocked by",
                div()
                    .id("pv-blocked")
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(look::blocked_mark())
                    .child(
                        div()
                            .flex_none()
                            .font_family(theme::code_font())
                            .text_color(theme::removed())
                            .child(b.key.clone()),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(b.summary.clone()))
                    .child(look::state_mark(&b.state, 14.))
                    .tip(format!(
                        "Blocked by {} {} · {}",
                        b.key,
                        b.summary,
                        look::display_state(&b.state.name)
                    ))
                    .into_any_element(),
            ));
        }
        let first = issue
            .as_ref()
            .and_then(|i| model::paragraphs(&i.description).into_iter().next());
        let desc = first.map(|p| {
            div()
                .mt(px(14.))
                .text_size(px(13.))
                .line_height(px(20.))
                .text_color(theme::muted())
                .child(markdown::body(
                    &p,
                    true,
                    "tasks-pv-desc".to_string(),
                    None,
                    None,
                    cx,
                ))
        });
        let open_key = r.key.clone();
        let acts = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .mt(px(16.))
            .child(
                controls::primary_button("tasks-pv-open", "Open", "↵").on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.open_issue(&open_key, cx)),
                ),
            )
            .children(url.map(|url| {
                let words = format!("Open in {}", crate::trackers::tracker_of(&url));
                look::small_button("tasks-pv-jira", "")
                    .size(px(28.))
                    .px_0()
                    .justify_center()
                    .child(
                        icons::icon("external")
                            .size(px(12.))
                            .text_color(theme::muted()),
                    )
                    .tip(format!("{words}  ⌥⌘J"))
                    .on_click(move |_, _, cx| cx.open_url(&url))
            }));
        frame()
            .child(
                div()
                    .id("tasks-pv")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .px(px(20.))
                    .py(px(18.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(look::kind_mark(&r.kind, r.subtask))
                            .child(div().text_color(theme::accent()).child(r.key.clone())),
                    )
                    .child(
                        div()
                            .mt(px(8.))
                            .text_size(px(18.))
                            .line_height(px(24.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(r.summary.clone()),
                    )
                    .child(rows)
                    .children(desc)
                    .child(acts),
            )
            .into_any_element()
    }

    /// The people beside Mine · All: you and the board's people by how many issues, five fitting, past five you and
    /// three more and `+N` holding everyone; a face picks that person's issues, several add up.
    pub fn faces(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let s = self.store.read(cx);
        let board = s.picked()?.clone();
        let rows = s.board_rows()?;
        let me = s.me.get(&board.site).cloned();
        let play = in_play(rows);
        let all = team(&play, me.as_ref());
        let picked: HashSet<String> = s.faces.iter().cloned().collect();
        let unit = item_word(&board.site);
        let id_of = |p: &Option<Person>| p.as_ref().map_or(NONE.to_owned(), |p| p.id.clone());
        let many = all.len() > 5;
        let mut row: Vec<&(Option<Person>, usize)> = Vec::new();
        let mut hidden = 0;
        if many {
            let lead = all.first().filter(|(p, _)| {
                me.as_ref()
                    .is_some_and(|me| p.as_ref().is_some_and(|p| p.id == me.id))
            });
            row.extend(lead);
            let room = 4 - row.len();
            let others: Vec<&(Option<Person>, usize)> = all.iter().skip(row.len()).collect();
            let on: Vec<&(Option<Person>, usize)> = others
                .iter()
                .copied()
                .filter(|x| picked.contains(&id_of(&x.0)))
                .collect();
            hidden = on.len().saturating_sub(room);
            let mut chosen: Vec<&(Option<Person>, usize)> = on.into_iter().take(room).collect();
            for x in &others {
                if chosen.len() >= room {
                    break;
                }
                if !chosen.iter().any(|c| id_of(&c.0) == id_of(&x.0))
                    && !picked.contains(&id_of(&x.0))
                {
                    chosen.push(x);
                }
            }
            chosen.sort_by_key(|x| all.iter().position(|a| id_of(&a.0) == id_of(&x.0)));
            row.extend(chosen);
        } else {
            row.extend(all.iter());
        }
        let more = all.len() - row.len();
        let mut out = div().flex().items_center().gap(px(2.));
        for (p, n) in row {
            let id = id_of(p);
            let on = picked.contains(&id);
            let mine = me
                .as_ref()
                .is_some_and(|me| p.as_ref().is_some_and(|p| p.id == me.id));
            let name = p
                .as_ref()
                .map_or("Unassigned".to_owned(), |p| p.name.clone());
            let face = match p {
                Some(p) => look::face(&p.name, mine, 18.),
                None => div()
                    .size(px(18.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme::faint())
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::icon("user").size(px(11.)).text_color(theme::muted())),
            };
            let pick = id.clone();
            out = out.child(
                div()
                    .id(SharedString::from(format!("face-{id}")))
                    .p(px(2.))
                    .rounded_full()
                    .cursor_pointer()
                    .when(on, |d| d.bg(theme::accent()))
                    .hover(|s| s.bg(theme::hover()))
                    .child(face)
                    .tip(format!(
                        "{name} · {}",
                        artoo_plugin_sdk::kit::format::plural(*n, unit)
                    ))
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_face(&pick, cx)),
                    ),
            );
        }
        if many {
            let open = self.pop_at(&Anchor::Faces, PopKind::People);
            out = out.child(
                div()
                    .id("faces-more")
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .h(px(22.))
                    .px(px(6.))
                    .rounded_full()
                    .text_size(px(11.))
                    .text_color(theme::muted())
                    .bg(theme::hover())
                    .cursor_pointer()
                    .when(open || hidden > 0, |d| {
                        d.border_1().border_color(theme::accent())
                    })
                    .child(format!("+{more}"))
                    .when(hidden > 0, |d| {
                        d.child(div().text_color(theme::accent()).child(hidden.to_string()))
                    })
                    .tip(format!("Everyone on this board · {}", all.len()))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.open_pop(PopKind::People, Anchor::Faces, "", cx)
                    }))
                    .children(self.pop_here(&Anchor::Faces, PopKind::People, cx)),
            );
        }
        Some(out.into_any_element())
    }

    /// The board picked, when its last read failed after one that did not: its site's host and when it was read.
    fn stale(&self, cx: &App) -> Option<(String, String)> {
        let s = self.store.read(cx);
        (s.board.as_ref())
            .filter(|b| s.stale.contains_key(*b))
            .and_then(|b| Some((b.0.clone(), s.read_at.get(b).copied()?)))
            .map(|(host, at)| (host, artoo_plugin_sdk::kit::format::clock(at)))
    }

    /// When the board picked was read, while it is shown as the Mac kept it.
    fn kept_at(&self, cx: &App) -> Option<String> {
        let s = self.store.read(cx);
        let board = s.board.as_ref().filter(|b| s.from_disk.contains(*b))?;
        let at = s.read_at.get(board).copied()?;
        Some(artoo_plugin_sdk::kit::format::clock(at))
    }

    fn page_frame(&self) -> Div {
        artoo_plugin_sdk::kit::island()
            .flex_1()
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .bg(theme::editor())
    }
}

/// Rows being read, inside the page's padding as its rows' text is, never against its edges.
pub fn skeleton_in_page(id: &'static str, rows: usize) -> Div {
    div()
        .px(px(16.))
        .py(px(16.))
        .child(controls::skeleton(id, rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Column;
    use crate::slices::SliceRead;

    fn st(id: &str, cat: &str) -> State {
        State {
            id: id.into(),
            name: id.into(),
            category: cat.into(),
        }
    }

    fn row(key: &str, state: &str, cat: &str, who: Option<&str>) -> Row {
        Row {
            key: key.into(),
            summary: format!("Summary of {key}"),
            kind: "Task".into(),
            state: st(state, cat),
            assignee: who.map(|w| Person {
                id: w.into(),
                name: w.into(),
            }),
            sprint: Some(Sprint {
                id: 34,
                name: "Sprint 34".into(),
                state: "active".into(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn info() -> BoardInfo {
        BoardInfo {
            columns: vec![
                Column {
                    name: "To do".into(),
                    states: vec!["todo".into()],
                },
                Column {
                    name: "In progress".into(),
                    states: vec!["prog".into()],
                },
                Column {
                    name: "Done".into(),
                    states: vec!["done".into()],
                },
            ],
            epics: vec![Epic {
                key: "PAY-400".into(),
                name: "Refunds".into(),
                color: 4,
                done: false,
            }],
            sprints: vec![Sprint {
                id: 34,
                name: "Sprint 34".into(),
                state: "active".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn grouped_by_state_each_column_holds_its_issues_and_sub_tasks_stay_under_their_parent() {
        let mut sub = row("PAY-413", "done", "done", Some("you"));
        sub.parent = Some("PAY-412".into());
        let mut parent = row("PAY-412", "prog", "indeterminate", Some("you"));
        parent.subtasks = (1, 1);
        let rows = [
            row("PAY-418", "todo", "new", None),
            parent,
            sub,
            row("PAY-395", "done", "done", Some("j")),
        ];
        let play: Vec<&Row> = rows.iter().collect();
        let me = Person {
            id: "you".into(),
            name: "You".into(),
        };
        let all = shelves(&play, &info(), Group::State, &Filter::default(), Some(&me));
        let keys: Vec<Vec<&str>> = all
            .iter()
            .map(|s| s.rows.iter().map(|r| r.key.as_str()).collect())
            .collect();
        assert_eq!(
            keys,
            vec![vec!["PAY-418"], vec!["PAY-412"], vec!["PAY-395"]]
        );
        let mine = Filter {
            mine: true,
            ..Default::default()
        };
        let keys: Vec<usize> = shelves(&play, &info(), Group::State, &mine, Some(&me))
            .iter()
            .map(|s| s.rows.len())
            .collect();
        assert_eq!(keys, vec![0, 1, 0]);
        let story = shelves(&play, &info(), Group::Story, &Filter::default(), Some(&me));
        assert_eq!(
            story[0].head,
            Head::Story {
                row: Box::new(rows[1].clone()),
                done: 1,
                all: 1
            }
        );
        assert_eq!(story[0].rows[0].key, "PAY-413");
        assert_eq!(
            story[1]
                .rows
                .iter()
                .map(|r| r.key.as_str())
                .collect::<Vec<_>>(),
            ["PAY-418", "PAY-395"]
        );
    }

    #[test]
    fn a_column_is_drawn_with_the_state_an_issue_is_in_else_its_first_by_its_place() {
        let rows = [row("PAY-412", "prog", "indeterminate", None)];
        assert_eq!(column_state(&info(), rows.iter(), 1), rows[0].state);
        let empty = column_state(&info(), rows.iter(), 2);
        assert_eq!((empty.id.as_str(), empty.name.as_str()), ("done", "Done"));
        assert!(empty.done());
        assert_eq!(column_state(&info(), rows.iter(), 0).category, "new");
        assert_eq!(column_state(&info(), rows.iter(), 9), State::default());
    }

    #[test]
    fn grouped_by_epic_no_epic_comes_last_and_counts_what_is_done() {
        let mut a = row("PAY-412", "prog", "indeterminate", None);
        a.epic = Some(Epic {
            key: "PAY-400".into(),
            name: "Refunds".into(),
            color: 0,
            done: false,
        });
        let mut b = row("PAY-390", "done", "done", None);
        b.epic = a.epic.clone();
        let rows = [a, b, row("PAY-401", "todo", "new", None)];
        let play: Vec<&Row> = rows.iter().collect();
        let all = shelves(&play, &info(), Group::Epic, &Filter::default(), None);
        assert_eq!(all.len(), 2);
        assert!(
            matches!(&all[0].head, Head::Epic { epic: Some(e), done: 1, all: 2 } if e.key == "PAY-400")
        );
        assert!(matches!(
            &all[1].head,
            Head::Epic {
                epic: None,
                all: 1,
                ..
            }
        ));
    }

    #[test]
    fn the_filter_finds_by_key_summary_and_label_and_people_add_up() {
        let mut r = row("PAY-412", "prog", "indeterminate", Some("you"));
        r.labels = vec!["refunds".into()];
        assert!(matches(&r, "412"));
        assert!(matches(&r, "summary label:refunds"));
        assert!(!matches(&r, "label:ledger"));
        let rows = [
            r,
            row("PAY-1", "todo", "new", None),
            row("PAY-2", "todo", "new", Some("k")),
        ];
        let play: Vec<&Row> = rows.iter().collect();
        let f = Filter {
            faces: vec!["you".into(), NONE.into()],
            ..Default::default()
        };
        assert_eq!(
            shown(&play, &f, None)
                .iter()
                .map(|r| r.key.as_str())
                .collect::<Vec<_>>(),
            ["PAY-412", "PAY-1"]
        );
        let me = Person {
            id: "you".into(),
            name: "You".into(),
        };
        let t = team(&play, Some(&me));
        assert_eq!(t[0].0.as_ref().map(|p| p.id.as_str()), Some("you"));
        assert!(t.last().unwrap().0.is_none());
    }

    #[test]
    fn all_active_sprints_are_a_part_each_holding_its_own_issues_and_groups() {
        let mut info = info();
        for (id, state) in [(35, "future"), (36, "active")] {
            info.sprints.push(Sprint {
                id,
                name: format!("Sprint {id}"),
                state: state.into(),
                ..Default::default()
            });
        }
        let read = |rows: Vec<Row>, count: usize| {
            let mut r = SliceRead::default();
            r.rows = rows;
            r.count = Some(count);
            r
        };
        let mut slices = Slices::default();
        slices.reads.insert(
            SliceId::Sprint(34),
            read(
                vec![
                    row("PAY-1", "todo", "new", Some("you")),
                    row("PAY-2", "done", "done", None),
                ],
                2,
            ),
        );
        slices.reads.insert(
            SliceId::Sprint(35),
            read(vec![row("PAY-9", "todo", "new", None)], 1),
        );
        slices.reads.insert(
            SliceId::Sprint(36),
            read(vec![row("PAY-3", "prog", "indeterminate", None)], 7),
        );
        let you = Person {
            id: "you".into(),
            name: "You".into(),
        };
        let parts = parts(&info, &slices, Group::State, &Filter::default(), Some(&you));
        let ids: Vec<u64> = parts.iter().map(|p| p.sprint.id).collect();
        assert_eq!(ids, [34, 36], "the active sprints, as the board lists them");
        let keys = |p: &Part| -> Vec<String> { p.rows.iter().map(|r| r.key.clone()).collect() };
        assert_eq!(keys(&parts[0]), ["PAY-1", "PAY-2"]);
        assert_eq!(keys(&parts[1]), ["PAY-3"]);
        assert_eq!(parts[1].count, Some(7));
        let groups: Vec<(&str, usize)> = (parts[0].shelves.iter())
            .map(|g| (g.id.as_str(), g.rows.len()))
            .collect();
        assert_eq!(
            groups,
            [
                ("sprint:34/state:To do", 1),
                ("sprint:34/state:In progress", 0),
                ("sprint:34/state:Done", 1)
            ]
        );
        assert!(done_group("sprint:34/state:Done") && !done_group("sprint:34/state:To do"));
        let mine = Filter {
            mine: true,
            ..Default::default()
        };
        let parts = super::parts(&info, &slices, Group::State, &mine, Some(&you));
        let shown: usize = parts[1].shelves.iter().map(|g| g.rows.len()).sum();
        assert_eq!(shown, 0, "the filters keep rows per part");
        assert_eq!(
            parts[1].rows.len(),
            1,
            "a part's progress counts all it holds"
        );
    }

    #[test]
    fn a_sprints_dates_say_their_month_once() {
        let (start, end) = ("2026-11-16T09:00:00.000Z", "2026-11-27T17:00:00.000Z");
        assert_eq!(span_words(Some(start), end), "Mon 16 – Fri 27 Nov");
        assert_eq!(
            span_words(Some(start), "2026-12-04T17:00:00.000Z"),
            "Mon 16 Nov – Fri 4 Dec"
        );
        assert_eq!(span_words(None, end), "Fri 27 Nov");
    }

    #[test]
    fn epics_are_not_in_play_and_every_sprint_of_the_slice_is() {
        let mut later = row("PAY-500", "todo", "new", None);
        later.sprint = Some(Sprint {
            id: 35,
            name: "Sprint 35".into(),
            state: "future".into(),
            ..Default::default()
        });
        let mut epic = row("PAY-400", "todo", "new", None);
        epic.kind = "Epic".into();
        let rows = vec![row("PAY-1", "todo", "new", None), later, epic];
        let keys: Vec<&str> = in_play(&rows).iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["PAY-1", "PAY-500"]);
    }
}
