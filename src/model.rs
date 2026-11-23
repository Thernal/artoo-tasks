//! What the plugin reads of a tracker and writes to it, as the contract's plain types (`artoo_tasks_api`), each read
//! asked of the site's tracker — Jira, Trello, or one a plugin added (`crate::trackers`); and what the pages make of
//! them: a description's paragraphs, an issue as an agent reads it, a tracker's times.

use crate::trackers;
use crate::wiki;
use artoo_plugin_sdk::core::markdown;
use artoo_plugin_sdk::serde_json::Value;

pub use artoo_tasks_api::{
    Attachment, Board, BoardId, BoardInfo, Brief, Caps, Column, Comment, Epic, Field, Issue, Kind,
    Link, Move, News, Person, Row, Site, Sprint, State, Told, Tracker, Write,
};

/// The tracker `site` is on, opened on it.
pub fn tracker(site: &Site) -> Box<dyn Tracker> {
    trackers::open(site)
}

/// Board `id`'s page on `site`.
pub fn board_url(site: &Site, id: &str) -> String {
    tracker(site).board_url(id)
}

/// Who the token is.
pub fn myself(site: &Site) -> Result<Person, String> {
    tracker(site).myself()
}

/// The boards of `site`.
pub fn boards(site: &Site) -> Result<Vec<Board>, String> {
    let mut boards = tracker(site).boards()?;
    for b in &mut boards {
        b.site = site.key().to_owned();
    }
    Ok(boards)
}

/// Board `id`'s columns, epics and sprints; a board without epics or sprints (kanban) has none, not a failure.
pub fn board_info(site: &Site, id: &str) -> Result<BoardInfo, String> {
    tracker(site).board_info(id)
}

/// Board `id`'s issues, newest change first.
pub fn board_issues(site: &Site, id: &str) -> Result<Vec<Row>, String> {
    tracker(site).board_issues(id)
}

/// Issue `key` whole.
pub fn read_issue(site: &Site, key: &str) -> Result<Issue, String> {
    let mut issue = tracker(site).issue(key)?;
    issue.site = site.key().to_owned();
    Ok(issue)
}

/// Issue `key` from the first of `sites` that has it: the others answer that they have no such issue.
pub fn read_issue_on(sites: &[Site], key: &str) -> Result<Issue, String> {
    let mut why = Vec::new();
    for site in sites {
        match read_issue(site, key) {
            Ok(issue) => return Ok(issue),
            Err(e) => why.push((e.contains(" has no "), e)),
        }
    }
    match why.iter().find(|(none, _)| !none) {
        Some((_, e)) => Err(e.clone()),
        None if sites.is_empty() => Err("No tracker is connected".into()),
        None => Err(format!("No tracker has {key}")),
    }
}

/// The issues a query in the tracker's language finds (JQL on Jira), at most `max`.
pub fn search(site: &Site, query: &str, max: usize) -> Result<Vec<Row>, String> {
    tracker(site).search(query, max)
}

/// The issues `words` find, by key or words, at most `max`: a link's picker.
pub fn find(site: &Site, words: &str, max: usize) -> Result<Vec<Row>, String> {
    tracker(site).find(words, max)
}

/// What others did lately on the issues `me` is on, the newest first.
pub fn news(site: &Site, me: &Person) -> Result<Vec<News>, String> {
    tracker(site).news(me)
}

/// The moves issue `key`'s workflow allows from its state, each with the fields it asks.
pub fn moves(site: &Site, key: &str) -> Result<Vec<Move>, String> {
    tracker(site).moves(key)
}

/// The fields issue `key`'s edit screen holds, in the tracker's order.
pub fn edit_screen(site: &Site, key: &str) -> Result<Vec<Field>, String> {
    tracker(site).edit_screen(key)
}

/// The people issue `key` may be assigned to, those `query` finds.
pub fn assignable(site: &Site, key: &str, query: &str) -> Result<Vec<Person>, String> {
    tracker(site).assignable(key, query)
}

/// The kinds of issue project `project` makes.
pub fn kinds(site: &Site, project: &str) -> Result<Vec<Kind>, String> {
    tracker(site).kinds(project)
}

/// The fields project `project`'s create screen asks for issues of kind `kind`.
pub fn create_screen(site: &Site, project: &str, kind: &str) -> Result<Vec<Field>, String> {
    tracker(site).create_screen(project, kind)
}

/// The projects the token reaches: `(key, name)`.
pub fn projects(site: &Site) -> Result<Vec<(String, String)>, String> {
    tracker(site).projects()
}

/// The kinds of link: `(name, inward, outward)`, `("Blocks", "is blocked by", "blocks")`.
pub fn link_kinds(site: &Site) -> Result<Vec<(String, String, String)>, String> {
    tracker(site).link_kinds()
}

/// An attachment's bytes, read with the site's token: for an image drawn where it stands.
pub fn attachment_bytes(site: &Site, url: &str) -> Result<Vec<u8>, String> {
    tracker(site).file(url)
}

/// A file attached to issue `key`.
pub fn attach(site: &Site, key: &str, file: &std::path::Path) -> Result<Value, String> {
    tracker(site).attach(key, file)
}

/// A change sent to `site`: the tracker's answer, or why it refused.
pub fn write(site: &Site, w: &Write) -> Result<Value, String> {
    tracker(site).write(w)
}

/// A description's paragraphs, each as markdown: its blocks — a paragraph, a heading, a list, a code block — as
/// markdown reads them from Jira's wiki text.
pub fn paragraphs(text: &str) -> Vec<String> {
    let md = wiki::to_markdown(text);
    let lines: Vec<&str> = md.lines().collect();
    (markdown::parse_lines(&md).into_iter())
        .filter_map(|(_, (first, last))| lines.get(first as usize - 1..last as usize))
        .map(|lines| lines.join("\n"))
        .filter(|p| !p.trim().is_empty())
        .collect()
}

/// An issue as an agent reads it: its facts, its description, its sub-tasks, links and comments.
pub fn digest(issue: &Issue) -> String {
    let r = &issue.row;
    let mut out = format!("# {} {}\n\n{} · {}", r.key, r.summary, r.kind, r.state.name);
    if let Some(a) = &r.assignee {
        out.push_str(&format!(" · assigned to {}", a.name));
    }
    if let Some(p) = &r.priority {
        out.push_str(&format!(" · {p} priority"));
    }
    if let Some(e) = &r.epic {
        out.push_str(&format!(" · epic {} {}", e.key, e.name));
    }
    if let Some(s) = &r.sprint {
        out.push_str(&format!(" · {}", s.name));
    }
    if !r.labels.is_empty() {
        out.push_str(&format!(" · labels {}", r.labels.join(", ")));
    }
    out.push_str(&format!("\n{}\n", issue.url));
    if let Some(parent) = &r.parent {
        out.push_str(&format!("A sub-task of {parent}.\n"));
    }
    if !issue.description.trim().is_empty() {
        out.push_str(&format!(
            "\n## Description\n\n{}\n",
            wiki::to_markdown(issue.description.trim())
        ));
    }
    if !issue.subtasks.is_empty() {
        out.push_str("\n## Sub-tasks\n\n");
        for s in &issue.subtasks {
            out.push_str(&format!("- {} {} ({})\n", s.key, s.summary, s.state.name));
        }
    }
    if !issue.links.is_empty() {
        out.push_str("\n## Linked issues\n\n");
        for l in &issue.links {
            out.push_str(&format!(
                "- {} {} {} ({})\n",
                l.relation, l.other.key, l.other.summary, l.other.state.name
            ));
        }
    }
    if !issue.comments.is_empty() {
        out.push_str("\n## Comments\n");
        for c in &issue.comments {
            let who = c.author.as_ref().map_or("Someone", |p| p.name.as_str());
            out.push_str(&format!(
                "\n{who} ({}):\n{}\n",
                day(&c.created),
                wiki::to_markdown(c.body.trim())
            ));
        }
    }
    out
}

/// `2026-11-10T09:12:00.000+0400` as `2026-11-10`.
pub fn day(stamp: &str) -> &str {
    stamp.get(..10).unwrap_or(stamp)
}

/// Days from 1970-01-01 to `y-m-d` (Howard Hinnant's `days_from_civil`).
fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A tracker's time — `2026-11-10T09:12:00.000+0400`, `…Z`, or a day alone `2026-11-14` — as Unix seconds.
pub fn stamp(text: &str) -> Option<i64> {
    let num = |a: usize, b: usize| text.get(a..b)?.parse::<i64>().ok();
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let mut secs = days(y, m, d) * 86_400;
    if text.len() >= 19 {
        secs += num(11, 13)? * 3600 + num(14, 16)? * 60 + num(17, 19)?;
        // The offset after the seconds and their fraction: `+0400`, `+04:00`, `Z`.
        let rest = &text[19..];
        let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
        if let Some(sign) = rest.chars().next().filter(|c| *c == '+' || *c == '-') {
            let digits: String = rest[1..].chars().filter(char::is_ascii_digit).collect();
            let (h, mi) = (
                digits.get(..2)?.parse::<i64>().ok()?,
                digits
                    .get(2..4)
                    .and_then(|x| x.parse::<i64>().ok())
                    .unwrap_or(0),
            );
            let off = h * 3600 + mi * 60;
            secs -= if sign == '+' { off } else { -off };
        }
    }
    Some(secs)
}

/// A day as the board says it: `Fri 14 Nov`.
pub fn day_words(text: &str) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let num = |a: usize, b: usize| text.get(a..b).and_then(|x| x.parse::<i64>().ok());
    match (num(0, 4), num(5, 7), num(8, 10)) {
        (Some(y), Some(m), Some(d)) if (1..=12).contains(&m) => {
            let week = (days(y, m, d) + 4).rem_euclid(7) as usize;
            format!("{} {d} {}", DAYS[week], MONTHS[m as usize - 1])
        }
        _ => text.to_owned(),
    }
}

/// How long ago a tracker's time was, as the kit says it (`3 h ago`); the time itself when it cannot be read.
pub fn ago(text: &str) -> String {
    match stamp(text) {
        Some(at) => artoo_plugin_sdk::kit::format::ago(at),
        None => day(text).to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trackers_time_reads_as_unix_seconds_and_words() {
        assert_eq!(stamp("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(stamp("1970-01-01T04:00:00.000+0400"), Some(0));
        assert_eq!(stamp("1970-01-02"), Some(86_400));
        assert_eq!(stamp("2026-11-14T17:00:00.000Z"), Some(1_794_675_600));
        assert_eq!(day_words("2026-11-14T17:00:00.000Z"), "Sat 14 Nov");
        assert_eq!(day_words("2026-11-13"), "Fri 13 Nov");
    }
}
