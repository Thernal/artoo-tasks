//! Jira, read: its boards, a board's issues, an issue whole, a search. Through `curl`, as Artoo reads its hosts, so
//! the VPN's routes and the Mac's proxies apply; the token goes on curl's standard input, never on its command line.
//!
//! Jira Server and Data Center take a personal access token (`Authorization: Bearer`); Jira Cloud an email and an
//! API token (Basic). Only `GET`: the first stage reads.

use artoo_plugin_sdk::serde_json::{self, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// A tracker the plugin reads: its site, and how it signs in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    /// `https://jira.example.com`, no slash at the end.
    pub url: String,
    /// Jira Cloud's sign-in: the email the API token is of. None: a personal access token (Server, Data Center).
    pub email: Option<String>,
    pub token: String,
}

impl Site {
    /// Its host: `jira.example.com`.
    pub fn host(&self) -> &str {
        let rest = self.url.split_once("://").map_or(self.url.as_str(), |(_, r)| r);
        rest.split('/').next().unwrap_or(rest)
    }
}

/// The plugin's settings file: `<data>/plugins/artoo-tasks/trackers.txt`, a line per site, `url [email]`.
pub fn settings_file() -> Option<PathBuf> {
    Some(
        artoo_plugin_sdk::core::platform::data_dir()?
            .join("plugins")
            .join("artoo-tasks")
            .join("trackers.txt"),
    )
}

/// Every site the settings name, each with its token: `~/.config/tokens/<host>`, as the Mac keeps internal hosts'
/// tokens; a site with no token is left out, and why is said. A fixture's sites are its `trackers.txt`, else one
/// example site, and take no token.
pub fn sites() -> (Vec<Site>, Vec<String>) {
    match fixture() {
        Some(dir) => {
            let text = std::fs::read_to_string(dir.join("trackers.txt")).unwrap_or_else(|_| "https://jira.example.com".into());
            named(&text, |_| Some(String::new()))
        }
        None => {
            let text = settings_file().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
            named(&text, token_of)
        }
    }
}

/// The sites `text` names, a line each (`url [email]`, `#` a comment), each with the token `token` gives its host; a
/// host named again is read once.
fn named(text: &str, token: impl Fn(&str) -> Option<String>) -> (Vec<Site>, Vec<String>) {
    let (mut sites, mut why) = (Vec::<Site>::new(), Vec::new());
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let mut words = line.split_whitespace();
        let url = words.next().unwrap_or_default().trim_end_matches('/').to_owned();
        let email = words.next().map(str::to_owned);
        let mut site = Site { url, email, token: String::new() };
        if sites.iter().any(|s| s.host() == site.host()) {
            continue;
        }
        match token(site.host()) {
            Some(token) => {
                site.token = token;
                sites.push(site);
            }
            None => why.push(format!("{} has no token in ~/.config/tokens/{}", site.url, site.host())),
        }
    }
    (sites, why)
}

fn token_of(host: &str) -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let file = PathBuf::from(home).join(".config").join("tokens").join(host);
    let token = std::fs::read_to_string(file).ok()?.trim().to_owned();
    (!token.is_empty()).then_some(token)
}

/// A value inside a curl config file's double quotes.
fn quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A folder of the API's answers to read instead of the tracker (`ARTOO_TASKS_FIXTURE`): pictures of the plugin
/// that show no one's real work, and tests.
pub fn fixture() -> Option<PathBuf> {
    std::env::var_os("ARTOO_TASKS_FIXTURE").map(PathBuf::from)
}

/// The fixture's file for `path` on `host`: what is before its query, `/` as `_` (`_rest_api_2_issue_PAY-412.json`),
/// in the host's folder when the fixture has one.
fn fixture_file(dir: &std::path::Path, host: &str, path: &str) -> PathBuf {
    let path = path.split('?').next().unwrap_or(path);
    let name = format!("{}.json", path.replace('/', "_"));
    let own = dir.join(host);
    if own.is_dir() { own.join(name) } else { dir.join(name) }
}

/// `GET` of `path` (`/rest/api/2/myself`) on `site`, as JSON; why not, in a sentence.
pub fn get(site: &Site, path: &str) -> Result<Value, String> {
    if let Some(dir) = fixture() {
        let file = fixture_file(&dir, site.host(), path);
        let text = std::fs::read_to_string(&file)
            .map_err(|_| format!("{} has no {}", site.host(), path.split('?').next().unwrap_or(path)))?;
        return serde_json::from_str(&text).map_err(|e| e.to_string());
    }
    let auth = match &site.email {
        Some(email) => {
            let pair = format!("{email}:{}", site.token);
            format!("Basic {}", base64(pair.as_bytes()))
        }
        None => format!("Bearer {}", site.token),
    };
    let config = format!(
        "url = \"{}{}\"\nheader = \"Authorization: {}\"\nheader = \"Accept: application/json\"\n",
        quoted(&site.url),
        quoted(path),
        quoted(&auth)
    );
    let mut child = Command::new("curl")
        .args(["-sS", "--max-time", "30", "-w", "\n%{http_code}", "-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "curl is not installed".to_owned())?;
    child
        .stdin
        .take()
        .ok_or("curl took no input")?
        .write_all(config.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        return Err(format!("Can't reach {}: {}", site.host(), said.trim().trim_start_matches("curl: ")));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, code) = text.rsplit_once('\n').unwrap_or((&text, "0"));
    match code.trim() {
        "200" => serde_json::from_str(body).map_err(|_| format!("{} answered what is not JSON", site.host())),
        "401" => Err(format!("{} refused the token", site.host())),
        "403" => Err(format!("{} does not let the token read this", site.host())),
        "404" => Err(format!("{} has no {}", site.host(), path.split('?').next().unwrap_or(path))),
        code => Err(format!("{} answered {code}", site.host())),
    }
}

/// `bytes` in base64, for Basic's header.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `text` for a URL's query.
pub fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_takes_base64() {
        assert_eq!(base64(b"a@b.c:tok"), "YUBiLmM6dG9r");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn every_site_named_is_read_once_each_with_its_token() {
        let text = "https://jira.example.com\n# a comment\nhttps://team.atlassian.net me@example.com\n\
                    https://jira.example.com/\nhttps://old.example.net";
        let (sites, why) = named(text, |host| (host != "old.example.net").then(|| format!("token of {host}")));
        let read: Vec<_> = sites.iter().map(|s| (s.host(), s.email.as_deref(), s.token.as_str())).collect();
        assert_eq!(
            read,
            [
                ("jira.example.com", None, "token of jira.example.com"),
                ("team.atlassian.net", Some("me@example.com"), "token of team.atlassian.net"),
            ]
        );
        assert_eq!(why, ["https://old.example.net has no token in ~/.config/tokens/old.example.net"]);
    }

    #[test]
    fn a_site_is_its_host() {
        let site = Site { url: "https://jira.example.com/jira".into(), email: None, token: String::new() };
        assert_eq!(site.host(), "jira.example.com");
    }
}
