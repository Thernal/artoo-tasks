//! The boards read last, kept on the Mac in the plugin's folder (`boards.json`): each one's issues, columns, epics
//! and sprints, painted at the next start before the tracker answers, and where it cannot be reached. Written in the
//! background, readable by you alone; an account's go when it is removed or its token typed anew.

use crate::model::{Board, BoardId, BoardInfo, Row};
use artoo_plugin_sdk::serde_json::{self, Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How many boards are kept, the ones read last.
pub const BOARDS: usize = 5;

/// A board as it was read.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub board: Board,
    pub info: BoardInfo,
    pub rows: Vec<Row>,
    /// When its issues were read, Unix seconds.
    pub at: i64,
}

#[derive(Default)]
pub struct Cache {
    /// Where it is written; none writes nothing.
    pub file: Option<PathBuf>,
    /// The boards kept, the one read last first.
    pub boards: Vec<Snapshot>,
    /// How many times it changed: an older writing never lands over a newer one.
    changes: u64,
}

impl Cache {
    /// What `file` keeps; nothing when it is not there or not readable.
    pub fn read(file: PathBuf) -> Self {
        let kept: Value = (std::fs::read_to_string(&file).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Cache {
            file: Some(file),
            boards: from_json(&kept),
            changes: 0,
        }
    }

    pub fn board(&self, id: &BoardId) -> Option<&Board> {
        (self.boards.iter())
            .map(|s| &s.board)
            .find(|b| b.key() == *id)
    }

    /// `snapshot` first among the boards kept, the oldest past `BOARDS` let go.
    pub fn put(&mut self, snapshot: Snapshot) {
        let id = snapshot.board.key();
        self.boards.retain(|s| s.board.key() != id);
        self.boards.insert(0, snapshot);
        self.boards.truncate(BOARDS);
        self.changes += 1;
    }

    /// The boards of account `host` let go; whether any was kept.
    pub fn forget(&mut self, host: &str) -> bool {
        let before = self.boards.len();
        self.boards.retain(|s| s.board.site != host);
        let gone = self.boards.len() != before;
        if gone {
            self.changes += 1;
        }
        gone
    }

    /// The writing of what is kept now, to be run in the background.
    pub fn writing(&self) -> Option<impl FnOnce() + Send + 'static> {
        let (file, boards, change) = (self.file.clone()?, self.boards.clone(), self.changes);
        Some(move || {
            static LAST: Mutex<u64> = Mutex::new(0);
            let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
            if change >= *last {
                *last = change;
                // A cache not written is read from the tracker next time.
                let _ = write(&file, &to_json(&boards).to_string());
            }
        })
    }
}

/// `text` in `file`, whole or not at all.
fn write(file: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let new = file.with_extension("json.new");
    let _ = std::fs::remove_file(&new);
    let mut open = std::fs::OpenOptions::new();
    open.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
    open.open(&new)?.write_all(text.as_bytes())?;
    std::fs::rename(new, file)
}

fn to_json(boards: &[Snapshot]) -> Value {
    json!({
        "boards": boards.iter().map(|s| json!({
            "board": s.board, "info": s.info, "rows": s.rows, "at": s.at,
        })).collect::<Vec<_>>(),
    })
}

fn from_json(v: &Value) -> Vec<Snapshot> {
    (v["boards"].as_array().into_iter().flatten())
        .filter_map(|s| {
            Some(Snapshot {
                board: serde_json::from_value(s["board"].clone()).ok()?,
                info: serde_json::from_value(s["info"].clone()).ok()?,
                rows: serde_json::from_value(s["rows"].clone()).ok()?,
                at: s["at"].as_i64()?,
            })
        })
        .take(BOARDS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(id: &str, host: &str) -> Snapshot {
        Snapshot {
            board: Board {
                id: id.into(),
                name: format!("Board {id}"),
                site: host.into(),
                ..Default::default()
            },
            info: BoardInfo::default(),
            rows: vec![Row {
                key: format!("PAY-{id}"),
                ..Default::default()
            }],
            at: 1,
        }
    }

    #[test]
    fn the_last_five_boards_are_kept_and_an_accounts_go_with_it() {
        let mut cache = Cache::default();
        for id in ["1", "2", "3", "4", "5", "6", "2"] {
            cache.put(snapshot(id, "acme.atlassian.net"));
        }
        let ids: Vec<&str> = cache.boards.iter().map(|s| s.board.id.as_str()).collect();
        assert_eq!(ids, ["2", "6", "5", "4", "3"]);
        cache.put(snapshot("9", "trello.com"));
        assert!(cache.forget("acme.atlassian.net"));
        assert_eq!(cache.boards.len(), 1);
        assert!(!cache.forget("acme.atlassian.net"));
    }

    #[test]
    fn what_is_written_reads_back_for_you_alone() {
        let dir = std::env::temp_dir().join(format!("artoo-tasks-cache-{}", std::process::id()));
        let file = dir.join("boards.json");
        let mut cache = Cache::read(file.clone());
        assert!(cache.boards.is_empty());
        cache.put(snapshot("1", "acme.atlassian.net"));
        (cache.writing().unwrap())();
        assert_eq!(Cache::read(file.clone()).boards, cache.boards);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
