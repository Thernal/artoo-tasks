//! Jira as a tracker: its boards, a board's issues, an issue whole, a search, the news of your issues; a move, an
//! edit, a comment, an issue made — each intent sent as the request Jira's REST documents.

use super as jira;
use crate::model::{
    Ask, Attachment, Board, BoardInfo, Brief, Column, Comment, Epic, Field, Issue, Kind, Link,
    Move, News, Page, Person, Row, Site, Sprint, State, Told, Tracker, Write, stamp,
};
use crate::trackers::SiteExt;
use crate::wiki;
use artoo_plugin_sdk::serde_json::{Value, json};

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

/// Someone from the API's user object.
fn person(v: &Value) -> Option<Person> {
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
        start: v["startDate"].as_str().map(str::to_owned),
        goal: text(&v["goal"]),
    })
}

/// A list's issue from the API's `issue` object: the Agile API's (`epic`, `sprint`) or the platform's.
fn row(v: &Value) -> Row {
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
fn issue(site: &Site, v: &Value) -> Issue {
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
        site: site.key().to_owned(),
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

fn comment(c: &Value) -> Comment {
    Comment {
        id: text(&c["id"]),
        author: person(&c["author"]),
        created: text(&c["created"]),
        updated: text(&c["updated"]),
        body: text(&c["body"]),
    }
}

/// Board `id`'s page on `site`: Jira Server's address, which Jira Cloud sends on to its own.
fn board_url(site: &Site, id: &str) -> String {
    format!("{}/secure/RapidBoard.jspa?rapidView={id}", site.url)
}

const ROW_FIELDS: &str = "summary,issuetype,status,assignee,priority,updated,parent,subtasks,comment,issuelinks,labels,project,epic,sprint,customfield_10020";

/// Who the token is.
fn myself(site: &Site) -> Result<Person, String> {
    let v = jira::get(site, "/rest/api/2/myself")?;
    person(&v).ok_or_else(|| format!("{} did not say who the token is", site.host()))
}

/// A `GET` of a path on the site, its refusal's status kept: `jira::read`, or a test's answers.
type Read<'a> = &'a (dyn Fn(&str) -> Result<Value, jira::Refused> + Sync);

/// A page of a list of values (boards, sprints) as Jira gives it: at most 50, whatever is asked.
const VALUES_PAGE: usize = 50;

/// How many values a list is read up to: a guard against a server that never says its last page is.
const VALUES_MAX: usize = 5000;

/// Every value of the list at `path` (its own query or none), page by page while a page says it is not the last.
fn values(read: Read, path: &str) -> Result<Vec<Value>, jira::Refused> {
    let join = if path.contains('?') { '&' } else { '?' };
    let mut all = Vec::new();
    loop {
        let v = read(&format!(
            "{path}{join}startAt={}&maxResults={VALUES_PAGE}",
            all.len()
        ))?;
        let page = v["values"].as_array().cloned().unwrap_or_default();
        let got = page.len();
        all.extend(page);
        if !more_values(&v, got, all.len()) {
            return Ok(all);
        }
    }
}

/// Whether a page of `got` values (`v`), `read` so far, leaves more to read: only when it says it is not the last.
fn more_values(v: &Value, got: usize, read: usize) -> bool {
    v["isLast"] == Value::Bool(false) && got > 0 && read < VALUES_MAX
}

/// The boards of `site`, every page of them, oldest first as Jira lists them.
fn boards(site: &Site) -> Result<Vec<Board>, String> {
    let read = |path: &str| jira::read(site, path);
    let boards = values(&read, "/rest/agile/1.0/board")?;
    Ok(boards.iter().map(|b| board(site, b)).collect())
}

fn board(site: &Site, b: &Value) -> Board {
    Board {
        // Jira's board id is a number.
        id: (b["id"].as_u64()).map_or_else(|| text(&b["id"]), |n| n.to_string()),
        name: text(&b["name"]),
        site: site.key().to_owned(),
        project: b["location"]["projectKey"].as_str().map(str::to_owned),
        kind: text(&b["type"]),
    }
}

/// A board's active and future sprints, as read.
enum Sprints {
    Read(Vec<Sprint>),
    /// The board has none: a kanban board, or one with sprints off.
    None,
    /// They could not be read.
    Unread,
}

/// Board `id`'s active and future sprints, every page of them; its `kind` (`scrum`, `kanban`, `simple`) as its
/// configuration says. A kanban board is not asked. A board of another kind that answers 400 (`The board does not
/// support sprints`: a team-managed board with sprints off) has none, which is no failure.
fn board_sprints(read: Read, id: &str, kind: &str) -> Sprints {
    if kind == "kanban" {
        return Sprints::None;
    }
    let path = format!("/rest/agile/1.0/board/{id}/sprint?state=active,future");
    match values(read, &path) {
        Ok(v) => Sprints::Read(v.iter().filter_map(sprint).collect()),
        Err(r) if r.code == 400 && kind != "scrum" => Sprints::None,
        Err(_) => Sprints::Unread,
    }
}

/// Board `id`'s columns, epics and sprints: its configuration first, which says whether it has sprints, its epics
/// asked alongside. Epics or sprints that could not be read are said so, not a failure.
fn board_info(read: Read, id: &str) -> Result<BoardInfo, String> {
    let list = |path: String| -> Option<Vec<Value>> {
        read(&path)
            .ok()
            .and_then(|v| v["values"].as_array().cloned())
    };
    let (config, epics, sprints) = std::thread::scope(|scope| {
        let epics = scope.spawn(|| {
            list(format!(
                "/rest/agile/1.0/board/{id}/epic?done=false&maxResults=100"
            ))
        });
        let config = read(&format!("/rest/agile/1.0/board/{id}/configuration"));
        let sprints = (config.as_ref().ok()).map(|c| board_sprints(read, id, &text(&c["type"])));
        // A read that panicked is one not made.
        (config, epics.join().ok().flatten(), sprints)
    });
    let columns = (config?["columnConfig"]["columns"]
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
    let sprints = sprints.unwrap_or(Sprints::Unread);
    Ok(BoardInfo {
        columns,
        epics_unread: epics.is_none(),
        sprints_unread: matches!(sprints, Sprints::Unread),
        sprintless: matches!(sprints, Sprints::None),
        epics: epics.iter().flatten().filter_map(epic).collect(),
        sprints: match sprints {
            Sprints::Read(sprints) => sprints,
            _ => Vec::new(),
        },
    })
}

/// Board `id`'s closed sprints, every page of them, the last closed first. A board without sprints has none.
fn past_sprints(read: Read, id: &str) -> Result<Vec<Sprint>, String> {
    let mut closed = match values(
        read,
        &format!("/rest/agile/1.0/board/{id}/sprint?state=closed"),
    ) {
        Ok(closed) => closed,
        Err(r) if r.code == 400 => return Ok(Vec::new()),
        Err(r) => return Err(r.said),
    };
    closed.sort_by_key(|s| std::cmp::Reverse(s["completeDate"].as_str().and_then(stamp)));
    Ok(closed.iter().filter_map(sprint).collect())
}

/// Board `id`'s issues, newest change first.
fn board_issues(site: &Site, id: &str) -> Result<Vec<Row>, String> {
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

/// Where slice `ask` of board `board` is read: the Agile API's address of its issues, which lists them in Jira's rank
/// order.
fn slice_path(board: &str, ask: &Ask) -> Result<String, String> {
    let agile = "/rest/agile/1.0";
    match (ask.slice.as_str(), ask.sprint_id(), ask.epic_key()) {
        ("backlog", ..) => Ok(format!("{agile}/board/{board}/backlog")),
        ("board", ..) => Ok(format!("{agile}/board/{board}/issue")),
        (_, Some(id), _) => Ok(format!("{agile}/sprint/{id}/issue")),
        (_, _, Some(key)) => Ok(format!("{agile}/epic/{}/issue", jira::encode(key))),
        (other, ..) => Err(format!("A Jira board has no slice {other}")),
    }
}

/// Whether `words` are one issue's key alone (`PAY-412`, `pay-412`).
fn is_key(words: &str) -> bool {
    words.split_once('-').is_some_and(|(project, n)| {
        project.starts_with(|c: char| c.is_ascii_alphabetic())
            && project
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !n.is_empty()
            && n.chars().all(|c| c.is_ascii_digit())
    })
}

/// `text` as a JQL string: in quotes, its backslashes and quotes escaped.
fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// What slice `ask` asks of Jira beyond its address, as JQL, or none: its words (a key alone by its key, `label:x` a
/// label, the rest Jira's text search), yours, its epic (Cloud names it the parent, `cloud`; Server its Epic Link).
/// A count (`count`) leaves sub-tasks out, as Jira's own counts of a sprint do.
fn slice_jql(ask: &Ask, cloud: bool, count: bool) -> Option<String> {
    let mut clauses = Vec::new();
    let words = ask.words.trim();
    if is_key(words) {
        clauses.push(format!("key = {}", quoted(&words.to_uppercase())));
    } else {
        let (labels, rest): (Vec<&str>, Vec<&str>) = words
            .split_whitespace()
            .partition(|w| w.to_lowercase().starts_with("label:"));
        clauses.extend(
            labels
                .iter()
                .map(|l| format!("labels = {}", quoted(&l["label:".len()..]))),
        );
        if !rest.is_empty() {
            clauses.push(format!("text ~ {}", quoted(&rest.join(" "))));
        }
    }
    if ask.mine {
        clauses.push("assignee = currentUser()".to_owned());
    }
    if let Some(epic) = &ask.epic {
        let field = if cloud { "parent" } else { "\"Epic Link\"" };
        clauses.push(format!("{field} = {}", quoted(epic)));
    }
    if count {
        clauses.push("issuetype not in subTaskIssueTypes()".to_owned());
    }
    (!clauses.is_empty()).then(|| clauses.join(" AND "))
}

/// Board `board`'s slice `ask`, on a Cloud site or not (`cloud`). A count (`max` 0) is one request, its total Jira's
/// count without sub-tasks. Else the issues from `start` on, sub-tasks among them, read on page by page by what each
/// answer gave (Jira may give fewer than asked) until `max` are read or the slice ends; the total is then how many
/// issues the slice holds with its sub-tasks, where its reading ends.
fn slice(read: Read, cloud: bool, board: &str, ask: &Ask) -> Result<Page, String> {
    let path = slice_path(board, ask)?;
    let jql = |count: bool| {
        slice_jql(ask, cloud, count)
            .map(|q| format!("&jql={}", jira::encode(&q)))
            .unwrap_or_default()
    };
    let total = |v: &Value, at_least: usize| v["total"].as_u64().map_or(at_least, |n| n as usize);
    if ask.max == 0 {
        let v = read(&format!("{path}?maxResults=0{}", jql(true)))?;
        return Ok(Page {
            rows: Vec::new(),
            total: total(&v, 0),
        });
    }
    let mut page = Page::default();
    loop {
        let start = ask.start + page.rows.len();
        let v = read(&format!(
            "{path}?startAt={start}&maxResults={}&fields={ROW_FIELDS}{}",
            ask.max - page.rows.len(),
            jql(false)
        ))?;
        let got = v["issues"].as_array().map_or(0, Vec::len);
        page.rows
            .extend(v["issues"].as_array().into_iter().flatten().map(row));
        page.total = total(&v, start + got);
        if got == 0 || page.rows.len() >= ask.max || start + got >= page.total {
            return Ok(page);
        }
    }
}

/// Issue `key` whole, through the Agile API, which adds its epic and sprint on Server and Cloud alike.
fn read_issue(site: &Site, key: &str) -> Result<Issue, String> {
    let v = jira::get(
        site,
        &format!("/rest/agile/1.0/issue/{}", jira::encode(key)),
    )?;
    Ok(issue(site, &v))
}

/// Where `site` searches by JQL: Jira Cloud's `/search/jql`, its older `/search` answering 410 Gone there; Server
/// and Data Center have only `/search`.
fn search_path(site: &Site) -> &'static str {
    if site.cloud() {
        "/rest/api/2/search/jql"
    } else {
        "/rest/api/2/search"
    }
}

/// The issues `jql` finds, at most `max`, for `purpose` (`find`) as `jira::get_for` reads it.
fn search_for(site: &Site, jql: &str, max: usize, purpose: &str) -> Result<Vec<Row>, String> {
    let v = jira::get_for(
        site,
        &format!(
            "{}?maxResults={max}&fields={ROW_FIELDS}&jql={}",
            search_path(site),
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

/// What others did lately on the issues `me` is on: assigned, mentioned, commented, moved. The newest first; one
/// news an issue and a kind, the latest.
fn news(site: &Site, me: &Person) -> Result<Vec<News>, String> {
    let jql = "(assignee = currentUser() OR reporter = currentUser() OR watcher = currentUser() \
               OR comment ~ currentUser()) AND updated >= -7d ORDER BY updated DESC";
    let v = jira::get_for(
        site,
        &format!(
            "{}?maxResults=30&expand=changelog&fields={ROW_FIELDS},reporter&jql={}",
            search_path(site),
            jira::encode(jql)
        ),
        "news",
    )?;
    Ok(news_of(&v, me, site.cloud()))
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
fn moves(site: &Site, key: &str) -> Result<Vec<Move>, String> {
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
        custom: text(&f["schema"]["custom"]),
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
fn edit_screen(site: &Site, key: &str) -> Result<Vec<Field>, String> {
    let v = jira::get(
        site,
        &format!("/rest/api/2/issue/{}/editmeta", jira::encode(key)),
    )?;
    Ok(fields(&v["fields"]))
}

/// The people issue `key` may be assigned to, those `query` finds; the fixture's and the tracker's order.
fn assignable(site: &Site, key: &str, query: &str) -> Result<Vec<Person>, String> {
    let by = if site.cloud() { "query" } else { "username" };
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
fn kinds(site: &Site, project: &str) -> Result<Vec<Kind>, String> {
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
fn create_screen(site: &Site, project: &str, kind: &str) -> Result<Vec<Field>, String> {
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
fn projects(site: &Site) -> Result<Vec<(String, String)>, String> {
    let v = jira::get(site, "/rest/api/2/project")?;
    Ok((v.as_array().into_iter().flatten())
        .map(|p| (text(&p["key"]), text(&p["name"])))
        .collect())
}

/// The kinds of link: `(name, inward, outward)`, `("Blocks", "is blocked by", "blocks")`.
fn link_kinds(site: &Site) -> Result<Vec<(String, String, String)>, String> {
    let v = jira::get(site, "/rest/api/2/issueLinkType")?;
    Ok((v["issueLinkTypes"].as_array().into_iter().flatten())
        .map(|t| (text(&t["name"]), text(&t["inward"]), text(&t["outward"])))
        .collect())
}

/// An attachment's bytes, read with the site's token: for an image drawn where it stands.
fn attachment_bytes(site: &Site, url: &str) -> Result<Vec<u8>, String> {
    jira::bytes(site, url)
}

/// A file attached to issue `key`.
fn attach(site: &Site, key: &str, file: &std::path::Path) -> Result<Value, String> {
    let path = format!("/rest/api/2/issue/{key}/attachments");
    jira::attach(site, &path, file)
}

/// Jira opened on a site: Cloud's (an email and an API token) or Server's and Data Center's (a personal access
/// token).
pub struct Jira(pub Site);

impl Jira {
    /// A `GET` on its site, its refusal's status kept.
    fn read(&self, path: &str) -> Result<Value, jira::Refused> {
        jira::read(&self.0, path)
    }
}

impl Tracker for Jira {
    fn myself(&self) -> Result<Person, String> {
        myself(&self.0)
    }
    fn boards(&self) -> Result<Vec<Board>, String> {
        boards(&self.0)
    }
    fn board_info(&self, board: &str) -> Result<BoardInfo, String> {
        board_info(&|path: &str| self.read(path), board)
    }
    fn board_issues(&self, board: &str) -> Result<Vec<Row>, String> {
        board_issues(&self.0, board)
    }
    fn slice(&self, board: &str, ask: &Ask) -> Result<Page, String> {
        slice(&|path: &str| self.read(path), self.0.cloud(), board, ask)
    }
    fn past_sprints(&self, board: &str) -> Result<Vec<Sprint>, String> {
        past_sprints(&|path: &str| self.read(path), board)
    }
    fn board_url(&self, board: &str) -> String {
        board_url(&self.0, board)
    }
    fn issue(&self, key: &str) -> Result<Issue, String> {
        read_issue(&self.0, key)
    }
    /// A key alone (`PAY-412`) by its key, else by its words.
    fn find(&self, words: &str, max: usize) -> Result<Vec<Row>, String> {
        let jql = if words.contains('-')
            && words.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            format!("key = \"{}\"", words.to_uppercase())
        } else {
            format!(
                "text ~ \"{}\" ORDER BY updated DESC",
                words.replace('"', "")
            )
        };
        search_for(&self.0, &jql, max, "find")
    }
    fn search(&self, query: &str, max: usize) -> Result<Vec<Row>, String> {
        search_for(&self.0, query, max, "")
    }
    fn news(&self, me: &Person) -> Result<Vec<News>, String> {
        news(&self.0, me)
    }
    fn moves(&self, key: &str) -> Result<Vec<Move>, String> {
        moves(&self.0, key)
    }
    fn edit_screen(&self, key: &str) -> Result<Vec<Field>, String> {
        edit_screen(&self.0, key)
    }
    fn assignable(&self, key: &str, query: &str) -> Result<Vec<Person>, String> {
        assignable(&self.0, key, query)
    }
    fn projects(&self) -> Result<Vec<(String, String)>, String> {
        projects(&self.0)
    }
    fn kinds(&self, project: &str) -> Result<Vec<Kind>, String> {
        kinds(&self.0, project)
    }
    fn create_screen(&self, project: &str, kind: &str) -> Result<Vec<Field>, String> {
        create_screen(&self.0, project, kind)
    }
    fn link_kinds(&self) -> Result<Vec<(String, String, String)>, String> {
        link_kinds(&self.0)
    }
    fn file(&self, url: &str) -> Result<Vec<u8>, String> {
        attachment_bytes(&self.0, url)
    }
    fn attach(&self, key: &str, file: &std::path::Path) -> Result<Value, String> {
        attach(&self.0, key, file)
    }
    fn write(&self, w: &Write) -> Result<Value, String> {
        let send = |method: &str, path: &str, body: Option<&Value>| {
            jira::send(&self.0, method, path, body)
        };
        send_write(&send, w, self.0.cloud())
    }
}

/// An intent as the request Jira's REST documents: its method, its path and what it sends. A person is given by
/// account id on Cloud (`cloud`), by user name on Server.
fn request(w: &Write, cloud: bool) -> (&'static str, String, Option<Value>) {
    let issue = |key: &str| format!("/rest/api/2/issue/{key}");
    match w {
        Write::Move { key, step, asked } => {
            let mut body = json!({ "transition": { "id": step } });
            if asked.as_object().is_some_and(|o| !o.is_empty()) {
                body["fields"] = asked.clone();
            }
            ("POST", format!("{}/transitions", issue(key)), Some(body))
        }
        Write::Assign { key, who } => {
            let body = if cloud {
                json!({ "accountId": who })
            } else {
                json!({ "name": who })
            };
            ("PUT", format!("{}/assignee", issue(key)), Some(body))
        }
        Write::Edit { key, fields } => ("PUT", issue(key), Some(json!({ "fields": fields }))),
        Write::Change {
            key,
            field,
            value,
            add,
        } => {
            let op = if *add { "add" } else { "remove" };
            let body = json!({ "update": { field: [{ op: value }] } });
            ("PUT", issue(key), Some(body))
        }
        Write::Epic { key, epic } => {
            let path = format!(
                "/rest/agile/1.0/epic/{}/issue",
                epic.as_deref().unwrap_or("none")
            );
            ("POST", path, Some(json!({ "issues": [key] })))
        }
        // Into a sprint, ranked before `before` there; into the Backlog, which takes no rank (`rank` follows).
        Write::Sprint {
            key,
            sprint,
            before,
        } => {
            let mut body = json!({ "issues": [key] });
            let path = match sprint {
                Some(id) => {
                    if let Some(before) = before {
                        body["rankBeforeIssue"] = json!(before);
                    }
                    format!("/rest/agile/1.0/sprint/{id}/issue")
                }
                None => "/rest/agile/1.0/backlog/issue".to_owned(),
            };
            ("POST", path, Some(body))
        }
        Write::Comment { key, body } => (
            "POST",
            format!("{}/comment", issue(key)),
            Some(json!({ "body": body })),
        ),
        Write::EditComment { key, id, body } => (
            "PUT",
            format!("{}/comment/{id}", issue(key)),
            Some(json!({ "body": body })),
        ),
        Write::DeleteComment { key, id } => {
            ("DELETE", format!("{}/comment/{id}", issue(key)), None)
        }
        Write::Link { kind, from, to } => {
            let body = json!({ "type": { "name": kind }, "outwardIssue": { "key": to }, "inwardIssue": { "key": from } });
            ("POST", "/rest/api/2/issueLink".to_owned(), Some(body))
        }
        Write::Create { fields } => (
            "POST",
            "/rest/api/2/issue".to_owned(),
            Some(json!({ "fields": fields })),
        ),
    }
}

/// A request sent: its method, path and body; the answer, or why not.
type Send<'a> = &'a dyn Fn(&str, &str, Option<&Value>) -> Result<Value, String>;

/// Intent `w` sent as `request` makes it. An issue put at the Backlog's top (`Write::Sprint` with no sprint and
/// `before`) is two requests: the move, then its rank. When the rank is refused, the issue stays in the Backlog where
/// Jira put it, and the answer is `{"unranked": "<Jira's words>"}`, to be said.
fn send_write(send: Send, w: &Write, cloud: bool) -> Result<Value, String> {
    let (method, path, body) = request(w, cloud);
    let answer = send(method, &path, body.as_ref())?;
    let Write::Sprint {
        key,
        sprint: None,
        before: Some(before),
    } = w
    else {
        return Ok(answer);
    };
    let (method, path, body) = rank(key, before);
    Ok(match send(method, &path, Some(&body)) {
        Ok(_) => answer,
        Err(said) => json!({ "unranked": said }),
    })
}

/// Issue `key` ranked before issue `before`: the Backlog's top, after a move there.
fn rank(key: &str, before: &str) -> (&'static str, String, Value) {
    let body = json!({ "issues": [key], "rankBeforeIssue": before });
    ("PUT", "/rest/agile/1.0/issue/rank".to_owned(), body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{digest, paragraphs};

    fn site() -> Site {
        Site {
            url: "https://jira.example.com".into(),
            kind: "jira".into(),
            ..Default::default()
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
    fn each_change_is_the_request_jira_documents() {
        let moved = Write::moved("PAY-412", "41", json!({"resolution": {"name": "Done"}}));
        assert_eq!(
            request(&moved, true),
            (
                "POST",
                "/rest/api/2/issue/PAY-412/transitions".to_owned(),
                Some(
                    json!({"transition": {"id": "41"}, "fields": {"resolution": {"name": "Done"}}})
                )
            )
        );
        let body = |w: Write, cloud: bool| request(&w, cloud).2;
        let path = |w: Write| request(&w, true).1;
        assert_eq!(
            body(Write::moved("PAY-412", "31", json!({})), true),
            Some(json!({"transition": {"id": "31"}}))
        );
        let who = Person {
            id: "u-k".into(),
            name: "Kestrel Okafor".into(),
        };
        assert_eq!(
            body(Write::assigned("PAY-412", Some(&who)), true),
            Some(json!({"accountId": "u-k"}))
        );
        assert_eq!(
            body(Write::assigned("PAY-412", None), false),
            Some(json!({"name": null}))
        );
        assert_eq!(
            path(Write::assigned("PAY-412", None)),
            "/rest/api/2/issue/PAY-412/assignee"
        );
        assert_eq!(
            body(
                Write::changed_many("PAY-412", "labels", json!("ledger"), true),
                true
            ),
            Some(json!({"update": {"labels": [{"add": "ledger"}]}}))
        );
        assert_eq!(
            path(Write::in_epic("PAY-412", None)),
            "/rest/agile/1.0/epic/none/issue"
        );
        assert_eq!(
            body(Write::in_sprint("PAY-412", Some(35)), true),
            Some(json!({"issues": ["PAY-412"]}))
        );
        assert_eq!(
            path(Write::in_sprint("PAY-412", None)),
            "/rest/agile/1.0/backlog/issue"
        );
        assert_eq!(
            request(&Write::comment_deleted("PAY-412", "9"), true).0,
            "DELETE"
        );
        assert_eq!(
            body(Write::linked("Blocks", "PAY-412", "PAY-425"), true),
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

    #[test]
    fn boards_are_read_on_while_a_page_says_it_is_not_the_last() {
        let page = |last: Value| json!({"isLast": last, "values": []});
        assert!(more_values(&page(json!(false)), 50, 50));
        assert!(!more_values(&page(json!(true)), 7, 307));
        // An older server's page, or a fixture's, says nothing: it is the last.
        assert!(!more_values(&json!({"values": []}), 3, 3));
        assert!(!more_values(&page(json!(false)), 0, 50));
        assert!(!more_values(&page(json!(false)), 50, VALUES_MAX));
    }

    #[test]
    fn jira_cloud_searches_by_jql_where_a_server_searches() {
        let cloud = Site {
            url: "https://acme.atlassian.net".into(),
            user: Some("you@acme.example".into()),
            kind: "jira".into(),
            ..Default::default()
        };
        assert_eq!(search_path(&cloud), "/rest/api/2/search/jql");
        assert_eq!(search_path(&site()), "/rest/api/2/search");
    }

    use std::sync::Mutex;

    /// Query parameter `name` of `path`, as a number.
    fn query(path: &str, name: &str) -> Option<usize> {
        let (_, q) = path.split_once('?')?;
        q.split('&')
            .find_map(|p| p.strip_prefix(&format!("{name}="))?.parse().ok())
    }

    /// A list of `all` issues answered as Jira answers `path`: at most `most` an answer, whatever is asked.
    fn issues_from(path: &str, all: usize, most: usize) -> Value {
        let start = query(path, "startAt").unwrap_or(0);
        let max = query(path, "maxResults").unwrap_or(most).min(most);
        let issues: Vec<Value> = (start..all.min(start + max))
            .map(|i| json!({"key": format!("PAY-{i}"), "fields": {}}))
            .collect();
        json!({"startAt": start, "maxResults": max, "total": all, "issues": issues})
    }

    #[test]
    fn a_slice_is_read_on_by_what_jira_gave_until_its_end_or_what_was_asked() {
        let asked = Mutex::new(Vec::new());
        let read = |p: &str| {
            asked.lock().unwrap().push(p.to_owned());
            Ok(issues_from(p, 78, 30))
        };
        let pages = || -> Vec<(Option<usize>, Option<usize>)> {
            let mut asked = asked.lock().unwrap();
            let pages = asked
                .iter()
                .map(|p| (query(p, "startAt"), query(p, "maxResults")))
                .collect();
            asked.clear();
            pages
        };
        let whole = Ask {
            max: 100,
            ..Ask::sprint(34)
        };
        let page = slice(&read, true, "7", &whole).unwrap();
        assert_eq!((page.rows.len(), page.total), (78, 78));
        assert_eq!(page.rows.last().map(|r| r.key.as_str()), Some("PAY-77"));
        // It ends at the slice's total.
        assert_eq!(
            pages(),
            [
                (Some(0), Some(100)),
                (Some(30), Some(70)),
                (Some(60), Some(40))
            ]
        );
        let fifty = Ask {
            start: 10,
            max: 50,
            ..Ask::backlog()
        };
        let page = slice(&read, true, "7", &fifty).unwrap();
        assert_eq!(page.rows.first().map(|r| r.key.as_str()), Some("PAY-10"));
        // It ends at what was asked.
        assert_eq!(page.rows.len(), 50);
        assert_eq!(pages(), [(Some(10), Some(50)), (Some(40), Some(20))]);
        // An answer with nothing in it ends the read, whatever its total says.
        let empty = |_: &str| Ok(json!({"total": 9, "issues": []}));
        assert_eq!(slice(&empty, true, "7", &whole).unwrap().rows.len(), 0);
    }

    #[test]
    fn each_slice_read_is_the_request_jira_documents() {
        let asked = Mutex::new(String::new());
        let read = |p: &str| {
            *asked.lock().unwrap() = p.to_owned();
            Ok(json!({"total": 0, "issues": []}))
        };
        let sent = |ask: Ask, cloud: bool| {
            slice(&read, cloud, "7", &ask).unwrap();
            asked.lock().unwrap().clone()
        };
        let rows = |ask: Ask| Ask { max: 50, ..ask };
        let jql = |q: &str| format!("&jql={}", jira::encode(q));
        let no_subs = "issuetype not in subTaskIssueTypes()";
        assert_eq!(
            sent(Ask::sprint(34), true),
            format!(
                "/rest/agile/1.0/sprint/34/issue?maxResults=0{}",
                jql(no_subs)
            )
        );
        assert_eq!(
            sent(rows(Ask::sprint(34)), true),
            format!("/rest/agile/1.0/sprint/34/issue?startAt=0&maxResults=50&fields={ROW_FIELDS}")
        );
        assert_eq!(
            sent(rows(Ask::backlog()), true),
            format!("/rest/agile/1.0/board/7/backlog?startAt=0&maxResults=50&fields={ROW_FIELDS}")
        );
        assert_eq!(
            sent(rows(Ask::board()), true),
            format!("/rest/agile/1.0/board/7/issue?startAt=0&maxResults=50&fields={ROW_FIELDS}")
        );
        assert_eq!(
            sent(Ask::epic("PAY-400"), true),
            format!(
                "/rest/agile/1.0/epic/PAY-400/issue?maxResults=0{}",
                jql(no_subs)
            )
        );
        let mine = Ask {
            mine: true,
            ..Ask::backlog()
        };
        assert_eq!(
            sent(mine, true),
            format!(
                "/rest/agile/1.0/board/7/backlog?maxResults=0{}",
                jql(&format!("assignee = currentUser() AND {no_subs}"))
            )
        );
        let filtered = Ask {
            words: "refund".into(),
            epic: Some("PAY-400".into()),
            ..rows(Ask::backlog())
        };
        assert!(
            sent(filtered.clone(), true)
                .ends_with(&jql("text ~ \"refund\" AND parent = \"PAY-400\""))
        );
        assert!(
            sent(filtered, false)
                .ends_with(&jql("text ~ \"refund\" AND \"Epic Link\" = \"PAY-400\""))
        );
        let key = Ask {
            words: " pay-412 ".into(),
            ..rows(Ask::backlog())
        };
        assert!(sent(key, true).ends_with(&jql("key = \"PAY-412\"")));
        assert!(slice(&read, true, "7", &Ask::default()).is_err());
    }

    #[test]
    fn the_words_asked_of_jira_are_escaped() {
        let ask = Ask {
            words: r#"say "hi" \ now label:ledger"#.into(),
            ..Ask::backlog()
        };
        assert_eq!(
            slice_jql(&ask, true, false).as_deref(),
            Some(r#"labels = "ledger" AND text ~ "say \"hi\" \\ now""#)
        );
        assert_eq!(slice_jql(&Ask::backlog(), true, false), None);
        assert!(is_key("PAY-412") && is_key("ab_2-7"));
        assert!(!is_key("PAY-") && !is_key("412-PAY") && !is_key("pay 412"));
    }

    #[test]
    fn a_board_says_whether_it_has_sprints() {
        let info = |kind: &'static str, sprints: Result<Value, jira::Refused>| {
            let asked = Mutex::new(Vec::new());
            let read = |p: &str| {
                asked.lock().unwrap().push(p.to_owned());
                if p.contains("/configuration") {
                    let to_do = json!({"name": "To do", "statuses": [{"id": "1"}]});
                    Ok(json!({"type": kind, "columnConfig": {"columns": [to_do]}}))
                } else if p.contains("/sprint") {
                    sprints.clone()
                } else {
                    Ok(json!({"values": []}))
                }
            };
            let info = board_info(&read, "7").unwrap();
            let asked = asked.lock().unwrap().iter().any(|p| p.contains("/sprint"));
            (info, asked)
        };
        let refused = |code| {
            Err(jira::Refused {
                code,
                said: "The board does not support sprints".into(),
            })
        };
        let one = json!({"isLast": true, "values": [{"id": 34, "name": "Sprint 34", "state": "active",
            "startDate": "2026-11-03T09:00:00.000Z", "goal": "Refunds that add up"}]});
        let (kanban, asked) = info("kanban", Ok(one.clone()));
        assert!(kanban.sprintless && !kanban.sprints_unread && kanban.sprints.is_empty());
        assert!(!asked, "a kanban board is asked no sprint");
        let (simple, asked) = info("simple", refused(400));
        assert!(simple.sprintless && !simple.sprints_unread && asked);
        let (scrum, _) = info("scrum", refused(400));
        assert!(!scrum.sprintless && scrum.sprints_unread);
        let (failed, _) = info("simple", refused(504));
        assert!(!failed.sprintless && failed.sprints_unread);
        let (scrum, _) = info("scrum", Ok(one));
        assert_eq!(scrum.columns.len(), 1);
        let s = &scrum.sprints[0];
        assert_eq!(
            (s.id, s.start.as_deref(), s.goal.as_str()),
            (34, Some("2026-11-03T09:00:00.000Z"), "Refunds that add up")
        );
    }

    #[test]
    fn past_sprints_are_read_on_while_a_page_says_it_is_not_the_last() {
        let asked = Mutex::new(Vec::new());
        let read = |p: &str| {
            asked.lock().unwrap().push(p.to_owned());
            let at = query(p, "startAt").unwrap_or(0);
            let done = format!("2026-0{}-05T16:30:00.000Z", at + 1);
            Ok(
                json!({"isLast": at >= 2, "values": [{"id": 30 + at, "state": "closed", "completeDate": done}]}),
            )
        };
        let past = past_sprints(&read, "7").unwrap();
        assert_eq!(past.iter().map(|s| s.id).collect::<Vec<_>>(), [32, 31, 30]);
        let asked = asked.lock().unwrap();
        assert_eq!(asked.len(), 3);
        assert_eq!(
            asked[0],
            "/rest/agile/1.0/board/7/sprint?state=closed&startAt=0&maxResults=50"
        );
        let none = |_: &str| {
            Err(jira::Refused {
                code: 400,
                said: "The board does not support sprints".into(),
            })
        };
        assert_eq!(past_sprints(&none, "10"), Ok(Vec::new()));
    }

    #[test]
    fn a_move_names_the_issue_it_goes_before() {
        let into = |sprint| Write::Sprint {
            key: "PAY-412".into(),
            sprint,
            before: Some("PAY-300".into()),
        };
        assert_eq!(
            request(&into(Some(35)), true),
            (
                "POST",
                "/rest/agile/1.0/sprint/35/issue".to_owned(),
                Some(json!({"issues": ["PAY-412"], "rankBeforeIssue": "PAY-300"}))
            )
        );
        let sent = Mutex::new(Vec::new());
        let refuse_rank = |method: &str, path: &str, body: Option<&Value>| {
            sent.lock()
                .unwrap()
                .push((method.to_owned(), path.to_owned(), body.cloned()));
            match path.ends_with("/rank") {
                true => Err("Ranking is not enabled".to_owned()),
                false => Ok(Value::Null),
            }
        };
        // The Backlog's top: the move, then the rank; a rank refused leaves it moved, Jira's words said.
        assert_eq!(
            send_write(&refuse_rank, &into(None), true),
            Ok(json!({"unranked": "Ranking is not enabled"}))
        );
        assert_eq!(
            *sent.lock().unwrap(),
            [
                (
                    "POST".to_owned(),
                    "/rest/agile/1.0/backlog/issue".to_owned(),
                    Some(json!({"issues": ["PAY-412"]}))
                ),
                (
                    "PUT".to_owned(),
                    "/rest/agile/1.0/issue/rank".to_owned(),
                    Some(json!({"issues": ["PAY-412"], "rankBeforeIssue": "PAY-300"}))
                ),
            ]
        );
        sent.lock().unwrap().clear();
        // A move into a sprint, or to the Backlog with no place, is one request.
        assert_eq!(
            send_write(&refuse_rank, &into(Some(35)), true),
            Ok(Value::Null)
        );
        send_write(&refuse_rank, &Write::in_sprint("PAY-412", None), true).unwrap();
        assert_eq!(sent.lock().unwrap().len(), 2);
        let refuse = |_: &str, _: &str, _: Option<&Value>| Err("PAY-412 is closed".to_owned());
        assert_eq!(
            send_write(&refuse, &into(None), true),
            Err("PAY-412 is closed".to_owned())
        );
    }

    #[test]
    fn the_fixture_board_reads_as_slices() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let site = Site {
            url: "https://acme.atlassian.net".into(),
            kind: "jira".into(),
            ..Default::default()
        };
        let read = |p: &str| jira::from_fixture(&dir, &site, p, "");
        let count = |ask: Ask| slice(&read, site.cloud(), "7", &ask).unwrap().total;
        assert_eq!(
            (
                count(Ask::sprint(34)),
                count(Ask::sprint(36)),
                count(Ask::backlog())
            ),
            (78, 136, 300)
        );
        let mine = count(Ask {
            mine: true,
            ..Ask::sprint(34)
        });
        assert!(mine > 0 && mine < 78);
        let whole = Ask {
            max: 1000,
            ..Ask::sprint(36)
        };
        let sprint = slice(&read, site.cloud(), "7", &whole).unwrap();
        assert!(sprint.total >= 136);
        assert_eq!(sprint.rows.len(), sprint.total);
        let backlog = Ask {
            max: 50,
            ..Ask::backlog()
        };
        let first = slice(&read, site.cloud(), "7", &backlog).unwrap();
        assert_eq!(first.rows.len(), 50);
        let info = board_info(&read, "7").unwrap();
        assert!(!info.sprintless && !info.sprints_unread);
        assert_eq!(
            info.sprints.iter().map(|s| s.id).collect::<Vec<_>>(),
            [34, 35, 36]
        );
        assert!(board_info(&read, "8").unwrap().sprintless);
        assert!(board_info(&read, "10").unwrap().sprintless);
        let past = past_sprints(&read, "7").unwrap();
        assert_eq!(
            past.iter().map(|s| s.id).collect::<Vec<_>>(),
            [33, 32, 31, 30, 29]
        );
    }
}
