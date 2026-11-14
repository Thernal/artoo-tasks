//! Which accounts a project's Tasks space reads: `<data>/plugins/artoo-tasks/projects.txt`, a line per project, its
//! folder then its accounts' hosts, a tab between (`/work/pay\tacme.atlassian.net trello.com`). The accounts
//! themselves are everyone's (`trackers.txt`); a project with no line has none yet, and its space asks which. A
//! fixture's links are its own `projects.txt`, written beside its `writes.log` and as ignored.

use std::collections::HashMap;
use std::path::PathBuf;

/// Each project's accounts, by host, as linked.
pub type Links = HashMap<String, Vec<String>>;

fn file() -> Option<PathBuf> {
    match crate::jira::fixture() {
        Some(dir) => Some(dir.join("projects.txt")),
        None => Some(crate::keep::folder()?.join("projects.txt")),
    }
}

/// Every project's accounts, as the file says.
pub fn read() -> Links {
    let text = file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .unwrap_or_default();
    parse(&text)
}

fn parse(text: &str) -> Links {
    (text.lines())
        .filter_map(|line| {
            let (project, hosts) = line.split_once('\t')?;
            let hosts: Vec<String> = hosts.split_whitespace().map(str::to_owned).collect();
            (!project.is_empty()).then(|| (project.to_owned(), hosts))
        })
        .collect()
}

fn text_of(links: &Links) -> String {
    let mut projects: Vec<_> = links.iter().filter(|(_, h)| !h.is_empty()).collect();
    projects.sort();
    (projects.into_iter())
        .map(|(p, hosts)| format!("{p}\t{}\n", hosts.join(" ")))
        .collect()
}

/// `links` with account `host` linked to `project` (`on`) or not, written.
pub fn set(links: &mut Links, project: &str, host: &str, on: bool) -> Result<(), String> {
    let hosts = links.entry(project.to_owned()).or_default();
    hosts.retain(|h| h != host);
    if on {
        hosts.push(host.to_owned());
    }
    let file = file().ok_or("No data folder")?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file, text_of(links)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_line_names_its_accounts() {
        let links = parse("/w/pay\tacme.atlassian.net trello.com\n/w/web\t\nbroken line\n");
        assert_eq!(
            links["/w/pay"],
            vec!["acme.atlassian.net".to_owned(), "trello.com".to_owned()]
        );
        assert!(links["/w/web"].is_empty());
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn a_project_without_accounts_is_not_written() {
        let mut links = parse("/w/pay\tacme.atlassian.net\n/w/web\ttrello.com\n");
        links.get_mut("/w/web").unwrap().clear();
        assert_eq!(text_of(&links), "/w/pay\tacme.atlassian.net\n");
    }
}
