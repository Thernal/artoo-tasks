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

/// The plugin's settings file: `<data>/plugins/artoo-tasks/trackers.txt`, a line per site, `url [email] [# label]`.
pub fn settings_file() -> Option<PathBuf> {
    Some(
        artoo_plugin_sdk::core::platform::data_dir()?
            .join("plugins")
            .join("artoo-tasks")
            .join("trackers.txt"),
    )
}

/// Every site the settings name, each with its token: `~/.config/tokens/<host>`, as the Mac keeps internal hosts'
/// tokens; a site with no token is left out, and why is said, as is one of a kind no plugin offers now. A fixture's
/// sites are its `trackers.txt`, else one example site, and take no token.
pub fn sites() -> (Vec<Site>, Vec<String>) {
    let read = match fixture() {
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
        Some(dir) => std::fs::read_to_string(dir.join("trackers.txt")).unwrap_or_default(),
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

/// The sites `text` names, a line each (`url [user] [kind=<id>] [# label]`, a line of `#` a comment), each with the token `token` gives
/// its host; a host named again is read once. A line's kind is the one it says, else the one claiming its host.
fn named(text: &str, token: impl Fn(&str) -> Option<String>) -> (Vec<Site>, Vec<String>) {
    let (mut sites, mut why) = (Vec::<Site>::new(), Vec::new());
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let (line, label) = line.split_once(" # ").unwrap_or((line, ""));
        let mut words = line.split_whitespace();
        let url = words
            .next()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_owned();
        let (kinds, users): (Vec<&str>, Vec<&str>) = words.partition(|w| w.starts_with("kind="));
        let mut site = Site {
            url,
            user: users.first().map(|u| (*u).to_owned()),
            label: label.trim().to_owned(),
            ..Default::default()
        };
        site.kind = match kinds.first() {
            Some(k) => k["kind=".len()..].to_owned(),
            None => trackers::kind_of(site.host()),
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
        // `ARTOO_TASKS_FIXTURE_SLOW=<ms>`: each read answers that late, for pictures of what draws while reading.
        if let Some(ms) = std::env::var("ARTOO_TASKS_FIXTURE_SLOW")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
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
        return match answers[site.host()].as_str() {
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
}
