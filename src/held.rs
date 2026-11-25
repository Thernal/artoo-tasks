//! A board read again quietly (the minute's refresh) shows what changed in place and holds what moved (round 41,
//! B41.7): a card someone else moved to another column, sprint or epic, one that came or went, stays as it was until
//! the slot's `1 moved ↑` is clicked; a changed field (a comment more, a new summary) changes where it is.

use crate::model::Row;
use artoo_plugin_sdk::kit::fresh::Held as Kind;

/// What a quiet read brought that is not shown yet: the read whole, and how much of it waits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Waiting {
    /// The rows as read: what is shown once the waiting is let in.
    pub rows: Vec<Row>,
    /// Rows shown that moved elsewhere, or are gone.
    pub moved: usize,
    /// Rows read that are not shown.
    pub new: usize,
}

impl Waiting {
    /// How many changes wait.
    pub fn count(&self) -> usize {
        self.moved + self.new
    }

    /// What the slot calls them: new when only new ones wait, else moved.
    pub fn kind(&self) -> Kind {
        if self.moved == 0 {
            Kind::NewAbove
        } else {
            Kind::MovedAbove
        }
    }

    /// Two reads' waiting (the active sprints' each) as one.
    pub fn add(&mut self, other: &Waiting) {
        self.moved += other.moved;
        self.new += other.new;
    }
}

/// Where a row stands on the board: its column, sprint, parent and epic. A row whose place changed has moved.
fn place(r: &Row) -> (&str, Option<u64>, Option<&str>, Option<&str>) {
    (
        r.state.id.as_str(),
        r.sprint.as_ref().map(|s| s.id),
        r.parent.as_deref(),
        r.epic.as_ref().map(|e| e.key.as_str()),
    )
}

/// The rows to show after a quiet read: `before` (what is shown) with each row still in its place taken from `read`,
/// in `before`'s order; and what waits, when anything moved, came or went. Nothing waiting, the read is shown whole.
pub fn merge(before: &[Row], read: Vec<Row>) -> (Vec<Row>, Option<Waiting>) {
    let find = |key: &str| read.iter().find(|r| r.key == key);
    let mut moved = 0;
    let shown: Vec<Row> = (before.iter())
        .map(|old| match find(&old.key) {
            Some(new) if place(new) == place(old) => new.clone(),
            _ => {
                moved += 1;
                old.clone()
            }
        })
        .collect();
    let new = (read.iter())
        .filter(|r| !before.iter().any(|b| b.key == r.key))
        .count();
    if moved == 0 && new == 0 {
        return (read, None);
    }
    (
        shown,
        Some(Waiting {
            rows: read,
            moved,
            new,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::State;

    fn row(key: &str, state: &str, comments: usize) -> Row {
        Row {
            key: key.into(),
            summary: format!("{key} summary"),
            state: State {
                id: state.into(),
                name: state.into(),
                category: "new".into(),
            },
            comments,
            ..Default::default()
        }
    }

    #[test]
    fn a_card_moved_by_someone_else_is_held_and_a_changed_field_changes_in_place() {
        let before = vec![row("PFM-1", "todo", 0), row("PFM-2", "todo", 3)];
        let read = vec![row("PFM-1", "doing", 0), row("PFM-2", "todo", 4)];
        let (shown, waiting) = merge(&before, read.clone());
        // PFM-1 stays in To do as it was; PFM-2's comments are 4 where it is.
        assert_eq!(
            shown,
            vec![row("PFM-1", "todo", 0), row("PFM-2", "todo", 4)]
        );
        let waiting = waiting.unwrap();
        assert_eq!((waiting.moved, waiting.new, waiting.count()), (1, 0, 1));
        assert_eq!(waiting.kind(), Kind::MovedAbove);
        assert_eq!(waiting.rows, read);
    }

    #[test]
    fn one_come_or_gone_waits_and_nothing_moved_shows_the_read_whole() {
        let before = vec![row("PFM-1", "todo", 0), row("PFM-2", "todo", 0)];
        let (shown, waiting) = merge(&before, vec![row("PFM-1", "todo", 1)]);
        assert_eq!(shown.len(), 2);
        assert_eq!(waiting.unwrap().count(), 1);
        let (_, waiting) = merge(
            &before,
            [before.clone(), vec![row("PFM-3", "todo", 0)]].concat(),
        );
        let waiting = waiting.unwrap();
        assert_eq!((waiting.new, waiting.kind()), (1, Kind::NewAbove));
        // The tracker's order changed, nothing moved: the read as it is.
        let read = vec![row("PFM-2", "todo", 0), row("PFM-1", "todo", 2)];
        assert_eq!(merge(&before, read.clone()), (read, None));
    }
}
