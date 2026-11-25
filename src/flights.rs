//! The reads on their way, by what they read: one per key, each numbered, so a second ask joins the first and an
//! answer overtaken by a newer ask is left out.

use artoo_plugin_sdk::gpui::Task;
use std::collections::HashMap;
use std::hash::Hash;

pub struct Flights<K> {
    asked: u64,
    /// Each key's read: its number, and its task, which stops when dropped.
    running: HashMap<K, (u64, Option<Task<()>>)>,
}

impl<K> Default for Flights<K> {
    fn default() -> Self {
        Flights {
            asked: 0,
            running: HashMap::new(),
        }
    }
}

impl<K: Clone + Eq + Hash> Flights<K> {
    /// A read of `key` to be made, by its number; none while one is on its way, unless that one is to be overtaken
    /// (`anew`: something was written since it was asked).
    pub fn begin(&mut self, key: &K, anew: bool) -> Option<u64> {
        if !anew && self.running.contains_key(key) {
            return None;
        }
        self.asked += 1;
        self.running.insert(key.clone(), (self.asked, None));
        Some(self.asked)
    }

    /// Whether a read of `key` is on its way.
    pub fn running(&self, key: &K) -> bool {
        self.running.contains_key(key)
    }

    /// The task of read `number`, kept for as long as it is wanted.
    pub fn hold(&mut self, key: &K, number: u64, task: Task<()>) {
        if let Some(read) = self.running.get_mut(key).filter(|r| r.0 == number) {
            read.1 = Some(task);
        }
    }

    /// Read `number` answered: whether it is still the one wanted.
    pub fn land(&mut self, key: &K, number: u64) -> bool {
        if self.running.get(key).is_none_or(|r| r.0 != number) {
            return false;
        }
        // Called from inside the task, which is left to finish.
        if let Some((_, Some(task))) = self.running.remove(key) {
            task.detach();
        }
        true
    }

    /// The reads of the keys `unwanted` names dropped; which they were.
    pub fn cancel(&mut self, unwanted: impl Fn(&K) -> bool) -> Vec<K> {
        let gone: Vec<K> = (self.running.keys())
            .filter(|k| unwanted(k))
            .cloned()
            .collect();
        for key in &gone {
            self.running.remove(key);
        }
        gone
    }
}
