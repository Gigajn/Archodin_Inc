# Basileia

> **Proprietary — all rights reserved.** This crate is the Basileia
> application built on the MIT-licensed Archodin library (ADR 0007). It is
> NOT open source; see [`LICENSE-BASILEIA`](LICENSE-BASILEIA). The library
> crates it depends on stay MIT (`LICENSE-ARCHODIN` at the repo root), so
> the two licenses never mix silently.

Basileia is the binary that makes the company runnable — the application
the Agora's module docs say "gives it a runner" (audit v2, SD-21: before
this crate, the repo had no `main.rs` / `[[bin]]` anywhere, so nothing
could be started). It assembles:

- **Agora** (`archodin-console`) — the web console: submit Jobs, follow
  them live (SSE), approve/reject, usage. Served **with auth, always**.
- **Graph** (`archodin-graph`) — the durable job machine: every Job runs
  through `Graph::stream_job` with checkpoints, so a Job resumed under the
  same id continues where it stopped; one node asks the Agora for approval
  before it runs.
- **Roster** — the eight company Roles (Strategos, Nakama, Ronin, Vigil,
  Codex, Explorador, Forja, Qadi), restored from the store or seeded on a
  fresh database.
- **SqlStore** (`archodin-store`) — SQLite by default (single file), so
  checkpoints, roster, skills and Akasha survive restarts.

## Run

```sh
# From the repo root. The admin password is REQUIRED — without it the
# server refuses to start (KR-07: auth is mandatory, never opt-in).
BASILEIA_ADMIN_PASSWORD='choose-a-strong-password' cargo run -p basileia
```

Then open `http://127.0.0.1:8080/` and log in with the admin user
(default `admin`) and the password above.

## Configuration (environment)

| Variable | Default | Meaning |
|---|---|---|
| `BASILEIA_BIND` | `127.0.0.1:8080` | Address/port to listen on. `127.0.0.1` = local only; use `0.0.0.0:8080` only behind a trusted network / reverse proxy. |
| `BASILEIA_DB` | `basileia.db` | SQLite file for the SqlStore (created when missing). `:memory:` selects an in-memory store (tests, throwaway runs — nothing survives a restart). |
| `BASILEIA_ADMIN_USER` | `admin` | Username for the Agora login. |
| `BASILEIA_ADMIN_PASSWORD` | *(required)* | Password for the Agora login. No default; empty is rejected. Hashed at startup, never written to disk. |

Notes:

- The default database file `basileia.db` is created in the current
  working directory. Confirm `.gitignore` covers `*.db` (fixed in PR #34)
  before running from the repo root, or set `BASILEIA_DB` to a path
  outside the repo.
- The server is single-user by design (one admin login). Login lockout
  (KR-08) comes with the KR-07 branch this crate is rebased onto: failed
  logins are counted per (client IP, username), 5 failures lock the account
  out with exponential backoff — which is also why the server is started
  with `into_make_service_with_connect_info`, so the real client IP (not
  `"unknown"`) reaches the lockout. Job quotas are builder knobs on the
  same branch (`.max_jobs` / `.max_approvals` / `.max_events_per_job`,
  default 1000 each); this crate leaves them at their defaults.
- The server is plain HTTP by design (no TLS). The session cookie is
  therefore created **without** the `Secure` flag
  (`.secure_cookie(false)` in `src/assembly.rs`): a `Secure` cookie is
  never sent back by a browser over a LAN address, so login would break
  the moment `BASILEIA_BIND=0.0.0.0` is used. Do not expose this server
  directly to the internet — put it behind a trusted network or a reverse
  proxy that terminates TLS, and revisit the cookie flag if TLS ever
  terminates here.
- No trading desk is built here (trading is deferred to the end, ADR 0007).

## Model accounts

Model accounts connect the server to cloud models (ADR 0020–0026).
Each account is four variables, named after its slug (`<ACCOUNT>` is
`[a-z0-9_]+`, e.g. `muse`):

| Variable | Meaning |
|---|---|
| `BASILEIA_MODEL_<ACCOUNT>_KEY` | API key (required, except `ollama`). Never commit it; `sk-canary-…` below is a placeholder. |
| `BASILEIA_MODEL_<ACCOUNT>_KIND` | `openai_compatible`, `gemini`, `claude`, or `ollama`. |
| `BASILEIA_MODEL_<ACCOUNT>_BASE_URL` | Optional — when empty, the per-kind default is used (see below). |
| `BASILEIA_MODEL_<ACCOUNT>_MODEL` | The model name. **A model name never has a default** (ADR 0022/0023, ADR 0026): it must always be written explicitly, here or per Role in the chain. A missing model fails loudly instead of guessing one. |

Per-kind default `BASE_URL` (you may leave `BASE_URL` empty when you
use the default): `openai_compatible` →
`https://api.openai.com/v1`, `gemini` →
`https://generativelanguage.googleapis.com/v1beta`, `claude` →
`https://api.anthropic.com`, `ollama` → `http://127.0.0.1:11434`.

### Vault key file (runbook)

The Vault key encrypts every model key the server stores, so it must
survive restarts. The chosen path is a **file, mode 0600** (ADR 0026),
read through `BASILEIA_VAULT_KEY_FILE`:

```sh
# 1. Generate a key (prints base64 of 32 random bytes; nothing is stored).
cargo run -p basileia -- generate-vault-key

# 2. Write it to a file that is NEVER created 0644: either create it
#    with a restrictive umask, or fix the mode right after writing.
umask 077
cargo run -p basileia -- generate-vault-key > ~/.config/basileia/vault.key
# …or, if the file already exists with the wrong mode:
install -m 0600 /dev/null ~/.config/basileia/vault.key
cargo run -p basileia -- generate-vault-key > ~/.config/basileia/vault.key
chmod 0600 ~/.config/basileia/vault.key

# 3. Point the server at it.
BASILEIA_VAULT_KEY_FILE="$HOME/.config/basileia/vault.key"
```

Behaviour: a file more permissive than `0600` is **refused at startup**
before its contents are read. `BASILEIA_VAULT_KEY` (inline base64) is
only a fallback — it is read first when set and non-empty, and the file
is read only when it is absent or empty. If neither is set, the server
still starts in **degraded** mode: env-provisioned keys keep working,
but accounts stored from the Agora are reported `locked` until a restart
with the Vault key (ADR 0021 §Decision 4). Never put the Vault key in
the repo, in the database, or through the Agora API.

### Example environment: the four cloud models

```sh
# Claude (native client). BASE_URL may be omitted: it defaults to
# https://api.anthropic.com. Replace <nama-model-claude> with the model
# you use — the provider's model list is the source of truth.
BASILEIA_MODEL_claude_KEY='sk-canary-claude'
BASILEIA_MODEL_claude_KIND='claude'
BASILEIA_MODEL_claude_MODEL='<nama-model-claude>'

# Gemini (native client). BASE_URL may be omitted: it defaults to
# https://generativelanguage.googleapis.com/v1beta.
BASILEIA_MODEL_gemini_KEY='sk-canary-gemini'
BASILEIA_MODEL_gemini_KIND='gemini'
BASILEIA_MODEL_gemini_MODEL='<nama-model-gemini>'

# ChatGPT (OpenAI, via openai_compatible). BASE_URL may be omitted: it
# defaults to https://api.openai.com/v1.
BASILEIA_MODEL_chatgpt_KEY='sk-canary-openai'
BASILEIA_MODEL_chatgpt_KIND='openai_compatible'
BASILEIA_MODEL_chatgpt_MODEL='<nama-model-openai>'

# Muse (Meta, via OpenRouter — kind openai_compatible, ADR 0026).
# BASE_URL MUST be set here: the default for openai_compatible is
# OpenAI's, not OpenRouter's. MODEL below is a configuration example,
# not a code default.
BASILEIA_MODEL_muse_KEY='sk-canary-openrouter'
BASILEIA_MODEL_muse_KIND='openai_compatible'
BASILEIA_MODEL_muse_BASE_URL='https://openrouter.ai/api/v1'
BASILEIA_MODEL_muse_MODEL='meta/muse-spark-open'
```

Notes:

- There is no `muse` or `openrouter` kind on purpose (ADR 0026):
  a kind is the shape of the API, not a model name. Anything that
  speaks the OpenAI API — including OpenRouter — is
  `openai_compatible` + `BASE_URL`.
- A `402` from a provider (no credits, typical of OpenRouter) is
  reported as its own error (`ProviderPaymentRequired` / health status
  `payment_required`): top the account up, do not chase the network
  and do not retry blindly.
- Health check caveat (ADR 0026, C5): OpenRouter's `GET /v1/models`
  is public and does not validate the key, so an `ok` health check
  there is not proof the key is valid. Health checks have never been
  run against a live provider yet — verify them by hand (C4).

### Ollama (local models) — reserved for later

Ollama is **not active now**: a slot is reserved for later (ADR 0026),
while the cloud models above are used. When the time comes, the shape
is (commented out on purpose — do not enable it yet):

```sh
# BASILEIA_MODEL_local_KIND='ollama'
# BASILEIA_MODEL_local_BASE_URL='http://127.0.0.1:11434'
# BASILEIA_MODEL_local_MODEL='<nama-model-ollama>'
# (no KEY: an ollama account needs no key; loopback-only.)
```

## What a Job does (current scope)

A submitted Job runs `intake` → `acknowledge` → `finish`:

1. `intake` records the request.
2. `acknowledge` pauses for a human approval in the Agora (approve to
   continue, reject to stop the Job).
3. `finish` writes the outcome.

Every step is checkpointed to the SqlStore. This is a real, durable job
pipeline — but the nodes are deterministic functions, not LLM agents yet:
the Roles currently have no Tools and no model chains (those are code and
hold API keys, so they cannot come from the store; choosing a provider
and its keys is a separate, credential-bearing step). Wiring real
agent brains (Strategos, Nakama, …) is the documented follow-up; it does
not change this crate's assembly.

## Tests

```sh
cargo test -p basileia
```

The smoke tests build the whole application state (store, roster, graph,
Agora with auth, router) without binding any network socket, and prove
assembly refuses to start without an admin password. They also cover
`Config::from_env` (reads, missing/empty password, bad bind address) and
the approval round-trip: a Job pauses at `acknowledge` with no answer,
then resumes from its checkpoint and finishes once approved
(`ScriptedApprover` stands in for the human at the console).
**Status: not yet executed** — no local cargo toolchain was available
when this crate was written; run them locally before merging.

## Status / known gaps (read before merging)

- **Not compiled yet.** Every call site in `src/assembly.rs` was verified
  line-by-line against the code actually read from `main` @ `520e6e17`
  (`Graph::builder` + `.entry`, `Auth::from_vault`, `hash_password`,
  `AgoraBuilder`, `pump`, `JobContext`, `SqlStore`, `Roster`,
  `save_roster` / `restore_roster` — see the module docs in
  `src/assembly.rs` for the exact file:line list), but the crate has
  never been compiled: there is no local checkout of the repo and no
  vendored dependencies in this environment. Run `cargo test -p basileia`
  + `cargo run -p basileia` (with `BASILEIA_ADMIN_PASSWORD` set) locally
  before merging.
- **Login end-to-end still needs a manual browser check.** The cookie
  reasoning above (plain HTTP → `Secure` off) is verified against the
  code, but nobody has actually opened `http://127.0.0.1:8080/`, logged
  in, and confirmed the session survives — and the same over a LAN IP
  with `BASILEIA_BIND=0.0.0.0`. Do that once, by hand, before merging.
- **Merge order (KR-07):** the KR-07 branch makes Agora auth mandatory
  by default and adds login lockout + job quotas. This crate already
  passes `.auth(...)`, and `Auth::from_vault` is identical in `main` and
  on the KR-07 branch, so no code change is needed — but rebase onto
  KR-07 (or merge KR-07 first) before merging, so the lockout and quota
  guarantees actually hold for the running server. The KR-04 checkpoint
  change needs no code change here either (this crate never calls
  `CheckpointStore::save()` directly), but expect its behaviour to
  change: a failed checkpoint save becomes a terminal event instead of
  a panic.
- **Open conflict (needs the owner's confirmation):** the finish plan
  (`archodin-finish-plan.md` §5) says the application binary lives
  OUTSIDE the repo, while ADR 0007 calls Basileia an "application crate"
  and the owner's latest goal (2026-11-10) is a runnable server for the
  Android chat app. This crate resolves it by building INSIDE the repo —
  the only way the system becomes runnable at all — and flags the conflict
  here explicitly. If the owner confirms "outside the repo", this crate
  moves out (e.g. to its own private repo) unchanged in structure.
- **Skills not re-read from the repo:** the `to-spec` / `to-tickets` /
  `rust-conventions` / `git-guardrails-claude-code` skill files were not
  re-read from `.agents/skills/` in this retry (rate-limit caution after
  the first attempt's 429); the `tdd` skill was consulted via the local
  copy and the fixes follow plain Rust conventions (no `unwrap`/`expect`
  in runtime paths — all failures are `Result`s; tests at public seams).
