//! A board as the space's own page shows it: which issues it shows (the slice picked, Mine or All, the people
//! picked, what the filter says), grouped by state, story or epic; drawn as a list beside a preview or as columns of
//! cards, a lane across them for each story or epic. What is drawn here reads plain data (`Shown`); the page's state
//! and its clicks are the page's (`page.rs`).

use crate::fresh;
use crate::keep::Group;
use crate::look::{self, sep};
use crate::model::{self, BoardInfo, Epic, Issue, Person, Row, Sprint, State};
use crate::page::{Anchor, Page, PopKind};
use crate::slices::{self, Progress, SliceId, Slices};
use crate::store::Read;
use artoo_plugin_sdk::gpui;
use artoo_plugin_sdk::gpui::{
    AnyElement, App, ClickEvent, Context, Div, FontWeight, IntoElement, Render, SharedString,
    Window, div, prelude::*,
};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, icons, markdown, px, theme};
use artoo_plugin_sdk::shell::MenuItem;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Instant;

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

/// How long `Read 78 of 78` is said once a slice is read whole, in seconds.
const READ_TOLD: i64 = 3;

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

/// What leads the facts line: the slice shown as the sprint picker, or the board's own name (a board without
/// sprints: no picker, no count, not its kind).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lead {
    Picker(String),
    Board(String),
}

/// What leads the facts line of board `board` as `info` lists it, its slice shown named `slice` when it is read by
/// slice.
pub fn lead_words(board: &str, info: &BoardInfo, slice: Option<&str>) -> Lead {
    match slice {
        Some(name) if !info.sprintless => Lead::Picker(name.to_owned()),
        _ => Lead::Board(board.to_owned()),
    }
}

/// What a slice shows in place of its issues: one line and one action each.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Empty {
    /// Being read, none yet: a skeleton, the facts line saying how far.
    Reading,
    /// Not read, and none read before: why, and Retry.
    Failed(String),
    /// A sprint with no issues: Plan from Backlog.
    Unplanned(String),
    /// The Backlog or a board holding none.
    Nothing,
    /// The filter's words or the people picked keep none: they are named.
    Hidden { words: String, people: Vec<String> },
    /// Mine keeps none of what the slice holds, so many as counted: Show all.
    NotYours(Option<usize>),
}

/// What a slice shows in place of its issues, if anything: as it is read (`read`), the sprint it is (`sprint`), the
/// issues it holds as read (`all`) and those `filter` keeps (`kept`); `asked` when its filter is asked of the tracker
/// (the Backlog), whose rows are then those it found.
pub fn empty_of(
    read: Option<&Read<()>>,
    sprint: Option<&str>,
    all: usize,
    kept: usize,
    filter: &Filter,
    asked: bool,
    count: Option<usize>,
) -> Option<Empty> {
    if kept > 0 {
        return None;
    }
    let named = !filter.words.is_empty() || !filter.faces.is_empty();
    let filtered = named || filter.mine;
    if all == 0 && !(asked && filtered) || all == 0 && matches!(read, None | Some(Read::Reading)) {
        return Some(match read {
            None | Some(Read::Reading) => Empty::Reading,
            Some(Read::Failed(why)) => Empty::Failed(why.clone()),
            Some(Read::Done(())) => match sprint {
                Some(name) => Empty::Unplanned(name.to_owned()),
                None => Empty::Nothing,
            },
        });
    }
    Some(if named {
        Empty::Hidden {
            words: filter.words.clone(),
            people: filter.faces.clone(),
        }
    } else {
        Empty::NotYours(count)
    })
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
    /// The Backlog's issues the filters keep, in the tracker's rank: listed so, ungrouped.
    pub ranked: Option<Vec<Row>>,
    /// What the slice shows in place of its issues.
    pub empty: Option<Empty>,
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
    /// Its read: on its way, done, or why not; All's the first of its sprints' that failed, else one on its way.
    pub state: Option<Read<()>>,
    /// How many issues are read, sub-tasks left out.
    pub read: usize,
    /// How many its filter finds, as counted: the Backlog's `Showing 50 of 214`.
    pub found: Option<usize>,
    /// It holds issues not read yet: the Backlog's Load more, a big board's.
    pub more: bool,
    /// When it was read whole last.
    pub at: Option<i64>,
}

/// The slice `slices` shows of a board as `info` lists it, once one is picked.
fn slice_shown(sl: &Slices, info: &BoardInfo) -> Option<SliceShown> {
    let id = sl.view.clone()?;
    let past = match &sl.past {
        Some(Read::Done(past)) => past.as_slice(),
        _ => &[],
    };
    let reads: Vec<&slices::SliceRead> = match &id {
        SliceId::All => slices::active(info)
            .filter_map(|s| sl.reads.get(&SliceId::Sprint(s.id)))
            .collect(),
        one => sl.reads.get(one).into_iter().collect(),
    };
    let failed = reads.iter().find_map(|r| match &r.state {
        Some(Read::Failed(why)) => Some(why.clone()),
        _ => None,
    });
    let state = match failed {
        Some(why) => Some(Read::Failed(why)),
        None if reads.is_empty() || reads.iter().any(|r| r.state != Some(Read::Done(()))) => {
            Some(Read::Reading)
        }
        None => Some(Read::Done(())),
    };
    let (count, _) = sl.counts(&id, info);
    let one = reads.first().filter(|_| reads.len() == 1).copied();
    Some(SliceShown {
        name: slices::name(&id, Some(info), past),
        sprint: (id.sprint())
            .and_then(|n| info.sprints.iter().chain(past).find(|s| s.id == n).cloned()),
        count,
        state,
        read: reads.iter().map(|r| r.read()).sum(),
        found: one.and_then(|r| r.found).or(count),
        more: one.is_some_and(|r| r.more()),
        at: reads.iter().filter_map(|r| r.at).max(),
        id,
    })
}

/// How long a group may be to fold moving: a longer one is drawn a line at a time, and opens and shuts at once.
const FOLDS_MOVING: usize = 40;

/// A line of the list: a sprint's head among the active ones, a group's head, a row of a group or of the ranked
/// Backlog, or a part's or a group's rows whole while they fold. `Option` is the sprint's part, if any.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Line {
    Sprint(usize),
    Head(Option<usize>, usize),
    Row(Option<usize>, usize, usize),
    Ranked(usize),
    PartFolding(usize),
    Folding(Option<usize>, usize),
}

/// The name of issue `key`'s line.
pub fn row_line(key: &str) -> String {
    format!("row-{key}")
}

/// The list's lines in order, each with the name it keeps from one read to the next.
pub fn list_lines(
    shown: &Shown,
    folded: &dyn Fn(&str) -> bool,
    moving: &dyn Fn(&str) -> bool,
) -> Vec<(String, Line)> {
    let mut out = Vec::new();
    if let Some(rows) = &shown.ranked {
        out.extend((rows.iter().enumerate()).map(|(i, r)| (row_line(&r.key), Line::Ranked(i))));
    } else if shown.parts.is_empty() {
        shelf_lines(&mut out, None, &shown.shelves, folded, moving);
    } else {
        for (p, part) in shown.parts.iter().enumerate() {
            let id = part_id(part.sprint.id);
            out.push((id.clone(), Line::Sprint(p)));
            let key = format!("shelf-{id}");
            if moving(&key) && part.rows.len() <= FOLDS_MOVING {
                out.push((format!("fold-{id}"), Line::PartFolding(p)));
            } else if !folded(&id) {
                shelf_lines(&mut out, Some(p), &part.shelves, folded, moving);
            }
        }
    }
    out
}

fn shelf_lines(
    out: &mut Vec<(String, Line)>,
    part: Option<usize>,
    shelves: &[Shelf],
    folded: &dyn Fn(&str) -> bool,
    moving: &dyn Fn(&str) -> bool,
) {
    for (s, shelf) in shelves.iter().enumerate() {
        // A story's head is its row.
        let head = match &shelf.head {
            Head::Story { row, .. } => row_line(&row.key),
            _ => format!("head-{}", shelf.id),
        };
        out.push((head, Line::Head(part, s)));
        if moving(&format!("shelf-{}", shelf.id)) && shelf.rows.len() <= FOLDS_MOVING {
            out.push((format!("fold-{}", shelf.id), Line::Folding(part, s)));
        } else if !folded(&shelf.id) {
            out.extend(
                (shelf.rows.iter().enumerate())
                    .map(|(r, row)| (row_line(&row.key), Line::Row(part, s, r))),
            );
        }
    }
}

/// A line of the swimlanes, by its group: the lane whole, or a long lane's head and then its nth cards across.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LaneLine {
    Whole(usize),
    Head(usize),
    Cards(usize, usize),
}

/// How many cards the lane's longest column holds.
fn deepest(shelf: &Shelf, info: &BoardInfo) -> usize {
    let mut held = vec![0; info.columns.len()];
    for r in &shelf.rows {
        if let Some(n) = held.get_mut(info.column_of(&r.state)) {
            *n += 1;
        }
    }
    held.into_iter().max().unwrap_or(0)
}

/// The swimlanes' lines. A lane no deeper than a screen or two is one line; a deeper one is cut into a line for each
/// card down its columns, so only those on screen are drawn, and it opens and shuts at once.
pub fn lane_lines(shown: &Shown, folded: &dyn Fn(&str) -> bool) -> Vec<(String, LaneLine)> {
    let mut out = Vec::new();
    for (s, shelf) in shown.shelves.iter().enumerate() {
        let deep = deepest(shelf, &shown.info);
        let long = deep > FOLDS_MOVING / 2;
        let lane = format!("lane-{}", shelf.id);
        if !long {
            out.push((lane, LaneLine::Whole(s)));
            continue;
        }
        out.push((lane, LaneLine::Head(s)));
        if !folded(&shelf.id) {
            out.extend(
                (0..deep).map(|n| (format!("lane-{}-{n}", shelf.id), LaneLine::Cards(s, n))),
            );
        }
    }
    out
}

impl Shown {
    /// What its lines are of: the board, the slice shown and how it is grouped.
    fn listed(&self) -> String {
        let slice = self.slice.as_ref().map(|s| &s.id);
        format!(
            "{}/{}/{slice:?}/{:?}",
            self.host, self.board_name, self.group
        )
    }

    /// The groups of sprint part `part`, or the list's own.
    fn shelves_of(&self, part: Option<usize>) -> Option<&[Shelf]> {
        match part {
            Some(p) => self.parts.get(p).map(|p| p.shelves.as_slice()),
            None => Some(&self.shelves),
        }
    }

    /// The rows in the order the list draws them, folded groups left out: what ↑ ↓ walk.
    pub fn order(&self, folded: &dyn Fn(&str) -> bool) -> Vec<String> {
        if let Some(rows) = &self.ranked {
            return rows.iter().map(|r| r.key.clone()).collect();
        }
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
        let info = s.board_info().cloned().unwrap_or_default();
        let slices = s.board_slices();
        let slice = slices.and_then(|sl| slice_shown(sl, &info));
        // A slice not read yet, or not read, shows none of its issues; a board of one list waits for its rows.
        let none = Vec::new();
        let rows = match &slice {
            Some(_) => s.board_rows().unwrap_or(&none),
            None => s.board_rows()?,
        };
        let view = s.kept.view(&board.key());
        let me = s.me.get(&board.site).cloned();
        let play = in_play(rows);
        let asked = slice.as_ref().is_some_and(|sl| sl.id == SliceId::Backlog);
        let filter = Filter {
            mine: s.mine,
            faces: s.faces.clone(),
            words: self.frame.filter(cx),
        };
        // The Backlog's words are asked of the tracker, which finds by more than the row shows.
        let local = Filter {
            words: if asked {
                String::new()
            } else {
                filter.words.clone()
            },
            ..filter.clone()
        };
        let kept = shown(&play, &local, me.as_ref());
        let all = play.iter().filter(|r| r.parent.is_none()).count();
        let shown_n = kept.iter().filter(|r| r.parent.is_none()).count();
        let empty = match &slice {
            Some(sl) => {
                let sprint = sl.sprint.as_ref().map(|s| s.name.as_str());
                empty_of(
                    sl.state.as_ref(),
                    sprint,
                    all,
                    shown_n,
                    &filter,
                    asked,
                    sl.count,
                )
            }
            None => empty_of(
                Some(&Read::Done(())),
                None,
                all,
                shown_n,
                &filter,
                false,
                Some(all),
            ),
        };
        let ranked = asked.then(|| {
            (kept.iter())
                .filter(|r| r.parent.is_none())
                .map(|r| (*r).clone())
                .collect()
        });
        let parts = match (slices, &slice) {
            (Some(sl), Some(shown)) if shown.id == SliceId::All => {
                parts(&info, sl, view.group, &local, me.as_ref())
            }
            _ => Vec::new(),
        };
        Some(Shown {
            host: board.site.clone(),
            slice,
            parts,
            ranked,
            empty,
            board_name: board.name.clone(),
            shelves: shelves(&play, &info, view.group, &local, me.as_ref()),
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
        // An open group's rows are drawn each by itself: the fold learns it was open, to move as it shuts.
        let open = !self.folded(id);
        for key in [format!("shelf-{id}"), format!("lane-{id}")] {
            self.folds.shutting(&key, open);
        }
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
    fn facts_line(&mut self, shown: &Shown, cx: &mut Context<Self>) -> Div {
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
        let slice_name = shown.slice.as_ref().map(|sl| sl.name.as_str());
        let mut line = match (
            lead_words(&shown.board_name, &shown.info, slice_name),
            &shown.slice,
        ) {
            (Lead::Picker(_), Some(slice)) => line.child(self.sprint_picker(slice, cx)),
            (Lead::Picker(name) | Lead::Board(name), _) => line.child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(name),
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
        let line = line.children(self.read_words(shown));
        // The Backlog is in the tracker's rank, ungrouped: its epic filter stands where Group does.
        if let Some(epic) = self.backlog_epic(shown, cx) {
            return line.child(div().flex_1()).child(epic);
        }
        line.child(div().flex_1()).child(
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

    /// How far the slice shown is read: `Reading 31 of 78 issues…`, then `Read 78 of 78` a moment.
    fn read_words(&self, shown: &Shown) -> Option<Div> {
        let slice = shown.slice.as_ref()?;
        let unit = item_word(&shown.host);
        let of = slice.count.unwrap_or(slice.read).max(slice.read);
        match &slice.state {
            // Nothing shown yet, the slot says `Reading…` over the tints; a big slice read page by page says how far.
            Some(Read::Reading) if slice.read > 0 && slice.count.is_some() => {
                Some(div().text_color(theme::accent()).child(format!(
                    "Reading {} of {} {unit}s…",
                    slices::grouped(slice.read),
                    slices::grouped(of)
                )))
            }
            Some(Read::Done(()))
                if slice
                    .at
                    .is_some_and(|at| crate::store::now() - at < READ_TOLD) =>
            {
                Some(div().text_color(theme::faint()).child(format!(
                    "Read {} of {}",
                    slices::grouped(slice.read),
                    slices::grouped(of)
                )))
            }
            _ => None,
        }
    }

    /// The Backlog's epic filter, asked of the tracker: `Epic: All ▾`.
    fn backlog_epic(&mut self, shown: &Shown, cx: &mut Context<Self>) -> Option<AnyElement> {
        if shown.slice.as_ref()?.id != SliceId::Backlog {
            return None;
        }
        let s = self.store.read(cx);
        let picked = s.board_slices().and_then(|sl| sl.filter.epic.clone());
        let name = match &picked {
            Some(key) => (shown.info.epics.iter())
                .find(|e| &e.key == key)
                .map_or(key.clone(), |e| e.name.clone()),
            None => "All".into(),
        };
        Some(
            look::small_button("tasks-backlog-epic", "")
                .child("Epic:")
                .child(
                    div()
                        .max_w(px(200.))
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(name),
                )
                .child(
                    icons::icon("chevron")
                        .size(px(12.))
                        .text_color(theme::muted()),
                )
                .tip("The Backlog’s issues of one epic, asked of Jira")
                .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                    this.backlog_epic_menu(e.position(), cx)
                }))
                .into_any_element(),
        )
    }

    /// Epic ▾ of the Backlog: All, or one of the board's epics.
    fn backlog_epic_menu(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let s = self.store.read(cx);
        let Some(board) = s.board.clone() else {
            return;
        };
        let now = s.board_slices().and_then(|sl| sl.filter.epic.clone());
        let epics = s.board_info().map(|i| i.epics.clone()).unwrap_or_default();
        let page = cx.entity().downgrade();
        let pick = move |epic: Option<String>| {
            let (page, board) = (page.clone(), board.clone());
            move |_: &mut gpui::Window, cx: &mut App| {
                page.update(cx, |p, cx| {
                    p.store.update(cx, |s, cx| {
                        let mut filter = (s.slices.get(&board))
                            .map(|sl| sl.filter.clone())
                            .unwrap_or_default();
                        filter.epic = epic.clone();
                        s.filter_backlog(&board, filter, cx);
                        cx.notify();
                    })
                })
                .ok();
            }
        };
        let mut items = vec![MenuItem::new("All", pick(None)).checked(now.is_none())];
        for e in epics.iter().filter(|e| !e.done) {
            let on = now.as_deref() == Some(e.key.as_str());
            items.push(MenuItem::new(e.name.clone(), pick(Some(e.key.clone()))).checked(on));
        }
        self.frame.menu(at, vec![items], cx);
    }

    /// Where the slice shown is, as an epic's tooltip says it: `in this sprint`, `in the Backlog`.
    fn slice_here(&self, cx: &App) -> &'static str {
        let s = self.store.read(cx);
        match s.board_slices().and_then(|sl| sl.view.as_ref()) {
            Some(SliceId::Sprint(_)) => "in this sprint",
            Some(SliceId::Backlog) => "in the Backlog",
            Some(SliceId::All) => "in the active sprints",
            _ => "on this board",
        }
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
            // The sprint's goal on hover (the owner, 2026-11-26), not a line of its own.
            .map(|d| {
                let goal = (slice.sprint.as_ref())
                    .map(|sp| sp.goal.clone())
                    .filter(|g| !g.is_empty());
                match goal {
                    Some(goal) => d.tip_two(
                        format!("{}  ·  Pick a sprint  S", slice.name),
                        format!("Goal: {goal}"),
                    ),
                    None => d.tip(format!("{}\nPick a sprint  S", slice.name)),
                }
            })
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
        // Its dates a clock and a few letters, the days and the goal in the tooltip (the owner, 2026-11-26: the goal
        // on hover only, the dates short).
        let when = sprint.end.as_deref().map(|end| {
            let now = crate::store::now();
            let left = slices::days_left_short(end, now);
            let mut tip = span_words(sprint.start.as_deref(), end);
            if let Some((words, _)) = slices::days_left(end, now) {
                tip = format!("{tip} · {words}");
            }
            let goal = sprint.goal.clone();
            div()
                .id(SharedString::from(format!("sprint-when-{id}")))
                .flex()
                .items_center()
                .gap(px(4.))
                .child(
                    icons::icon("clock")
                        .size(px(12.))
                        .text_color(theme::muted()),
                )
                .children(left.map(|(words, few)| {
                    div()
                        .when(few, |d| d.text_color(theme::warning()))
                        .child(words)
                }))
                .map(|d| {
                    if goal.is_empty() {
                        d.tip(tip)
                    } else {
                        d.tip_two(tip, format!("Goal: {goal}"))
                    }
                })
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
                .into_any_element();
        };
        let fold_id = part_id(id);
        // As light as the design's head, not the list's darkest (the owner, 2026-11-26).
        head.bg(theme::panel())
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
                    .children(when)
                    .child(how_far),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.fold(&fold_id, cx)))
            .into_any_element()
    }

    /// The board as a list beside the preview, or its columns of cards.
    pub fn board(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let s = self.store.read(cx);
        let board = s.picked().cloned();
        let rows_read = match s.board.as_ref().and_then(|b| s.rows.get(b)) {
            Some(Read::Done(_)) => Some(Read::Done(())),
            Some(Read::Failed(why)) => Some(Read::Failed(why.clone())),
            Some(Read::Reading) => Some(Read::Reading),
            None => None,
        };
        // A board read by slice draws its facts line whatever its slice's read: the other slices stay a click away.
        let by_slice = s.board_slices().is_some_and(|sl| sl.view.is_some());
        let failure = s.failure_shown();
        let now = Instant::now();
        let nothing = !matches!(rows_read, Some(Read::Done(_)));
        self.fresh.track(s.reading_shown(), nothing, now);
        if board.is_none() {
            return self.loading_page(None, false, cx);
        }
        match rows_read {
            _ if by_slice => {}
            None | Some(Read::Reading) => return self.loading_page(None, false, cx),
            Some(Read::Failed(why)) => return self.cant_read(None, why, cx),
            Some(Read::Done(_)) => {}
        }
        let Some(shown) = self.board_shown(cx).map(Rc::new) else {
            return match failure {
                Some((why, false)) => self.cant_read(None, why, cx),
                _ => self.loading_page(None, false, cx),
            };
        };
        self.ask_slice(&shown, cx);
        // The pick stays among the rows shown, the first when it is not.
        let order = shown.order(&|id| self.folded(id));
        if self
            .pick
            .as_ref()
            .is_none_or(|k| !order.contains(k) && !shown.all.iter().any(|r| &r.key == k))
        {
            self.pick = order.first().cloned();
        }
        let banner = self.slice_banner(&shown, cx);
        let unreached = self.unreached_band(&shown, cx);
        if let Some(empty) = shown.empty.clone() {
            let facts = self.facts_line(&shown, cx);
            match empty {
                Empty::Reading => return self.loading_page(Some(facts), true, cx),
                Empty::Failed(why) => return self.cant_read(Some(facts), why, cx),
                _ => {}
            }
            let body = self.empty_body(&empty, &shown, cx);
            return (self.page_frame())
                .relative()
                .child(facts)
                .children(banner)
                .child(body)
                .child(self.fresh.hairline(cx))
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
        let foot = self.slice_foot(&shown, cx);
        let selection = self.selection_bar(&shown, cx);
        // The board fades in over its tints when they were shown.
        let phase = self.fresh.phase(now);
        let arriving = phase.content < 1.;
        if shown.columns {
            return (self.page_frame())
                .relative()
                .children(unreached)
                .child(self.facts_line(&shown, cx))
                .children(head)
                .children(banner)
                .children(note)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .when(arriving, |d| {
                            d.opacity(phase.content).child(fresh::frames())
                        })
                        .child(self.columns(shown.clone(), cx)),
                )
                .children(selection)
                .children(foot)
                .child(self.fresh.hairline(cx))
                .into_any_element();
        }
        let list = (self.list_frame())
            .relative()
            .children(unreached)
            .child(self.facts_line(&shown, cx))
            .children(head)
            .children(banner)
            .children(note)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when(arriving, |d| {
                        d.opacity(phase.content).child(fresh::frames())
                    })
                    .child(self.list_body(shown.clone(), cx)),
            )
            .children(selection)
            .children(foot)
            .child(self.fresh.hairline(cx))
            .into_any_element();
        let preview = self.preview(&shown, cx);
        self.frame.list_page(list, preview, cx)
    }

    /// Its site not reached, a copy kept on screen: a band over the list, `Can't reach acme.atlassian.net · boards
    /// from 14:32` and Retry, as the design has it, not a word in the bar's middle (the owner, 2026-11-26).
    fn unreached_band(&mut self, shown: &Shown, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Some(artoo_plugin_sdk::kit::fresh::Slot::CantReach(at)) = self.store.read(cx).slot()
        else {
            return None;
        };
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.))
                .h(px(32.))
                .px(px(12.))
                // The island's top corners are its own: a clip is square (the owner, 2026-11-26).
                .when(!artoo_plugin_sdk::kit::flat(), |s| {
                    s.rounded_t(px(artoo_plugin_sdk::kit::ISLAND_RADIUS - 1.))
                })
                .border_b_1()
                .border_color(theme::border())
                .bg(theme::warning_bg())
                .text_size(px(12.))
                .text_color(theme::warning())
                .child(
                    div()
                        .flex_none()
                        .size(px(6.))
                        .rounded_full()
                        .bg(theme::warning()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(format!("Can’t reach {} · boards from {at}", shown.host)),
                )
                .child(
                    look::small_button("tasks-unreached-retry", "Retry")
                        .tip("Read again  ⌘R")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.read_again(cx))),
                )
                .into_any_element(),
        )
    }

    /// What the slice shown asks of the tracker as the page stands: the Backlog's filter (its words, Mine, its
    /// epic), each epic's whole count while grouped by epic, and a moment's `Read 78 of 78` once a read is whole.
    fn ask_slice(&mut self, shown: &Shown, cx: &mut Context<Self>) {
        let Some(slice) = &shown.slice else {
            return;
        };
        let s = self.store.read(cx);
        let Some(board) = s.board.clone() else {
            return;
        };
        if slice.id == SliceId::Backlog {
            let epic = s.board_slices().and_then(|sl| sl.filter.epic.clone());
            let filter = slices::Filter {
                words: self.frame.filter(cx),
                mine: s.mine,
                epic,
            };
            self.store
                .update(cx, |s, cx| s.filter_backlog(&board, filter, cx));
        }
        if shown.group == Group::Epic {
            let epics: Vec<String> = (shown.shelves.iter())
                .filter_map(|g| match &g.head {
                    Head::Epic { epic: Some(e), .. } => Some(e.key.clone()),
                    _ => None,
                })
                .collect();
            self.store.update(cx, |s, cx| {
                for epic in &epics {
                    s.read_epic_count(&board, epic, cx);
                }
            });
        }
        if let (Some(Read::Done(())), Some(at)) = (&slice.state, slice.at)
            && self.read_told != Some(at)
            && crate::store::now() - at < READ_TOLD
        {
            self.read_told = Some(at);
            cx.spawn(async move |page, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(READ_TOLD as u64))
                    .await;
                page.update(cx, |_, cx| cx.notify()).ok();
            })
            .detach();
        }
    }

    /// Over the slice's issues: no sprint running (the Backlog shown, the next sprint offered), or the slice not read
    /// again (why, Retry; what was read stays, faint).
    fn slice_banner(&mut self, shown: &Shown, cx: &mut Context<Self>) -> Option<AnyElement> {
        let slice = shown.slice.as_ref()?;
        let bar = |id: &'static str, warn: bool| {
            div()
                .id(id)
                .flex_none()
                .mx(px(12.))
                .my(px(8.))
                .px(px(12.))
                .h(px(32.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(6.))
                .border_1()
                .border_color(if warn {
                    theme::warning()
                } else {
                    theme::border()
                })
                .text_size(px(12.))
                .text_color(theme::muted())
                .whitespace_nowrap()
        };
        let strong = |words: String| {
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(words)
        };
        let running = slices::active(&shown.info).next().is_some();
        if slice.id != SliceId::Backlog || running || shown.info.sprintless {
            return None;
        }
        let next = slices::next_sprint(&shown.info).map(|s| s.id);
        Some(
            bar("tasks-no-sprint", false)
                .child(
                    icons::icon("clock")
                        .size(px(14.))
                        .text_color(theme::muted()),
                )
                .child(strong("No sprint is running".into()))
                .child("Showing the Backlog")
                .children(next.map(|id| {
                    div()
                        .id("tasks-next-sprint")
                        .text_color(theme::accent())
                        .cursor_pointer()
                        .child("Open the next sprint")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.show_slice(SliceId::Sprint(id), cx)
                        }))
                }))
                .into_any_element(),
        )
    }

    /// Retry: the slice shown read again.
    fn retry_link(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("tasks-slice-retry")
            .text_color(theme::accent())
            .cursor_pointer()
            .child("Retry")
            .tip("Read again  ⌘R")
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.store
                    .update(cx, |s, cx| s.read_board(crate::store::Ask::Reload, cx))
            }))
            .into_any_element()
    }

    /// Slice `slice` of the board picked shown.
    pub fn show_slice(&mut self, slice: SliceId, cx: &mut Context<Self>) {
        self.store.update(cx, |s, cx| {
            if let Some(board) = s.board.clone() {
                s.show_slice(&board, slice, cx);
            }
        });
    }

    /// Under the list: the Backlog's `Showing 50 of 2,123 issues · Load 50 more`, a big board's `Showing 2,000 of
    /// 3,412 issues`.
    fn slice_foot(&mut self, shown: &Shown, cx: &mut Context<Self>) -> Option<AnyElement> {
        let slice = shown.slice.as_ref()?;
        let backlog = slice.id == SliceId::Backlog;
        if !(backlog || slice.id == SliceId::Board && slice.more) {
            return None;
        }
        let of = slice.found.unwrap_or(slice.read).max(slice.read);
        let unit = item_word(&shown.host);
        let units = if of == 1 {
            unit.to_owned()
        } else {
            format!("{unit}s")
        };
        let more = (backlog && slice.more).then(|| {
            look::small_button("tasks-load-more", "")
                .child(format!("Load {} more", slices::PAGE.min(of - slice.read)))
                .tip("The next page, in the Backlog’s rank")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.store.update(cx, |s, cx| {
                        if let Some(board) = s.board.clone() {
                            s.load_more(&board, cx);
                        }
                    })
                }))
        });
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .h(px(40.))
                .px(px(16.))
                .border_t_1()
                .border_color(theme::border())
                .text_size(px(12.))
                .text_color(theme::muted())
                .child("Showing")
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(slices::grouped(slice.read)),
                )
                .child("of")
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(slices::grouped(of)),
                )
                .child(units)
                .child(div().flex_1())
                .children(more)
                .into_any_element(),
        )
    }

    /// What the slice shows in place of its issues: one line, one action.
    fn empty_body(&mut self, empty: &Empty, shown: &Shown, cx: &mut Context<Self>) -> AnyElement {
        if *empty == Empty::Reading {
            return skeleton_in_page("tasks-rows", 8).into_any_element();
        }
        let unit = item_word(&shown.host);
        let here = match shown.slice.as_ref().map(|sl| &sl.id) {
            Some(SliceId::Sprint(_)) => "in this sprint",
            Some(SliceId::Backlog) => "in the Backlog",
            Some(SliceId::All) => "in the active sprints",
            _ => "on this board",
        };
        let action = |id: &'static str, words: String| {
            div()
                .id(id)
                .text_size(px(13.))
                .text_color(theme::accent())
                .cursor_pointer()
                .child(words)
        };
        let (line, act): (String, Option<AnyElement>) = match empty {
            Empty::Reading => unreachable!(),
            Empty::Failed(why) => {
                let what = match shown.slice.as_ref().map(|sl| &sl.id) {
                    Some(SliceId::Sprint(_)) => "this sprint",
                    Some(SliceId::Backlog) => "the Backlog",
                    _ => "this board",
                };
                (
                    format!("Couldn’t read {what} · {why}"),
                    Some(self.retry_link(cx)),
                )
            }
            Empty::Unplanned(name) => (
                format!("Nothing planned in {name} yet"),
                Some(
                    action("tasks-plan", "Plan from Backlog".into())
                        .tip("The Backlog, to move issues in from")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.show_slice(SliceId::Backlog, cx)
                        }))
                        .into_any_element(),
                ),
            ),
            Empty::Nothing => (format!("No {unit}s {here}"), None),
            Empty::Hidden { words, people } => {
                let names = self.people_names(people, shown);
                let mut said = names.clone();
                if !words.is_empty() {
                    said.push(format!("“{words}”"));
                }
                // The filter's words are the tab's to clear; the people picked are the page's.
                let act = (!people.is_empty()).then(|| {
                    action("tasks-show-everyone", "Show everyone".into())
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.store.update(cx, |s, cx| {
                                s.faces.clear();
                                cx.notify();
                            })
                        }))
                        .into_any_element()
                });
                (format!("No {unit} matches {}", said.join(" and ")), act)
            }
            Empty::NotYours(count) => (
                format!("Nothing assigned to you {here}"),
                Some(
                    action(
                        "tasks-all",
                        match count {
                            Some(n) => format!(
                                "Show all {} {unit}{}",
                                slices::grouped(*n),
                                if *n == 1 { "" } else { "s" }
                            ),
                            None => "Show all".into(),
                        },
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_mine(false, cx)))
                    .into_any_element(),
                ),
            ),
        };
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .px(px(24.))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(line),
            )
            .children(act)
            .into_any_element()
    }

    /// The names of people `ids` on the board's issues (`NONE` Unassigned).
    fn people_names(&self, ids: &[String], shown: &Shown) -> Vec<String> {
        (ids.iter())
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
            .collect()
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

    /// The list's groups, each with its head and rows: of them, what is on screen.
    fn list_body(&mut self, shown: Rc<Shown>, cx: &mut Context<Self>) -> AnyElement {
        let lines = list_lines(&shown, &|id| self.folded(id), &|key| {
            self.motion.recent(key)
        });
        let (ids, lines): (Vec<_>, Vec<_>) = lines.into_iter().unzip();
        let state = self.list_lines.show(&shown.listed(), ids);
        let draw = cx.processor(
            move |this: &mut Self, ix: usize, _, cx| match lines.get(ix) {
                Some(line) => div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .children(this.list_line(&shown, *line, cx))
                    .into_any_element(),
                None => div().into_any_element(),
            },
        );
        gpui::list(state, draw)
            .flex_1()
            .min_h_0()
            .into_any_element()
    }

    fn list_line(
        &mut self,
        shown: &Shown,
        line: Line,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let unit = item_word(&shown.host);
        Some(match line {
            Line::Sprint(p) => {
                let part = shown.parts.get(p)?;
                let folded = self.folded(&part_id(part.sprint.id));
                self.sprint_head(&part.sprint, &part.rows, part.count, unit, Some(folded), cx)
            }
            Line::Head(p, s) => {
                let shelf = shown.shelves_of(p)?.get(s)?;
                let folded = self.folded(&shelf.id);
                self.shelf_head(shelf, folded, cx)
            }
            Line::Row(p, s, r) => {
                let row = shown.shelves_of(p)?.get(s)?.rows.get(r)?;
                self.row(row, shown, cx)
            }
            Line::Ranked(r) => self.row(shown.ranked.as_ref()?.get(r)?, shown, cx),
            Line::PartFolding(p) => {
                let id = part_id(shown.parts.get(p)?.sprint.id);
                let folded = self.folded(&id);
                let groups = self.shelf_list(&shown.parts[p].shelves, shown, cx);
                return self.folding(&format!("shelf-{id}"), !folded, groups);
            }
            Line::Folding(p, s) => {
                let shelf = shown.shelves_of(p)?.get(s)?;
                let folded = self.folded(&shelf.id);
                let rows = self.shelf_rows(shelf, shown, cx);
                return self.folding(&format!("shelf-{}", shelf.id), !folded, rows);
            }
        })
    }

    fn shelf_rows(&mut self, shelf: &Shelf, shown: &Shown, cx: &mut Context<Self>) -> Div {
        let mut rows = div().flex().flex_col();
        for r in &shelf.rows {
            rows = rows.child(self.row(r, shown, cx));
        }
        rows
    }

    /// Groups `shelves`, each with its head and rows, drawn whole: a part folding.
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
            let rows = self.shelf_rows(shelf, shown, cx);
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
                    .child(self.issue_link("head", &row.key, look::key(&row.key), cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child(row.summary.clone()),
                    )
                    .child(look::count_chip(format!("{done} of {all}")))
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
                let s = self.store.read(cx);
                let e = s.epic(e);
                let whole = s
                    .board
                    .as_ref()
                    .and_then(|b| s.epic_counts.get(&(b.0.clone(), e.key.clone())))
                    .and_then(Read::done)
                    .copied();
                let here = self.slice_here(cx);
                let where_else = match whole {
                    Some(n) => format!(
                        "{} has {} in all: {all} {here}, {} elsewhere",
                        e.name,
                        artoo_plugin_sdk::kit::format::plural(n, "issue"),
                        n.saturating_sub(*all)
                    ),
                    None => format!("{}: {all} {here}; its whole count is being read", e.name),
                };
                head.child(look::kind_mark("Epic", false))
                    .child(self.issue_link("head", &e.key, look::key(&e.key), cx))
                    .child(look::epic_lozenge(&e))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id(SharedString::from(format!("epic-else-{}", e.key)))
                            .child(
                                icons::icon("layers")
                                    .size(px(12.))
                                    .text_color(theme::muted()),
                            )
                            .tip(where_else),
                    )
                    .child(look::count_chip(format!("{done} of {all} done")))
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

    /// A row: who has it first (a dashed person for no one), then its title with its state and ⋯ always there, and
    /// under them its kind and key, priority, sub-tasks, comments, what blocks it, its epic. Where moving is, a
    /// checkbox before it selects it; a move the tracker refused tints it, Jira's words on its glyph.
    fn row(&mut self, r: &Row, shown: &Shown, cx: &mut Context<Self>) -> AnyElement {
        let picked = self.pick.as_deref() == Some(&r.key);
        let me = shown
            .me
            .as_ref()
            .is_some_and(|me| r.assignee.as_ref().is_some_and(|p| p.id == me.id));
        let s = self.store.read(cx);
        let saving = s.saving.iter().any(|(k, _)| *k == r.key);
        let refused = (s.refused.get(&(r.key.clone(), "sprint".into())))
            .or_else(|| s.refused.get(&(r.key.clone(), "rank".into())))
            .cloned();
        let epic = r.epic.as_ref().map(|e| s.epic(e));
        let key = r.key.clone();
        let anchor = Anchor::Row(r.key.clone());
        let who_open = self.pop_at(&anchor, PopKind::Assign);
        let movable = shown.slice.is_some() && !shown.info.sprintless;
        let checked = self.selected.contains(&r.key);
        let selecting = !self.selected.is_empty();
        let mut facts = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .min_w_0()
            .text_size(px(12.))
            .text_color(theme::muted())
            .whitespace_nowrap()
            .child(look::kind_mark(&r.kind, r.subtask))
            .child(self.issue_link("row", &r.key, look::key(&r.key), cx));
        if let Some(p) = &r.priority {
            facts = facts.child(look::priority_fact(&r.key, p));
        }
        // The key, its priority, its epic; what else it has at the row's far end (the owner, 2026-11-26).
        if shown.group != Group::Epic
            && let Some(e) = &epic
        {
            facts = facts.child(look::epic_lozenge(e));
        }
        facts = facts.child(div().flex_1());
        if r.subtasks.1 > 0 {
            facts = facts.child(look::subtasks_fact(&r.key, r.subtasks.0, r.subtasks.1));
        }
        if r.comments > 0 {
            facts = facts.child(look::comments_fact(&r.key, r.comments));
        }
        if let Some(b) = r.blocked_by.first() {
            facts = facts.child(look::blocked_fact(&r.key, b));
        }
        let who = self.who_control(r, me, anchor.clone(), who_open, cx);
        let chip = self.state_control(r, anchor.clone(), true, cx);
        let (more_key, menu_key) = (key.clone(), key.clone());
        let more = div()
            .id(SharedString::from(format!("row-more-{}", r.key)))
            .relative()
            .flex_none()
            .size(px(20.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .text_color(theme::muted())
            .hover(|s| s.bg(theme::hover()))
            .child("⋯")
            .tip(if movable {
                "More  ·  Move to  M"
            } else {
                "More"
            })
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.row_menu(&more_key, e.position(), cx)
            }))
            .children(self.pop_here(&anchor, PopKind::MoveTo, cx));
        let check_key = key.clone();
        // Only while some are picked to move together: it keeps no room before the face otherwise (the owner,
        // 2026-11-26); the row's menu has Select.
        let check = (movable && selecting).then(|| {
            div()
                .id(SharedString::from(format!("row-check-{}", r.key)))
                .flex_none()
                .mt(px(3.))
                .size(px(14.))
                .rounded(px(3.))
                .border_1()
                .border_color(if checked {
                    theme::accent()
                } else {
                    theme::faint()
                })
                .flex()
                .items_center()
                .justify_center()
                .when(checked, |d| {
                    d.bg(theme::accent()).child(
                        icons::icon("check")
                            .size(px(10.))
                            .text_color(theme::on_accent()),
                    )
                })
                .cursor_pointer()
                .tip("Select, to move several together")
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.toggle_selected(&check_key, cx)
                }))
        });
        let glyph = refused.map(|why| {
            div()
                .id(SharedString::from(format!("row-refused-{}", r.key)))
                .flex_none()
                .child(
                    icons::icon("alert")
                        .size(px(13.))
                        .text_color(theme::warning()),
                )
                .tip(why)
        });
        let tinted = glyph.is_some();
        let indent = shown.group == Group::Story && r.parent.is_some();
        div()
            .id(SharedString::from(format!("row-{}", r.key)))
            .group("tasks-row")
            .flex_none()
            .flex()
            // Its person at the top, on the summary's line, not in the row's middle (the owner, 2026-11-26).
            .items_start()
            .pt(px(9.))
            .gap(px(6.))
            .h(px(62.))
            .pl(px(if indent { 28. } else { 8. }))
            .pr(px(12.))
            .border_b_1()
            .border_color(theme::border())
            .when(tinted, |d| d.bg(theme::warning_bg()))
            .when(picked && !tinted, |d| d.bg(theme::selected()))
            .when(checked && !picked && !tinted, |d| d.bg(theme::hover()))
            .when(!picked && !tinted, |d| d.hover(|s| s.bg(theme::hover())))
            .when(saving, |d| d.opacity(0.7))
            .children(check)
            .child(div().flex_none().mt(px(1.)).child(who))
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
                            .children(glyph)
                            .child(chip)
                            .child(more),
                    )
                    .child(facts.h(px(18.))),
            )
            .tip(format!("{} {}", r.key, r.summary))
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                this.pick_row(&key, cx);
                if e.click_count() > 1 {
                    this.open_issue(&key, cx);
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    this.row_menu(&menu_key, e.position, cx)
                }),
            )
            .into_any_element()
    }

    /// Issue `key` checked or let go on the board.
    pub(crate) fn toggle_selected(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(at) = self.selected.iter().position(|k| k == key) {
            self.selected.remove(at);
        } else {
            self.selected.push(key.to_owned());
        }
        cx.notify();
    }

    /// Under the list while issues are checked: `2 selected`, their keys, Move to ▾, and × to let them go.
    fn selection_bar(&mut self, shown: &Shown, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.selected.is_empty() || shown.slice.is_none() {
            return None;
        }
        let (count, keys) = slices::selection_words(&self.selected);
        let open = self.pop_at(&Anchor::Selection, PopKind::MoveTo);
        let movable = self.can_move(&self.selected, cx);
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.))
                .h(px(40.))
                .mx(px(8.))
                .mb(px(8.))
                .px(px(12.))
                .rounded(px(8.))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .shadow_lg()
                .text_size(px(12.))
                .text_color(theme::muted())
                .whitespace_nowrap()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(count),
                )
                .child(div().min_w_0().truncate().child(keys))
                .child(div().flex_1())
                .child(
                    div()
                        .relative()
                        .child(
                            look::small_button("tasks-selection-move", "")
                                .when(open, |d| d.border_1().border_color(theme::accent()))
                                .when(!movable, |d| d.opacity(0.5))
                                .child("Move to")
                                .child(
                                    icons::icon("chevron")
                                        .size(px(12.))
                                        .text_color(theme::muted()),
                                )
                                .tip(if movable {
                                    "Move them to a sprint or the Backlog"
                                } else {
                                    "A past sprint’s issues are read only"
                                })
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.open_selection_move(cx)
                                })),
                        )
                        .children(self.pop_here(&Anchor::Selection, PopKind::MoveTo, cx)),
                )
                .child(
                    div()
                        .id("tasks-selection-clear")
                        .size(px(20.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.))
                        .hover(|s| s.bg(theme::hover()))
                        .child(icons::icon("x").size(px(12.)).text_color(theme::muted()))
                        .tip("Let them go")
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.selected.clear();
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
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
    fn columns(&mut self, shown: Rc<Shown>, cx: &mut Context<Self>) -> AnyElement {
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
            let Head::State { name, .. } = &shelf.head else {
                continue;
            };
            // The head scrolls with the cards, as the first line.
            let ids = std::iter::once(String::new())
                .chain(shelf.rows.iter().map(|r| row_line(&r.key)))
                .collect();
            // The Backlog column stands apart on the panel's colour, the others on the window's (the owner,
            // 2026-11-26: the backlog's colours as the design's).
            let backlog = name.eq_ignore_ascii_case("backlog");
            let lines = self.column_lines.entry(name.clone()).or_default();
            let state = lines.show(&shown.listed(), ids);
            let of = shown.clone();
            let draw = cx.processor(move |this: &mut Self, line: usize, _, cx| {
                let cell = div().w_full().flex().flex_col().pb(px(6.));
                let Some(shelf) = of.shelves.get(ix) else {
                    return cell.into_any_element();
                };
                let Head::State { name, state } = &shelf.head else {
                    return cell.into_any_element();
                };
                match line.checked_sub(1) {
                    None => cell.child(this.column_head(ix, name, state, shelf.rows.len(), cx)),
                    Some(r) => cell
                        .children((shelf.rows.get(r)).map(|r| this.card(r, state.done(), &of, cx))),
                }
                .into_any_element()
            });
            cols = cols.child(
                div()
                    .id(SharedString::from(format!("col-{ix}")))
                    .flex_1()
                    .min_w(px(180.))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .p(px(6.))
                    .pb(px(0.))
                    .rounded(px(8.))
                    .bg(if backlog {
                        theme::panel()
                    } else {
                        theme::base()
                    })
                    .when(backlog, |d| {
                        d.border_1().border_color(theme::island_border())
                    })
                    .drag_over::<Dragged>(|d, _, _, _| d.bg(theme::hover()))
                    .on_drop(
                        cx.listener(move |this, d: &Dragged, _, cx| this.drop_card(&d.key, ix, cx)),
                    )
                    .child(gpui::list(state, draw).flex_1().min_h_0()),
            );
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
            // Its count in plain words beside its name, as the design's head.
            .child(
                div()
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
    fn lanes(&mut self, shown: Rc<Shown>, cx: &mut Context<Self>) -> AnyElement {
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
        let lines = lane_lines(&shown, &|id| self.folded(id));
        let (ids, lines): (Vec<_>, Vec<_>) = lines.into_iter().unzip();
        let state = self.lane_lines.show(&shown.listed(), ids);
        let draw = cx.processor(
            move |this: &mut Self, ix: usize, _, cx| match lines.get(ix) {
                Some(line) => this.lane_line(&shown, &states, *line, cx),
                None => div().into_any_element(),
            },
        );
        let body = gpui::list(state, draw).flex_1().min_h_0().pb(px(2.));
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

    /// A line of the swimlanes: a lane whole, or a long lane's head and its cards a row across the columns at a
    /// time, each drawn as its part of the lane's box.
    fn lane_line(
        &mut self,
        shown: &Shown,
        states: &[State],
        line: LaneLine,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (LaneLine::Whole(s) | LaneLine::Head(s) | LaneLine::Cards(s, _)) = line;
        let Some(shelf) = shown.shelves.get(s) else {
            return div().into_any_element();
        };
        let column =
            |ix: usize| (shelf.rows.iter()).filter(move |r| shown.info.column_of(&r.state) == ix);
        let part = div()
            .flex_none()
            .flex()
            .flex_col()
            .px(px(6.))
            .bg(theme::base());
        let folded = self.folded(&shelf.id);
        let cell = || div().flex_1().min_w(px(180.)).flex().flex_col().gap(px(6.));
        let (lane, last) = match line {
            LaneLine::Whole(_) => {
                let mut lane = (part.pb(px(6.)).rounded(px(8.)))
                    .child(self.lane_head(shelf, folded, shown, cx));
                // Its cards fold in and out moving; folded and still, they are not drawn.
                let key = format!("lane-{}", shelf.id);
                if !folded || self.motion.recent(&key) {
                    let mut cells = div().flex().gap(px(8.));
                    for (ix, state) in states.iter().enumerate() {
                        let mut cards = cell();
                        for r in column(ix) {
                            cards = cards.child(self.card(r, state.done(), shown, cx));
                        }
                        cells = cells.child(cards);
                    }
                    lane = lane.children(self.folding(&key, !folded, cells));
                }
                (lane, true)
            }
            LaneLine::Head(_) => {
                let lane = (part.rounded_t(px(8.)))
                    .when(folded, |d| d.pb(px(6.)).rounded_b(px(8.)))
                    .child(self.lane_head(shelf, folded, shown, cx));
                (lane, folded)
            }
            LaneLine::Cards(_, at) => {
                let mut cells = div().flex().gap(px(8.));
                for (ix, state) in states.iter().enumerate() {
                    let card = column(ix).nth(at);
                    cells = cells.child(
                        cell().children(card.map(|r| self.card(r, state.done(), shown, cx))),
                    );
                }
                let last = at + 1 == deepest(shelf, &shown.info);
                let lane = part.pb(px(6.)).when(last, |d| d.rounded_b(px(8.)));
                (lane.child(cells), last)
            }
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .when(last, |d| d.pb(px(8.)))
            .child(lane)
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
                    .child(self.issue_link("lane", &row.key, look::key(&row.key), cx))
                    .child(title(row.summary.clone()))
                    .child(look::count_chip(format!("{done} of {all}")))
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
                    .child(self.issue_link("lane", &e.key, look::key(&e.key), cx))
                    .child(look::epic_lozenge(&e))
                    .child(look::count_chip(format!("{done} of {all}")))
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
            .child(self.issue_link("card", &r.key, look::key(&r.key), cx));
        if let Some(p) = &r.priority {
            meta = meta.child(look::priority_fact(&r.key, p));
        }
        // A card says its key, priority and what blocks it, as the design's cards do: no counts (the owner,
        // 2026-11-26: no comment count on the cards).
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
            // On the editor's colour, lighter than the column under it, as the design's cards.
            .bg(theme::editor())
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
        let frame = || {
            artoo_plugin_sdk::kit::island()
                .size_full()
                .min_w_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .bg(theme::editor())
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
                    .child(self.issue_link("pv-parent", &p.key, look::key(&p.key), cx))
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
                    .child(self.issue_link("pv-epic", &e.key, look::key(&e.key), cx))
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
        // A board read by slice: the issue's sprint is where it moves (⌥M), but in a past sprint.
        if shown.slice.is_some() && !shown.info.sprintless {
            let movable = self.can_move(std::slice::from_ref(&r.key), cx);
            let open = self.pop_at(&Anchor::Preview, PopKind::MoveTo);
            let name = (r.sprint.as_ref()).map_or("Backlog".to_owned(), |s| s.name.clone());
            let k = r.key.clone();
            rows = rows.child(kv(
                "Sprint",
                div()
                    .id("pv-sprint")
                    .relative()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .px(px(4.))
                    .ml(px(-4.))
                    .rounded(px(4.))
                    .child(div().min_w_0().truncate().child(name))
                    .when(open, |d| d.border_1().border_color(theme::accent()))
                    .when(movable, |d| {
                        d.cursor_pointer()
                            .hover(|s| s.bg(theme::hover()))
                            .child(
                                icons::icon("chevron")
                                    .size(px(12.))
                                    .text_color(theme::muted()),
                            )
                            .tip("Move to another sprint or the Backlog  ⌥M")
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.open_move_to(Anchor::Preview, &k, cx)
                            }))
                    })
                    .when(!movable, |d| d.tip("A past sprint’s issues are read only"))
                    .children(self.pop_here(&Anchor::Preview, PopKind::MoveTo, cx))
                    .into_any_element(),
            ));
        } else if let Some(sp) = &r.sprint {
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
                        self.issue_link(
                            "pv-blocked",
                            &b.key,
                            div()
                                .flex_none()
                                .font_family(theme::code_font())
                                .text_color(theme::removed())
                                .child(b.key.clone()),
                            cx,
                        ),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(b.summary.clone()))
                    .child(look::state_mark(&b.state, 14.))
                    .map(|d| {
                        let (title, line) = look::blocked_tip(b);
                        d.tip_two(title, line)
                    })
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
                            .child(self.issue_link(
                                "pv",
                                &r.key,
                                div().text_color(theme::accent()).child(r.key.clone()),
                                cx,
                            )),
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
                // A circle as the faces are, those picked of it a badge on its corner (the owner, 2026-11-26).
                div()
                    .id("faces-more")
                    .relative()
                    .flex_none()
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_size(px(10.))
                    .text_color(theme::muted())
                    .bg(theme::hover())
                    .cursor_pointer()
                    .when(open || hidden > 0, |d| {
                        d.border_1().border_color(theme::accent())
                    })
                    .child(format!("+{more}"))
                    .when(hidden > 0, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(-5.))
                                .right(px(-5.))
                                .min_w(px(13.))
                                .h(px(13.))
                                .px(px(3.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(theme::accent())
                                .text_size(px(9.))
                                .text_color(theme::on_accent())
                                .child(hidden.to_string()),
                        )
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

    /// The board being read with nothing to show yet (round 41): the facts line when its slice is known, still tints
    /// where the sprint's head (`sprint`), the issues and the preview will be, the hairline once the read is long.
    fn loading_page(
        &mut self,
        facts: Option<Div>,
        sprint: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = Instant::now();
        let phase = self.fresh.phase(now);
        let moving = self.fresh.skeleton_moves(now);
        let tints = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .when(sprint, |d| d.child(fresh::skeleton_sprint_head()))
            .child(fresh::skeleton_list());
        let list = (self.list_frame())
            .relative()
            .children(facts)
            .child(fresh::veil(tints, phase, moving))
            .child(self.fresh.hairline(cx))
            .into_any_element();
        let preview = (artoo_plugin_sdk::kit::island())
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .bg(theme::editor())
            .child(fresh::veil(fresh::skeleton_preview(), phase, false))
            .into_any_element();
        self.frame.list_page(list, preview, cx)
    }

    /// Nothing read and the read failed (round 41): the facts line kept when its slice is known, what is the case and
    /// why where the issues would be, Retry; the slot says `Can't read the list ↻`.
    fn cant_read(&mut self, facts: Option<Div>, why: String, cx: &mut Context<Self>) -> AnyElement {
        let retry = self.retry_link(cx);
        (self.page_frame())
            .relative()
            .children(facts)
            .child(fresh::message(
                "Can’t read the list".into(),
                why,
                Some(retry),
            ))
            .child(self.fresh.hairline(cx))
            .into_any_element()
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
    fn each_empty_slice_says_why_in_one_line() {
        let done = Read::Done(());
        let none = Filter::default();
        let mine = Filter {
            mine: true,
            ..Default::default()
        };
        let hidden = Filter {
            words: "payments".into(),
            faces: vec!["kestrel".into()],
            ..Default::default()
        };
        let sprint = Some("Sprint 35");
        // a. an empty sprint
        assert_eq!(
            empty_of(Some(&done), sprint, 0, 0, &none, false, Some(0)),
            Some(Empty::Unplanned("Sprint 35".into()))
        );
        // b. Mine empty in a busy sprint
        assert_eq!(
            empty_of(Some(&done), sprint, 78, 0, &mine, false, Some(78)),
            Some(Empty::NotYours(Some(78)))
        );
        // c. a filter and a person hiding everything
        assert_eq!(
            empty_of(Some(&done), sprint, 78, 0, &hidden, false, Some(78)),
            Some(Empty::Hidden {
                words: "payments".into(),
                people: vec!["kestrel".into()]
            })
        );
        // c. in the Backlog, whose rows are what Jira found for the words
        assert_eq!(
            empty_of(Some(&done), None, 0, 0, &hidden, true, Some(300)),
            Some(Empty::Hidden {
                words: "payments".into(),
                people: vec!["kestrel".into()]
            })
        );
        // e. being read, none yet; then some shown
        let reading = Read::Reading;
        assert_eq!(
            empty_of(Some(&reading), sprint, 0, 0, &none, false, Some(78)),
            Some(Empty::Reading)
        );
        assert_eq!(
            empty_of(Some(&reading), None, 0, 0, &hidden, true, None),
            Some(Empty::Reading)
        );
        assert_eq!(
            empty_of(Some(&reading), sprint, 31, 31, &none, false, Some(78)),
            None
        );
        // f. not read, none read before
        let failed = Read::Failed("Jira answered 504".into());
        assert_eq!(
            empty_of(Some(&failed), sprint, 0, 0, &none, false, None),
            Some(Empty::Failed("Jira answered 504".into()))
        );
        // An empty Backlog or board
        assert_eq!(
            empty_of(Some(&done), None, 0, 0, &none, true, Some(0)),
            Some(Empty::Nothing)
        );
    }

    #[test]
    fn a_board_without_sprints_leads_with_its_name_and_no_picker() {
        let mut info = info();
        assert_eq!(
            lead_words("Payments", &info, Some("Sprint 34")),
            Lead::Picker("Sprint 34".into())
        );
        assert_eq!(
            lead_words("Trello", &info, None),
            Lead::Board("Trello".into())
        );
        info.sprintless = true;
        info.sprints.clear();
        assert_eq!(
            lead_words("Platform", &info, Some("")),
            Lead::Board("Platform".into())
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

    /// Only the lines on screen are drawn, so the list is a line for each head and row.
    #[test]
    fn the_list_is_a_line_for_each_head_and_row_and_a_folded_group_has_its_head_alone() {
        let rows = [
            row("A-1", "todo", "new", None),
            row("A-2", "todo", "new", None),
            row("A-3", "done", "done", None),
        ];
        let play: Vec<&Row> = rows.iter().collect();
        let info = info();
        let shown = Shown {
            host: String::new(),
            slice: None,
            parts: Vec::new(),
            ranked: None,
            empty: None,
            board_name: String::new(),
            shelves: shelves(&play, &info, Group::State, &Filter::default(), None),
            info,
            all: rows.to_vec(),
            me: None,
            group: Group::State,
            columns: false,
            count: 3,
            subtasks: 0,
            people_picked: Vec::new(),
        };
        let still = |_: &str| false;
        let lines = list_lines(&shown, &|id| id == "state:Done", &still);
        let names: Vec<&str> = lines.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "head-state:To do",
                "row-A-1",
                "row-A-2",
                "head-state:In progress",
                "head-state:Done"
            ]
        );
        // A group short enough to move is one line while it folds.
        let moving = list_lines(&shown, &|_| false, &|key| key == "shelf-state:To do");
        assert_eq!(moving[1].1, Line::Folding(None, 0));
        // A lane longer than the screen is a head and a line for each card down it.
        assert_eq!(deepest(&shown.shelves[0], &shown.info), 2);
    }
}
