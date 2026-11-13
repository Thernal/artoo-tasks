//! What a tracker is to the Tasks plugin, and what a plugin builds against to add one of its own — a company's
//! tracker beside Jira and Trello.
//!
//! A tracker plugin depends on this crate and the SDK, as Tasks does. In its `install` it offers its kind: what it is
//! called, the hosts its sites are on, what its sign-in asks, what it has (`Caps`), and how a site of it is opened as
//! a `Tracker`. Tasks reads every kind offered when its space opens; a site whose host a kind claims (Settings →
//! Accounts, or the plugin's `trackers.txt`) is read and written through it, its boards and issues drawn as Jira's.
//!
//! ```ignore
//! use artoo_plugin_sdk::{self as sdk, Meta, gpui::App};
//! use artoo_tasks_api::{self as tasks, Caps, Site, Tracker, TrackerKind};
//!
//! sdk::plugin!(
//!     Meta {
//!         id: "acme-desk",
//!         name: "Acme Desk",
//!         version: env!("CARGO_PKG_VERSION"),
//!         repository: "",
//!         provides: &["tasks.tracker.acme@1"],
//!         requires: &[],
//!     },
//!     install
//! );
//!
//! fn install(cx: &mut App) {
//!     let kind = TrackerKind {
//!         id: "acme".into(),
//!         name: "Acme Desk".into(),
//!         hosts: vec!["desk.acme.example".into()],
//!         unit: "project".into(),
//!         caps: Caps { priority: true, ..Caps::NONE },
//!         ..Default::default()
//!     };
//!     tasks::offer(cx, kind, |site: Site| Box::new(Acme { site }) as Box<dyn Tracker>);
//! }
//! ```
//!
//! The model speaks Jira's words (`model`'s first lines say which); a tracker maps its own onto them. Writes come
//! as intents (`Write`); one the tracker has no place for is refused with a sentence (`Acme Desk has no sprints`).

mod model;
mod tracker;
pub mod wiki;
mod wire;

pub use model::*;
pub use tracker::*;
pub use wire::{Call, MAJOR, Offered, Remote, key_of, offer, offered, wire};
