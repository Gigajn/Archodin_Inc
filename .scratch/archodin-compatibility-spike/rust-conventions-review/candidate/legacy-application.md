# Basileia and legacy application scope

Read this when maintaining application persistence or the existing AAHC Python-parity router. These are conditional compatibility rules, not generic Archodin requirements or authorization to resume unrevalidated legacy tickets. Consult the current ticket, glossary and ADRs; a legacy migration needs its own approved scope.

## Legacy router parity

For the existing Python-parity router, preserve the established meanings of `NO_KEYS`, `ALL_COOLDOWN`, `ALL_FAILED` and `UNKNOWN_CLUSTER`. Use the existing router contract and Python oracle to determine conditions, ordering and public response shape; do not guess them from the code names. Represent failures with typed Rust variants and retain compatibility mapping at the application boundary. Test each applicable parity case offline through the public router surface and injected transport.

New generic Provider errors describe generic capabilities and failures instead; they do not acquire these legacy codes. Keeping the old mapping in its actual application scope preserves compatibility without leaking policy into the library.

## Application persistence and idle work

Where the application's employee model and NFR-PRF-01 still apply, idle employees are database rows, not one active loop per configured employee. Spawn bounded work when there is a Job and stop/join owned work when it ends. This constraint neither requires a generic library to own employee tables nor decides Basileia's future schema.

Keep application SQL behind a data boundary, outside handlers. Group related multi-table writes in one transaction so a failed deployment does not leave partially applied organizational state. These rules apply when using that persistence model, not to pure Provider or Tool contracts.
