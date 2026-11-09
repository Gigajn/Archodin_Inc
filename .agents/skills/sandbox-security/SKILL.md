---
name: sandbox-security
description: Use when executing agent-written code or wiring the container runtime. Isolation baseline, docker.sock hazard, and an escape checklist to test before calling it done.
---

# Sandbox security

## Threat model
Agent-written code is untrusted. Assume it will try to read host files, read provider keys from the environment, reach the internet, escape the container, exhaust CPU/memory/PIDs, or leave processes behind.

## Baseline for every sandbox container
- Non-root user (uid 1000+), user namespace remap where available.
- `--cap-drop ALL`, `--security-opt no-new-privileges`.
- Read-only root filesystem; `tmpfs` for `/tmp`; the only writable path is `/workspace`.
- `--network none` by default. Network access is an explicit per-skill allowlist, not an inherited default.
- Limits: `--pids-limit`, `--memory`, `--cpus`, `--ulimit nofile`.
- No `--privileged`, no host PID/IPC namespaces, no device mounts.
- Default seccomp profile; AppArmor where the host provides it.
- The supervisor enforces wall-clock timeout and output-size cap — a child cannot be trusted to police itself.
- One container per project workspace, labelled with the project id; reap orphans at startup and at shutdown.

## docker.sock is root on the host
Mounting `/var/run/docker.sock` into a container that runs agent code gives that code control of the Docker daemon, which is root on the host. The actual need is narrow: create/start/stop/remove labelled containers. In order of preference:

1. A socket proxy with an endpoint allowlist (containers create/start/stop/remove, images pull) and no other API surface.
2. A remote Docker context over TLS with a restricted certificate.
3. Direct mount — only with a written risk acceptance in the spec, never as a convenience.

## Escape checklist — test before claiming isolation
- [ ] Writing outside `/workspace` fails.
- [ ] Reading `/etc/shadow` fails.
- [ ] The process environment contains no provider API keys; pass only what the skill needs.
- [ ] Reaching the cloud metadata address and the host gateway is blocked.
- [ ] A fork bomb hits the PID limit and the container survives while the host stays responsive.
- [ ] `kill -9` on the supervisor leaves no orphan containers or processes.
- [ ] From inside, the Docker API is unreachable.
- [ ] A run that times out is killed, not merely abandoned.

## Secrets
Never forward the whole environment into a sandbox. Build a minimal environment per run and redact it in logs.
