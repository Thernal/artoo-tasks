//! The trackers' sites and how they are reached: the sites Settings names (`trackers.txt`), each with its token, and
//! a request to one through `curl`, as Artoo reads its hosts, so the VPN's routes and the Mac's proxies apply; the
//! token goes on curl's standard input, never on its command line, and so does what is sent. Jira's reads and writes
//! are `tracker`'s; Trello's go through here too.
//!
//! Jira Server and Data Center take a personal access token (`Authorization: Bearer`); Jira Cloud an email and an
//! API token (Basic); Trello an API key and a token. Nothing is written but on the person's act.

pub mod tracker;

pub use crate::model::Site;
use crate::trackers::{self, SiteExt};
use artoo_plugin_sdk::serde_json::{self, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn plugin_folder() -> Option<PathBuf> {
    Some(
        artoo_plugin_sdk::core::platform::data_dir()?
            .join("plugins")
            .join("artoo-tasks"),
    )
}

/// The plugin's accounts file: `<data>/plugins/artoo-tasks/accounts.txt`, a line per account,
/// `url [email] [kind=<id>] [id=<key>] [# label]`. Accounts made by a build that knew one account per host live in
/// `trackers.txt`, read into this file once and left as they were, for that build.
pub fn settings_file() -> Option<PathBuf> {
    let folder = plugin_folder()?;
    carry_over(&folder);
    Some(folder.join("accounts.txt"))
}

/// `trackers.txt` of a build that knew one account per host copied to `accounts.txt` in `folder`, once: the old
/// file stays as it was for that build, and a host's first account keeps its key, so its token, its links and its
/// boards carry over unchanged.
fn carry_over(folder: &std::path::Path) {
    let (file, old) = (folder.join("accounts.txt"), folder.join("trackers.txt"));
    if !file.exists() && old.is_file() {
        // A failed copy is tried again next time.
        let _ = std::fs::copy(&old, &file);
    }
}

/// Every account the settings name, each with its token: `~/.config/tokens/<key>` (the host for the first account on
/// a site, as the Mac keeps internal hosts' tokens); a site with no token is left out, and why is said, as is one of a kind no plugin offers now. A fixture's
/// sites are its `trackers.txt`, else one example site, and take no token.
pub fn sites() -> (Vec<Site>, Vec<String>) {
    let read = match fixture() {
        Some(dir) => {
            let text = std::fs::read_to_string(fixture_sites(&dir))
                .unwrap_or_else(|_| "https://jira.example.com".into());
            named(&text, |_| Some(String::new()))
        }
        None => {
            let text = settings_file()
                .and_then(|f| std::fs::read_to_string(f).ok())
                .unwrap_or_default();
            named(&text, token_of)
        }
    };
    let (sites, mut why) = read;
    let known = trackers::kinds();
    let (ours, gone): (Vec<Site>, Vec<Site>) =
        (sites.into_iter()).partition(|s| known.iter().any(|k| k.id == s.kind));
    why.extend(gone.iter().map(|s| {
        format!(
            "{} is a tracker of `{}`, whose plugin is not installed or is off",
            s.host(),
            s.kind
        )
    }));
    (ours, why)
}

/// What the news are listed under: the trackers' name when they are all Jira or all Trello, else `Tasks`. Read as
/// the plugin installs, before other plugins offer their kinds: a site of theirs is known by the kind its line says.
pub fn trackers_named() -> &'static str {
    let text = match fixture() {
        Some(dir) => std::fs::read_to_string(fixture_sites(&dir)).unwrap_or_default(),
        None => (settings_file().and_then(|f| std::fs::read_to_string(f).ok())).unwrap_or_default(),
    };
    let (sites, _) = named(&text, |_| Some(String::new()));
    named_by(&sites)
}

fn named_by(sites: &[Site]) -> &'static str {
    if sites.iter().all(SiteExt::trello) && !sites.is_empty() {
        "Trello"
    } else if sites.iter().all(|s| s.kind == trackers::JIRA) {
        "Jira"
    } else {
        "Tasks"
    }
}

/// The accounts `text` names, each with its line, in order, no token read. A second account on a host is told apart
/// by its key: the one its line says, else `host~<its user or label>`; the same key, or without a key the same host
/// and user, named again is read once. A
/// line's kind is the one it says, else the one claiming its host.
pub fn entries(text: &str) -> Vec<(String, Site)> {
    let mut out = Vec::<(String, Site)>::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let (words, label) = line.split_once(" # ").unwrap_or((line, ""));
        let mut words = words.split_whitespace();
        let url = words
            .next()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_owned();
        let (tagged, users): (Vec<&str>, Vec<&str>) =
            words.partition(|w| w.starts_with("kind=") || w.starts_with("id="));
        let word = |tag: &str| {
            (tagged.iter())
                .find_map(|w| w.strip_prefix(tag))
                .map(str::to_owned)
        };
        let mut site = Site {
            url,
            user: users.first().map(|u| (*u).to_owned()),
            label: label.trim().to_owned(),
            id: word("id=").unwrap_or_default(),
            ..Default::default()
        };
        site.kind = word("kind=").unwrap_or_else(|| trackers::kind_of(site.host()));
        let sites: Vec<Site> = out.iter().map(|(_, s)| s.clone()).collect();
        if site.id.is_empty() {
            // A line with no key of its own that names a host and user again is the same account, read once.
            if sites
                .iter()
                .any(|s| s.host() == site.host() && s.user == site.user)
            {
                continue;
            }
            site.id = free_key(&sites, &site);
        }
        if sites.iter().all(|s| s.key() != site.key()) {
            out.push((line.to_owned(), site));
        }
    }
    out
}

/// The line of `site`: `url [user] [kind=<id>] [id=<key>] [# label]`, the kind said when it is not Jira, the key when
/// it is not the host.
pub fn line_of(site: &Site) -> String {
    let mut line = site.url.clone();
    if let Some(user) = site.user.as_deref().filter(|u| !u.is_empty()) {
        line.push_str(&format!(" {user}"));
    }
    if site.kind != trackers::JIRA {
        line.push_str(&format!(" kind={}", site.kind));
    }
    if site.key() != site.host() {
        line.push_str(&format!(" id={}", site.key()));
    }
    if !site.label.is_empty() {
        line.push_str(&format!(" # {}", site.label));
    }
    line
}

/// The accounts `text` names (`entries`), each with the token `token` gives its key; one with none is left out and
/// said.
fn named(text: &str, token: impl Fn(&str) -> Option<String>) -> (Vec<Site>, Vec<String>) {
    let (mut sites, mut why) = (Vec::<Site>::new(), Vec::new());
    for (_, mut site) in entries(text) {
        match token(site.key()) {
            Some(token) => {
                site.token = token;
                sites.push(site);
            }
            None => why.push(format!(
                "{} has no token in ~/.config/tokens/{}",
                site.url,
                site.key()
            )),
        }
    }
    (sites, why)
}

/// The key a new account on `site`'s host takes among `sites`: the host for the first, else `host~<name>`, its user
/// or label made of letters and digits, a number when that is taken or there is none.
pub fn free_key(sites: &[Site], site: &Site) -> String {
    let host = site.host();
    if !sites.iter().any(|s| s.key() == host) {
        return host.to_owned();
    }
    let name: String = (site.user.as_deref().filter(|u| !u.is_empty()))
        .unwrap_or(site.label.as_str())
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_lowercase();
    let taken = |key: &str| sites.iter().any(|s| s.key() == key);
    let named = format!("{host}~{name}");
    if !name.is_empty() && !taken(&named) {
        return named;
    }
    (2..)
        .map(|n| format!("{host}~{n}"))
        .find(|key| !taken(key))
        .unwrap_or_default()
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

/// The fixture's accounts file: `trackers.txt`, or the one `ARTOO_TASKS_FIXTURE_SITES` names (two accounts on one site).
fn fixture_sites(dir: &std::path::Path) -> PathBuf {
    dir.join(std::env::var("ARTOO_TASKS_FIXTURE_SITES").unwrap_or_else(|_| "trackers.txt".into()))
}

/// A folder of the API's answers to read instead of the tracker (`ARTOO_TASKS_FIXTURE`): pictures of the plugin
/// that show no one's real work, and tests.
pub fn fixture() -> Option<PathBuf> {
    std::env::var_os("ARTOO_TASKS_FIXTURE").map(PathBuf::from)
}

/// The fixture's folder of `site`: the account's own (`<key>/`), else its host's, else the fixture's.
fn fixture_home(dir: &std::path::Path, site: &Site) -> PathBuf {
    let own = dir.join(site.key());
    match own.is_dir() {
        true => own,
        false => dir.join(site.host()),
    }
}

/// The fixture's file for `path` on `site`: what is before its query, `/` as `_` (`_rest_api_2_issue_PAY-412.json`),
/// in the account's or the host's folder when the fixture has one.
fn fixture_file(dir: &std::path::Path, site: &Site, path: &str) -> PathBuf {
    let path = bare(path);
    let name = format!("{}.json", path.replace('/', "_"));
    let own = fixture_home(dir, site);
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
        // `ARTOO_TASKS_FIXTURE_SLOW=<ms>`: each read answers that late, for pictures of what draws while reading.
        if let Some(ms) = std::env::var("ARTOO_TASKS_FIXTURE_SLOW")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
        let own = (!purpose.is_empty())
            .then(|| fixture_file(&dir, site, &format!("{}@{purpose}", bare(path))));
        let file = own
            .filter(|f| f.is_file())
            .unwrap_or_else(|| fixture_file(&dir, site, path));
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
        let line = format!("{method} {}{path} {sent}\n", site.key());
        let log = dir.join("writes.log");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .and_then(|mut f| f.write_all(line.as_bytes()));
        let file = fixture_file(&dir, site, &format!("{method}{path}"));
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
        let own = fixture_home(&dir, site).join("files").join(name);
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
        let key = site.user.as_deref().unwrap_or_default();
        return format!(
            "OAuth oauth_consumer_key=\"{key}\", oauth_token=\"{}\"",
            site.token
        );
    }
    match &site.user {
        Some(email) => {
            let pair = format!("{email}:{}", site.token);
            format!("Basic {}", base64(pair.as_bytes()))
        }
        None => format!("Bearer {}", site.token),
    }
}

thread_local! {
    /// How long a read waits for the site, in seconds: 30, 10 for the first read of a site being connected.
    static LIMIT: std::cell::Cell<u64> = const { std::cell::Cell::new(30) };
}

/// Why connecting a site stopped: it did not answer, it refused the sign-in, or said something else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Probe {
    /// The site was not reached; what is said after `Can't reach the site ·`.
    Reach(String),
    /// The site answered and refused the email and token.
    Refused,
    Other(String),
}

/// The first read of a site being connected, who the token is, within 10 s; a fixture answers from its
/// `probes.json` (`{"<host>": "reach" | "token"}`), a site it does not name answers.
pub fn probe(site: &Site) -> Result<(), Probe> {
    if let Some(dir) = fixture() {
        let text = std::fs::read_to_string(dir.join("probes.json")).unwrap_or_default();
        let answers: Value = serde_json::from_str(&text).unwrap_or_default();
        return match answers[site.key()].as_str() {
            Some("reach") => Err(Probe::Reach("no answer in 10 s".into())),
            Some("token") => Err(Probe::Refused),
            _ => Ok(()),
        };
    }
    LIMIT.with(|l| l.set(10));
    let read = crate::model::myself(site);
    LIMIT.with(|l| l.set(30));
    read.map(|_| ()).map_err(|why| {
        let reach = format!("Can't reach {}: ", site.host());
        match why.strip_prefix(&reach) {
            Some(said) => Probe::Reach(said.to_owned()),
            None if why.ends_with(" refused the token") => Probe::Refused,
            None => Probe::Other(why),
        }
    })
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
    let limit = LIMIT.with(std::cell::Cell::get);
    let mut child = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            &limit.to_string(),
            "-w",
            "\n%{http_code}",
            "-K",
            "-",
        ])
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
    if out.status.code() == Some(28) {
        return Err(format!(
            "Can't reach {}: no answer in {limit} s",
            site.host()
        ));
    }
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
    fn a_line_keeps_its_label_and_an_older_one_has_none() {
        let text = "https://a.atlassian.net me@example.com # Acme work\nhttps://jira.example.com";
        let (sites, _) = named(text, |_| Some(String::new()));
        assert_eq!(sites[0].label, "Acme work");
        assert_eq!(sites[0].user.as_deref(), Some("me@example.com"));
        assert_eq!(sites[1].label, "");
    }

    #[test]
    fn basic_takes_base64() {
        assert_eq!(base64(b"a@b.c:tok"), "YUBiLmM6dG9r");
        assert_eq!(base64(b"ab"), "YWI=");
    }

    #[test]
    fn a_line_says_its_kind_else_its_host_does() {
        let text =
            "https://desk.acme.example kind=acme\nhttps://trello.com key\nhttps://jira.example.com";
        let (sites, _) = named(text, |_| Some(String::new()));
        let read: Vec<_> = (sites.iter())
            .map(|s| (s.host(), s.kind.as_str(), s.user.as_deref()))
            .collect();
        assert_eq!(
            read,
            [
                ("desk.acme.example", "acme", None),
                ("trello.com", "trello", Some("key")),
                ("jira.example.com", "jira", None),
            ]
        );
        assert_eq!(named_by(&sites), "Tasks");
        assert_eq!(named_by(&sites[1..2]), "Trello");
        assert_eq!(named_by(&sites[2..]), "Jira");
        assert_eq!(named_by(&[]), "Jira");
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
            .map(|s| (s.host(), s.user.as_deref(), s.token.as_str()))
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
    fn two_accounts_on_one_site_are_kept_apart_by_key() {
        let text = "https://acme.atlassian.net me@example.com\n\
                    https://acme.atlassian.net ops@example.com # Ops\n\
                    https://acme.atlassian.net ops@example.com\n\
                    https://acme.atlassian.net id=acme.atlassian.net~9 # Nine";
        let (sites, why) = named(text, |key| Some(format!("token of {key}")));
        let read: Vec<_> = (sites.iter())
            .map(|s| (s.key(), s.token.as_str()))
            .collect();
        assert_eq!(
            read,
            [
                ("acme.atlassian.net", "token of acme.atlassian.net"),
                (
                    "acme.atlassian.net~opsexamplecom",
                    "token of acme.atlassian.net~opsexamplecom"
                ),
                ("acme.atlassian.net~9", "token of acme.atlassian.net~9"),
            ]
        );
        assert!(why.is_empty());
        assert!(sites.iter().all(|s| s.host() == "acme.atlassian.net"));
    }

    #[test]
    fn a_layout_of_one_account_per_host_reads_as_it_did() {
        let text = "https://jira.example.com\nhttps://team.atlassian.net me@example.com # Team";
        let (sites, _) = named(text, |_| Some(String::new()));
        let keys: Vec<_> = sites.iter().map(Site::key).collect();
        assert_eq!(keys, ["jira.example.com", "team.atlassian.net"]);
    }

    #[test]
    fn a_line_written_reads_back_the_same_account() {
        let text = "https://acme.atlassian.net me@example.com\n\
                    https://acme.atlassian.net ops@example.com # Ops";
        let all = entries(text);
        let again: String = all.iter().map(|(_, s)| line_of(s) + "\n").collect();
        assert_eq!(
            again,
            "https://acme.atlassian.net me@example.com\n\
             https://acme.atlassian.net ops@example.com id=acme.atlassian.net~opsexamplecom # Ops\n"
        );
        let back = entries(&again);
        assert_eq!(
            back.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>(),
            all.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_second_accounts_key_holds_when_the_first_is_gone() {
        let all = entries(
            "https://acme.atlassian.net me@example.com\n\
             https://acme.atlassian.net ops@example.com",
        );
        let frozen = line_of(&all[1].1);
        let left = entries(&frozen);
        assert_eq!(left[0].1.key(), "acme.atlassian.net~opsexamplecom");
    }

    #[test]
    fn a_0_1_layout_is_carried_over_once_and_kept() {
        let dir = std::env::temp_dir().join(format!("artoo-tasks-carry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = "https://acme.atlassian.net me@example.com # Acme\nhttps://trello.com key\n";
        std::fs::write(dir.join("trackers.txt"), old).unwrap();
        carry_over(&dir);
        assert_eq!(
            std::fs::read_to_string(dir.join("accounts.txt")).unwrap(),
            old
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("trackers.txt")).unwrap(),
            old
        );
        let keys: Vec<_> = entries(old)
            .iter()
            .map(|(_, s)| s.key().to_owned())
            .collect();
        assert_eq!(keys, ["acme.atlassian.net", "trello.com"]);
        // Changed afterwards, the new file is no longer overwritten by the old one.
        std::fs::write(dir.join("accounts.txt"), "https://other.example.net\n").unwrap();
        carry_over(&dir);
        assert_eq!(
            std::fs::read_to_string(dir.join("accounts.txt")).unwrap(),
            "https://other.example.net\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fixture_account_reads_its_own_folder_else_its_hosts() {
        let dir = std::env::temp_dir().join(format!("artoo-tasks-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("acme.atlassian.net")).unwrap();
        std::fs::create_dir_all(dir.join("acme.atlassian.net~ops")).unwrap();
        let site = |id: &str| Site {
            url: "https://acme.atlassian.net".into(),
            id: id.into(),
            ..Default::default()
        };
        assert_eq!(
            fixture_home(&dir, &site("")),
            dir.join("acme.atlassian.net")
        );
        assert_eq!(
            fixture_home(&dir, &site("acme.atlassian.net~ops")),
            dir.join("acme.atlassian.net~ops")
        );
        assert_eq!(
            fixture_home(&dir, &site("acme.atlassian.net~other")),
            dir.join("acme.atlassian.net")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
