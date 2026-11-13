# Workspace configuration (shared)

Written by `agent-rules`; the Git delivery rows are the `git-delivery` skill's. Local rows (scratch root, task naming)
live in `.misc/workspace.md`; on a conflicting value the local table wins.

| Setting | Value |
|---|---|
| Hub tier | minimal: `AGENTS.md` alone, the configuration here as in Artoo's own repository |
| Second agent runtime | none: Codex reads `AGENTS.md`; no skill sync |
| Instructions | none yet |
| Doc language | English |

## Git delivery

| Row | Value |
|---|---|
| Commit subject | Conventional Commits — `<type>(<scope>): <subject>`, imperative, lowercase, no trailing period (9 of 9 so far) |
| Subject enforcement | none — no hook |
| Ticket reference | GitHub issue `#<n>` of Thernal/artoo-tasks: subject ends with `(#<n>)`; the commit that finishes it says `Closes #<n>` in its body |
| Branches | none: work is committed on `main` itself, as in Artoo's repository |
| Subject without a ticket | `chore: <description>`, or the issue opened first |
| Integration | rebase |
| Delivered shape | preserved commits |
| Protected branches | `main` |
| Release | a tag and its zips (`scripts/package.sh`) only when the developer asks |
| Commit attribution | on — `Co-Authored-By` trailer |
| Never staged | `.misc/`, `CLAUDE.local.md`, `.claude/settings.local.json` |

## Artifact placement

Defaults: **local** unless promotion is deliberate; **own** unless a source of truth is named. A skill resolves: its
own default → this table → the local table, last one winning.

| Kind | Root | Visibility | Ownership |
|---|---|---|---|
| Agent entry point | `AGENTS.md`, `CLAUDE.md` | shared | own |
| Workspace configuration | `.agents/workspace.md` + `.misc/workspace.md` | split | own |
| Vendored skills | `.claude/skills/` (`git-delivery`, `task-workspace`, `knowledge-lookup`, `agent-rules`) | shared | foreign — Thernal/knowledge, the commit in each `.origin` |
| Task workspace | `.misc/tasks/` | local | own |
| Reports, logs, drafts | `.misc/` | local | own |
| Fixture | `fixtures/`, from `scripts/fixture.py` | shared | own — generated, kept in git |
