//! Trello behind the plugin's model: a board's lists are its columns and its states, a card an issue (its key the
//! card's short link, `kZ3fG9aB`), a card's first member its assignee, its comments and attachments the issue's; a
//! member's notifications the news. What the model writes in Jira's shape is sent as Trello takes it; what Trello has
//! no place for (an epic, a sprint, a link, a priority) is refused, and why is said.
//!
//! Signed in with an API key (the site's `email`) and a token. A person is their user name, as a mention writes them
//! (`@jhale`); descriptions and comments are markdown on Trello and Jira's wiki text in the model.

use crate::jira::{self, Site};
use crate::model::{
    Attachment, Board, BoardInfo, Column, Comment, Field, Issue, Kind, Move, News, Person, Row,
    State, Told, Write, day, stamp,
};
use crate::wiki;
use artoo_plugin_sdk::serde_json::{Map, Value, json};

/// A card's fields as every read asks them.
const CARD: &str =
    "name,desc,idList,idMembers,shortLink,shortUrl,dateLastActivity,labels,badges,due,idBoard";

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

/// A member as the model names people: their user name, and their full name shown.
fn member(v: &Value) -> Option<Person> {
    let id = v["username"].as_str()?;
    let name = v["fullName"]
        .as_str()
        .filter(|n| !n.is_empty())
        .unwrap_or(id);
    Some(Person {
        id: id.to_owned(),
        name: name.to_owned(),
    })
}

/// A list's category by its name, or by being the board's first: `new`, `indeterminate`, `done`.
fn category(name: &str, first: bool) -> &'static str {
    let name = name.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
    if has(&["done", "complete", "shipped", "closed", "released"]) {
        "done"
    } else if first || has(&["to do", "todo", "backlog", "inbox", "ideas"]) {
        "new"
    } else {
        "indeterminate"
    }
}

/// A board's open lists as states, in its order.
fn states(board: &Value) -> Vec<State> {
    (board["lists"].as_array().into_iter().flatten())
        .enumerate()
        .map(|(i, l)| State {
            id: text(&l["id"]),
            name: text(&l["name"]),
            category: category(l["name"].as_str().unwrap_or_default(), i == 0).to_owned(),
        })
        .collect()
}

/// A card as a list shows it: its list the state, its first member the assignee, its labels by name (a label with
/// none by its colour).
fn card_row(c: &Value, state: State, members: Vec<Person>, board: &str) -> Row {
    Row {
        key: text(&c["shortLink"]),
        summary: text(&c["name"]),
        kind: "Card".into(),
        state,
        assignee: members.into_iter().next(),
        updated: text(&c["dateLastActivity"]),
        comments: c["badges"]["comments"].as_u64().unwrap_or(0) as usize,
        labels: (c["labels"].as_array().into_iter().flatten())
            .filter_map(|l| {
                (l["name"].as_str().filter(|n| !n.is_empty()))
                    .or(l["color"].as_str())
                    .map(str::to_owned)
            })
            .collect(),
        project: board.to_owned(),
        ..Default::default()
    }
}

/// A card read alone, with its list, its members and its board: as a row, its state by the list's name.
fn read_row(c: &Value) -> Row {
    let state = State {
        id: text(&c["list"]["id"]),
        name: text(&c["list"]["name"]),
        category: category(c["list"]["name"].as_str().unwrap_or_default(), false).to_owned(),
    };
    let members = (c["members"].as_array().into_iter().flatten())
        .filter_map(member)
        .collect();
    card_row(
        c,
        state,
        members,
        c["board"]["shortLink"].as_str().unwrap_or_default(),
    )
}

/// A card's key or id, or something else (a Jira key): what Trello is not asked about.
fn card_like(key: &str) -> bool {
    matches!(key.len(), 8 | 24) && key.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Board `id` whole: its lists, its open cards, its members.
fn board(site: &Site, id: &str) -> Result<Value, String> {
    jira::get(
        site,
        &format!(
            "/1/boards/{id}?fields=name,shortLink&lists=open&list_fields=name,pos&cards=open&card_fields={CARD}\
             &members=all&member_fields=fullName,username"
        ),
    )
}

/// A card's board, by its short link.
fn board_of(site: &Site, key: &str) -> Result<String, String> {
    let c = jira::get(
        site,
        &format!("/1/cards/{key}?fields=idList&board=true&board_fields=shortLink"),
    )?;
    Ok(text(&c["board"]["shortLink"]))
}

pub fn board_url(id: &str) -> String {
    format!("https://trello.com/b/{id}")
}

pub fn myself(site: &Site) -> Result<Person, String> {
    let v = jira::get(site, "/1/members/me?fields=fullName,username")?;
    member(&v).ok_or_else(|| format!("{} did not say who the token is", site.host()))
}

/// The open boards the token's member is on, each its own project.
pub fn boards(site: &Site) -> Result<Vec<Board>, String> {
    let v = jira::get(
        site,
        "/1/members/me/boards?filter=open&fields=name,shortLink",
    )?;
    Ok((v.as_array().into_iter().flatten())
        .map(|b| Board {
            id: text(&b["shortLink"]),
            name: text(&b["name"]),
            site: site.host().to_owned(),
            project: Some(text(&b["shortLink"])),
        })
        .collect())
}

/// A board's lists as its columns, a state each; Trello has no epics or sprints.
pub fn board_info(site: &Site, id: &str) -> Result<BoardInfo, String> {
    let b = board(site, id)?;
    Ok(BoardInfo {
        columns: states(&b)
            .into_iter()
            .map(|s| Column {
                name: s.name,
                states: vec![s.id],
            })
            .collect(),
        ..Default::default()
    })
}

/// A board's open cards, the latest touched first.
pub fn board_issues(site: &Site, id: &str) -> Result<Vec<Row>, String> {
    let b = board(site, id)?;
    let states = states(&b);
    let people: Vec<(String, Person)> = (b["members"].as_array().into_iter().flatten())
        .filter_map(|m| Some((text(&m["id"]), member(m)?)))
        .collect();
    let short = b["shortLink"].as_str().unwrap_or(id);
    let mut rows: Vec<Row> = (b["cards"].as_array().into_iter().flatten())
        .map(|c| {
            let list = text(&c["idList"]);
            let state = (states.iter().find(|s| s.id == list).cloned()).unwrap_or(State {
                id: list,
                ..Default::default()
            });
            let members = (c["idMembers"].as_array().into_iter().flatten())
                .filter_map(|id| people.iter().find(|(m, _)| Some(m.as_str()) == id.as_str()))
                .map(|(_, p)| p.clone())
                .collect();
            card_row(c, state, members, short)
        })
        .collect();
    rows.sort_by(|a, b| b.updated.cmp(&a.updated));
    Ok(rows)
}

/// When a Trello id was made: its first four bytes are the Unix time.
fn made_at(id: &str) -> Option<i64> {
    i64::from_str_radix(id.get(..8)?, 16).ok()
}

/// Unix seconds as the trackers write a time: `2026-11-13T09:12:00.000Z`.
fn iso(secs: i64) -> String {
    // Howard Hinnant's `civil_from_days`.
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

/// Card `key` whole: its list among its board's, its members, its comments and attachments.
pub fn read_issue(site: &Site, key: &str) -> Result<Issue, String> {
    if !card_like(key) {
        return Err(format!("{} has no {key}", site.host()));
    }
    let c = jira::get(
        site,
        &format!(
            "/1/cards/{key}?fields={CARD}&list=true&board=true&board_fields=name,shortLink&members=true\
             &member_fields=fullName,username&actions=commentCard,createCard&actions_limit=100\
             &action_memberCreator_fields=fullName,username&attachments=true\
             &attachment_fields=name,bytes,mimeType,date,url"
        ),
    )?;
    let mut row = read_row(&c);
    // The state's category as the board's lists say it, the first list new.
    if let Ok(b) = board(site, &row.project)
        && let Some(s) = states(&b).into_iter().find(|s| s.id == row.state.id)
    {
        row.state = s;
    }
    let actions: Vec<&Value> = c["actions"].as_array().into_iter().flatten().collect();
    // Trello says the newest first; the page reads the oldest first.
    let comments = (actions.iter().rev())
        .filter(|a| a["type"] == "commentCard")
        .map(|a| Comment {
            id: text(&a["id"]),
            author: member(&a["memberCreator"]),
            created: text(&a["date"]),
            updated: (a["data"]["dateLastEdited"].as_str())
                .map_or_else(|| text(&a["date"]), str::to_owned),
            body: wiki::from_markdown(a["data"]["text"].as_str().unwrap_or_default()),
        })
        .collect();
    let reporter = (actions.iter())
        .find(|a| a["type"] == "createCard")
        .and_then(|a| member(&a["memberCreator"]));
    let description = wiki::from_markdown(c["desc"].as_str().unwrap_or_default());
    let due = c["due"].as_str().map(|d| day(d).to_owned());
    Ok(Issue {
        site: site.host().to_owned(),
        url: (c["shortUrl"].as_str())
            .map_or_else(|| format!("https://trello.com/c/{key}"), str::to_owned),
        reporter,
        created: c["id"]
            .as_str()
            .and_then(made_at)
            .map(iso)
            .unwrap_or_default(),
        fields: json!({
            "summary": row.summary,
            "description": description,
            "duedate": due,
            "labels": row.labels,
        }),
        description,
        due,
        comments,
        attachments: (c["attachments"].as_array().into_iter().flatten())
            .map(|a| Attachment {
                id: text(&a["id"]),
                name: text(&a["name"]),
                size: a["bytes"].as_u64().unwrap_or(0),
                mime: text(&a["mimeType"]),
                author: None,
                created: text(&a["date"]),
                content: text(&a["url"]),
            })
            .collect(),
        row,
        ..Default::default()
    })
}

/// The cards `jql`'s words find (Trello has no JQL: what is quoted in it, else the words before `ORDER BY`).
pub fn search(site: &Site, jql: &str, max: usize, purpose: &str) -> Result<Vec<Row>, String> {
    let v = jira::get_for(
        site,
        &format!(
            "/1/search?query={}&modelTypes=cards&cards_limit={}&card_fields={CARD}&card_list=true\
             &card_members=true&card_board=true&board_fields=shortLink",
            jira::encode(&words_of(jql)),
            max.clamp(1, 1000)
        ),
        purpose,
    )?;
    Ok((v["cards"].as_array().into_iter().flatten())
        .map(read_row)
        .collect())
}

/// A JQL's words: `text ~ "rounding"`'s `rounding`, `key = "kZ3fG9aB"`'s key, else the query before `ORDER BY`.
fn words_of(jql: &str) -> String {
    if let Some((_, rest)) = jql.split_once('"')
        && let Some((words, _)) = rest.split_once('"')
    {
        return words.to_owned();
    }
    let upper = jql.to_uppercase();
    let end = upper.find("ORDER BY").unwrap_or(jql.len());
    jql[..end].trim().to_owned()
}

/// Every list of the card's board but the one it is in: a move each, asking nothing.
pub fn moves(site: &Site, key: &str) -> Result<Vec<Move>, String> {
    let c = jira::get(
        site,
        &format!("/1/cards/{key}?fields=idList&board=true&board_fields=shortLink"),
    )?;
    let here = text(&c["idList"]);
    let b = board(site, c["board"]["shortLink"].as_str().unwrap_or_default())?;
    Ok(states(&b)
        .into_iter()
        .filter(|s| s.id != here)
        .map(|s| Move {
            id: s.id.clone(),
            name: s.name.clone(),
            to: s,
            asks: Vec::new(),
        })
        .collect())
}

/// What Details edits of a card: its due date.
pub fn edit_screen() -> Vec<Field> {
    vec![Field {
        id: "duedate".into(),
        name: "Due date".into(),
        kind: "date".into(),
        ..Default::default()
    }]
}

/// The members of the card's board, those whose name or user name has `query`.
pub fn assignable(site: &Site, key: &str, query: &str) -> Result<Vec<Person>, String> {
    let board = board_of(site, key)?;
    let v = jira::get(
        site,
        &format!("/1/boards/{board}/members?fields=fullName,username"),
    )?;
    let query = query.to_lowercase();
    Ok((v.as_array().into_iter().flatten())
        .filter_map(member)
        .filter(|p| p.id.to_lowercase().contains(&query) || p.name.to_lowercase().contains(&query))
        .collect())
}

/// A card is all Trello makes.
pub fn kinds() -> Vec<Kind> {
    vec![Kind {
        id: "card".into(),
        name: "Card".into(),
        subtask: false,
    }]
}

/// What making a card on board `board` asks: its list, the first one unless picked.
pub fn create_screen(site: &Site, board: &str) -> Result<Vec<Field>, String> {
    let lists: Vec<(String, String)> = states(&self::board(site, board)?)
        .into_iter()
        .map(|s| (s.id, s.name))
        .collect();
    Ok(vec![Field {
        id: "idList".into(),
        name: "List".into(),
        required: true,
        kind: "option".into(),
        default: lists.first().cloned(),
        values: lists,
        ..Default::default()
    }])
}

/// The boards, as the projects a card is made in: `(short link, name)`.
pub fn projects(site: &Site) -> Result<Vec<(String, String)>, String> {
    Ok(boards(site)?.into_iter().map(|b| (b.id, b.name)).collect())
}

/// What others did lately on your cards: added you, mentioned you, commented, moved it. The newest first; one news a
/// card and a kind, the latest.
pub fn news(site: &Site, me: &Person) -> Result<Vec<News>, String> {
    let v = jira::get_for(
        site,
        "/1/members/me/notifications?filter=addedToCard,mentionedOnCard,commentCard,changeCard&limit=30\
         &memberCreator_fields=fullName,username",
        "news",
    )?;
    let mut rows: Vec<(String, Option<Row>)> = Vec::new();
    let mut out: Vec<News> = Vec::new();
    for n in v.as_array().into_iter().flatten() {
        let Some(who) = member(&n["memberCreator"]).filter(|w| w.id != me.id) else {
            continue;
        };
        let d = &n["data"];
        let said = d["text"].as_str().unwrap_or_default();
        let (told, says) = match n["type"].as_str().unwrap_or_default() {
            "addedToCard" => (Told::Assigned, format!("{} added you to it", who.name)),
            "mentionedOnCard" => (Told::Mentioned, format!("{} mentioned you", who.name)),
            "commentCard" => (Told::Changed, format!("{} commented", who.name)),
            "changeCard" if d["listAfter"].is_object() => (
                Told::Changed,
                format!("{} moved it to {}", who.name, text(&d["listAfter"]["name"])),
            ),
            _ => continue,
        };
        let key = text(&d["card"]["shortLink"]);
        if out.iter().any(|o| o.row.key == key && o.told == told) {
            continue;
        }
        // The card as it is now, read once; at most a dozen cards a read.
        if !rows.iter().any(|(k, _)| *k == key) {
            if rows.len() >= 12 {
                continue;
            }
            let read = jira::get(
                site,
                &format!(
                    "/1/cards/{key}?fields={CARD}&list=true&board=true&board_fields=shortLink&members=true\
                     &member_fields=fullName,username"
                ),
            );
            rows.push((key.clone(), read.ok().map(|c| read_row(&c))));
        }
        let Some(row) = rows
            .iter()
            .find(|(k, _)| *k == key)
            .and_then(|(_, r)| r.clone())
        else {
            continue;
        };
        // Its mentions by name, of the people the card knows: `@Avery.Quinn`.
        let people: Vec<&Person> = (row.assignee.iter()).chain([&who, me]).collect();
        let named = wiki::mentions_as(said.trim(), |u| {
            (people.iter())
                .find(|p| p.id == u)
                .map(|p| wiki::handle(&p.name))
        });
        out.push(News {
            told,
            id: text(&n["id"]),
            row,
            who: who.name.clone(),
            says,
            text: named.chars().take(160).collect(),
            at: stamp(n["date"].as_str().unwrap_or_default()).unwrap_or_default(),
            was: None,
        });
    }
    out.sort_by_key(|n| std::cmp::Reverse(n.at));
    Ok(out)
}

/// A member's id by their user name: what a card's members are set by.
fn member_id(site: &Site, username: &str) -> Result<String, String> {
    let v = jira::get(
        site,
        &format!("/1/members/{}?fields=id", jira::encode(username)),
    )?;
    v["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{} has no member {username}", site.host()))
}

/// Wiki text the model wrote, as Trello's markdown.
fn markdown(v: &Value) -> String {
    wiki::to_markdown(v.as_str().unwrap_or_default())
}

fn card(site: &Site, key: &str, body: Value) -> Result<Value, String> {
    jira::send(site, "PUT", &format!("/1/cards/{key}"), Some(&body))
}

/// A change the model wrote as Jira takes it, sent as Trello does: a move sets the card's list, an assignee its
/// members, the summary and the description its name and text; comments are its comment actions; an issue made a
/// card in the list picked. The rest is refused with why.
pub fn send(site: &Site, w: &Write) -> Result<Value, String> {
    let body = w.body.clone().unwrap_or(Value::Null);
    if w.path.starts_with("/rest/agile/1.0/epic") {
        return Err("Trello has no epics".into());
    }
    if w.path.starts_with("/rest/agile/") {
        return Err("Trello has no sprints".into());
    }
    if w.path == "/rest/api/2/issueLink" {
        return Err("Trello's cards are not linked".into());
    }
    if w.path == "/rest/api/2/issue" && w.method == "POST" {
        return create(site, &body["fields"]);
    }
    let rest = w
        .path
        .strip_prefix("/rest/api/2/issue/")
        .unwrap_or_default();
    match (w.method, rest.split('/').collect::<Vec<_>>().as_slice()) {
        ("POST", [key, "transitions"]) => {
            card(site, key, json!({ "idList": body["transition"]["id"] }))
        }
        ("PUT", [key, "assignee"]) => {
            let ids = match body["name"].as_str().or(body["accountId"].as_str()) {
                Some(who) => member_id(site, who)?,
                None => String::new(),
            };
            card(site, key, json!({ "idMembers": ids }))
        }
        ("PUT", [key]) => {
            if body["update"].is_object() {
                return Err("A card's labels are changed on Trello".into());
            }
            let mut out = Map::new();
            for (field, v) in body["fields"].as_object().into_iter().flatten() {
                let (to, v) = match field.as_str() {
                    "summary" => ("name", v.clone()),
                    "description" => ("desc", json!(markdown(v))),
                    "duedate" => ("due", v.clone()),
                    "priority" => return Err("Trello's cards have no priority".into()),
                    "labels" => return Err("A card's labels are changed on Trello".into()),
                    other => return Err(format!("Trello's cards have no {other}")),
                };
                out.insert(to.into(), v);
            }
            card(site, key, Value::Object(out))
        }
        ("POST", [key, "comment"]) => jira::send(
            site,
            "POST",
            &format!("/1/cards/{key}/actions/comments"),
            Some(&json!({ "text": markdown(&body["body"]) })),
        ),
        ("PUT", [_, "comment", id]) => jira::send(
            site,
            "PUT",
            &format!("/1/actions/{id}"),
            Some(&json!({ "text": markdown(&body["body"]) })),
        ),
        ("DELETE", [_, "comment", id]) => {
            jira::send(site, "DELETE", &format!("/1/actions/{id}"), None)
        }
        _ => Err(format!("Trello can't take {} {}", w.method, w.path)),
    }
}

/// A card made from the create screen's fields: on the board picked as the project, in the list picked or the
/// board's first; its key, the short link, as Jira answers one made.
fn create(site: &Site, f: &Value) -> Result<Value, String> {
    if f["parent"].is_object() || f["parent"].is_string() {
        return Err("Trello's cards have no sub-tasks".into());
    }
    let list = match f["idList"]["id"].as_str().or(f["idList"].as_str()) {
        Some(list) => list.to_owned(),
        None => {
            let b = board(site, f["project"]["key"].as_str().unwrap_or_default())?;
            (states(&b).into_iter().next())
                .map(|s| s.id)
                .ok_or("The board has no list")?
        }
    };
    let mut card = json!({ "idList": list, "name": f["summary"], "pos": "top" });
    if let Some(d) = f["description"].as_str() {
        card["desc"] = json!(wiki::to_markdown(d));
    }
    if let Some(who) = f["assignee"]["name"].as_str() {
        card["idMembers"] = json!(member_id(site, who)?);
    }
    if let Some(due) = f["duedate"].as_str() {
        card["due"] = json!(due);
    }
    let made = jira::send(site, "POST", "/1/cards", Some(&card))?;
    Ok(json!({ "key": made["shortLink"], "id": made["id"] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_says_its_category_by_name_or_place() {
        assert_eq!(category("Ideas", false), "new");
        assert_eq!(category("Next up", true), "new");
        assert_eq!(category("Doing", false), "indeterminate");
        assert_eq!(category("Done ✓", false), "done");
    }

    #[test]
    fn a_trello_id_tells_when_it_was_made() {
        let at = made_at("6553f1a0aa00000000000000").unwrap();
        assert_eq!(iso(at), "2023-11-14T22:16:00.000Z");
        assert_eq!(stamp(&iso(at)), Some(at));
    }

    #[test]
    fn a_jql_gives_its_words() {
        assert_eq!(
            words_of("text ~ \"rounding\" ORDER BY updated DESC"),
            "rounding"
        );
        assert_eq!(words_of("refund ORDER BY updated"), "refund");
        assert!(card_like("kZ3fG9aB") && !card_like("PAY-412"));
    }

    #[test]
    fn a_board_maps_into_rows() {
        let b = json!({
            "shortLink": "kZ3fG9aB",
            "lists": [{"id": "l1", "name": "Next up"}, {"id": "l2", "name": "Doing"}, {"id": "l3", "name": "Done"}],
            "members": [{"id": "m1", "username": "jhale", "fullName": "Juniper Hale"}],
            "cards": [{"shortLink": "c1aaaaaa", "name": "Refund", "idList": "l2", "idMembers": ["m1"],
                       "dateLastActivity": "2026-11-12T10:00:00.000Z", "labels": [{"name": "", "color": "red"}],
                       "badges": {"comments": 2}}],
        });
        let states = states(&b);
        assert_eq!(
            states
                .iter()
                .map(|s| s.category.as_str())
                .collect::<Vec<_>>(),
            ["new", "indeterminate", "done"]
        );
        let c = &b["cards"][0];
        let row = card_row(
            c,
            states[1].clone(),
            vec![member(&b["members"][0]).unwrap()],
            "kZ3fG9aB",
        );
        assert_eq!(row.key, "c1aaaaaa");
        assert_eq!(row.state.name, "Doing");
        assert_eq!(row.assignee.unwrap().id, "jhale");
        assert_eq!(row.labels, ["red"]);
        assert_eq!((row.comments, row.project.as_str()), (2, "kZ3fG9aB"));
    }

    #[test]
    fn what_trello_has_no_place_for_is_refused() {
        let site = Site {
            url: "https://trello.com".into(),
            email: Some("key".into()),
            token: "token".into(),
        };
        let epic = Write::in_epic("c1aaaaaa", Some("PAY-1"));
        assert_eq!(send(&site, &epic), Err("Trello has no epics".into()));
        let link = Write::linked("Blocks", "c1aaaaaa", "c2aaaaaa");
        assert!(send(&site, &link).is_err());
        let priority = Write::edited("c1aaaaaa", json!({ "priority": { "name": "High" } }));
        assert_eq!(
            send(&site, &priority),
            Err("Trello's cards have no priority".into())
        );
    }
}
