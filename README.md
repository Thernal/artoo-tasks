# Tasks — task management for Artoo

An [Artoo](https://github.com/Thernal/artoo) plugin: your tracker's boards and issues as the Tasks space, beside
Inbox, Code, Branches and Merge requests. The first stage reads Jira; nothing is written to the tracker.

- **Tasks** (`⌃⌘5`): the side panel's boards; a board's issues grouped by state, `Mine · All`; an issue's page — its
  facts, description, sub-tasks, comments and details.
- **A note for your agent on a paragraph** of an issue's description: `+` on the paragraph's edge, the app's
  composer, `⌘↵` adds it to your drafts in the Agents window, where it waits and goes like a note on a line. The agent
  reads it as `PAY-412 · description · ¶4` with the paragraph as its context.
- **Tools for agents**, through Artoo's MCP server while Artoo runs: `jira_issue` (an issue whole) and `jira_search`
  (a JQL query).

## Connect

Name each site on a line of `~/Library/Application Support/Artoo/plugins/artoo-tasks/trackers.txt`:

```text
https://jira.example.com
https://example.atlassian.net you@example.com
```

A site alone is Jira Server or Data Center, read with a personal access token; a site with an email is Jira Cloud,
read with that email and an API token. The token is the file `~/.config/tokens/<host>`.

## Build and run

Against a released Artoo: `sh scripts/build.sh`, then Settings → Plugins → Local → Add… this folder (the template's
way, see `scripts/`). Against a clone of Artoo, with its SDK built in `target/debug/deps`:

```sh
ARTOO_PLUGIN_DEV="$PWD/target/debug/libartoo_tasks.dylib" path/to/artoo/target/debug/artoo
```

`ARTOO_TASKS_FIXTURE=<folder>` reads the API's answers from files there instead of the tracker
(`_rest_api_2_issue_PAY-412.json`), for pictures that show no one's real work.

## Tests

`cargo test` builds the tests against the SDK like the plugin; the test binary then needs the SDK and Rust's own `std`
on the library path, which Cargo does not give it on macOS:

```sh
DYLD_LIBRARY_PATH="path/to/artoo/target/debug/deps:$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/lib" \
  target/debug/deps/artoo_tasks-<hash>
```
