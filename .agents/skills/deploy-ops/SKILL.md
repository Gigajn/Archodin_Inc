---
name: deploy-ops
description: Use when packaging or operating the 24/7 stack: compose/systemd, restart policy, health checks, log rotation, and RSS/CPU measurement against the Python baseline.
---

# Deploy and ops (24/7)

## Compose baseline
- Compose v2. Services: `aahc-core-backend` (8000), `aahc-office-ui` (3000), `redis-broker` (6379), `chromadb-memory` (8001), `ollama-fallback` (11434).
- Every service: `restart: always`.
- Health check per service, and `depends_on: condition: service_healthy` instead of bare ordering.
- Bind `127.0.0.1:<port>:<port>` by default. Exposing to a network is an explicit decision, never a default.
- Secrets via `env_file: .env`; never baked into the image or the compose file.
- Named volumes for the SQLite file, Chroma data, and Ollama models — containers are disposable, data is not.
- Logging: `json-file` with `max-size: 10m`, `max-file: 3`. Unbounded logs fill the disk and kill the office silently.
- No `privileged: true`. No `/var/run/docker.sock` mount on a container that runs agent code — see the `sandbox-security` skill.

## systemd alternative (when Docker is unavailable)
Use a unit with `Restart=always`, `RestartSec=2`, `EnvironmentFile=`, `WorkingDirectory=`, `ExecStart=`, a non-root `User=`, `NoNewPrivileges=yes`, `ProtectSystem=strict`, and `ReadWritePaths=` limited to the data and workspace directories.

## Verification that actually proves the claim
- `kill -9` the main process, record the timestamp, and show it serving `/health` again. Record the gap in seconds.
- Reboot test: if you cannot reboot the host, simulate stop-all → start-all and say explicitly that this is a simulation.
- RSS: `ps -o rss= -p <pid>` (KiB). CPU: sample `/proc/<pid>/stat` utime+stime deltas over at least 60 seconds, and state the sampling method.
- Measure on the deployment artifact, not on an ad-hoc binary.
- Idle means the office is deployed but has no work. State the load; a number without a load is not a measurement.
- Compare against the Python baseline honestly: 45.7 MB RSS idle, ~0.07% of one core. Publish a smaller or absent saving as it is.

## Failure modes to check
- SQLite on a network filesystem: do not.
- A restart loop hides a crash — check restart counts and logs, not just "it is up".
- Timezone: schedule in UTC, render in local time; never mix.
- Health check that only tests the port, not the database connection, reports healthy while broken.
