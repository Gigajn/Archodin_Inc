# Archodin is an MIT-licensed library; Basileia is the proprietary application

Archodin is an MIT-licensed library. The Basileia application built on it — the binary that assembles Agora, the Graph, the Roster, and the SqlStore into the running company — is proprietary (all rights reserved) and stays private. This supersedes ADR 0003, which said Archodin's own code is closed-source, and settles the spec conflict found by the 2026-11-10 audit: issue #2 described Archodin as closed-source on a langchain-rust soft fork, while the Basileia spec in `.scratch/` describes Archodin as an independent MIT library with Basileia as the proprietary application. The Basileia spec is authoritative from now on.

The imported langchain-rust code stays under its MIT license (ADR 0001 is unchanged: upstream files are not patched, and fixes sent back upstream are MIT by definition). Archodin's own library crates are MIT; the Basileia application crate is proprietary and must carry its own proprietary notice, so the two licenses never mix silently. Trading (AAHC tickets 11 paper + 12 LIVE) stays in scope but is deferred to the end. The Python `backend/` is deleted (ticket 17); the product is full Rust.

## Considered Options

- **Everything closed-source**: simpler (one repo, one license, no boundary to police), but the library could never be shared or reused by others.
- **Everything MIT**: simplest licensing, but the Basileia application would be open too, which the owner does not want.
