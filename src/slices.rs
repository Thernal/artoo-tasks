//! A board read by slice: a sprint, All active sprints, the Backlog, or the whole board when it has no sprints. Each
//! slice is read, counted and failed on its own. Here is what is held of a board's slices, which one opens, and a
//! move's bookkeeping; the reads themselves are the store's.

use crate::model::{Ask, BoardInfo, Page, Person, Row, Sprint, Write, day_words, stamp};
use crate::store::Read;
use std::collections::HashMap;

/// How many issues a page of a slice asks for: what Jira's Agile API gives at most on Cloud.
pub const PAGE: usize = 50;

/// How many issues a board without sprints is read up to: its Done may hold thousands.
pub const BOARD_MOST: usize = 2000;

/// A slice of a board.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SliceId {
    /// An active, future or closed sprint.
    Sprint(u64),
    /// All active sprints, sprint after sprint.
    All,
    Backlog,
    /// A board without sprints, whole.
    Board,
}

impl SliceId {
    /// As it is kept: `sprint:34`, `all`, `backlog`, `board`.
    pub fn key(&self) -> String {
        match self {
            SliceId::Sprint(id) => format!("sprint:{id}"),
            SliceId::All => "all".into(),
            SliceId::Backlog => "backlog".into(),
            SliceId::Board => "board".into(),
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "all" => Some(SliceId::All),
            "backlog" => Some(SliceId::Backlog),
            "board" => Some(SliceId::Board),
            _ => key
                .strip_prefix("sprint:")?
                .parse()
                .ok()
                .map(SliceId::Sprint),
        }
    }

    /// What the tracker is asked for it; none for All, which is its sprints' reads joined.
    pub fn ask(&self) -> Option<Ask> {
        match self {
            SliceId::Sprint(id) => Some(Ask::sprint(*id)),
            SliceId::Backlog => Some(Ask::backlog()),
            SliceId::Board => Some(Ask::board()),
            SliceId::All => None,
        }
    }

    /// The sprint an issue moved here is in: none in the Backlog.
    pub fn sprint(&self) -> Option<u64> {
        match self {
            SliceId::Sprint(id) => Some(*id),
            _ => None,
        }
    }

    /// How many issues one read of it reads: a sprint to its end, a board up to `BOARD_MOST`, the Backlog a page.
    fn most(&self) -> usize {
        match self {
            SliceId::Sprint(_) | SliceId::All => usize::MAX,
            SliceId::Board => BOARD_MOST,
            SliceId::Backlog => PAGE,
        }
    }
}

/// The Backlog's filter: its words, yours, an epic. The Backlog is not read whole, so the tracker is asked; a sprint,
/// read whole, is filtered where it is shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub words: String,
    pub mine: bool,
    pub epic: Option<String>,
}

/// What is read of one slice.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SliceRead {
    /// Its issues in the tracker's order, sub-tasks among them.
    pub rows: Vec<Row>,
    /// Its issues' read: none asked yet, on its way, done, or why not (what was read before stays).
    pub state: Option<Read<()>>,
    /// How many issues it holds, sub-tasks left out, as the tracker counts them: `78`.
    pub count: Option<usize>,
    /// How many of them are yours.
    pub mine: Option<usize>,
    /// How many its filter finds, as counted: the Backlog's `Showing 50 of 214`.
    pub found: Option<usize>,
    /// Its counts are on their way.
    pub counting: bool,
    /// Its counts were asked and answered, or failed.
    pub counted: bool,
    /// Where its reading ends: the issues it holds as read, sub-tasks among them.
    pub total: Option<usize>,
    /// When it was read last, Unix seconds.
    pub at: Option<i64>,
    /// The filter it was last asked with (the Backlog's).
    pub filter: Filter,
    /// The filter its rows were read with: the one before, while a new one's read is on its way or failed.
    rows_filter: Filter,
    /// How many issues the read on its way wants.
    want: usize,
    /// A read again's pages, gathered apart and shown once whole: what was read before stays on screen meanwhile.
    fresh: Option<Vec<Row>>,
}

impl SliceRead {
    /// How many issues are read, sub-tasks left out, as `count` counts them: `Reading 31 of 78`.
    pub fn read(&self) -> usize {
        self.rows.iter().filter(|r| !r.subtask).count()
    }

    /// Whether its rows are the tracker's own order, none left out: read with no filter.
    pub fn in_order(&self) -> bool {
        self.rows_filter == Filter::default()
    }

    /// Whether it holds issues not read yet: the Backlog's Load more.
    pub fn more(&self) -> bool {
        self.total.is_some_and(|t| self.rows.len() < t)
    }

    /// The tracker's ask for its page from `start`, as far as the read wants.
    fn ask(&self, slice: &SliceId, start: usize) -> Option<Ask> {
        let f = self.filter.clone();
        Some(Ask {
            start,
            max: self.want.saturating_sub(start).min(PAGE),
            words: f.words,
            mine: f.mine,
            epic: f.epic,
            ..slice.ask()?
        })
    }
}

/// One read of a slice: a page of its issues from `start`; the first brings its counts, as asked (`count`) and yours.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Step {
    pub start: usize,
    pub page: Page,
    pub count: Option<usize>,
    pub mine: Option<usize>,
}

/// What a read of a slice is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// To be shown: read when it was not.
    Show,
    /// Read again, as far as it was read (⌘R, Retry, a refresh, a filter changed).
    Again,
    /// The Backlog's next page.
    More,
}

/// The active sprints of a board, as it lists them.
pub fn active(info: &BoardInfo) -> impl Iterator<Item = &Sprint> {
    info.sprints.iter().filter(|s| s.state == "active")
}

/// The sprint offered when none runs: the next to start, else the first the board lists.
pub fn next_sprint(info: &BoardInfo) -> Option<&Sprint> {
    (info.sprints.iter())
        .filter(|s| s.state == "future")
        .min_by_key(|s| s.start.as_deref().and_then(stamp).unwrap_or(i64::MAX))
}

/// Whether `slice` is still one of the board's, and open: a closed sprint is not.
fn holds(info: &BoardInfo, slice: &SliceId) -> bool {
    match slice {
        SliceId::Sprint(id) => (info.sprints.iter()).any(|s| s.id == *id && s.state != "closed"),
        SliceId::All => active(info).next().is_some(),
        SliceId::Backlog => !info.sprintless,
        SliceId::Board => info.sprintless,
    }
}

/// `slice`'s name as the tracker names it: a sprint's own, never shortened here. Empty for a board without sprints,
/// whose name is the board's.
pub fn name(slice: &SliceId, info: Option<&BoardInfo>, past: &[Sprint]) -> String {
    match slice {
        SliceId::Sprint(id) => (info.iter().flat_map(|i| &i.sprints))
            .chain(past)
            .find(|s| s.id == *id)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        SliceId::All => "All active sprints".into(),
        SliceId::Backlog => "Backlog".into(),
        SliceId::Board => String::new(),
    }
}

/// A sprint's days left on `now` (Unix seconds), by the calendar day it ends, in words; and whether they are few
/// (three or fewer, or past): drawn orange.
pub fn days_left(end: &str, now: i64) -> Option<(String, bool)> {
    let days = stamp(end)?.div_euclid(86_400) - now.div_euclid(86_400);
    Some(match days {
        ..-1 => (format!("ended {} days ago", -days), true),
        -1 => ("ended yesterday".into(), true),
        0 => ("ends today".into(), true),
        1 => ("1 day left".into(), true),
        days => (format!("{days} days left"), days <= 3),
    })
}

/// A number as people read it: `2,123`.
pub fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (ix, c) in digits.chars().enumerate() {
        if ix > 0 && (digits.len() - ix).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A sprint's line under its name in the sprint picker: `78 issues · ends Fri 28 Nov · 5 days left`, `No issues yet ·
/// starts Wed 3 Dec`, `ended Fri 14 Nov`; its issues left out until they are counted.
pub fn sprint_words(sprint: &Sprint, count: Option<usize>, unit: &str, now: i64) -> String {
    let day = |d: &Option<String>| d.as_deref().map(day_words);
    let mut out = Vec::new();
    if sprint.state == "closed" {
        out.extend(day(&sprint.end).map(|d| format!("ended {d}")));
        return out.join(" · ");
    }
    match count {
        Some(0) => out.push(format!("No {unit}s yet")),
        Some(n) => out.push(artoo_plugin_sdk::kit::format::plural(n, unit)),
        None => {}
    }
    if sprint.state == "future" {
        out.extend(day(&sprint.start).map(|d| format!("starts {d}")));
    } else {
        out.extend(day(&sprint.end).map(|d| format!("ends {d}")));
        out.extend((sprint.end.as_deref()).and_then(|e| Some(days_left(e, now)?.0)));
    }
    out.join(" · ")
}

/// How far a slice's issues have got, sub-tasks left out: done, in progress or review, to do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub done: usize,
    pub doing: usize,
    pub todo: usize,
}

impl Progress {
    pub fn of<'a>(rows: impl IntoIterator<Item = &'a Row>) -> Self {
        let mut p = Progress::default();
        for r in rows
            .into_iter()
            .filter(|r| !r.subtask && r.parent.is_none())
        {
            match r.state.category.as_str() {
                "done" => p.done += 1,
                "indeterminate" => p.doing += 1,
                _ => p.todo += 1,
            }
        }
        p
    }

    /// Of how many: the slice's count, else the issues read.
    pub fn all(&self, count: Option<usize>) -> usize {
        count.unwrap_or(self.done + self.doing + self.todo)
    }

    /// Beside its bar: `24 / 78`.
    pub fn short(&self, count: Option<usize>) -> String {
        format!("{} / {}", self.done, self.all(count))
    }

    /// Its bar's tooltip and accessible name: `24 done · 23 in progress or review · 31 to do of 78 issues in Sprint 4`.
    pub fn words(&self, count: Option<usize>, unit: &str, slice: &str) -> String {
        let all = self.all(count);
        let unit = if all == 1 {
            unit.to_owned()
        } else {
            format!("{unit}s")
        };
        format!(
            "{} done · {} in progress or review · {} to do of {all} {unit} in {slice}",
            self.done, self.doing, self.todo
        )
    }
}

/// A place issues may be moved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// A sprint, or the Backlog.
    pub slice: SliceId,
    /// The Backlog's top.
    pub top: bool,
    /// Where the issue is now: ticked, and moving there does nothing.
    pub here: bool,
}

/// Where issues in sprints `of` (none for one in the Backlog) may be moved on a board as `info` lists it: each active
/// and future sprint, the Backlog's top, the Backlog. None when one is in a closed sprint (it is read only) or the
/// board has no sprints. One issue sees every place, its own ticked; several, only what can take them, a place holding
/// them all left out.
pub fn targets(info: &BoardInfo, of: &[Option<&Sprint>]) -> Vec<Target> {
    if info.sprintless || of.is_empty() || of.iter().flatten().any(|s| s.state == "closed") {
        return Vec::new();
    }
    let all_in = |id: Option<u64>| of.iter().all(|s| s.map(|s| s.id) == id);
    let one = of.len() == 1;
    let sprints = (info.sprints.iter())
        .filter(|s| s.state == "active")
        .chain(info.sprints.iter().filter(|s| s.state == "future"));
    let mut out: Vec<Target> = sprints
        .map(|s| Target {
            slice: SliceId::Sprint(s.id),
            top: false,
            here: all_in(Some(s.id)),
        })
        .collect();
    out.push(Target {
        slice: SliceId::Backlog,
        top: true,
        here: false,
    });
    out.push(Target {
        slice: SliceId::Backlog,
        top: false,
        here: all_in(None),
    });
    if !one {
        out.retain(|t| !t.here);
    }
    out
}

/// The selection bar's words: `2 selected`, and their keys, `PAY-412, PAY-431`, past three `and 2 more`.
pub fn selection_words(keys: &[String]) -> (String, String) {
    let shown = keys.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    let rest = keys.len().saturating_sub(3);
    let listed = if rest > 0 {
        format!("{shown} and {rest} more")
    } else {
        shown
    };
    (format!("{} selected", grouped(keys.len())), listed)
}

/// Where an issue moved was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub key: String,
    pub from: SliceId,
    /// The issue that followed it there, when it was read: Undo ranks it back before that one.
    pub before: Option<String>,
    /// Its place among the slice's rows; none when it was not among them.
    at: Option<usize>,
    /// Its rows as they were: itself, then its sub-tasks.
    rows: Vec<Row>,
}

impl Placed {
    /// Its sprint before the move.
    pub fn sprint(&self) -> Option<Sprint> {
        self.rows.first().and_then(|r| r.sprint.clone())
    }
}

/// A move of issues to a slice, as shown: what a refusal and Undo need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub to: SliceId,
    /// To the Backlog's top.
    pub top: bool,
    pub placed: Vec<Placed>,
}

impl Moved {
    pub fn keys(&self) -> Vec<String> {
        self.placed.iter().map(|p| p.key.clone()).collect()
    }
}

/// How many pages of the Backlog are read, at most, to find what followed an issue.
const FOLLOW_PAGES: usize = 50;

/// Each issue of `moved` that left the Backlog with what followed it unknown given the issue after it, one not moved,
/// in the tracker's whole Backlog, read page by page by `page` from the issue asked.
pub fn follow(moved: &mut Moved, mut page: impl FnMut(usize) -> Option<Page>) {
    let keys = moved.keys();
    let mut order: Vec<String> = Vec::new();
    let mut start = 0;
    for _ in 0..FOLLOW_PAGES {
        let Some(read) = page(start) else {
            return;
        };
        start += read.rows.len();
        order.extend(
            (read.rows.iter())
                .filter(|r| !r.subtask)
                .map(|r| r.key.clone()),
        );
        let mut unknown = false;
        for p in
            (moved.placed.iter_mut()).filter(|p| p.from == SliceId::Backlog && p.before.is_none())
        {
            p.before = (order.iter().skip_while(|k| **k != p.key).skip(1))
                .find(|k| !keys.contains(k))
                .cloned();
            unknown |= p.before.is_none();
        }
        if !unknown || read.rows.is_empty() || start >= read.total {
            return;
        }
    }
}

/// The writes that undo `moved`: each issue back into the slice it came from, ranked before the issue that followed it.
pub fn undo_writes(moved: &Moved) -> Vec<Write> {
    (moved.placed.iter())
        .map(|p| Write::Sprint {
            key: p.key.clone(),
            sprint: p.from.sprint(),
            before: p.before.clone(),
        })
        .collect()
}

/// Count `n` moved by `by`, when it is known.
fn shift(n: &mut Option<usize>, by: isize) {
    if let Some(n) = n {
        *n = n.saturating_add_signed(by);
    }
}

/// Slice `r`'s counts as `rows` leave it (`sign` -1) or join it (+1); `me` whose issues are yours.
fn recount(r: &mut SliceRead, rows: &[Row], me: Option<&Person>, sign: isize) {
    let issues = rows.iter().filter(|r| !r.subtask);
    let yours = (issues.clone())
        .filter(|row| me.is_some_and(|me| row.assignee.as_ref().is_some_and(|p| p.id == me.id)))
        .count();
    shift(&mut r.count, sign * issues.count() as isize);
    shift(&mut r.mine, sign * yours as isize);
    shift(&mut r.total, sign * rows.len() as isize);
}

/// What is held of one board's slices.
#[derive(Clone, Debug, Default)]
pub struct Slices {
    pub reads: HashMap<SliceId, SliceRead>,
    /// The slice shown.
    pub view: Option<SliceId>,
    /// The Backlog's filter as typed.
    pub filter: Filter,
    /// The board's closed sprints, the last closed first, once Past sprints is opened.
    pub past: Option<Read<Vec<Sprint>>>,
}

impl Slices {
    /// The read of `slice` that `how` calls for, marked as on its way: what the tracker is asked first. The first page
    /// is read with its counts.
    pub fn begin(&mut self, slice: &SliceId, how: How) -> Option<Ask> {
        slice.ask()?;
        let typed = self.filter.clone();
        let r = self.reads.entry(slice.clone()).or_default();
        let start = match how {
            How::Show if r.state.is_some() => return None,
            How::More if r.state != Some(Read::Done(())) || !r.more() => return None,
            How::More => r.rows.len(),
            How::Show | How::Again => 0,
        };
        let same = *slice != SliceId::Backlog || r.filter == typed;
        r.want = match how {
            How::More => start + PAGE,
            // The Backlog read again as far as it was, while its filter stays.
            How::Again if *slice == SliceId::Backlog && same => r.rows.len().max(PAGE),
            _ => slice.most(),
        };
        if *slice == SliceId::Backlog && how != How::More {
            r.filter = typed;
        }
        r.fresh = (start == 0 && !r.rows.is_empty()).then(Vec::new);
        r.state = Some(Read::Reading);
        r.ask(slice, start)
    }

    /// Step `step` of `slice` taken in at `now`: its rows (a read again's gathered apart until whole) and its counts.
    /// The next page to ask while the read goes on.
    pub fn take(&mut self, slice: &SliceId, step: Step, now: i64) -> Option<Ask> {
        let r = self.reads.get_mut(slice)?;
        if step.count.is_some() {
            r.counted = true;
            r.found = step.count;
            if r.filter == Filter::default() {
                r.count = step.count;
            }
        }
        if step.mine.is_some() {
            r.mine = step.mine;
        }
        let next = step.start + step.page.rows.len();
        r.total = Some(step.page.total.max(next));
        let got = !step.page.rows.is_empty();
        match &mut r.fresh {
            Some(fresh) => fresh.extend(step.page.rows),
            None => {
                r.rows.extend(step.page.rows);
                r.rows_filter = r.filter.clone();
            }
        }
        if got && next < r.want && next < step.page.total {
            return r.ask(slice, next);
        }
        if let Some(fresh) = r.fresh.take() {
            r.rows = fresh;
            r.rows_filter = r.filter.clone();
        }
        r.state = Some(Read::Done(()));
        r.at = Some(now);
        None
    }

    /// `slice`'s read failed: why, over what was read before.
    pub fn fail(&mut self, slice: &SliceId, why: String) {
        if let Some(r) = self.reads.get_mut(slice) {
            r.fresh = None;
            r.counted = true;
            r.state = Some(Read::Failed(why));
        }
    }

    /// `slice`'s read given up (its board left): asked again when it is shown.
    pub fn dropped(&mut self, slice: &SliceId) {
        if let Some(r) = self.reads.get_mut(slice) {
            r.fresh = None;
            if r.state == Some(Read::Reading) {
                r.state = None;
            }
        }
    }

    /// `slice`'s counts wanted (the picker's, which slice opens), unless known, or on their way, or read `again`: what
    /// the tracker is asked.
    pub fn begin_count(&mut self, slice: &SliceId, again: bool) -> Option<Ask> {
        let ask = slice.ask()?;
        let r = self.reads.entry(slice.clone()).or_default();
        if !again && (r.counting || r.count.is_some()) {
            return None;
        }
        r.counting = true;
        Some(ask)
    }

    /// `slice`'s counts as read: how many it holds and how many are yours; none taken when they could not be read.
    pub fn take_count(&mut self, slice: &SliceId, read: Result<(usize, Option<usize>), String>) {
        let Some(r) = self.reads.get_mut(slice) else {
            return;
        };
        r.counting = false;
        r.counted = true;
        if let Ok((count, mine)) = read {
            r.count = Some(count);
            r.mine = mine.or(r.mine);
        }
    }

    /// The slice the board opens on (S2): `remembered` while it still exists and is not closed; else the active
    /// sprint that holds your issues, else the one that ends first; with no active sprint, the Backlog. None while the
    /// active sprints' counts that decide it are on their way.
    pub fn opens(&self, remembered: Option<&SliceId>, info: &BoardInfo) -> Option<SliceId> {
        if let Some(slice) = remembered.filter(|s| holds(info, s)) {
            return Some(slice.clone());
        }
        if info.sprintless {
            return Some(SliceId::Board);
        }
        let active: Vec<&Sprint> = active(info).collect();
        let read = |s: &Sprint| self.reads.get(&SliceId::Sprint(s.id));
        match active.as_slice() {
            [] => Some(SliceId::Backlog),
            [one] => Some(SliceId::Sprint(one.id)),
            several => {
                if several
                    .iter()
                    .any(|s| read(s).is_none_or(|r| r.counting || !r.counted))
                {
                    return None;
                }
                let yours: Vec<&Sprint> = (several.iter().copied())
                    .filter(|s| read(s).and_then(|r| r.mine).unwrap_or(0) > 0)
                    .collect();
                let among = if yours.is_empty() {
                    several
                } else {
                    &yours[..]
                };
                (among.iter())
                    .min_by_key(|s| s.end.as_deref().and_then(stamp).unwrap_or(i64::MAX))
                    .map(|s| SliceId::Sprint(s.id))
            }
        }
    }

    /// How many issues `slice` holds and how many are yours, as counted; All's are its sprints' added up, once each is
    /// counted.
    pub fn counts(&self, slice: &SliceId, info: &BoardInfo) -> (Option<usize>, Option<usize>) {
        let of = |s: &SliceId| {
            self.reads
                .get(s)
                .map_or((None, None), |r| (r.count, r.mine))
        };
        match slice {
            SliceId::All => {
                let each: Vec<_> = active(info).map(|s| of(&SliceId::Sprint(s.id))).collect();
                (
                    each.iter().map(|c| c.0).sum(),
                    each.iter().map(|c| c.1).sum(),
                )
            }
            slice => of(slice),
        }
    }

    /// All active sprints' issues, sprint after sprint as the board lists them.
    pub fn all_rows<'a>(&'a self, info: &'a BoardInfo) -> impl Iterator<Item = &'a Row> {
        active(info)
            .filter_map(|s| self.reads.get(&SliceId::Sprint(s.id)))
            .flat_map(|r| r.rows.iter())
    }

    /// The slice read whose rows hold issue `key`, and its place there.
    fn find(&self, key: &str) -> Option<(SliceId, usize)> {
        self.reads.iter().find_map(|(slice, r)| {
            let at = r.rows.iter().position(|row| row.key == key)?;
            Some((slice.clone(), at))
        })
    }

    /// Issues `keys` moved to `to` (a sprint or the Backlog; its top with `top`) as shown at once: each leaves its
    /// slice with its sub-tasks and joins `to`, named in sprint `sprint`, the counts following; `me` is whose issues are
    /// yours. An issue in no slice read is taken from `known` (its row as read elsewhere), from the slice its sprint
    /// names.
    pub fn place(
        &mut self,
        keys: &[String],
        to: &SliceId,
        top: bool,
        sprint: Option<Sprint>,
        me: Option<&Person>,
        known: impl Fn(&str) -> Option<Row>,
    ) -> Moved {
        let mut moved = Moved {
            to: to.clone(),
            top,
            placed: Vec::new(),
        };
        let mut on_top = 0;
        for key in keys {
            let placed = match self.find(key) {
                Some((from, _)) if from == *to && !top => continue,
                Some((from, at)) => {
                    let r = self.reads.get_mut(&from).expect("found");
                    // A filtered Backlog's next row is not the issue that follows it there.
                    let known = from != SliceId::Backlog || r.in_order();
                    let before = (r.rows[at + 1..].iter())
                        .find(|row| !row.subtask && !keys.contains(&row.key))
                        .filter(|_| known)
                        .map(|row| row.key.clone());
                    let (rows, stay): (Vec<Row>, Vec<Row>) = (r.rows.drain(..))
                        .partition(|row| row.key == *key || row.parent.as_deref() == Some(key));
                    r.rows = stay;
                    recount(r, &rows, me, -1);
                    Placed {
                        key: key.clone(),
                        from,
                        before,
                        at: Some(at),
                        rows,
                    }
                }
                None => {
                    let Some(row) = known(key) else {
                        continue;
                    };
                    let from = row
                        .sprint
                        .as_ref()
                        .map_or(SliceId::Backlog, |s| SliceId::Sprint(s.id));
                    if let Some(r) = self.reads.get_mut(&from) {
                        recount(r, std::slice::from_ref(&row), me, -1);
                    }
                    Placed {
                        key: key.clone(),
                        from,
                        before: None,
                        at: None,
                        rows: vec![row],
                    }
                }
            };
            for r in self.reads.values_mut() {
                if let Some(fresh) = &mut r.fresh {
                    fresh.retain(|row| row.key != *key && row.parent.as_deref() != Some(key));
                }
            }
            if let Some(r) = self.reads.get_mut(to) {
                let mut rows = placed.rows.clone();
                rows.iter_mut().for_each(|row| row.sprint = sprint.clone());
                recount(r, &rows, me, 1);
                if r.state.is_some() {
                    let at = if top { on_top } else { r.rows.len() };
                    on_top += rows.len();
                    r.rows.splice(at..at, rows);
                }
            }
            moved.placed.push(placed);
        }
        moved
    }

    /// Issues `keys` of move `moved` put back where they were, their counts with them: a refusal, or Undo.
    pub fn take_back(&mut self, moved: &Moved, keys: &[String], me: Option<&Person>) {
        for p in moved.placed.iter().rev().filter(|p| keys.contains(&p.key)) {
            if let Some(r) = self.reads.get_mut(&moved.to) {
                let (gone, stay): (Vec<Row>, Vec<Row>) = (r.rows.drain(..))
                    .partition(|row| row.key == p.key || row.parent.as_deref() == Some(&p.key));
                r.rows = stay;
                let gone = if gone.is_empty() {
                    p.rows.clone()
                } else {
                    gone
                };
                recount(r, &gone, me, -1);
            }
            if let Some(r) = self.reads.get_mut(&p.from) {
                recount(r, &p.rows, me, 1);
                if let Some(at) = p.at.filter(|_| r.state.is_some()) {
                    let at = at.min(r.rows.len());
                    r.rows.splice(at..at, p.rows.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprint(id: u64, state: &str, end: Option<&str>) -> Sprint {
        Sprint {
            id,
            name: format!("Sprint {id}"),
            state: state.into(),
            end: end.map(str::to_owned),
            ..Default::default()
        }
    }

    fn row(key: &str, sprint: Option<u64>, who: Option<&str>) -> Row {
        Row {
            key: key.into(),
            sprint: sprint.map(|id| self::sprint(id, "active", None)),
            assignee: who.map(|id| Person {
                id: id.into(),
                name: id.into(),
            }),
            ..Default::default()
        }
    }

    fn sub(key: &str, parent: &str) -> Row {
        Row {
            key: key.into(),
            subtask: true,
            parent: Some(parent.into()),
            ..Default::default()
        }
    }

    fn info(sprints: Vec<Sprint>) -> BoardInfo {
        BoardInfo {
            sprints,
            ..Default::default()
        }
    }

    fn counted(mine: usize) -> SliceRead {
        SliceRead {
            count: Some(10),
            mine: Some(mine),
            counted: true,
            ..Default::default()
        }
    }

    fn keys(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|r| r.key.as_str()).collect()
    }

    #[test]
    fn days_left_are_counted_by_the_day_a_sprint_ends_and_few_from_three() {
        let now = stamp("2026-11-23T09:30:00.000Z").unwrap();
        let left = |end: &str| days_left(end, now).unwrap();
        assert_eq!(
            left("2026-11-28T17:00:00.000Z"),
            ("5 days left".into(), false)
        );
        assert_eq!(
            left("2026-11-27T08:00:00.000Z"),
            ("4 days left".into(), false)
        );
        assert_eq!(
            left("2026-11-26T17:00:00.000Z"),
            ("3 days left".into(), true)
        );
        assert_eq!(
            left("2026-11-24T17:00:00.000Z"),
            ("1 day left".into(), true)
        );
        assert_eq!(
            left("2026-11-23T17:00:00.000Z"),
            ("ends today".into(), true)
        );
        assert_eq!(
            left("2026-11-22T17:00:00.000Z"),
            ("ended yesterday".into(), true)
        );
        assert_eq!(
            left("2026-11-20T17:00:00.000Z"),
            ("ended 3 days ago".into(), true)
        );
        assert_eq!(days_left("someday", now), None);
    }

    #[test]
    fn an_issue_moves_to_any_open_sprint_or_the_backlog_its_place_ticked() {
        let mut info = info(vec![
            sprint(34, "active", None),
            sprint(35, "future", None),
            sprint(36, "active", None),
        ]);
        fn places(info: &BoardInfo, of: &[Option<&Sprint>]) -> Vec<(String, bool, bool)> {
            (targets(info, of).into_iter())
                .map(|t| (t.slice.key(), t.top, t.here))
                .collect()
        }
        let s34 = sprint(34, "active", None);
        assert_eq!(
            places(&info, &[Some(&s34)]),
            [
                ("sprint:34".into(), false, true),
                ("sprint:36".into(), false, false),
                ("sprint:35".into(), false, false),
                ("backlog".into(), true, false),
                ("backlog".into(), false, false),
            ],
            "in a sprint: the active sprints first, then the future, its own ticked"
        );
        assert_eq!(
            places(&info, &[None]),
            [
                ("sprint:34".into(), false, false),
                ("sprint:36".into(), false, false),
                ("sprint:35".into(), false, false),
                ("backlog".into(), true, false),
                ("backlog".into(), false, true),
            ],
            "in the Backlog: its top is still a move"
        );
        let past = sprint(31, "closed", None);
        assert_eq!(
            places(&info, &[Some(&past)]),
            [],
            "a past sprint is read only"
        );
        assert_eq!(places(&info, &[Some(&s34), Some(&past)]), []);
        assert_eq!(
            places(&info, &[Some(&s34), Some(&s34)]),
            [
                ("sprint:36".into(), false, false),
                ("sprint:35".into(), false, false),
                ("backlog".into(), true, false),
                ("backlog".into(), false, false),
            ],
            "several: only what can take them"
        );
        assert_eq!(places(&info, &[Some(&s34), None]).len(), 5);
        info.sprintless = true;
        assert_eq!(places(&info, &[None]), []);
    }

    #[test]
    fn the_selection_bar_says_how_many_and_which() {
        let keys = |n: usize| -> Vec<String> { (1..=n).map(|i| format!("PAY-{i}")).collect() };
        assert_eq!(
            selection_words(&keys(2)),
            ("2 selected".into(), "PAY-1, PAY-2".into())
        );
        assert_eq!(
            selection_words(&keys(5)),
            ("5 selected".into(), "PAY-1, PAY-2, PAY-3 and 2 more".into())
        );
    }

    #[test]
    fn a_number_is_grouped_by_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(950), "950");
        assert_eq!(grouped(2123), "2,123");
        assert_eq!(grouped(1234567), "1,234,567");
    }

    #[test]
    fn a_sprint_says_its_issues_and_when_it_ends_or_starts() {
        let now = stamp("2026-11-23T09:30:00.000Z").unwrap();
        let mut active = sprint(34, "active", Some("2026-11-28T17:00:00.000Z"));
        active.start = Some("2026-11-17T09:00:00.000Z".into());
        assert_eq!(
            sprint_words(&active, Some(78), "issue", now),
            "78 issues · ends Sat 28 Nov · 5 days left"
        );
        assert_eq!(
            sprint_words(&active, None, "issue", now),
            "ends Sat 28 Nov · 5 days left"
        );
        let mut future = sprint(35, "future", None);
        future.start = Some("2026-12-03T09:00:00.000Z".into());
        assert_eq!(
            sprint_words(&future, Some(0), "issue", now),
            "No issues yet · starts Thu 3 Dec"
        );
        let closed = sprint(33, "closed", Some("2026-11-14T17:00:00.000Z"));
        assert_eq!(
            sprint_words(&closed, Some(40), "issue", now),
            "ended Sat 14 Nov"
        );
    }

    #[test]
    fn progress_counts_issues_by_their_state_and_says_of_the_slice_count() {
        let at = |key: &str, category: &str| Row {
            key: key.into(),
            state: crate::model::State {
                category: category.into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let rows = vec![
            at("PAY-1", "done"),
            at("PAY-2", "done"),
            at("PAY-3", "indeterminate"),
            at("PAY-4", "new"),
            sub("PAY-5", "PAY-4"),
        ];
        let p = Progress::of(&rows);
        assert_eq!(
            p,
            Progress {
                done: 2,
                doing: 1,
                todo: 1
            }
        );
        assert_eq!(p.short(None), "2 / 4");
        assert_eq!(p.short(Some(78)), "2 / 78");
        assert_eq!(
            p.words(Some(78), "issue", "Sprint 4"),
            "2 done · 1 in progress or review · 1 to do of 78 issues in Sprint 4"
        );
        assert_eq!(
            Progress::of(&rows[3..4]).words(None, "card", "Sprint 4"),
            "0 done · 0 in progress or review · 1 to do of 1 card in Sprint 4"
        );
    }

    #[test]
    fn a_slice_is_kept_by_its_key() {
        for slice in [
            SliceId::Sprint(34),
            SliceId::All,
            SliceId::Backlog,
            SliceId::Board,
        ] {
            assert_eq!(SliceId::parse(&slice.key()), Some(slice));
        }
        assert_eq!(SliceId::parse("sprint:x"), None);
    }

    #[test]
    fn the_remembered_slice_opens_while_it_is_open() {
        let board = info(vec![
            sprint(34, "active", Some("2026-11-14T17:00:00.000Z")),
            sprint(35, "future", None),
        ]);
        let s = Slices::default();
        let opens = |r: SliceId| s.opens(Some(&r), &board);
        assert_eq!(opens(SliceId::Sprint(35)), Some(SliceId::Sprint(35)));
        assert_eq!(opens(SliceId::Backlog), Some(SliceId::Backlog));
        assert_eq!(opens(SliceId::All), Some(SliceId::All));
        // Gone, or closed: the rule decides.
        assert_eq!(opens(SliceId::Sprint(29)), Some(SliceId::Sprint(34)));
        assert_eq!(opens(SliceId::Board), Some(SliceId::Sprint(34)));
        let closed = info(vec![sprint(29, "closed", None)]);
        assert_eq!(
            s.opens(Some(&SliceId::Sprint(29)), &closed),
            Some(SliceId::Backlog)
        );
    }

    #[test]
    fn with_none_remembered_your_sprint_opens_else_the_one_that_ends_first() {
        let board = info(vec![
            sprint(34, "active", Some("2026-11-14T17:00:00.000Z")),
            sprint(36, "active", Some("2026-11-21T17:00:00.000Z")),
            sprint(37, "active", Some("2026-11-28T17:00:00.000Z")),
        ]);
        let mut s = Slices::default();
        // Their counts on their way: not yet.
        assert_eq!(s.opens(None, &board), None);
        s.reads.insert(SliceId::Sprint(34), counted(0));
        s.reads.insert(SliceId::Sprint(36), counted(4));
        s.reads.insert(SliceId::Sprint(37), counted(2));
        assert_eq!(s.opens(None, &board), Some(SliceId::Sprint(36)));
        s.reads.get_mut(&SliceId::Sprint(36)).unwrap().counting = true;
        assert_eq!(s.opens(None, &board), None);
        s.reads.insert(SliceId::Sprint(36), counted(0));
        assert_eq!(s.opens(None, &board), Some(SliceId::Sprint(37)));
        // None yours: the one that ends first.
        s.reads.insert(SliceId::Sprint(37), counted(0));
        assert_eq!(s.opens(None, &board), Some(SliceId::Sprint(34)));
        // A count that failed counts as none of yours.
        let failed = SliceRead {
            counted: true,
            ..Default::default()
        };
        s.reads.insert(SliceId::Sprint(34), failed);
        assert_eq!(s.opens(None, &board), Some(SliceId::Sprint(34)));
        let one = info(vec![sprint(34, "active", None), sprint(35, "future", None)]);
        assert_eq!(
            Slices::default().opens(None, &one),
            Some(SliceId::Sprint(34))
        );
    }

    #[test]
    fn with_no_sprint_running_the_backlog_opens_and_the_next_sprint_is_offered() {
        let mut later = sprint(40, "future", None);
        later.start = Some("2026-12-07T09:00:00.000Z".into());
        let mut sooner = sprint(41, "future", None);
        sooner.start = Some("2026-11-30T09:00:00.000Z".into());
        let board = info(vec![later, sooner]);
        assert_eq!(
            Slices::default().opens(None, &board),
            Some(SliceId::Backlog)
        );
        assert_eq!(next_sprint(&board).map(|s| s.id), Some(41));
        let none = BoardInfo {
            sprintless: true,
            ..Default::default()
        };
        assert_eq!(Slices::default().opens(None, &none), Some(SliceId::Board));
        assert_eq!(
            Slices::default().opens(Some(&SliceId::Backlog), &none),
            Some(SliceId::Board)
        );
    }

    /// A page of `n` issues from `start`, of `total`.
    fn step(start: usize, n: usize, total: usize, count: Option<usize>) -> Step {
        Step {
            start,
            page: Page {
                rows: (start..start + n)
                    .map(|i| row(&format!("PAY-{i}"), Some(34), None))
                    .collect(),
                total,
            },
            count,
            mine: count.map(|_| 3),
        }
    }

    #[test]
    fn a_sprint_is_read_to_its_end_page_by_page() {
        let mut s = Slices::default();
        let sprint = SliceId::Sprint(34);
        let first = s.begin(&sprint, How::Show).unwrap();
        assert_eq!((first.start, first.max), (0, PAGE));
        assert_eq!(s.begin(&sprint, How::Show), None);
        let next = s.take(&sprint, step(0, 50, 80, Some(78)), 1).unwrap();
        let r = &s.reads[&sprint];
        assert_eq!((r.read(), r.count, r.mine), (50, Some(78), Some(3)));
        assert_eq!(r.state, Some(Read::Reading));
        assert_eq!(
            (next.start, next.max, next.slice.as_str()),
            (50, PAGE, "sprint:34")
        );
        assert_eq!(s.take(&sprint, step(50, 30, 80, None), 2), None);
        let r = &s.reads[&sprint];
        assert_eq!(
            (r.rows.len(), r.state.clone(), r.at),
            (80, Some(Read::Done(())), Some(2))
        );
        // Read again: the rows read stay on screen until the read is whole.
        s.begin(&sprint, How::Again).unwrap();
        s.take(&sprint, step(0, 50, 81, Some(79)), 3).unwrap();
        assert_eq!(s.reads[&sprint].rows.len(), 80);
        s.take(&sprint, step(50, 31, 81, None), 4);
        assert_eq!(s.reads[&sprint].rows.len(), 81);
        // A failure is said over what was read.
        s.begin(&sprint, How::Again).unwrap();
        s.fail(&sprint, "Can't reach acme.atlassian.net".into());
        let r = &s.reads[&sprint];
        assert_eq!(r.rows.len(), 81);
        assert!(matches!(r.state, Some(Read::Failed(_))));
    }

    #[test]
    fn the_backlog_loads_a_page_at_a_time_and_asks_its_filter() {
        let mut s = Slices::default();
        let backlog = SliceId::Backlog;
        s.begin(&backlog, How::Show).unwrap();
        assert_eq!(s.take(&backlog, step(0, 50, 300, Some(300)), 1), None);
        assert!(s.reads[&backlog].more());
        let more = s.begin(&backlog, How::More).unwrap();
        assert_eq!((more.start, more.max), (50, PAGE));
        s.take(&backlog, step(50, 50, 300, None), 2);
        assert_eq!(s.reads[&backlog].rows.len(), 100);
        // Read again as far as it was loaded.
        let again = s.begin(&backlog, How::Again).unwrap();
        assert_eq!((again.start, again.max), (0, PAGE));
        assert!(s.take(&backlog, step(0, 50, 300, Some(300)), 3).is_some());
        // A filter asks the tracker from the start; the count stays the Backlog's own.
        s.filter = Filter {
            words: "refund".into(),
            mine: true,
            epic: Some("PAY-400".into()),
        };
        let filtered = s.begin(&backlog, How::Again).unwrap();
        assert_eq!(
            (
                filtered.words.as_str(),
                filtered.mine,
                filtered.epic.as_deref()
            ),
            ("refund", true, Some("PAY-400"))
        );
        assert_eq!(s.take(&backlog, step(0, 12, 12, Some(12)), 4), None);
        let r = &s.reads[&backlog];
        assert_eq!((r.rows.len(), r.count, r.found), (12, Some(300), Some(12)));
        assert!(!r.more());
        assert_eq!(s.begin(&backlog, How::More), None);
    }

    #[test]
    fn a_count_read_alone_is_kept_and_not_asked_twice() {
        let mut s = Slices::default();
        let sprint = SliceId::Sprint(36);
        assert!(s.begin_count(&sprint, false).is_some());
        assert_eq!(s.begin_count(&sprint, false), None);
        s.take_count(&sprint, Ok((136, Some(9))));
        let r = &s.reads[&sprint];
        assert_eq!((r.count, r.mine, r.counting), (Some(136), Some(9), false));
        assert_eq!(s.begin_count(&sprint, false), None);
        assert!(s.begin_count(&sprint, true).is_some());
        s.take_count(&sprint, Err("504".into()));
        assert_eq!(s.reads[&sprint].count, Some(136));
        assert_eq!(s.begin_count(&SliceId::All, false), None);
    }

    /// A board with sprint 34 (PAY-1, its sub-task, PAY-2 yours, PAY-3) and the Backlog (PAY-9), both read.
    fn two_slices() -> Slices {
        let mut s = Slices::default();
        let read = |rows: Vec<Row>, count: usize| SliceRead {
            total: Some(rows.len()),
            rows,
            count: Some(count),
            mine: Some(1),
            state: Some(Read::Done(())),
            ..Default::default()
        };
        s.reads.insert(
            SliceId::Sprint(34),
            read(
                vec![
                    row("PAY-1", Some(34), None),
                    sub("PAY-11", "PAY-1"),
                    row("PAY-2", Some(34), Some("kestrel")),
                    row("PAY-3", Some(34), None),
                ],
                3,
            ),
        );
        s.reads.insert(
            SliceId::Backlog,
            read(vec![row("PAY-9", None, Some("kestrel"))], 1),
        );
        s
    }

    fn me() -> Person {
        Person {
            id: "kestrel".into(),
            name: "Kestrel".into(),
        }
    }

    #[test]
    fn a_move_leaves_one_slice_and_joins_the_other_with_its_counts() {
        let mut s = two_slices();
        let keys = vec!["PAY-1".to_owned(), "PAY-2".to_owned()];
        let moved = s.place(&keys, &SliceId::Backlog, true, None, Some(&me()), |_| None);
        let (sprint, backlog) = (&s.reads[&SliceId::Sprint(34)], &s.reads[&SliceId::Backlog]);
        assert_eq!(keys_of(sprint), ["PAY-3"]);
        assert_eq!(
            (sprint.count, sprint.mine, sprint.total),
            (Some(1), Some(0), Some(1))
        );
        // To the top, in the order moved, each with its sub-tasks; in no sprint now.
        assert_eq!(keys_of(backlog), ["PAY-1", "PAY-11", "PAY-2", "PAY-9"]);
        assert_eq!(
            (backlog.count, backlog.mine, backlog.total),
            (Some(3), Some(2), Some(4))
        );
        assert!(backlog.rows.iter().all(|r| r.sprint.is_none()));
        // What Undo needs: where each was, and the issue after it that was not moved too.
        let placed: Vec<_> = (moved.placed.iter())
            .map(|p| (p.key.as_str(), p.from.clone(), p.before.as_deref()))
            .collect();
        assert_eq!(
            placed,
            [
                ("PAY-1", SliceId::Sprint(34), Some("PAY-3")),
                ("PAY-2", SliceId::Sprint(34), Some("PAY-3")),
            ]
        );
        assert_eq!(
            undo_writes(&moved)[0],
            Write::Sprint {
                key: "PAY-1".into(),
                sprint: Some(34),
                before: Some("PAY-3".into())
            }
        );
        // Undo: each back in its place, the counts as before.
        s.take_back(&moved, &moved.keys(), Some(&me()));
        assert_eq!(
            s.reads[&SliceId::Sprint(34)],
            two_slices().reads[&SliceId::Sprint(34)]
        );
        assert_eq!(
            s.reads[&SliceId::Backlog],
            two_slices().reads[&SliceId::Backlog]
        );
    }

    fn keys_of(r: &SliceRead) -> Vec<&str> {
        keys(&r.rows)
    }

    #[test]
    fn a_filtered_backlog_names_no_follower_and_the_whole_one_is_read_for_it() {
        let mut s = two_slices();
        let key = vec!["PAY-9".to_owned()];
        s.reads.get_mut(&SliceId::Backlog).unwrap().rows_filter.mine = true;
        let mut moved = s.place(&key, &SliceId::Backlog, true, None, Some(&me()), |_| None);
        assert_eq!(moved.placed[0].before, None);
        let row = |k: &str| Row {
            key: k.into(),
            ..Default::default()
        };
        let mut asked = Vec::new();
        follow(&mut moved, |start| {
            asked.push(start);
            let all = ["PAY-7", "PAY-9", "PAY-10"];
            Some(Page {
                rows: all.iter().skip(start).take(2).map(|k| row(k)).collect(),
                total: all.len(),
            })
        });
        assert_eq!(asked, [0, 2]);
        assert_eq!(moved.placed[0].before.as_deref(), Some("PAY-10"));
    }

    #[test]
    fn rows_a_filter_read_stay_out_of_order_when_the_read_without_it_fails() {
        let mut s = two_slices();
        let backlog = SliceId::Backlog;
        s.reads.get_mut(&backlog).unwrap().rows_filter.mine = true;
        s.begin(&backlog, How::Again);
        s.fail(&backlog, "504".into());
        assert!(!s.reads[&backlog].in_order());
    }

    #[test]
    fn a_refused_issue_alone_goes_back() {
        let mut s = two_slices();
        let keys = vec!["PAY-2".to_owned(), "PAY-3".to_owned()];
        let to = sprint(35, "future", None);
        s.reads.insert(SliceId::Sprint(35), SliceRead::default());
        let moved = s.place(&keys, &SliceId::Sprint(35), false, Some(to), None, |_| None);
        assert_eq!(
            s.reads[&SliceId::Sprint(35)].rows.len(),
            0,
            "not read: its rows are not made up"
        );
        s.take_back(&moved, &["PAY-2".to_owned()], None);
        assert_eq!(
            keys_of(&s.reads[&SliceId::Sprint(34)]),
            ["PAY-1", "PAY-11", "PAY-2"]
        );
        assert_eq!(s.reads[&SliceId::Sprint(34)].count, Some(2));
        // The last issue moved has nothing after it to go back before.
        assert_eq!(moved.placed[1].before, None);
        assert_eq!(undo_writes(&moved)[1].key(), Some("PAY-3"));
    }

    #[test]
    fn an_issue_already_there_is_not_moved_and_one_unread_is_moved_by_its_row() {
        let mut s = two_slices();
        let moved = s.place(
            &["PAY-3".to_owned()],
            &SliceId::Sprint(34),
            false,
            None,
            None,
            |_| None,
        );
        assert!(moved.placed.is_empty());
        let elsewhere = |key: &str| Some(row(key, Some(34), None));
        let moved = s.place(
            &["PAY-77".to_owned()],
            &SliceId::Backlog,
            false,
            None,
            None,
            elsewhere,
        );
        assert_eq!(moved.placed[0].from, SliceId::Sprint(34));
        assert_eq!(s.reads[&SliceId::Sprint(34)].count, Some(2));
        assert_eq!(keys_of(&s.reads[&SliceId::Backlog]), ["PAY-9", "PAY-77"]);
        assert_eq!(moved.placed[0].sprint().map(|s| s.id), Some(34));
    }

    #[test]
    fn all_active_sprints_are_their_reads_joined() {
        let mut s = two_slices();
        s.reads.insert(
            SliceId::Sprint(36),
            SliceRead {
                rows: vec![row("PAY-50", Some(36), None)],
                ..Default::default()
            },
        );
        let board = info(vec![
            sprint(34, "active", None),
            sprint(35, "future", None),
            sprint(36, "active", None),
        ]);
        let all: Vec<&str> = s.all_rows(&board).map(|r| r.key.as_str()).collect();
        assert_eq!(all, ["PAY-1", "PAY-11", "PAY-2", "PAY-3", "PAY-50"]);
        assert_eq!(name(&SliceId::Sprint(36), Some(&board), &[]), "Sprint 36");
        assert_eq!(
            name(
                &SliceId::Sprint(29),
                Some(&board),
                &[sprint(29, "closed", None)]
            ),
            "Sprint 29"
        );
    }
}
