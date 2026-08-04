# Local IPC: HTTP + WebSocket (not ZeroMQ in v1)

The Python market engine and Rust TUI communicate on localhost with **HTTP** for commands and state snapshots and **WebSocket** for live updates (quotes, bars, indicator payloads). Payloads may be JSON or MessagePack.

**Why not ZeroMQ in v1:** one TUI subscriber does not need a second messaging stack; HTTP+WS is easier to test, document, and extend (debug pages, extra clients). Localhost latency is not the bottleneck versus vendor feeds and indicator compute.

**Why not Redis/Postgres in v1:** engine memory holds hot state; the engine can snapshot on TUI connect. File-backed workspace and optional parquet cover restart of configuration and history warm-start.

**Evolution:** keep HTTP+WS as the TUI contract. Add Redis or an internal bus (e.g. ZMQ/NATS) later only if multiple local consumers or shared durable cache clearly require it.

**Publish policy:** conflate/throttle UI events; do not forward every market tick to the TUI.
