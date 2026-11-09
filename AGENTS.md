# Archodin_inc

## Agent skills

### Issue tracker

Issues and specs live as local markdown under `.scratch/<feature>/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Default canonical vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: root `GLOSSARY.md` + `docs/adr/`. See `docs/agents/domain.md`.

## Working rules

<!-- managed: agent-repo-ops -->
- **Ground truth first.** Before claiming any path is missing, run `pwd`, `ls`, `git status` in this turn. A failed read at a guessed path is not evidence of absence.
- **One ticket at a time.** Pick from `python3 scripts/frontier.py`; never start a blocked ticket. Treat its output as candidates, not authorization: follow the eligibility and legacy revalidation rules in `docs/agents/issue-tracker.md`.
- **TDD.** Red before green, one seam at a time; never bulk-write tests.
- **Gate before done.** `./scripts/check.sh` must pass; paste the gate output into the ticket's `## Comments` before setting `**Status:** done`.
- **One commit per ticket:** `feat(<scope>): <ticket title>`.
- **Read before write.** Read the code plus `GLOSSARY.md`/relevant ADRs before editing.

## Multi-agent roles

Roles are separated by context; artifacts on disk are the handoff.

| Role | Reads | Writes |
|---|---|---|
| planner | intent, spec | `spec.md`, `docs/adr/` |
| ticket-cutter | spec | `.scratch/<feature>/issues/NN-*.md` |
| implementer | one ticket | code + tests |
| reviewer | the diff | findings in ticket `## Comments` |
| verifier | ticket + gates | gate output in `## Comments` |

Spawn roles as isolated agents with paths and constraints only; never by replaying a conversation.

## Gates

<!-- managed: agent-repo-ops -->
Run `./scripts/check.sh` before any ticket is done. It runs: cargo fmt --check, frontier CLI tests, cargo clippy -D warnings, cargo test --all.
