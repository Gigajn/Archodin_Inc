# fetchkit is reference only — its two useful parts are covered better elsewhere

`fetchkit` (v0.7.2, MIT) is **not** adopted as a dependency and **not** ported. The original VENDOR-PORT verdict from the first review is overturned by the grill, because both components that made it attractive are now covered better by the two adopted crates, and the one component that is unique to it is not needed:

- **GitHub fetching** (`GitHubCodeFetcher`, official Contents API, base64 decode) is covered by octocrab (ADR 0008), which adds what fetchkit lacks: token auth, the rate-limit API, and typed handlers. fetchkit's fetcher sends no token at all, so it hits GitHub's anonymous limit of 60 requests/hour — fatal for Ronin, which fetches many `SKILL.md` files. Porting it would mean re-adding the token support upstream does not have.
- **DNS pinning** (`DnsPolicy`, resolve-then-check plus `resolve_to_addrs` at the transport — the strongest pinning guarantee found) is covered by http-acl (ADR 0010), which closes the same rebinding hole through filter-in-resolver and is a maintained dependency. Porting fetchkit's pinning would mean owning ~600 lines of guard code plus a second egress policy with different defaults (redirect max 10 vs 5, body cap 10 MB vs 2 MiB).
- **HTML→Markdown** (hand-rolled, adapted from Defuddle) is the one part neither adopted crate provides — and Ronin does not need it. Ronin fetches `SKILL.md`, which is already Markdown; search results arrive as JSON with snippets, not HTML pages. If a future need for HTML→Markdown appears, it is a separate decision, not a reason to adopt fetchkit now.

Adopting fetchkit as a direct dependency would also have pulled in a ~382-crate dependency surface (including PDF/Office/JS-runtime crates) and a second egress policy whose defaults differ from `http_guard`'s — two sources of truth for what may leave the machine. `http_guard` stays the single source of truth for egress policy (ADR 0010).

## Considered Options

- **Adopt fetchkit as a dependency**: rejected — dependency surface, second egress policy, and the missing token support.
- **Vendor-port `GitHubCodeFetcher` + `DnsPolicy`**: the original verdict, rejected on grill — both parts are covered better by adopted crates (ADR 0008, ADR 0010), so the port would duplicate them.
- **Reference only (chosen)**: keep fetchkit's `dns.rs` (notably the 6to4 and IPv4-compatible cases) and its `tests/ssrf_security.rs` (38 tests) as a reference for `http_guard`'s own tests, and its redirect-per-hop loop as a design reference if `http_guard` ever needs manual redirect handling.
