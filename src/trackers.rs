//! The kinds of tracker the plugin reads: Jira and Trello, its own, and every kind another plugin offers
//! (`artoo_tasks_api::offer`), read when the store is made and when the sites change. A site is of the kind that
//! claims its host, Jira's when none does; it is read and written through that kind's `Tracker`.

use crate::jira::Site;
use crate::jira::tracker::Jira;
use crate::trello::Trello;
use artoo_plugin_sdk::gpui::App;
use artoo_tasks_api::{Call, Caps, Remote, Tracker, TrackerKind};
use std::sync::RwLock;

pub const JIRA: &str = "jira";
pub const TRELLO: &str = "trello";

/// The kinds other plugins offer, as last read: the trackers' reads run off the window's thread, where the app's
/// services are not.
static OFFERED: RwLock<Vec<(TrackerKind, Call)>> = RwLock::new(Vec::new());

/// Whether a Jira site on `host` is Atlassian's Cloud, whose token is read with an email; any other host's is a
/// server's, read with a personal access token alone, an email sent with it refused.
pub fn cloud_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host.ends_with(".atlassian.net") || host.ends_with(".jira.com")
}

/// Jira: Cloud (an email and an API token) or Server and Data Center (a personal access token), on any host.
pub fn jira() -> TrackerKind {
    TrackerKind {
        id: JIRA.into(),
        name: "Jira".into(),
        hosts: Vec::new(),
        // Asked on a Cloud host alone (`cloud_host`).
        user: Some(("Email".into(), "The email the API token is of".into())),
        // Cloud's page or the profile's, by the site: `accounts`.
        token_page: None,
        unit: "project".into(),
        caps: Caps::ALL,
    }
}

/// Trello: an API key and a token; its boards are the projects, its cards the issues.
pub fn trello() -> TrackerKind {
    TrackerKind {
        id: TRELLO.into(),
        name: "Trello".into(),
        hosts: vec!["trello.com".into()],
        user: Some(("API key".into(), "Your Trello API key".into())),
        token_page: Some((
            "Make them in Trello's Power-Up admin".into(),
            "https://trello.com/power-ups/admin".into(),
        )),
        unit: "board".into(),
        caps: Caps {
            attachments: true,
            ..Caps::NONE
        },
    }
}

/// Reads the kinds other plugins offer; one offered in another shape is left out and said on the log.
pub fn read_offered(cx: &App) {
    let (kinds, why) = artoo_tasks_api::offered(cx);
    for why in why {
        eprintln!("artoo-tasks: a tracker is offered in a shape this plugin does not read: {why}");
    }
    if let Ok(mut offered) = OFFERED.write() {
        *offered = kinds;
    }
}

/// Every kind, in the order a host is claimed: Trello, those offered, then Jira, which takes any host.
pub fn kinds() -> Vec<TrackerKind> {
    let mut out = vec![trello()];
    if let Ok(offered) = OFFERED.read() {
        out.extend(offered.iter().map(|(k, _)| k.clone()));
    }
    out.push(jira());
    out
}

/// Kind `id`; Jira's when there is none of it.
pub fn kind(id: &str) -> TrackerKind {
    (kinds().into_iter())
        .find(|k| k.id == id)
        .unwrap_or_else(jira)
}

/// The kind a site on `host` is of: the first that claims it, else Jira.
pub fn kind_of(host: &str) -> String {
    (kinds().into_iter())
        .find(|k| k.claims(host))
        .map_or_else(|| JIRA.to_owned(), |k| k.id)
}

/// The tracker `site` is on, opened on it. A site of a kind no plugin offers any more is read as Jira.
pub fn open(site: &Site) -> Box<dyn Tracker> {
    match site.kind.as_str() {
        TRELLO => Box::new(Trello(site.clone())),
        JIRA | "" => Box::new(Jira(site.clone())),
        id => {
            let offered = (OFFERED.read().ok()).and_then(|o| {
                (o.iter())
                    .find(|(k, _)| k.id == id)
                    .map(|(k, call)| (k.name.clone(), call.clone()))
            });
            match offered {
                Some((name, call)) => Box::new(Remote {
                    name,
                    call,
                    site: site.clone(),
                }),
                None => Box::new(Jira(site.clone())),
            }
        }
    }
}

/// What the plugin asks of a site beside its contract's fields.
pub trait SiteExt {
    /// Trello's: `https://trello.com`, its API key in `user`.
    fn trello(&self) -> bool;
    /// Jira Cloud's: signed in with an email and an API token, people by account id.
    fn cloud(&self) -> bool;
    /// What its tracker is called: `Jira`, `Trello`, `Acme Desk`.
    fn tracker(&self) -> String;
    /// What its tracker has.
    fn caps(&self) -> Caps;
}

impl SiteExt for Site {
    fn trello(&self) -> bool {
        self.kind == TRELLO
    }

    fn cloud(&self) -> bool {
        matches!(self.kind.as_str(), JIRA | "") && self.user.is_some()
    }

    fn tracker(&self) -> String {
        kind(&self.kind).name
    }

    fn caps(&self) -> Caps {
        kind(&self.kind).caps
    }
}

/// The tracker an address is on, as a button names it: `Open in Trello`.
pub fn tracker_of(url: &str) -> String {
    let site = Site {
        url: url.to_owned(),
        ..Default::default()
    };
    kind(&kind_of(site.host())).name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_trellos_or_else_jiras() {
        assert_eq!(kind_of("trello.com"), TRELLO);
        assert_eq!(kind_of("acme.atlassian.net"), JIRA);
        assert_eq!(tracker_of("https://trello.com/c/hP4sK7vL"), "Trello");
        assert_eq!(
            tracker_of("https://jira.example.com/browse/PAY-412"),
            "Jira"
        );
        let site = Site {
            url: "https://acme.atlassian.net".into(),
            user: Some("me@example.com".into()),
            kind: JIRA.into(),
            ..Default::default()
        };
        assert!(site.cloud() && !site.trello() && site.caps() == Caps::ALL);
    }

    #[test]
    fn a_cloud_host_is_atlassians() {
        assert!(cloud_host("acme.atlassian.net") && cloud_host("Acme.Atlassian.NET"));
        assert!(cloud_host("acme.jira.com"));
        assert!(!cloud_host("jira.example.com") && !cloud_host("atlassian.net.example.com"));
    }
}
