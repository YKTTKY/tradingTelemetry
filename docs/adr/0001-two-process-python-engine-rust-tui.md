# Two-process local app: Python market engine + Rust TUI

v1 runs as two local processes. A **Python market engine** owns market-data vendors (primary LSE), candle construction, and indicator computation (MA, volume profile, optional GARCH/GEX). A **Rust (Ratatui) TUI** owns presentation, layout, mouse/keyboard, watchlists, and indicator configuration UI; it does not call LSE directly in v1.

**Why not one process:** Python is the practical home for data SDKs, pandas-style series work, and scientific indicators; Rust is preferred for a responsive, low-overhead terminal UI. Splitting keeps each side in its strength language.

**Why not three processes / Postgres-first:** Single-user desktop desk; extra services add failure modes without clear v1 benefit. Persistence and IPC stay local and minimal until proven insufficient.
