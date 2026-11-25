//! How a tracker kind crosses from the plugin adding it to Tasks: offered through the app's services
//! (`artoo_plugin_sdk::services`) under `tasks.tracker.<id>@1`, and read back by Tasks.
//!
//! Two plugins are two builds, and a type of this crate compiled into each is two types to the app's services. So
//! what crosses is built of std's and the SDK's types alone: the kind as JSON, and one call — a method's name and its
//! arguments as JSON in, its answer as JSON out — which this crate turns back into the `Tracker` on each side. A
//! plugin built against an older contract still answers: what it does not send is left at its default.

use crate::model::{Ask, Person};
use crate::tracker::{Site, Tracker, TrackerKind, Write, read_slice};
use artoo_plugin_sdk::gpui::App;
use artoo_plugin_sdk::serde::Serialize;
use artoo_plugin_sdk::serde::de::DeserializeOwned;
use artoo_plugin_sdk::serde_json::{self, Value, json};
use artoo_plugin_sdk::services;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;

/// A call to a tracker across the crossing: the method's name and `{"site": …, <its arguments>}`, the answer.
pub type Call = Arc<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>;

/// What is offered: the kind as JSON, and the call.
pub type Offered = (Value, Call);

/// The contract's major version: a kind is offered and read under it.
pub const MAJOR: u32 = 1;

const PREFIX: &str = "tasks.tracker.";

/// The key a kind is offered under: `tasks.tracker.acme@1`. A plugin offering one names it in its `Meta::provides`.
pub fn key_of(id: &str) -> String {
    format!("{PREFIX}{id}@{MAJOR}")
}

/// Offers tracker kind `kind` to Tasks, each site of it opened by `open`. Called from the plugin's `install`: it
/// reads nothing, so it runs before or after Tasks' own.
pub fn offer(
    cx: &mut App,
    kind: TrackerKind,
    open: impl Fn(Site) -> Box<dyn Tracker> + Send + Sync + 'static,
) {
    let key = key_of(&kind.id);
    services::offer(cx, &key, wire(kind, open));
}

/// Every tracker kind plugins offer, each with its call; one offered in another shape is left out, and why is said.
pub fn offered(cx: &App) -> (Vec<(TrackerKind, Call)>, Vec<String>) {
    let (mut kinds, mut why) = (Vec::new(), Vec::new());
    let ours = format!("@{MAJOR}");
    for key in services::offered(cx) {
        if !key.starts_with(PREFIX) || !key.ends_with(&ours) {
            continue;
        }
        let read = services::service::<Offered>(cx, &key).and_then(|(about, call)| {
            let kind: TrackerKind = serde_json::from_value(about).map_err(|e| e.to_string())?;
            Ok((kind, call))
        });
        match read {
            Ok(k) => kinds.push(k),
            Err(e) => why.push(format!("{key}: {e}")),
        }
    }
    (kinds, why)
}

fn answer<T: Serialize>(read: Result<T, String>) -> Result<Value, String> {
    read.and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()))
}

/// The kind and its call, as `offer` offers them: each call opens the tracker on the site it names and asks it.
pub fn wire(
    kind: TrackerKind,
    open: impl Fn(Site) -> Box<dyn Tracker> + Send + Sync + 'static,
) -> Offered {
    let about = serde_json::to_value(&kind).unwrap_or_default();
    let call = move |method: &str, ask: Value| -> Result<Value, String> {
        let site: Site = serde_json::from_value(ask["site"].clone()).map_err(|e| e.to_string())?;
        let t = open(site);
        let text = |k: &str| ask[k].as_str().unwrap_or_default().to_owned();
        let max = ask["max"].as_u64().unwrap_or(20) as usize;
        match method {
            "myself" => answer(t.myself()),
            "boards" => answer(t.boards()),
            "board_info" => answer(t.board_info(&text("board"))),
            "board_issues" => answer(t.board_issues(&text("board"))),
            "slice" => {
                let a: Ask =
                    serde_json::from_value(ask["ask"].clone()).map_err(|e| e.to_string())?;
                answer(t.slice(&text("board"), &a))
            }
            "past_sprints" => answer(t.past_sprints(&text("board"))),
            "board_url" => Ok(json!(t.board_url(&text("board")))),
            "issue" => answer(t.issue(&text("key"))),
            "find" => answer(t.find(&text("words"), max)),
            "search" => answer(t.search(&text("query"), max)),
            "news" => {
                let me: Person = serde_json::from_value(ask["me"].clone()).unwrap_or_default();
                answer(t.news(&me))
            }
            "moves" => answer(t.moves(&text("key"))),
            "edit_screen" => answer(t.edit_screen(&text("key"))),
            "assignable" => answer(t.assignable(&text("key"), &text("query"))),
            "projects" => answer(t.projects()),
            "kinds" => answer(t.kinds(&text("project"))),
            "create_screen" => answer(t.create_screen(&text("project"), &text("kind"))),
            "link_kinds" => answer(t.link_kinds()),
            "file" => t.file(&text("url")).map(|b| json!(hex(&b))),
            "attach" => t.attach(&text("key"), Path::new(&text("file"))),
            "write" => {
                let w: Write =
                    serde_json::from_value(ask["write"].clone()).map_err(|e| e.to_string())?;
                t.write(&w)
            }
            other => Err(format!("{} does not know {other}", kind.name)),
        }
    };
    (about, Arc::new(call))
}

/// A tracker another plugin offered, opened on `site`: each method a call across.
pub struct Remote {
    pub name: String,
    pub call: Call,
    pub site: Site,
}

impl Remote {
    fn ask<T: DeserializeOwned>(&self, method: &str, mut args: Value) -> Result<T, String> {
        args["site"] = serde_json::to_value(&self.site).unwrap_or_default();
        // A panic in the other plugin is its failure, not Tasks'.
        let v = catch_unwind(AssertUnwindSafe(|| (self.call)(method, args)))
            .map_err(|_| format!("{} failed on {method}", self.name))??;
        serde_json::from_value(v).map_err(|e| format!("{} answered {method} oddly: {e}", self.name))
    }

    fn host(&self) -> String {
        self.site.key().to_owned()
    }

    /// Whether `why` is the other side's build not knowing `method`: one built before the method was.
    fn unknown(&self, why: &str, method: &str) -> bool {
        why.ends_with(&format!(" does not know {method}"))
    }
}

impl Tracker for Remote {
    fn myself(&self) -> Result<Person, String> {
        self.ask("myself", json!({}))
    }

    fn boards(&self) -> Result<Vec<crate::Board>, String> {
        let mut boards: Vec<crate::Board> = self.ask("boards", json!({}))?;
        for b in &mut boards {
            b.site = self.host();
        }
        Ok(boards)
    }

    fn board_info(&self, board: &str) -> Result<crate::BoardInfo, String> {
        self.ask("board_info", json!({ "board": board }))
    }

    fn board_issues(&self, board: &str) -> Result<Vec<crate::Row>, String> {
        self.ask("board_issues", json!({ "board": board }))
    }

    fn slice(&self, board: &str, ask: &Ask) -> Result<crate::Page, String> {
        match self.ask("slice", json!({ "board": board, "ask": ask })) {
            Err(why) if self.unknown(&why, "slice") => read_slice(self, board, ask),
            read => read,
        }
    }

    fn past_sprints(&self, board: &str) -> Result<Vec<crate::Sprint>, String> {
        match self.ask("past_sprints", json!({ "board": board })) {
            Err(why) if self.unknown(&why, "past_sprints") => Ok(Vec::new()),
            read => read,
        }
    }

    fn board_url(&self, board: &str) -> String {
        self.ask("board_url", json!({ "board": board }))
            .unwrap_or_default()
    }

    fn issue(&self, key: &str) -> Result<crate::Issue, String> {
        let mut issue: crate::Issue = self.ask("issue", json!({ "key": key }))?;
        issue.site = self.host();
        Ok(issue)
    }

    fn find(&self, words: &str, max: usize) -> Result<Vec<crate::Row>, String> {
        self.ask("find", json!({ "words": words, "max": max }))
    }

    fn search(&self, query: &str, max: usize) -> Result<Vec<crate::Row>, String> {
        self.ask("search", json!({ "query": query, "max": max }))
    }

    fn news(&self, me: &Person) -> Result<Vec<crate::News>, String> {
        self.ask("news", json!({ "me": me }))
    }

    fn moves(&self, key: &str) -> Result<Vec<crate::Move>, String> {
        self.ask("moves", json!({ "key": key }))
    }

    fn edit_screen(&self, key: &str) -> Result<Vec<crate::Field>, String> {
        self.ask("edit_screen", json!({ "key": key }))
    }

    fn assignable(&self, key: &str, query: &str) -> Result<Vec<Person>, String> {
        self.ask("assignable", json!({ "key": key, "query": query }))
    }

    fn projects(&self) -> Result<Vec<(String, String)>, String> {
        self.ask("projects", json!({}))
    }

    fn kinds(&self, project: &str) -> Result<Vec<crate::Kind>, String> {
        self.ask("kinds", json!({ "project": project }))
    }

    fn create_screen(&self, project: &str, kind: &str) -> Result<Vec<crate::Field>, String> {
        self.ask("create_screen", json!({ "project": project, "kind": kind }))
    }

    fn link_kinds(&self) -> Result<Vec<(String, String, String)>, String> {
        self.ask("link_kinds", json!({}))
    }

    fn file(&self, url: &str) -> Result<Vec<u8>, String> {
        let text: String = self.ask("file", json!({ "url": url }))?;
        unhex(&text).ok_or_else(|| format!("{} sent a file oddly", self.name))
    }

    fn attach(&self, key: &str, file: &Path) -> Result<Value, String> {
        self.ask("attach", json!({ "key": key, "file": file }))
    }

    fn write(&self, w: &Write) -> Result<Value, String> {
        self.ask("write", json!({ "write": w }))
    }
}

/// Bytes as hex, as a file crosses.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Board, Caps, Row, State};
    use std::sync::Mutex;

    /// A tracker of one board and one issue, keeping what is written.
    struct Fake {
        site: Site,
        written: Arc<Mutex<Vec<Write>>>,
    }

    impl Tracker for Fake {
        fn myself(&self) -> Result<Person, String> {
            Ok(Person {
                id: "me".into(),
                name: format!("Me on {}", self.site.host()),
            })
        }
        fn boards(&self) -> Result<Vec<Board>, String> {
            Ok(vec![Board {
                id: "1".into(),
                name: "Desk".into(),
                ..Default::default()
            }])
        }
        fn board_info(&self, _: &str) -> Result<crate::BoardInfo, String> {
            Ok(Default::default())
        }
        fn board_issues(&self, _: &str) -> Result<Vec<Row>, String> {
            Ok(vec![Row {
                key: "D-1".into(),
                state: State {
                    id: "1".into(),
                    name: "Open".into(),
                    category: "new".into(),
                },
                ..Default::default()
            }])
        }
        fn board_url(&self, board: &str) -> String {
            format!("{}/b/{board}", self.site.url)
        }
        fn issue(&self, key: &str) -> Result<crate::Issue, String> {
            Err(format!("{} has no {key}", self.site.host()))
        }
        fn find(&self, words: &str, _: usize) -> Result<Vec<Row>, String> {
            Ok(vec![Row {
                summary: words.into(),
                ..Default::default()
            }])
        }
        fn moves(&self, _: &str) -> Result<Vec<crate::Move>, String> {
            Ok(Vec::new())
        }
        fn edit_screen(&self, _: &str) -> Result<Vec<crate::Field>, String> {
            Ok(Vec::new())
        }
        fn assignable(&self, _: &str, _: &str) -> Result<Vec<Person>, String> {
            Ok(Vec::new())
        }
        fn projects(&self) -> Result<Vec<(String, String)>, String> {
            Ok(vec![("1".into(), "Desk".into())])
        }
        fn kinds(&self, _: &str) -> Result<Vec<crate::Kind>, String> {
            Ok(Vec::new())
        }
        fn create_screen(&self, _: &str, _: &str) -> Result<Vec<crate::Field>, String> {
            Ok(Vec::new())
        }
        fn file(&self, _: &str) -> Result<Vec<u8>, String> {
            Ok(vec![0, 7, 255])
        }
        fn write(&self, w: &Write) -> Result<Value, String> {
            if let Write::Sprint { .. } = w {
                panic!("no sprints here");
            }
            self.written.lock().unwrap().push(w.clone());
            Ok(json!({ "key": "D-2" }))
        }
    }

    #[test]
    fn a_tracker_answers_across_the_crossing_as_itself() {
        let written = Arc::new(Mutex::new(Vec::new()));
        let kept = written.clone();
        let kind = TrackerKind {
            id: "desk".into(),
            name: "Desk".into(),
            hosts: vec!["desk.example.org".into()],
            caps: Caps {
                priority: true,
                ..Caps::NONE
            },
            ..Default::default()
        };
        let (about, call) = wire(kind.clone(), move |site| {
            Box::new(Fake {
                site,
                written: kept.clone(),
            })
        });
        assert_eq!(serde_json::from_value::<TrackerKind>(about).unwrap(), kind);
        let remote = Remote {
            name: kind.name.clone(),
            call,
            site: Site {
                url: "https://desk.example.org".into(),
                kind: "desk".into(),
                ..Default::default()
            },
        };
        assert_eq!(remote.myself().unwrap().name, "Me on desk.example.org");
        assert_eq!(remote.boards().unwrap()[0].site, "desk.example.org");
        assert_eq!(remote.board_issues("1").unwrap()[0].state.category, "new");
        assert_eq!(remote.board_url("1"), "https://desk.example.org/b/1");
        assert_eq!(
            remote.issue("D-9"),
            Err("desk.example.org has no D-9".into())
        );
        // A tracker with no query language is asked the query's words.
        assert_eq!(
            remote.search("text ~ \"refund\"", 5).unwrap()[0].summary,
            "refund"
        );
        assert_eq!(remote.news(&Person::default()), Ok(Vec::new()));
        assert_eq!(remote.file("x").unwrap(), [0, 7, 255]);
        assert!(remote.attach("D-1", Path::new("/a.png")).is_err());
        let moved = Write::moved("D-1", "2", json!({}));
        assert_eq!(remote.write(&moved), Ok(json!({ "key": "D-2" })));
        assert_eq!(*written.lock().unwrap(), [moved]);
        assert_eq!(
            remote.write(&Write::in_sprint("D-1", Some(3))),
            Err("Desk failed on write".into())
        );
    }

    #[test]
    fn a_slice_is_read_across_or_out_of_the_board_when_the_other_side_is_older() {
        let kind = TrackerKind {
            id: "desk".into(),
            name: "Desk".into(),
            ..Default::default()
        };
        let open = |site: Site| -> Box<dyn Tracker> {
            Box::new(Fake {
                site,
                written: Arc::default(),
            })
        };
        let (_, call) = wire(kind, open);
        // A build from before slices: it does not know the calls, as its wire says of any it does not know.
        let older: Call = Arc::new(move |method: &str, ask: Value| match method {
            "slice" | "past_sprints" => Err(format!("Desk does not know {method}")),
            _ => call(method, ask),
        });
        let site = Site {
            url: "https://desk.example.org".into(),
            ..Default::default()
        };
        let (_, call) = wire(
            TrackerKind {
                name: "Desk".into(),
                ..Default::default()
            },
            open,
        );
        for call in [call, older] {
            let remote = Remote {
                name: "Desk".into(),
                call,
                site: site.clone(),
            };
            let ask = crate::Ask {
                max: 50,
                ..crate::Ask::backlog()
            };
            let page = remote.slice("1", &ask).unwrap();
            assert_eq!((page.rows[0].key.as_str(), page.total), ("D-1", 1));
            assert_eq!(remote.past_sprints("1"), Ok(Vec::new()));
        }
        // Another refusal is the other side's answer, not an older build.
        let refusing: Call =
            Arc::new(|_: &str, _: Value| Err("desk.example.org refused the token".into()));
        let remote = Remote {
            name: "Desk".into(),
            call: refusing,
            site,
        };
        assert_eq!(
            remote.slice("1", &crate::Ask::backlog()),
            Err("desk.example.org refused the token".into())
        );
    }

    #[test]
    fn a_kind_is_offered_under_its_id_and_the_major() {
        assert_eq!(key_of("acme"), "tasks.tracker.acme@1");
        assert_eq!(unhex(&hex(&[1, 171, 0])), Some(vec![1, 171, 0]));
        assert_eq!(unhex("zz"), None);
    }
}
