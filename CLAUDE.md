# Archodin

Archodin is a closed-source Rust library for an AI-staffed company, built on the langchain-rust code in this repo (see `GLOSSARY.md` and `docs/adr/`). Upstream langchain-rust files stay untouched except for fixes we would send back upstream (ADR 0001); Archodin code lives in its own crates.

Write code, docs, GLOSSARY and ADRs in English; talk with Kudo in Indonesian.

## Agent skills

### Issue tracker

Issues and specs live in GitHub Issues on Gigajn/Archodin_Inc. See `docs/agents/issue-tracker.md`.

### Triage labels

The five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.
