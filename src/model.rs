//! What the plugin reads of a tracker, as plain types — a board and its columns, an issue in a list, an issue whole,
//! the moves its workflow allows, the fields its edit screen holds — and what it writes, each change a request
//! (`Write`) a test reads.

use crate::jira::{self, Site};
use crate::wiki;
use artoo_plugin_sdk::core::markdown;
use artoo_plugin_sdk::serde_json::{Value, json};

/// A board as the plugin keys it: its site's host and its id there — two sites may each have a board 7.
pub type BoardId = (String, u64);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Board {
    pub id: u64,
    pub name: String,
    /// The site it is on (its host).
    pub site: String,
    /// Its project's key (`PAY`), when the board is a project's.
    pub project: Option<String>,
}

impl Board {
    pub fn key(&self) -> BoardId {
        (self.site.clone(), self.id)
    }
}

/// Someone on the tracker: Jira Cloud's account id, or Server's user name, and the name shown.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Person {
    pub id: String,
    pub name: String,
}

impl Person {
    /// The first name: `Juniper` of `Juniper Hale`.
    pub fn first(&self) -> &str {
        self.name.split_whitespace().next().unwrap_or(&self.name)
    }
}

/// A workflow's state: its id, its name, its category (`new`, `indeterminate`, `done`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub id: String,
    pub name: String,
    pub category: String,
}

impl State {
    pub fn done(&self) -> bool {
        self.category == "done"
    }
}

/// An epic: its key, its name, its colour on the tracker's palette (1–14; 0 none said) and whether it is done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Epic {
    pub key: String,
    pub name: String,
    pub color: u8,
    pub done: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sprint {
    pub id: u64,
    pub name: String,
    /// `active`, `future`, `closed`.
    pub state: String,
    /// When it ends: `2026-11-14T17:00:00.000Z`.
    pub end: Option<String>,
}

/// Another issue, as a link or a sub-task's parent names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Brief {
    pub key: String,
    pub summary: String,
    pub kind: String,
    pub state: State,
}

/// A link to another issue: how this one relates to it (`is blocked by`), the link's type and id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    pub id: String,
    /// The type's name: `Blocks`.
    pub kind: String,
    /// The relation read from this issue: `is blocked by`, `blocks`, `relates to`.
    pub relation: String,
    pub other: Brief,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub mime: String,
    pub author: Option<Person>,
    pub created: String,
    /// Where its bytes are, read with the site's token.
    pub content: String,
}

impl Attachment {
    pub fn image(&self) -> bool {
        self.mime.starts_with("image/")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub author: Option<Person>,
    pub created: String,
    pub updated: String,
    pub body: String,
}

/// An issue as a list shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub key: String,
    pub summary: String,
    pub kind: String,
    /// A sub-task's type.
    pub subtask: bool,
    pub state: State,
    pub assignee: Option<Person>,
    pub priority: Option<String>,
    pub updated: String,
    /// A sub-task's parent issue (not an epic).
    pub parent: Option<String>,
    pub epic: Option<Epic>,
    pub sprint: Option<Sprint>,
    /// Its sub-tasks: how many done, of how many.
    pub subtasks: (usize, usize),
    pub comments: usize,
    /// The open issues it is blocked by.
    pub blocked_by: Vec<Brief>,
    pub labels: Vec<String>,
    /// Its project's key: `PAY`.
    pub project: String,
}

/// An issue whole, as its page shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Issue {
    pub row: Row,
    /// The site it is on (its host).
    pub site: String,
    pub description: String,
    pub reporter: Option<Person>,
    pub created: String,
    pub due: Option<String>,
    pub subtasks: Vec<Row>,
    pub comments: Vec<Comment>,
    pub links: Vec<Link>,
    pub attachments: Vec<Attachment>,
    pub url: String,
    /// Every field as the tracker sent it: what Details shows by the edit screen's fields.
    pub fields: Value,
}

/// A move the workflow allows from an issue's state, and the fields it asks for (Done's resolution).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Move {
    pub id: String,
    pub name: String,
    pub to: State,
    pub asks: Vec<Field>,
}

/// A field of an edit or create screen: its id, its name, whether it must be filled, its kind and the values it
/// takes when they are a list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Field {
    pub id: String,
    pub name: String,
    pub required: bool,
    /// The schema's type: `string`, `array`, `priority`, `user`, `date`, `number`, `option`, `version`, `component`.
    pub kind: String,
    /// An array's items' type: `string` (labels), `component`, `version`, `option`, `json` (sprint).
    pub items: String,
    /// The values it takes, `(id, name)`; empty when any.
    pub values: Vec<(String, String)>,
    /// The value the screen fills in when none is picked, `(id, name)`: a priority's `Medium`.
    pub default: Option<(String, String)>,
}

impl Field {
    /// Many values (labels, components, fix versions) or one.
    pub fn many(&self) -> bool {
        self.kind == "array"
    }
}

/// A board's columns, each with the states it holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub states: Vec<String>,
}

/// What a board says beside its issues: its columns, its epics, its sprints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardInfo {
    pub columns: Vec<Column>,
    pub epics: Vec<Epic>,
    pub sprints: Vec<Sprint>,
}

impl BoardInfo {
    /// The sprint going on.
    pub fn active(&self) -> Option<&Sprint> {
        self.sprints.iter().find(|s| s.state == "active")
    }

    /// The column state `id` is in, by its place; the last when none holds it.
    pub fn column_of(&self, state: &State) -> usize {
        (self.columns.iter())
            .position(|c| c.states.contains(&state.id))
            .unwrap_or(self.columns.len().saturating_sub(1))
    }
}

/// A type an issue is made as, in a project.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kind {
    pub id: String,
    pub name: String,
    pub subtask: bool,
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

/// Someone from the API's user object.
pub fn person(v: &Value) -> Option<Person> {
    let name = v["displayName"].as_str().or(v["name"].as_str())?;
    let id = v["accountId"]
        .as_str()
        .or(v["name"].as_str())
        .or(v["key"].as_str())
        .unwrap_or(name);
    Some(Person {
        id: id.to_owned(),
        name: name.to_owned(),
    })
}

fn state(v: &Value) -> State {
    State {
        id: text(&v["id"]),
        name: text(&v["name"]),
        category: text(&v["statusCategory"]["key"]),
    }
}

fn brief(v: &Value) -> Brief {
    let f = &v["fields"];
    Brief {
        key: text(&v["key"]),
        summary: text(&f["summary"]),
        kind: text(&f["issuetype"]["name"]),
        state: state(&f["status"]),
    }
}

/// `color_4` as 4.
fn color(v: &Value) -> u8 {
    let key = v["color"]["key"]
        .as_str()
        .or(v["color"].as_str())
        .unwrap_or_default();
    key.trim_start_matches("color_").parse().unwrap_or(0)
}

/// An epic from the Agile API's `epic` object.
fn epic(v: &Value) -> Option<Epic> {
    let key = v["key"].as_str()?;
    Some(Epic {
        key: key.to_owned(),
        name: v["name"]
            .as_str()
            .or(v["summary"].as_str())
            .unwrap_or(key)
            .to_owned(),
        color: color(v),
        done: v["done"].as_bool().unwrap_or(false),
    })
}

fn sprint(v: &Value) -> Option<Sprint> {
    Some(Sprint {
        id: v["id"].as_u64()?,
        name: text(&v["name"]),
        state: text(&v["state"]),
        end: v["endDate"].as_str().map(str::to_owned),
    })
}

/// A list's issue from the API's `issue` object: the Agile API's (`epic`, `sprint`) or the platform's.
pub fn row(v: &Value) -> Row {
    let f = &v["fields"];
    let parent = &f["parent"];
    let parent_is_epic = parent["fields"]["issuetype"]["name"] == "Epic"
        || parent["fields"]["issuetype"]["hierarchyLevel"].as_i64() == Some(1);
    let epic = epic(&f["epic"]).or_else(|| {
        parent_is_epic.then(|| Epic {
            key: text(&parent["key"]),
            name: text(&parent["fields"]["summary"]),
            color: 0,
            done: parent["fields"]["status"]["statusCategory"]["key"] == "done",
        })
    });
    let sprint = sprint(&f["sprint"]).or_else(|| {
        (f["customfield_10020"].as_array().into_iter().flatten())
            .filter_map(sprint)
            .find(|s| s.state != "closed")
    });
    let subs: Vec<&Value> = f["subtasks"].as_array().into_iter().flatten().collect();
    let done = subs
        .iter()
        .filter(|s| s["fields"]["status"]["statusCategory"]["key"] == "done")
        .count();
    let blocked_by = (f["issuelinks"].as_array().into_iter().flatten())
        .filter(|l| l["type"]["name"] == "Blocks" && l["inwardIssue"].is_object())
        .map(|l| brief(&l["inwardIssue"]))
        .filter(|b| !b.state.done())
        .collect();
    let key = text(&v["key"]);
    Row {
        project: f["project"]["key"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                key.split_once('-')
                    .map(|(p, _)| p.to_owned())
                    .unwrap_or_default()
            }),
        key,
        summary: text(&f["summary"]),
        kind: text(&f["issuetype"]["name"]),
        subtask: f["issuetype"]["subtask"].as_bool().unwrap_or(false),
        state: state(&f["status"]),
        assignee: person(&f["assignee"]),
        priority: f["priority"]["name"].as_str().map(str::to_owned),
        updated: text(&f["updated"]),
        parent: (!parent_is_epic)
            .then(|| parent["key"].as_str().map(str::to_owned))
            .flatten(),
        epic,
        sprint,
        subtasks: (done, subs.len()),
        comments: f["comment"]["total"]
            .as_u64()
            .map(|n| n as usize)
            .unwrap_or_else(|| f["comment"]["comments"].as_array().map_or(0, Vec::len)),
        blocked_by,
        labels: (f["labels"].as_array().into_iter().flatten())
            .filter_map(|l| l.as_str().map(str::to_owned))
            .collect(),
    }
}

/// The issue whole from the API's `issue` object.
pub fn issue(site: &Site, v: &Value) -> Issue {
    let f = &v["fields"];
    let head = self::row(v);
    let links = (f["issuelinks"].as_array().into_iter().flatten())
        .filter_map(|l| {
            let (other, relation) = if l["inwardIssue"].is_object() {
                (&l["inwardIssue"], &l["type"]["inward"])
            } else {
                (&l["outwardIssue"], &l["type"]["outward"])
            };
            other.is_object().then(|| Link {
                id: text(&l["id"]),
                kind: text(&l["type"]["name"]),
                relation: text(relation),
                other: brief(other),
            })
        })
        .collect();
    Issue {
        url: format!("{}/browse/{}", site.url, head.key),
        site: site.host().to_owned(),
        description: text(&f["description"]),
        reporter: person(&f["reporter"]),
        created: text(&f["created"]),
        due: f["duedate"].as_str().map(str::to_owned),
        subtasks: (f["subtasks"].as_array().into_iter().flatten())
            .map(self::row)
            .collect(),
        comments: (f["comment"]["comments"].as_array().into_iter().flatten())
            .map(comment)
            .collect(),
        links,
        attachments: (f["attachment"].as_array().into_iter().flatten())
            .map(|a| Attachment {
                id: text(&a["id"]),
                name: text(&a["filename"]),
                size: a["size"].as_u64().unwrap_or(0),
                mime: text(&a["mimeType"]),
                author: person(&a["author"]),
                created: text(&a["created"]),
                content: text(&a["content"]),
            })
            .collect(),
        fields: f.clone(),
        row: head,
    }
}

pub fn comment(c: &Value) -> Comment {
    Comment {
        id: text(&c["id"]),
        author: person(&c["author"]),
        created: text(&c["created"]),
        updated: text(&c["updated"]),
        body: text(&c["body"]),
    }
}

/// Board `id`'s page on `site`: Jira Server's address, which Jira Cloud sends on to its own.
pub fn board_url(site: &Site, id: u64) -> String {
    format!("{}/secure/RapidBoard.jspa?rapidView={id}", site.url)
}

const ROW_FIELDS: &str = "summary,issuetype,status,assignee,priority,updated,parent,subtasks,comment,issuelinks,labels,project,epic,sprint,customfield_10020";

/// Who the token is.
pub fn myself(site: &Site) -> Result<Person, String> {
    let v = jira::get(site, "/rest/api/2/myself")?;
    person(&v).ok_or_else(|| format!("{} did not say who the token is", site.host()))
}

/// The boards of `site`.
pub fn boards(site: &Site) -> Result<Vec<Board>, String> {
    let v = jira::get(site, "/rest/agile/1.0/board?maxResults=100")?;
    Ok((v["values"].as_array().into_iter().flatten())
        .map(|b| Board {
            id: b["id"].as_u64().unwrap_or_default(),
            name: text(&b["name"]),
            site: site.host().to_owned(),
            project: b["location"]["projectKey"].as_str().map(str::to_owned),
        })
        .collect())
}

/// Board `id`'s columns, epics and sprints; a board without epics or sprints (kanban) has none, not a failure.
pub fn board_info(site: &Site, id: u64) -> Result<BoardInfo, String> {
    let config = jira::get(site, &format!("/rest/agile/1.0/board/{id}/configuration"))?;
    let columns = (config["columnConfig"]["columns"]
        .as_array()
        .into_iter()
        .flatten())
    .map(|c| Column {
        name: text(&c["name"]),
        states: (c["statuses"].as_array().into_iter().flatten())
            .map(|s| text(&s["id"]))
            .collect(),
    })
    .filter(|c| !c.states.is_empty())
    .collect();
    let list = |path: String| -> Vec<Value> {
        jira::get(site, &path)
            .ok()
            .and_then(|v| v["values"].as_array().cloned())
            .unwrap_or_default()
    };
    let epics = list(format!(
        "/rest/agile/1.0/board/{id}/epic?done=false&maxResults=100"
    ))
    .iter()
    .filter_map(epic)
    .collect();
    let sprints = list(format!(
        "/rest/agile/1.0/board/{id}/sprint?state=active,future&maxResults=50"
    ))
    .iter()
    .filter_map(sprint)
    .collect();
    Ok(BoardInfo {
        columns,
        epics,
        sprints,
    })
}

/// Board `id`'s issues, newest change first.
pub fn board_issues(site: &Site, id: u64) -> Result<Vec<Row>, String> {
    let v = jira::get(
        site,
        &format!(
            "/rest/agile/1.0/board/{id}/issue?maxResults=200&fields={ROW_FIELDS}&jql={}",
            jira::encode("ORDER BY updated DESC")
        ),
    )?;
    Ok(v["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .map(row)
        .collect())
}

/// Issue `key` whole, through the Agile API, which adds its epic and sprint on Server and Cloud alike.
pub fn read_issue(site: &Site, key: &str) -> Result<Issue, String> {
    let v = jira::get(
        site,
        &format!("/rest/agile/1.0/issue/{}", jira::encode(key)),
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
    search_for(site, jql, max, "")
}

/// `search` for `purpose` (`find`), as `jira::get_for` reads it.
pub fn search_for(site: &Site, jql: &str, max: usize, purpose: &str) -> Result<Vec<Row>, String> {
    let v = jira::get_for(
        site,
        &format!(
            "/rest/api/2/search?maxResults={max}&fields={ROW_FIELDS}&jql={}",
            jira::encode(jql)
        ),
        purpose,
    )?;
    Ok(v["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .map(row)
        .collect())
}

/// What news is about: an issue assigned to you, you mentioned, an issue of yours changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Told {
    Assigned,
    Mentioned,
    Changed,
}

/// Something someone else did lately on an issue you are on.
#[derive(Clone, Debug, PartialEq)]
pub struct News {
    pub told: Told,
    /// The change's or the comment's id: the same news, the same id.
    pub id: String,
    pub row: Row,
    /// Who did it.
    pub who: String,
    /// What they did, them first: `Juniper Hale assigned it to you`.
    pub says: String,
    /// The start of what they wrote; empty: nothing written.
    pub text: String,
    /// When, Unix seconds.
    pub at: i64,
    /// The assignee before, when that changed.
    pub was: Option<String>,
}

/// What others did lately on the issues `me` is on: assigned, mentioned, commented, moved. The newest first; one
/// news an issue and a kind, the latest.
pub fn news(site: &Site, me: &Person) -> Result<Vec<News>, String> {
    let jql = "(assignee = currentUser() OR reporter = currentUser() OR watcher = currentUser() \
               OR comment ~ currentUser()) AND updated >= -7d ORDER BY updated DESC";
    let v = jira::get_for(
        site,
        &format!(
            "/rest/api/2/search?maxResults=30&expand=changelog&fields={ROW_FIELDS},reporter&jql={}",
            jira::encode(jql)
        ),
        "news",
    )?;
    Ok(news_of(&v, me, site.email.is_some()))
}

/// The news a search with changelogs (`v`) holds for `me`, on Jira Cloud (`cloud`) or Server.
fn news_of(v: &Value, me: &Person, cloud: bool) -> Vec<News> {
    let mention = if cloud {
        format!("[~accountid:{}]", me.id)
    } else {
        format!("[~{}]", me.id)
    };
    let mut out: Vec<News> = Vec::new();
    for i in v["issues"].as_array().into_iter().flatten() {
        let row = row(i);
        let comments = i["fields"]["comment"]["comments"].as_array();
        let people: Vec<Person> = (row.assignee.iter().cloned())
            .chain(person(&i["fields"]["reporter"]))
            .chain(
                comments
                    .into_iter()
                    .flatten()
                    .filter_map(|c| person(&c["author"])),
            )
            .chain(std::iter::once(me.clone()))
            .collect();
        let mine = row.assignee.as_ref().is_some_and(|a| a.id == me.id)
            || person(&i["fields"]["reporter"]).is_some_and(|r| r.id == me.id);
        let mut found: Vec<News> = Vec::new();
        for h in i["changelog"]["histories"].as_array().into_iter().flatten() {
            let Some(who) = person(&h["author"]).filter(|w| w.id != me.id) else {
                continue;
            };
            let at = stamp(h["created"].as_str().unwrap_or_default()).unwrap_or_default();
            for item in h["items"].as_array().into_iter().flatten() {
                let field = item["fieldId"]
                    .as_str()
                    .or(item["field"].as_str())
                    .unwrap_or_default();
                let to = item["toString"].as_str().unwrap_or_default();
                let (told, says, was) = match field {
                    "assignee" if item["to"].as_str() == Some(me.id.as_str()) => (
                        Told::Assigned,
                        format!("{} assigned it to you", who.name),
                        item["fromString"].as_str().map(str::to_owned),
                    ),
                    "status" if mine => (
                        Told::Changed,
                        format!("{} moved it to {to}", who.name),
                        None,
                    ),
                    "assignee" if mine => (
                        Told::Changed,
                        if to.is_empty() {
                            format!("{} unassigned it", who.name)
                        } else {
                            format!("{} assigned it to {to}", who.name)
                        },
                        None,
                    ),
                    _ => continue,
                };
                found.push(News {
                    told,
                    id: text(&h["id"]),
                    row: row.clone(),
                    who: who.name.clone(),
                    says,
                    text: String::new(),
                    at,
                    was,
                });
            }
        }
        for c in i["fields"]["comment"]["comments"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let c = comment(c);
            let Some(who) = c.author.filter(|w| w.id != me.id) else {
                continue;
            };
            let told = if c.body.contains(&mention) {
                Told::Mentioned
            } else if mine {
                Told::Changed
            } else {
                continue;
            };
            let says = match told {
                Told::Mentioned => format!("{} mentioned you", who.name),
                _ => format!("{} commented", who.name),
            };
            // Its mentions by name, of the people the issue knows: `@Avery.Quinn`.
            let named = wiki::mentions_as(&wiki::to_markdown(c.body.trim()), |n| {
                (people.iter())
                    .find(|p| {
                        let id = n.strip_prefix("accountid:").unwrap_or(n);
                        p.id == id
                    })
                    .map(|p| wiki::handle(&p.name))
            });
            let words: String = named.chars().take(160).collect();
            found.push(News {
                told,
                id: c.id.clone(),
                row: row.clone(),
                who: who.name.clone(),
                says,
                text: words,
                at: stamp(&c.created).unwrap_or_default(),
                was: None,
            });
        }
        // The latest of each kind on this issue.
        found.sort_by_key(|n| std::cmp::Reverse(n.at));
        for n in found {
            if !out
                .iter()
                .any(|o| o.row.key == n.row.key && o.told == n.told)
            {
                out.push(n);
            }
        }
    }
    out.sort_by_key(|n| std::cmp::Reverse(n.at));
    out
}

/// The moves issue `key`'s workflow allows from its state, each with the fields it asks.
pub fn moves(site: &Site, key: &str) -> Result<Vec<Move>, String> {
    let v = jira::get(
        site,
        &format!(
            "/rest/api/2/issue/{}/transitions?expand=transitions.fields",
            jira::encode(key)
        ),
    )?;
    Ok((v["transitions"].as_array().into_iter().flatten())
        .map(|t| Move {
            id: text(&t["id"]),
            name: text(&t["name"]),
            to: state(&t["to"]),
            asks: fields(&t["fields"])
                .into_iter()
                .filter(|f| f.required)
                .collect(),
        })
        .collect())
}

/// A screen's fields from the API's map of them (`editmeta`'s `fields`, a move's).
fn fields(map: &Value) -> Vec<Field> {
    (map.as_object().into_iter().flatten())
        .map(|(id, f)| field(id, f))
        .collect()
}

fn field(id: &str, f: &Value) -> Field {
    Field {
        id: id.to_owned(),
        name: text(&f["name"]),
        required: f["required"].as_bool().unwrap_or(false),
        kind: text(&f["schema"]["type"]),
        items: text(&f["schema"]["items"]),
        values: (f["allowedValues"].as_array().into_iter().flatten())
            .map(|a| {
                let name = a["name"]
                    .as_str()
                    .or(a["value"].as_str())
                    .unwrap_or_default();
                (text(&a["id"]), name.to_owned())
            })
            .collect(),
        default: (f["defaultValue"].is_object()).then(|| {
            let d = &f["defaultValue"];
            let name = d["name"]
                .as_str()
                .or(d["value"].as_str())
                .unwrap_or_default();
            (text(&d["id"]), name.to_owned())
        }),
    }
}

/// The fields issue `key`'s edit screen holds, in the tracker's order.
pub fn edit_screen(site: &Site, key: &str) -> Result<Vec<Field>, String> {
    let v = jira::get(
        site,
        &format!("/rest/api/2/issue/{}/editmeta", jira::encode(key)),
    )?;
    Ok(fields(&v["fields"]))
}

/// The people issue `key` may be assigned to, those `query` finds; the fixture's and the tracker's order.
pub fn assignable(site: &Site, key: &str, query: &str) -> Result<Vec<Person>, String> {
    let by = if site.email.is_some() {
        "query"
    } else {
        "username"
    };
    let v = jira::get(
        site,
        &format!(
            "/rest/api/2/user/assignable/search?issueKey={}&{by}={}&maxResults=50",
            jira::encode(key),
            jira::encode(query)
        ),
    )?;
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .filter_map(person)
        .collect())
}

/// The kinds of issue project `project` makes.
pub fn kinds(site: &Site, project: &str) -> Result<Vec<Kind>, String> {
    let v = jira::get(
        site,
        &format!(
            "/rest/api/2/issue/createmeta/{}/issuetypes",
            jira::encode(project)
        ),
    )?;
    let list = v["values"].as_array().or(v["issueTypes"].as_array());
    Ok((list.into_iter().flatten())
        .map(|t| Kind {
            id: text(&t["id"]),
            name: text(&t["name"]),
            subtask: t["subtask"].as_bool().unwrap_or(false),
        })
        .collect())
}

/// The fields project `project`'s create screen asks for issues of kind `kind`.
pub fn create_screen(site: &Site, project: &str, kind: &str) -> Result<Vec<Field>, String> {
    let v = jira::get(
        site,
        &format!(
            "/rest/api/2/issue/createmeta/{}/issuetypes/{kind}",
            jira::encode(project)
        ),
    )?;
    let list = v["values"].as_array().or(v["fields"].as_array());
    Ok((list.into_iter().flatten())
        .map(|f| {
            field(
                f["fieldId"]
                    .as_str()
                    .or(f["key"].as_str())
                    .unwrap_or_default(),
                f,
            )
        })
        .collect())
}

/// The projects the token reaches.
pub fn projects(site: &Site) -> Result<Vec<(String, String)>, String> {
    let v = jira::get(site, "/rest/api/2/project")?;
    Ok((v.as_array().into_iter().flatten())
        .map(|p| (text(&p["key"]), text(&p["name"])))
        .collect())
}

/// The kinds of link: `(name, inward, outward)`, `("Blocks", "is blocked by", "blocks")`.
pub fn link_kinds(site: &Site) -> Result<Vec<(String, String, String)>, String> {
    let v = jira::get(site, "/rest/api/2/issueLinkType")?;
    Ok((v["issueLinkTypes"].as_array().into_iter().flatten())
        .map(|t| (text(&t["name"]), text(&t["inward"]), text(&t["outward"])))
        .collect())
}

/// An attachment's bytes, read with the site's token: for an image drawn where it stands.
pub fn attachment_bytes(site: &Site, url: &str) -> Result<Vec<u8>, String> {
    jira::bytes(site, url)
}

/// A change to the tracker as a request: its method, its path and what it sends.
#[derive(Clone, Debug, PartialEq)]
pub struct Write {
    pub method: &'static str,
    pub path: String,
    pub body: Option<Value>,
}

impl Write {
    fn new(method: &'static str, path: String, body: Value) -> Self {
        Self {
            method,
            path,
            body: Some(body),
        }
    }

    /// Sent to `site`: the tracker's answer, or why it refused.
    pub fn send(&self, site: &Site) -> Result<Value, String> {
        jira::send(site, self.method, &self.path, self.body.as_ref())
    }

    /// Issue `key` moved by `step` (a move's id), with the fields it asked for (`{"resolution": {"name": "Done"}}`).
    pub fn moved(key: &str, step: &str, asked: Value) -> Self {
        let mut body = json!({ "transition": { "id": step } });
        if asked.as_object().is_some_and(|o| !o.is_empty()) {
            body["fields"] = asked;
        }
        Self::new("POST", format!("/rest/api/2/issue/{key}/transitions"), body)
    }

    /// Issue `key` assigned to `who`, or to no one: by account id on Cloud, by user name on Server.
    pub fn assigned(key: &str, who: Option<&Person>, cloud: bool) -> Self {
        let id = who
            .map(|p| Value::String(p.id.clone()))
            .unwrap_or(Value::Null);
        let body = if cloud {
            json!({ "accountId": id })
        } else {
            json!({ "name": id })
        };
        Self::new("PUT", format!("/rest/api/2/issue/{key}/assignee"), body)
    }

    /// Issue `key`'s fields set: `{"summary": "…"}`, `{"priority": {"name": "High"}}`.
    pub fn edited(key: &str, fields: Value) -> Self {
        Self::new(
            "PUT",
            format!("/rest/api/2/issue/{key}"),
            json!({ "fields": fields }),
        )
    }

    /// A value added to or taken from issue `key`'s field of many (`labels`, `components` by name).
    pub fn changed_many(key: &str, field: &str, value: Value, add: bool) -> Self {
        let op = if add { "add" } else { "remove" };
        Self::new(
            "PUT",
            format!("/rest/api/2/issue/{key}"),
            json!({ "update": { field: [{ op: value }] } }),
        )
    }

    /// Issue `key` put in epic `epic`, or in none.
    pub fn in_epic(key: &str, epic: Option<&str>) -> Self {
        let path = format!("/rest/agile/1.0/epic/{}/issue", epic.unwrap_or("none"));
        Self::new("POST", path, json!({ "issues": [key] }))
    }

    /// Issue `key` put in sprint `sprint`, or back in the backlog.
    pub fn in_sprint(key: &str, sprint: Option<u64>) -> Self {
        let path = match sprint {
            Some(id) => format!("/rest/agile/1.0/sprint/{id}/issue"),
            None => "/rest/agile/1.0/backlog/issue".to_owned(),
        };
        Self::new("POST", path, json!({ "issues": [key] }))
    }

    /// A comment on issue `key`, in the tracker's wiki text.
    pub fn commented(key: &str, body: &str) -> Self {
        Self::new(
            "POST",
            format!("/rest/api/2/issue/{key}/comment"),
            json!({ "body": body }),
        )
    }

    /// Comment `id` of issue `key` given a new text.
    pub fn comment_edited(key: &str, id: &str, body: &str) -> Self {
        Self::new(
            "PUT",
            format!("/rest/api/2/issue/{key}/comment/{id}"),
            json!({ "body": body }),
        )
    }

    pub fn comment_deleted(key: &str, id: &str) -> Self {
        Self {
            method: "DELETE",
            path: format!("/rest/api/2/issue/{key}/comment/{id}"),
            body: None,
        }
    }

    /// A link: `from` relates to `to` as `kind`'s outward words say (`PAY-412` blocks `PAY-425`).
    pub fn linked(kind: &str, from: &str, to: &str) -> Self {
        let body = json!({ "type": { "name": kind }, "outwardIssue": { "key": to }, "inwardIssue": { "key": from } });
        Self::new("POST", "/rest/api/2/issueLink".to_owned(), body)
    }

    /// An issue made, its fields as the create screen takes them.
    pub fn created(fields: Value) -> Self {
        Self::new(
            "POST",
            "/rest/api/2/issue".to_owned(),
            json!({ "fields": fields }),
        )
    }
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
    use artoo_plugin_sdk::serde_json::json;

    fn site() -> Site {
        Site {
            url: "https://jira.example.com".into(),
            email: None,
            token: String::new(),
        }
    }

    #[test]
    fn an_issue_reads_from_the_api() {
        let v = json!({"key": "PAY-412", "fields": {
            "summary": "Refund rounds down on partial captures",
            "issuetype": {"name": "Bug"}, "status": {"id": "3", "name": "In Progress", "statusCategory": {"key": "indeterminate"}},
            "assignee": {"displayName": "Juniper Hale", "accountId": "u-j"}, "priority": {"name": "High"},
            "description": "A partial capture refunds less.\r\n\r\nDone when it equals.",
            "labels": ["refunds"], "subtasks": [{"key": "PAY-413", "fields": {"summary": "Reproduce", "status": {"name": "Done", "statusCategory": {"key": "done"}}}}],
            "epic": {"key": "PAY-400", "name": "Refunds that add up", "color": {"key": "color_4"}},
            "issuelinks": [{"id": "1", "type": {"name": "Blocks", "inward": "is blocked by", "outward": "blocks"},
                "inwardIssue": {"key": "PAY-401", "fields": {"summary": "Rate limit", "status": {"name": "To Do", "statusCategory": {"key": "new"}}}}}],
            "comment": {"comments": [{"id": "9", "author": {"displayName": "Ember Lind"}, "created": "2026-11-10T09:12:00.000+0400", "body": "Seen twice."}]}
        }});
        let issue = issue(&site(), &v);
        assert_eq!(issue.url, "https://jira.example.com/browse/PAY-412");
        assert_eq!(
            issue.row.assignee.as_ref().map(|p| p.id.as_str()),
            Some("u-j")
        );
        assert_eq!(issue.row.subtasks, (1, 1));
        assert_eq!(
            issue.row.epic.as_ref().map(|e| (e.key.as_str(), e.color)),
            Some(("PAY-400", 4))
        );
        assert_eq!(
            issue
                .row
                .blocked_by
                .iter()
                .map(|b| b.key.as_str())
                .collect::<Vec<_>>(),
            ["PAY-401"]
        );
        assert_eq!(issue.links[0].relation, "is blocked by");
        assert_eq!(issue.row.project, "PAY");
        assert_eq!(
            paragraphs(&issue.description),
            ["A partial capture refunds less.", "Done when it equals."]
        );
        let text = digest(&issue);
        assert!(text.starts_with("# PAY-412 Refund rounds down on partial captures\n\nBug · In Progress · assigned to Juniper Hale · High priority · epic PAY-400 Refunds that add up · labels refunds"));
        assert!(text.contains("- PAY-413 Reproduce (Done)"));
        assert!(text.contains("- is blocked by PAY-401 Rate limit (To Do)"));
        assert!(text.contains("Ember Lind (2026-11-10):\nSeen twice."));
    }

    #[test]
    fn an_epic_that_is_the_parent_is_the_epic_not_the_parent() {
        let v = json!({"key": "PAY-418", "fields": {"issuetype": {"name": "Story"},
            "parent": {"key": "PAY-400", "fields": {"summary": "Refunds that add up", "issuetype": {"name": "Epic", "hierarchyLevel": 1}}}}});
        let r = row(&v);
        assert_eq!(r.parent, None);
        assert_eq!(r.epic.map(|e| e.key), Some("PAY-400".into()));
        let v = json!({"key": "PAY-413", "fields": {"issuetype": {"name": "Sub-task", "subtask": true},
            "parent": {"key": "PAY-412", "fields": {"summary": "Refund", "issuetype": {"name": "Bug"}}}}});
        assert_eq!(row(&v).parent.as_deref(), Some("PAY-412"));
    }

    #[test]
    fn a_trackers_time_reads_as_unix_seconds_and_words() {
        assert_eq!(stamp("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(stamp("1970-01-01T04:00:00.000+0400"), Some(0));
        assert_eq!(stamp("1970-01-02"), Some(86_400));
        assert_eq!(stamp("2026-11-14T17:00:00.000Z"), Some(1_794_675_600));
        assert_eq!(day_words("2026-11-14T17:00:00.000Z"), "Sat 14 Nov");
        assert_eq!(day_words("2026-11-13"), "Fri 13 Nov");
    }

    #[test]
    fn each_change_is_the_request_jira_documents() {
        let moved = Write::moved("PAY-412", "41", json!({"resolution": {"name": "Done"}}));
        assert_eq!(
            (moved.method, moved.path.as_str()),
            ("POST", "/rest/api/2/issue/PAY-412/transitions")
        );
        assert_eq!(
            moved.body,
            Some(json!({"transition": {"id": "41"}, "fields": {"resolution": {"name": "Done"}}}))
        );
        assert_eq!(
            Write::moved("PAY-412", "31", json!({})).body,
            Some(json!({"transition": {"id": "31"}}))
        );
        let who = Person {
            id: "u-k".into(),
            name: "Kestrel Okafor".into(),
        };
        assert_eq!(
            Write::assigned("PAY-412", Some(&who), true).body,
            Some(json!({"accountId": "u-k"}))
        );
        assert_eq!(
            Write::assigned("PAY-412", None, false).body,
            Some(json!({"name": null}))
        );
        assert_eq!(
            Write::assigned("PAY-412", None, true).path,
            "/rest/api/2/issue/PAY-412/assignee"
        );
        assert_eq!(
            Write::changed_many("PAY-412", "labels", json!("ledger"), true).body,
            Some(json!({"update": {"labels": [{"add": "ledger"}]}}))
        );
        assert_eq!(
            Write::in_epic("PAY-412", None).path,
            "/rest/agile/1.0/epic/none/issue"
        );
        assert_eq!(
            Write::in_sprint("PAY-412", Some(35)).body,
            Some(json!({"issues": ["PAY-412"]}))
        );
        assert_eq!(
            Write::in_sprint("PAY-412", None).path,
            "/rest/agile/1.0/backlog/issue"
        );
        assert_eq!(Write::comment_deleted("PAY-412", "9").method, "DELETE");
        assert_eq!(
            Write::linked("Blocks", "PAY-412", "PAY-425").body,
            Some(
                json!({"type": {"name": "Blocks"}, "outwardIssue": {"key": "PAY-425"}, "inwardIssue": {"key": "PAY-412"}})
            )
        );
    }

    #[test]
    fn news_says_what_others_did_on_your_issues() {
        let me = Person {
            id: "u-you".into(),
            name: "Avery Quinn".into(),
        };
        let who = |id: &str, name: &str| json!({"accountId": id, "displayName": name});
        let issue = |key: &str, assignee: Value, histories: Value, comments: Value| {
            json!({"key": key, "fields": {
                "summary": "Refunds", "status": {"name": "In Progress", "statusCategory": {"key": "indeterminate"}},
                "assignee": assignee, "reporter": null, "comment": {"comments": comments}},
                "changelog": {"histories": histories}})
        };
        let v = json!({"issues": [
            issue("PAY-412", who("u-you", "Avery Quinn"), json!([
                {"id": "1", "author": who("u-jh", "Juniper Hale"), "created": "2026-11-13T07:30:00.000+0000",
                 "items": [{"fieldId": "assignee", "from": "u-jh", "fromString": "Juniper Hale", "to": "u-you",
                            "toString": "Avery Quinn"}]}]),
                json!([{"id": "9", "author": who("u-el", "Ember Lind"), "created": "2026-11-13T08:00:00.000+0000",
                        "updated": "2026-11-13T08:00:00.000+0000", "body": "[~accountid:u-you] can you look?"}])),
            issue("PAY-390", who("u-you", "Avery Quinn"), json!([
                {"id": "2", "author": who("u-ko", "Kestrel Okafor"), "created": "2026-11-12T15:00:00.000+0000",
                 "items": [{"fieldId": "status", "fromString": "In Review", "toString": "Done"}]},
                {"id": "3", "author": who("u-you", "Avery Quinn"), "created": "2026-11-12T16:00:00.000+0000",
                 "items": [{"fieldId": "status", "fromString": "Done", "toString": "In Review"}]}]), json!([])),
            issue("PAY-500", who("u-ko", "Kestrel Okafor"), json!([]), json!([
                {"id": "10", "author": who("u-jh", "Juniper Hale"), "created": "2026-11-13T09:00:00.000+0000",
                 "updated": "2026-11-13T09:00:00.000+0000", "body": "Not about you"}])),
        ]});
        let news = news_of(&v, &me, true);
        let said: Vec<(&str, Told, &str)> = (news.iter())
            .map(|n| (n.row.key.as_str(), n.told, n.says.as_str()))
            .collect();
        assert_eq!(
            said,
            vec![
                ("PAY-412", Told::Mentioned, "Ember Lind mentioned you"),
                ("PAY-412", Told::Assigned, "Juniper Hale assigned it to you"),
                ("PAY-390", Told::Changed, "Kestrel Okafor moved it to Done"),
            ]
        );
        assert_eq!(news[0].text, "@Avery.Quinn can you look?");
        assert_eq!(news[1].was.as_deref(), Some("Juniper Hale"));
    }
}
