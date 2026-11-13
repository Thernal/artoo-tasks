//! Jira, read and written: its boards, a board's issues, an issue whole, a search; a move, an edit, a comment, an
//! issue made. Through `curl`, as Artoo reads its hosts, so the VPN's routes and the Mac's proxies apply; the token
//! goes on curl's standard input, never on its command line, and so does what is sent.
//!
//! Jira Server and Data Center take a personal access token (`Authorization: Bearer`); Jira Cloud an email and an
//! API token (Basic). Nothing is written but on the person's act.

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
        let rest = self
            .url
            .split_once("://")
            .map_or(self.url.as_str(), |(_, r)| r);
        rest.split('/').next().unwrap_or(rest)
    }

    /// Trello, not Jira: `https://trello.com` with its API key where Jira Cloud has the email.
    pub fn trello(&self) -> bool {
        self.host() == "trello.com"
    }

    /// Jira Cloud: signed in with an email and an API token, people by account id.
    pub fn cloud(&self) -> bool {
        self.email.is_some() && !self.trello()
    }

    /// What its tracker is called: `Jira`, `Trello`.
    pub fn tracker(&self) -> &'static str {
        if self.trello() { "Trello" } else { "Jira" }
    }
}

/// The tracker an address is on, as a button names it: `Open in Trello`.
pub fn tracker_of(url: &str) -> &'static str {
    if url.starts_with("https://trello.com/") {
        "Trello"
    } else {
        "Jira"
    }
}

/// What the news are listed under: the trackers' name — `Jira`, `Trello`, or `Tasks` when there are both.
pub fn trackers_named() -> &'static str {
    let text = match fixture() {
        Some(dir) => std::fs::read_to_string(dir.join("trackers.txt")).unwrap_or_default(),
        None => (settings_file().and_then(|f| std::fs::read_to_string(f).ok())).unwrap_or_default(),
    };
    let (sites, _) = named(&text, |_| Some(String::new()));
    match (
        sites.iter().any(Site::trello),
        sites.iter().any(|s| !s.trello()),
    ) {
        (true, true) => "Tasks",
        (true, false) => "Trello",
        _ => "Jira",
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
            let text = std::fs::read_to_string(dir.join("trackers.txt"))
                .unwrap_or_else(|_| "https://jira.example.com".into());
            named(&text, |_| Some(String::new()))
        }
        None => {
            let text = settings_file()
                .and_then(|f| std::fs::read_to_string(f).ok())
                .unwrap_or_default();
            named(&text, token_of)
        }
    }
}

/// The sites `text` names, a line each (`url [email]`, `#` a comment), each with the token `token` gives its host; a
/// host named again is read once.
fn named(text: &str, token: impl Fn(&str) -> Option<String>) -> (Vec<Site>, Vec<String>) {
    let (mut sites, mut why) = (Vec::<Site>::new(), Vec::new());
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let url = words
            .next()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_owned();
        let email = words.next().map(str::to_owned);
        let mut site = Site {
            url,
            email,
            token: String::new(),
        };
        if sites.iter().any(|s| s.host() == site.host()) {
            continue;
        }
        match token(site.host()) {
            Some(token) => {
                site.token = token;
                sites.push(site);
            }
            None => why.push(format!(
                "{} has no token in ~/.config/tokens/{}",
                site.url,
                site.host()
            )),
        }
    }
    (sites, why)
}

fn token_of(host: &str) -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let file = PathBuf::from(home)
        .join(".config")
        .join("tokens")
        .join(host);
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
    let path = bare(path);
    let name = format!("{}.json", path.replace('/', "_"));
    let own = dir.join(host);
    if own.is_dir() {
        own.join(name)
    } else {
        dir.join(name)
    }
}

/// `GET` of `path` (`/rest/api/2/myself`) on `site`, as JSON; why not, in a sentence.
pub fn get(site: &Site, path: &str) -> Result<Value, String> {
    get_for(site, path, "")
}

/// `get`, for `purpose` (`news`): a fixture answers it from `<path>@<purpose>.json` when it has one, so two reads of
/// one address with other queries (JQL) can answer apart.
pub fn get_for(site: &Site, path: &str, purpose: &str) -> Result<Value, String> {
    if let Some(dir) = fixture() {
        let own = (!purpose.is_empty())
            .then(|| fixture_file(&dir, site.host(), &format!("{}@{purpose}", bare(path))));
        let file = own
            .filter(|f| f.is_file())
            .unwrap_or_else(|| fixture_file(&dir, site.host(), path));
        let text = std::fs::read_to_string(&file)
            .map_err(|_| format!("{} has no {}", site.host(), bare(path)))?;
        return serde_json::from_str(&text).map_err(|e| e.to_string());
    }
    call(site, "GET", path, None)
}

/// `method` (`POST`, `PUT`, `DELETE`) of `path` with `body` as JSON; the answer, `null` when there is none (204).
/// A refusal says the tracker's reason (`PAY-415 is still open`). A fixture records it in its `writes.log` and answers
/// from `<METHOD><path>.json` when it has one: `{"answers": [{"when": "<in the body>", "status": 400,
/// "errorMessages": […]}, …]}` refuses or answers by what was sent; else `{}`.
pub fn send(site: &Site, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    if let Some(dir) = fixture() {
        let sent = body.map(Value::to_string).unwrap_or_default();
        let line = format!("{method} {}{path} {sent}\n", site.host());
        let log = dir.join("writes.log");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .and_then(|mut f| f.write_all(line.as_bytes()));
        let file = fixture_file(&dir, site.host(), &format!("{method}{path}"));
        let Ok(text) = std::fs::read_to_string(file) else {
            return Ok(Value::Object(Default::default()));
        };
        let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let Some(answers) = v["answers"].as_array() else {
            return Ok(v);
        };
        let Some(answer) = answers
            .iter()
            .find(|a| a["when"].as_str().is_some_and(|w| sent.contains(w)))
        else {
            return Ok(Value::Object(Default::default()));
        };
        return match answer["status"].as_u64() {
            Some(code) if code >= 400 => Err(refusal(site, code, answer)),
            _ => Ok(answer["body"].clone()),
        };
    }
    call(site, method, path, body)
}

/// A file sent as an attachment to `path` (`/rest/api/2/issue/PAY-412/attachments`): multipart, with the header
/// Jira asks for it. A fixture records it.
pub fn attach(site: &Site, path: &str, file: &std::path::Path) -> Result<Value, String> {
    if fixture().is_some() {
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return send(
            site,
            "POST",
            path,
            Some(&serde_json::json!({ "file": name })),
        );
    }
    let config = format!(
        "url = \"{}{}\"\nheader = \"Authorization: {}\"\nheader = \"Accept: application/json\"\nheader = \"X-Atlassian-Token: no-check\"\nform = \"file=@{}\"\n",
        // Trello's API is its own host.
        quoted(if site.trello() {
            "https://api.trello.com"
        } else {
            &site.url
        }),
        quoted(path),
        quoted(&auth(site)),
        quoted(&file.to_string_lossy())
    );
    run(site, path, config)
}

/// The bytes at `url` on `site` (an attachment's content), read with its token. A fixture's are its `files/<name>`,
/// the address's last part.
pub fn bytes(site: &Site, url: &str) -> Result<Vec<u8>, String> {
    if let Some(dir) = fixture() {
        let name = bare(url).rsplit('/').next().unwrap_or_default();
        let own = dir.join(site.host()).join("files").join(name);
        return std::fs::read(&own).map_err(|_| format!("{} has no {name}", site.host()));
    }
    let config = format!(
        "url = \"{}\"\nheader = \"Authorization: {}\"\nlocation\nfail\n",
        quoted(url),
        quoted(&auth(site))
    );
    let mut child = Command::new("curl")
        .args(["-sS", "--max-time", "60", "-K", "-"])
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
        return Err(format!(
            "{} did not give the file: {}",
            site.host(),
            said.trim().trim_start_matches("curl: ")
        ));
    }
    Ok(out.stdout)
}

/// The path without its query: what a fixture's file and a missing page are named by.
fn bare(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

fn auth(site: &Site) -> String {
    if site.trello() {
        let key = site.email.as_deref().unwrap_or_default();
        return format!(
            "OAuth oauth_consumer_key=\"{key}\", oauth_token=\"{}\"",
            site.token
        );
    }
    match &site.email {
        Some(email) => {
            let pair = format!("{email}:{}", site.token);
            format!("Basic {}", base64(pair.as_bytes()))
        }
        None => format!("Bearer {}", site.token),
    }
}

/// `method` of `path` on the tracker itself.
fn call(site: &Site, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
    let mut config = format!(
        "url = \"{}{}\"\nrequest = \"{method}\"\nheader = \"Authorization: {}\"\nheader = \"Accept: application/json\"\n",
        // Trello's API is its own host.
        quoted(if site.trello() {
            "https://api.trello.com"
        } else {
            &site.url
        }),
        quoted(path),
        quoted(&auth(site))
    );
    if let Some(body) = body {
        config.push_str("header = \"Content-Type: application/json\"\n");
        config.push_str(&format!(
            "data-binary = \"{}\"\n",
            quoted(&body.to_string())
        ));
    }
    run(site, path, config)
}

/// curl with `config` on its standard input; the answer as JSON, `null` when empty.
fn run(site: &Site, path: &str, config: String) -> Result<Value, String> {
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
        return Err(format!(
            "Can't reach {}: {}",
            site.host(),
            said.trim().trim_start_matches("curl: ")
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, code) = text.rsplit_once('\n').unwrap_or((&text, "0"));
    let code: u64 = code.trim().parse().unwrap_or(0);
    let parsed = || {
        serde_json::from_str::<Value>(body)
            .map_err(|_| format!("{} answered what is not JSON", site.host()))
    };
    match code {
        200..=299 if body.trim().is_empty() => Ok(Value::Null),
        200..=299 => parsed(),
        401 => Err(format!("{} refused the token", site.host())),
        404 => Err(format!("{} has no {}", site.host(), bare(path))),
        code => Err(refusal(
            site,
            code,
            &serde_json::from_str(body).unwrap_or_default(),
        )),
    }
}

/// What a refusal says: Jira's messages and its fields' (`errorMessages`, `errors`), else its status.
fn refusal(site: &Site, code: u64, v: &Value) -> String {
    let mut said: Vec<String> = (v["errorMessages"].as_array().into_iter().flatten())
        .filter_map(|m| m.as_str().map(str::to_owned))
        .collect();
    if let Some(fields) = v["errors"].as_object() {
        said.extend(
            fields
                .values()
                .filter_map(|m| m.as_str().map(str::to_owned)),
        );
    }
    match (said.is_empty(), code) {
        (false, _) => said.join(" · "),
        (true, 403) => format!("{} does not let the token do this", site.host()),
        (true, code) => format!("{} answered {code}", site.host()),
    }
}

/// `bytes` in base64, for Basic's header.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
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
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
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
        let (sites, why) = named(text, |host| {
            (host != "old.example.net").then(|| format!("token of {host}"))
        });
        let read: Vec<_> = sites
            .iter()
            .map(|s| (s.host(), s.email.as_deref(), s.token.as_str()))
            .collect();
        assert_eq!(
            read,
            [
                ("jira.example.com", None, "token of jira.example.com"),
                (
                    "team.atlassian.net",
                    Some("me@example.com"),
                    "token of team.atlassian.net"
                ),
            ]
        );
        assert_eq!(
            why,
            ["https://old.example.net has no token in ~/.config/tokens/old.example.net"]
        );
    }

    #[test]
    fn a_site_is_its_host() {
        let site = Site {
            url: "https://jira.example.com/jira".into(),
            email: None,
            token: String::new(),
        };
        assert_eq!(site.host(), "jira.example.com");
    }
}
