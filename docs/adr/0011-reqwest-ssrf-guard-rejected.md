# reqwest-ssrf-guard is rejected as a dependency (reference only)

`reqwest-ssrf-guard` (v0.2.0, MIT) is **not** adopted as a dependency. It is kept as reference only — its `ip.rs` (notably the NAT64 `64:ff9b::/96` and 6to4 `2002::/16` embedded-IPv4 cases) is a checklist against which `http_guard`'s own IP ranges are compared, and nothing more.

The crate is not rejected for being wrong. Its mechanism is sound: it filters inside `Resolve::resolve()` (one `lookup_host`, results filtered, filtered results handed to reqwest to dial), which closes the classic DNS-rebinding TOCTOU in the standard reqwest path — the same mechanism http-acl uses, verified the same way. Its tests pass (52 unit + 9 integration on Rust 1.99.1). It is rejected on maintenance and installation grounds, against the crate that wins the same slot (ADR 0010):

- **One maintainer, quiet.** 83 commits total, 68 by one person, 15 by renovate; no other contributors; no substantive change since 2026-09-28 (~45 days before review); no `SECURITY.md`, no fuzzing, no independent audit. A security-critical dependency with bus factor 1 and no security process is a liability Archodin would inherit.
- **Four-piece installation.** Full protection needs `configure()` *plus* the `middleware` (or manual `validate_url`) for IP literals, *plus* `.no_proxy()`, *plus* `.https_only(true)` — and the crate does not set scheme checking itself. Forgetting one piece silently drops protection (an IP-literal URL bypasses the resolver entirely; a proxy makes the resolver irrelevant). http-acl's three-piece wiring is already demanding; four pieces with a silent-failure mode each is worse.
- **Unsafe defaults.** `Acl::new()` defaults to allow-all; forgetting `.deny_local_network()` means no protection. `allow_host` disables IP filtering entirely. These are footguns in exactly the place Archodin can least afford them.
- **http-acl already covers the ground.** The richer ACL, the active maintenance, and the shipped IPv6-embedded fix (ADR 0010) make the choice between the two straightforward; adopting both would mean two guard implementations to keep consistent.

## Considered Options

- **Adopt reqwest-ssrf-guard**: rejected for the reasons above.
- **Vendor-port its logic into `http_guard`**: considered, but the port would duplicate what http-acl already provides as a maintained dependency, and Archodin would then own the guard code without the upstream's test suite.
- **Reference only (chosen)**: keep `ip.rs` as a checklist for IP-range completeness; if http-acl ever becomes unmaintained, this crate is the first fallback to re-evaluate.
