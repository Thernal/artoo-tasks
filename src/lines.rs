//! What a list of the board draws: only the lines on screen, so a board of thousands of issues scrolls as one of ten.

use artoo_plugin_sdk::gpui::{ListAlignment, ListOffset, ListState, px};
use std::ops::Range;

/// How far past the screen lines are drawn, so a quick scroll finds them there.
const OVERDRAW: f32 = 300.;

/// A list's lines by name, and where it is scrolled to.
#[derive(Clone)]
pub struct Lines {
    pub state: ListState,
    ids: Vec<String>,
}

impl Default for Lines {
    fn default() -> Self {
        Self {
            state: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
            ids: Vec::new(),
        }
    }
}

/// Which of `old`'s lines `new` replaces, and with how many: nothing when they are the same.
fn changed(old: &[String], new: &[String]) -> Option<(Range<usize>, usize)> {
    if old == new {
        return None;
    }
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = (old[head..].iter().rev())
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    Some((head..old.len() - tail, new.len() - head - tail))
}

impl Lines {
    /// The list now holds the lines `ids`. Scrolled down, the line at its top stays there while it is still among
    /// them, so a read again does not move what is being looked at.
    pub fn show(&mut self, ids: Vec<String>) -> ListState {
        if let Some((old, count)) = changed(&self.ids, &ids) {
            let top = self.state.logical_scroll_top();
            let scrolled = top.item_ix > 0 || top.offset_in_item > px(0.);
            let name = self.ids.get(top.item_ix).filter(|_| scrolled);
            let kept = name.and_then(|name| ids.iter().position(|id| id == name));
            self.state.splice(old, count);
            if let Some(item_ix) = kept {
                self.state.scroll_to(ListOffset { item_ix, ..top });
            }
            self.ids = ids;
        }
        self.state.clone()
    }

    /// The line `id` scrolled into sight.
    pub fn reveal(&self, id: &str) {
        if let Some(ix) = self.ids.iter().position(|x| x == id) {
            self.state.scroll_to_reveal_item(ix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(s: &str) -> Vec<String> {
        s.chars().map(String::from).collect()
    }

    #[test]
    fn only_the_lines_that_changed_are_replaced() {
        assert_eq!(changed(&ids("abc"), &ids("abc")), None);
        assert_eq!(changed(&ids("abcd"), &ids("abxcd")), Some((2..2, 1)));
        assert_eq!(changed(&ids("abcd"), &ids("ad")), Some((1..3, 0)));
        assert_eq!(changed(&ids("abcd"), &ids("axyd")), Some((1..3, 2)));
        assert_eq!(changed(&ids("aa"), &ids("aaa")), Some((2..2, 1)));
        assert_eq!(changed(&ids(""), &ids("ab")), Some((0..0, 2)));
    }
}
