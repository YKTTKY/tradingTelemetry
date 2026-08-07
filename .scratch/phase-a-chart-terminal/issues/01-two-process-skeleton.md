# 01 — Two-process skeleton (uv engine + TUI IPC heartbeat)

**What to build:** A day trader can start a local **market engine** (Python, managed with **uv**) and a **TUI** (Rust/Ratatui). The TUI connects over localhost **HTTP** (snapshot) + **WebSocket**, sees that the engine is up via **feed status**, and can leave a minimal **Welcome** into an empty workspace shell. No market series yet — only the two-process desk and IPC heartbeat.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] Engine project is created and run via **uv** (lockfile, sync, run, test entrypoints exist)
- [x] TUI process starts and connects to the engine over HTTP + WebSocket on localhost
- [x] HTTP serves a snapshot that includes feed status (engine reachable)
- [x] WebSocket accepts a connection and can deliver at least a trivial live event (e.g. heartbeat/status)
- [x] TUI shows Welcome then a shell that reflects connected/disconnected feed status
- [x] Engine defaults to **fake** vendor mode when no real vendor is selected (even if fake serves no bars yet)
- [x] No ZeroMQ, Redis, or Postgres; TUI does not call any market vendor
- [x] Smoke path documented enough to start engine (uv) and TUI (cargo) in two terminals

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- ADRs: two-process Python engine + Rust TUI; HTTP+WS IPC
