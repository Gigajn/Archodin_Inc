# Ronin talks to GitHub through octocrab, not hand-rolled reqwest

Ronin searches GitHub (repositories and code for `SKILL.md`) and fetches the file through the `octocrab` crate (v0.54.9, MIT/Apache-2.0), behind the existing `SearchEngine` seam in `ronin.rs`. The hand-rolled `GitHubSearch::items()` / `fetch()` goes away; the typed handlers (`search().repositories()`, `search().code()`, `repos().get_content()` / `raw_file()`) replace it. Octocrab is pinned to the exact version (`=0.54.9`) with `default-features = false` and only the `reqwest` feature (plus `retry` only if the rate-limit-aware config is chosen explicitly), so it uses the same reqwest 0.13 Archodin already uses and does not pull in its default hyper client.

Three compensations are mandatory, because the SSRF guard currently lives outside the HTTP client in `ronin.rs::get_text_checked`, not inside it: (1) the injected client is `http_guard::shared_client()` (ADR 0010), so timeout, redirect policy and the guard DNS resolver travel with it; (2) the operator-configured base URI (`api.github.com` by default, or a self-hosted API URL) is validated once with `validate_endpoint` at construction; (3) the body cap is enforced in the wrapper — `get_content` checks `Content.size` before base64 decode, `raw_file` streaming is read through `read_capped` (2 MiB). Without these three, adoption would be a KR-01 regression; with them it is a net gain, because typed handlers mean Ronin no longer fetches arbitrary URLs from search results — only `api.github.com` plus the fixed `SKILL.md` path — so the SSRF surface shrinks.

## Why this crate, and the hard questions

- **Why octocrab and not hand-rolled?** The hand-rolled client re-implements search pagination, content decoding, token auth and rate-limit handling by hand, and each of those is a place to get SSRF or auth wrong. Octocrab is the most mature GitHub client in Rust (1.459 stars, ~762k downloads/month, used by ~356 crates), its reqwest is exactly 0.13 (one version, no duplication), and it accepts a caller-provided `reqwest::Client` as a first-class API (`ReqwestClientConfig::Custom` + `build_with_reqwest`).
- **What if the maintainer stops (bus factor ~1)?** In 2026 the work concentrates on one maintainer (Dmytro Horskyi, 54 of 119 commits). Mitigation: exact pin + lockfile, and the `SearchEngine` seam means the hand-rolled `GitHubSearch` can be restored behind the same interface if octocrab ever has to go.
- **API 0.x — breaking changes every minor?** Yes, 0.54.9 is 0.x and `Error` / `Page<T>` are `#[non_exhaustive]`. The code must not match on them exhaustively and must handle variants loosely, so a minor upgrade does not break the build.
- **What about the Tower `FollowRedirectLayer` vs `shared_client`'s redirect policy?** Unverified at decision time. The integration must run the existing SSRF redirect test (a redirect to a metadata address) against the octocrab path; if Tower follows the redirect before the client policy sees it, the redirect policy has to be set on the Tower layer instead.
- **Retry behaviour?** The default is `RetryConfig::Simple(3)`, not rate-limit-aware. The integration chooses explicitly: either `HandleRateLimits` or no retry, and records the choice in a test.

## Considered Options

- **Keep the hand-rolled GitHub client**: zero new dependencies, but every GitHub behaviour stays hand-maintained and the SSRF surface stays wide (arbitrary hit URLs fetched directly).
- **Use `raw_file` only, skip `get_content`**: simpler, but `get_content` gives the typed `Content` with `size` for the pre-decode cap check; both are kept, each behind the cap.
- **Vendor octocrab's code**: unnecessary — the crate is compatible and maintained; vendoring would freeze a large API surface Archodin would then own.

## Deviation recorded at integration (2026-11-12)

`raw_file` is **not** used, against the "both are kept" line above. `RepoHandler::raw_file` forces a `ref` parameter (changing the request contract) and returns `http::Response<BoxBody>`, which cannot go through `read_capped(reqwest::Response)`; `get_content().raw()` has the same limitation. Only `get_content()` (JSON) is used, behind a three-layer cap: `Content.size` checked before decode, base64 payload length bounded, decoded length checked (needed because a 2 MiB and a 2 MiB+1 payload have the same encoded length after padding). `Content::decoded_content()` is deliberately not used — it `unwrap()`s on bad base64, which would panic on a job path (KR-06). The deviation is sound: the cap is enforced, and no `raw_file` path exists to bypass it.
