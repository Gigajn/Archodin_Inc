# skills_vault

The company's skill warehouse. One folder per skill, discovered automatically by
`init_db.py` (scan) and by `skill_hunter.py` (search, install, verify).

## Folder format

```
skills_vault/
└── <skill_id>/
    ├── SKILL.md           # metadata header + usage instructions (read by the LLM)
    ├── tool_executor.py   # runnable Python executor
    └── requirements.txt   # skill-scoped dependencies
```

### SKILL.md

The first 12 lines carry a `key: value` header. Recognised keys:

| Key | Meaning | Example |
| --- | --- | --- |
| `name` | Display name | `web_search` |
| `category` | `CORE`, `DEV`, `TRADING`, `MARKETING`, `CS` | `CORE` |
| `source_type` | `LOCAL_PYTHON`, `MCP_SERVER`, `API_WRAPPER` | `API_WRAPPER` |
| `install_command` | Recorded for the sandbox runner (never executed by the hunter) | `pip install httpx` |

Everything after the header is prose the agent reads: when to use the skill, usage
examples, and limits.

### tool_executor.py

Must be a standalone script:

- `python tool_executor.py --selftest` → prints `OK`, exits 0, touches no network.
  This is what `verify_skill()` runs in the sandbox before flipping `is_verified`.
- `python tool_executor.py --query "<payload>"` → performs the real work and prints
  JSON on stdout.

### Verification state

A skill only becomes usable once `skills_registry.is_verified = 1`. Registering
(`init_db.py`, `sync_vault_to_registry`) always starts at `0`; only a passing
sandbox smoke test flips it.
