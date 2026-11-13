# Tasks — rules for agents

What the plugin is and what it does: `README.md`. It is one Rust package (`artoo-tasks`, a library Artoo loads), the
contract crate other tracker plugins build against (`api/`, `artoo-tasks-api`) and an example of such a plugin
(`examples/sample-tracker`). Its screens and decisions come from Artoo's Design repository
(`docs/specs/task-management.md`); a new feature starts there. One workflow: change the plugin, build it against
Artoo's SDK, test it, try it in Artoo on the fixture.

Configuration: `.agents/workspace.md` (shared) and `.misc/workspace.md` (local; on a conflicting value the local table
wins).

- **Everything written here is English** — code, comments, docs, commits.
- **The SDK is no Cargo dependency.** rustc is given it by flags, as std is: build with `sh scripts/build.sh
  [debug|release] [<artoo-version>] [<target>]` (the version defaults to the first line of `artoo-versions.txt`; the
  SDK is fetched into `.artoo-sdk/`, or `ARTOO_SDK=<folder>` names one). A plain `cargo build`, `cargo test` or
  `cargo clippy` fails without those flags. The flags reach the contract crate too.
- **Before a commit:** `sh scripts/check.sh` passes — rustfmt, clippy with warnings as errors and the tests of the
  plugin, its contract and the example plugin, built with the same flags (a test needs the SDK and Rust's `std` on the
  library path, which it sets). CI runs it on every push to `main` (`.github/workflows/check.yml`).
- **A release** is `sh scripts/package.sh <artoo-version> <target>`: `dist/<id>-<artoo-version>-<target>.zip`, the
  name Artoo's Settings → Plugins looks for. Only a build for the Artoo version it was built against is installable.
- **Real trackers are read, never written, while developing.** Writes are tried on the fixture alone
  (`ARTOO_TASKS_FIXTURE=fixtures`, written by `python3 scripts/fixture.py`; a write is recorded in its `writes.log`).
  Fixtures, tests, pictures and commits hold invented people and issues, never a real colleague, site or ticket.
- **The contract crate (`api/`) crosses plugins as JSON.** A type compiled into two plugins' builds is two types to
  `TypeId`, so only std and SDK types cross Artoo's services: a kind as a `Value`, a tracker as one call by name. A
  field added later is `#[serde(default)]`, so an older build still reads it; a change an older build cannot read is
  a new major, a new key (`tasks.tracker.<id>@2`, `MAJOR`).
- **The model speaks Jira's words** (field ids and values as Jira's REST takes them, wiki markup); every other tracker
  maps onto them, and a write comes as an intent (`Write`) a tracker refuses in a sentence when it has no place for it.
  A page asks what a tracker has (`Caps`), never which tracker it is.

## Scratch workspace

`.misc/` is git-ignored (`.gitignore`) and used by default, without being asked, for anything that is not a finished
deliverable. When something is mentioned without a path ("the log", "the review report"), look here first.

| Material | Directory | Naming |
|---|---|---|
| Task brief, plan, progress, questions | `tasks/` | per the `task-workspace` skill |
| Review, audit or architecture report | `reports/` | `YYYY-MM-DD-topic-review.md` |
| Logs and diagnostic dumps | `logs/` | dated, topic-named |
| Builds, pictures, patches made for a task | `artifacts/` | task-named subdirectory |

Durable documentation goes to `README.md` or here, never to `.misc/`; nothing in `.misc/` is cited as the source of a
rule.

## Task workspace

A task large enough for a plan gets a directory under `.misc/tasks/`: `task.md` first, then `plan.md`, then
`progress.md` as soon as work starts — the `task-workspace` skill is the protocol. Its settings are the local table's
(naming, register, `merge_request.md`, `assets/`). Work spanning Artoo's repositories (the app, Design and this
plugin) is planned where the local table says.

## Commits and delivery

Commit splitting, subject format, branches, integration, pushing and attribution are the `git-delivery` skill; read it
before committing, branching, rebasing or pushing. Its settings are `.agents/workspace.md` → Git delivery.

Preserve the developer's unrelated changes: do not stage, commit, revert or reformat their work unless asked; leave
changes in the working tree unless a commit was asked for. Answer in the developer's language; code and docs stay
English. Report what changed, the key files, what was verified and what was not. Never imply a check that was not
run: "checked" means run or read, not reasoned about.

## Protected files

Generated or machine-owned, never committed or edited by hand: `target/`, `examples/*/target`, `.artoo-sdk/`,
`dist/`, `Cargo.lock`, `writes.log`. `fixtures/` is generated too, but kept in git: change `scripts/fixture.py` and run
it again rather than editing a fixture by hand. Never commit a local SDK path, a token or a site's real address.

## Code comments

- A doc line says what an item is in the domain's words, as the code around it does; a comment says why, not what.
  Self-explanatory code carries none, and a name that needs a comment usually needs a better name.
- No comment teaches Rust, GPUI or a tracker's REST; one justifies a choice of this plugin or a constraint from
  outside it. Never repeat a reason the declaration already gives at its call sites.
- A comment that no longer matches the code is corrected or deleted with the change.
- A comment that points outside the code carries a tag on one line: `TODO(ref)` (work not done; an issue),
  `FIXME(ref)` (wrong behaviour shipped knowingly; an issue), `WORKAROUND(ref)` (a library's or a tracker's bug; its
  upstream issue or version). A tag without its reference is a finding.

## Do not

- Do not write to a real tracker to try a change; the fixture takes writes.
- Do not add a type of `api/` to what crosses plugins in place of JSON, and do not change what an older build reads
  without a new major.
- Do not add the SDK as a Cargo dependency or build without `scripts/build.sh`'s flags.
- Do not put real names, sites, tickets or tokens into fixtures, tests, pictures or commits.
- Do not commit `.misc/`, `CLAUDE.local.md` or `.claude/settings.local.json`.
