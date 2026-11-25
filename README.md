# Tasks — task management for Artoo

An [Artoo](https://github.com/Thernal/artoo) plugin: your tracker's boards and issues as the Tasks space, beside
Inbox, Code, Branches and Merge requests. It reads and writes Jira (Cloud, Server and Data Center) and Trello, and any tracker another plugin adds (A tracker of
your own).

- **A board** (`⌃⌘5`): the side panel's boards under each site's host, filtered by name or key (`⌘P` in the Tasks
  space, its boards beside; Search files elsewhere, an issue's page too), Drafts at its foot; a board's issues as a
  list beside a preview, or as columns (List · Board, `V`), grouped by state, story or epic (`G`) — on the columns a
  lane across them for each story or epic, folding as the list's groups do —, `Mine · All` with
  the people's faces beside them, the filter (`⌘F`). Each row's state chip moves the issue (`⌥S`), its face assigns
  it (`A`, `I` to you; a reassignment says from whom to whom in a toast with Undo, `⌘Z`); the epic, the sprint,
  Blocked by. An issue's key, wherever Tasks writes it, opens the issue in the browser with `⌘` click (`Ctrl`
  elsewhere); a plain click does what the line it is on does.
- **A Jira board by slice**: the facts line's sprint name picks what is shown (`S`): an active or future sprint, All
  active sprints (`A`, a group each), the Backlog (`B`; in Jira's rank, its filter, Mine and epic asked of Jira, 50
  issues at a time) or a past sprint, read only; a board without sprints is shown whole, led by its name. A sprint's
  head says its dates, days left, goal and how far it has got (`24 / 78`); the bar's `All` and `Mine` are Jira's
  counts, and an empty slice says why in one line. An issue moves to a sprint or the Backlog (its top too) from its
  `⋯`, a right click, `M` or the preview's Sprint (`⌥M`); issues checked move together; a toast says where, with
  Undo (`⌘Z`), and a move Jira refuses leaves the row where it was, Jira's words on it.
- **An issue's page**: its facts as controls (state by the workflow, the fields a move asks, assignee, priority,
  type, epic, sprint); the key and the title selected to copy (`⌘C`; the branch name and link in `⋯`); the title and
  the description edited in place, kept on this Mac until saved, a change someone else saved meanwhile compared before
  yours goes; sub-tasks with an empty row (`N`), linked issues (`L`) and attachments, shown while there are none too
  (a file dropped on the page is attached), pictures where they stand in the description and the comments (a click
  opens one whole in Artoo's viewer), details from the edit screen. Comments: yours to everyone, edited and deleted, a reply mentioning who wrote last
  (`R`), people mentioned by name (`@Juniper.Hale`); `⇥` sends it to your agent instead.
- **Someone else's change**: what is on screen is read again as the window comes to the front, and each minute while
  it is there; a line over the title says who changed what, dots mark it. A board's refresh reads its issues (a Jira
  board's, the slice shown); its
  columns, epics and sprints are read again on a reload (`⌘R`) or after ten minutes. Behind, the news is read every
  three minutes.
- **The last boards**: the five read last are kept on the Mac (`boards.json`, yours alone) and shown at the next
  start before the tracker answers, or while it cannot be reached; an account removed takes its boards with it.
- **New task** (`⌥⌘N`): a sheet over the window — project, type, epic, summary, description, assignee, priority,
  labels, sprint, then what the project's create screen requires; `⌘↵` creates it, `⌘S` keeps a draft. From a
  column (its `+`, or a state group's) it says the state it lands in, and once made it is moved there.
- **The Inbox**: an issue assigned to you (also a toast), a mention of you, a change on an issue of yours — each
  switched in Settings → Notifications.
- **⌘K**: the Tasks tab finds the issues read by key or words, and the boards by name; a key not read yet
  (`PAY-427`) is read and is the top hit; in All, Tasks comes first among what is found. Here offers what the page on
  screen does.
- **Notes for your agent**: `+` on a paragraph of the description (`C`), or on a board (`⋯`, Note for an agent);
  the agent reads it with its place, `PAY-412 · description · ¶4`, or the board's issues as filtered.
- **Trello**, behind the same pages: a board's lists are its columns and states, a card an issue (its key the card's
  short link), its first member the assignee; comments with `@user` mentions, attachments, the due date; New task makes
  a card in the list picked; the Inbox has your notifications. What Trello has no place for — epics, sprints,
  priorities, links, sub-tasks — is not shown, and a write of it is refused with why.
- **Tools for agents**, through Artoo's MCP server while Artoo runs: `jira_issue`, `jira_search`, `jira_comment` — a
  comment an agent writes waits on the issue's page for you to post, edit or discard — and `jira_draft`: a new issue
  an agent writes waits under Drafts, `Your agent`, for you to open, check and create; the tool never creates one.

## Connect

An account is kept once, for every project; each project reads the accounts it picks. A project that reads none
yet shows `Connect a tracker` in the Tasks space: the accounts kept, each with Use, and Add… for a new one — its site,
your email (Jira Cloud) or API key (Trello) and a token, in the same card; `+` beside Boards connects another later,
and a host's menu in the side panel stops reading it there. Settings → Accounts → Trackers keeps the accounts: Test,
Change, Remove, Add a tracker. The accounts are kept in
`~/Library/Application Support/Artoo/plugins/artoo-tasks/accounts.txt`, one a line (a second account on the same site has an `id=` of its own; an older `trackers.txt` is carried over once), and which a project reads in
`projects.txt` beside it (the project's folder, a tab, its accounts' hosts):

```text
https://jira.example.com
https://example.atlassian.net you@example.com
https://trello.com <your API key>
https://desk.acme.example kind=acme
```

A site alone is Jira Server or Data Center, read with a personal access token; a site with an email is Jira Cloud,
read with that email and an API token; `trello.com` is Trello, read with your API key and a token made for it
(trello.com/power-ups/admin). A site of a tracker another plugin adds says its kind (`kind=acme`, Accounts writes it):
while that plugin is off, the site is left out and Settings says why, rather than read as Jira. The token is the file
`~/.config/tokens/<host>` (mode 600).

## A tracker of your own

A company's tracker is added by a plugin of its own, in Rust, beside Tasks: it depends on the SDK and on this
repository's `api/` crate (`artoo-tasks-api`) alone, and in its `install` offers its kind — its name, the hosts its
sites are on, what its sign-in asks, what it has (priority, types, sprints, sub-tasks, labels, project keys, a workflow,
links, attachments) — and how a site of it is opened as a `Tracker`:

```rust
fn install(cx: &mut App) {
    let kind = TrackerKind {
        id: "acme".into(),
        name: "Acme Desk".into(),
        hosts: vec!["desk.acme.example".into()],
        unit: "project".into(),
        caps: Caps { priority: true, project_keys: true, ..Caps::NONE },
        ..Default::default()
    };
    artoo_tasks_api::offer(cx, kind, |site| Box::new(Acme { site }) as Box<dyn Tracker>);
}
```

A site on one of its hosts (Settings → Accounts, or `trackers.txt`) is then read and written through it: its boards,
an issue's page, a move, an assignee, an edit, a comment, New task, the Inbox's news, ⌘K and the agents' tools, as
Jira's. What it has not is not drawn. The model speaks Jira's words — field ids and values as Jira's REST takes them,
text in Jira's wiki markup (`artoo_tasks_api::wiki` turns markdown into it and back) — and a write comes as an intent
(`Write::Move`, `Write::Comment`, …) the tracker refuses with a sentence when it has no place for it.

The plugin is built as Tasks is (`scripts/build.sh` in its own folder, the same SDK) and added the same way (Settings →
Plugins → Local, or `ARTOO_PLUGIN_DEV`, several apart by `:`). It needs nothing of Tasks at build time but the crate:
the kind crosses to Tasks as JSON through the app's services (`tasks.tracker.<id>@1`), so the two plugins' builds of
the crate need not be the same build. `examples/sample-tracker` is one: Sample Desk, its tickets in memory, on
`desk.example.org`.

## Build and run

Against a released Artoo: `sh scripts/build.sh`, then Settings → Plugins → Local → Add… this folder (the template's
way, see `scripts/`). Against a clone of Artoo, with its SDK built in `target/debug/deps`:

```sh
ARTOO_PLUGIN_DEV="$PWD/target/debug/libartoo_tasks.dylib" path/to/artoo/target/debug/artoo
```

`ARTOO_TASKS_FIXTURE=<folder>` reads the API's answers from files there instead of the tracker, for pictures that show
no one's real work: `python3 scripts/fixture.py` writes `fixtures/` (Jira's Payments board with two active sprints,
a future one, five closed and a Backlog of 300; the kanban boards Ledger and Platform; Wallet, without sprints; Checkout,
with no sprint running; Trello's Website relaunch). Its own `trackers.txt` names
its sites, a folder named for a site's host holds that site's answers (`_rest_api_2_issue_PAY-412.json`), and a write
is recorded in its `writes.log` and answered from `POST_rest_api_2_issue.json` when there is one. A list is paged by
the read's `startAt` and `maxResults` as Jira pages it; a read of closed sprints, or with JQL for yours, for words or
for an epic, is answered from `<file>@closed.json`, `@mine`, `@words` or `@epic` when there is one; a file
`{"status": 400, "errorMessages": […]}` refuses. Each epic's issues wherever they are answer its whole count, and a
move of PAY-418 into the future sprint is refused (`POST_rest_agile_1.0_sprint_35_issue.json`). `ARTOO_TASKS_FIXTURE_SLOW=<ms>` answers each read that late, to see
what draws while reading, and `ARTOO_TASKS_FIXTURE_FAIL=<part>` fails every read whose address holds it.

## Tests

`sh scripts/check.sh` runs rustfmt, clippy with warnings as errors and the tests — of the plugin, its contract
(`api/`) and `examples/sample-tracker` — against the SDK of the first Artoo release in `artoo-versions.txt` (or
`ARTOO_SDK=<folder>`). A test binary needs the SDK and Rust's own `std` on the library path, which Cargo does not give
it; the script sets it. CI runs the same on every push (`.github/workflows/check.yml`), and a `v*` tag builds the
release for each Artoo release listed (`.github/workflows/release.yml`).
