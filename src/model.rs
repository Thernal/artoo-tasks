//! What the plugin reads of a tracker, as plain types: a board, an issue in a list, an issue whole.

use crate::jira::{self, Site};
use crate::wiki;
use artoo_plugin_sdk::core::markdown;
use artoo_plugin_sdk::serde_json::Value;

/// A board as the plugin keys it: its site's host and its id there — two sites may each have a board 7.
pub type BoardId = (String, u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub id: u64,
    pub name: String,
    /// The site it is on (its host).
    pub site: String,
}

impl Board {
    pub fn key(&self) -> BoardId {
        (self.site.clone(), self.id)
    }
}

/// An issue as a list shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub key: String,
    pub summary: String,
    pub kind: String,
    pub state: String,
    /// The state's category: `new`, `indeterminate`, `done`.
    pub category: String,
    pub assignee: Option<String>,
    pub priority: Option<String>,
    pub updated: String,
    pub parent: Option<String>,
}

/// An issue whole, as its page shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Issue {
    pub row: Row,
    /// The site it is on (its host).
    pub site: String,
    pub description: String,
    pub reporter: Option<String>,
    pub labels: Vec<String>,
    pub created: String,
    pub subtasks: Vec<Row>,
    pub comments: Vec<Comment>,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comment {
    pub author: String,
    pub created: String,
    pub body: String,
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn name(v: &Value) -> Option<String> {
    v["displayName"].as_str().or(v["name"].as_str()).map(str::to_owned)
}

/// A list's issue from the API's `issue` object.
pub fn row(v: &Value) -> Row {
    let f = &v["fields"];
    Row {
        key: text(&v["key"]),
        summary: text(&f["summary"]),
        kind: text(&f["issuetype"]["name"]),
        state: text(&f["status"]["name"]),
        category: text(&f["status"]["statusCategory"]["key"]),
        assignee: name(&f["assignee"]),
        priority: name(&f["priority"]),
        updated: text(&f["updated"]),
        parent: f["parent"]["key"].as_str().map(str::to_owned),
    }
}

/// The issue whole from the API's `issue` object.
pub fn issue(site: &Site, v: &Value) -> Issue {
    let f = &v["fields"];
    let head = self::row(v);
    Issue {
        url: format!("{}/browse/{}", site.url, head.key),
        site: site.host().to_owned(),
        row: head,
        description: text(&f["description"]),
        reporter: name(&f["reporter"]),
        labels: (f["labels"].as_array().into_iter().flatten())
            .filter_map(|l| l.as_str().map(str::to_owned))
            .collect(),
        created: text(&f["created"]),
        subtasks: (f["subtasks"].as_array().into_iter().flatten()).map(self::row).collect(),
        comments: (f["comment"]["comments"].as_array().into_iter().flatten())
            .map(|c| Comment {
                author: name(&c["author"]).unwrap_or_default(),
                created: text(&c["created"]),
                body: text(&c["body"]),
            })
            .collect(),
    }
}

/// Board `id`'s page on `site`: Jira Server's address, which Jira Cloud sends on to its own.
pub fn board_url(site: &Site, id: u64) -> String {
    format!("{}/secure/RapidBoard.jspa?rapidView={id}", site.url)
}

const ROW_FIELDS: &str = "summary,issuetype,status,assignee,priority,updated,parent";

/// The boards of `site`.
pub fn boards(site: &Site) -> Result<Vec<Board>, String> {
    let v = jira::get(site, "/rest/agile/1.0/board?maxResults=100")?;
    Ok((v["values"].as_array().into_iter().flatten())
        .map(|b| Board {
            id: b["id"].as_u64().unwrap_or_default(),
            name: text(&b["name"]),
            site: site.host().to_owned(),
        })
        .collect())
}

/// Board `id`'s issues not done, newest change first, then the done ones; `mine` those assigned to you.
pub fn board_issues(site: &Site, id: u64, mine: bool) -> Result<Vec<Row>, String> {
    let jql = if mine {
        "assignee = currentUser() ORDER BY updated DESC"
    } else {
        "ORDER BY updated DESC"
    };
    let v = jira::get(
        site,
        &format!(
            "/rest/agile/1.0/board/{id}/issue?maxResults=100&fields={ROW_FIELDS}&jql={}",
            jira::encode(jql)
        ),
    )?;
    Ok(v["issues"].as_array().into_iter().flatten().map(row).collect())
}

/// Issue `key` whole.
pub fn read_issue(site: &Site, key: &str) -> Result<Issue, String> {
    let v = jira::get(
        site,
        &format!(
            "/rest/api/2/issue/{}?fields={ROW_FIELDS},description,reporter,labels,created,subtasks,comment",
            jira::encode(key)
        ),
    )?;
    Ok(issue(site, &v))
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

/// The issues `jql` finds, at most `max`.
pub fn search(site: &Site, jql: &str, max: usize) -> Result<Vec<Row>, String> {
    let v = jira::get(
        site,
        &format!(
            "/rest/api/2/search?maxResults={max}&fields={ROW_FIELDS}&jql={}",
            jira::encode(jql)
        ),
    )?;
    Ok(v["issues"].as_array().into_iter().flatten().map(row).collect())
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

/// An issue as an agent reads it: its facts, its description, its sub-tasks and comments.
pub fn digest(issue: &Issue) -> String {
    let r = &issue.row;
    let mut out = format!("# {} {}\n\n{} · {}", r.key, r.summary, r.kind, r.state);
    if let Some(a) = &r.assignee {
        out.push_str(&format!(" · assigned to {a}"));
    }
    if let Some(p) = &r.priority {
        out.push_str(&format!(" · {p} priority"));
    }
    if !issue.labels.is_empty() {
        out.push_str(&format!(" · labels {}", issue.labels.join(", ")));
    }
    out.push_str(&format!("\n{}\n", issue.url));
    if let Some(parent) = &r.parent {
        out.push_str(&format!("A sub-task of {parent}.\n"));
    }
    if !issue.description.trim().is_empty() {
        out.push_str(&format!("\n## Description\n\n{}\n", wiki::to_markdown(issue.description.trim())));
    }
    if !issue.subtasks.is_empty() {
        out.push_str("\n## Sub-tasks\n\n");
        for s in &issue.subtasks {
            out.push_str(&format!("- {} {} ({})\n", s.key, s.summary, s.state));
        }
    }
    if !issue.comments.is_empty() {
        out.push_str("\n## Comments\n");
        for c in &issue.comments {
            out.push_str(&format!("\n{} ({}):\n{}\n", c.author, day(&c.created), wiki::to_markdown(c.body.trim())));
        }
    }
    out
}

/// `2026-11-10T09:12:00.000+0400` as `2026-11-10`.
pub fn day(stamp: &str) -> &str {
    stamp.get(..10).unwrap_or(stamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_plugin_sdk::serde_json::json;

    #[test]
    fn an_issue_reads_from_the_api() {
        let site = Site { url: "https://jira.example.com".into(), email: None, token: String::new() };
        let v = json!({"key": "PAY-412", "fields": {
            "summary": "Refund rounds down on partial captures",
            "issuetype": {"name": "Bug"}, "status": {"name": "In Progress", "statusCategory": {"key": "indeterminate"}},
            "assignee": {"displayName": "Juniper Hale"}, "priority": {"name": "High"},
            "description": "A partial capture refunds less.\r\n\r\nDone when it equals.",
            "labels": ["refunds"], "subtasks": [{"key": "PAY-413", "fields": {"summary": "Reproduce", "status": {"name": "Done"}}}],
            "comment": {"comments": [{"author": {"displayName": "Ember Lind"}, "created": "2026-11-10T09:12:00.000+0400", "body": "Seen twice."}]}
        }});
        let issue = issue(&site, &v);
        assert_eq!(issue.url, "https://jira.example.com/browse/PAY-412");
        assert_eq!(paragraphs(&issue.description), ["A partial capture refunds less.", "Done when it equals."]);
        let text = digest(&issue);
        assert!(text.starts_with("# PAY-412 Refund rounds down on partial captures\n\nBug · In Progress · assigned to Juniper Hale · High priority · labels refunds"));
        assert!(text.contains("- PAY-413 Reproduce (Done)"));
        assert!(text.contains("Ember Lind (2026-11-10):\nSeen twice."));
    }
}
