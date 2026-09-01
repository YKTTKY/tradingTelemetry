# 01 — Paper desk: account, SQLite, additive snapshot, togglable panel

**What to build:** A day trader can open a togglable paper panel and see a local **paper account** (name + USD balance) that survives engine restart. The book lives in **SQLite**, not workspace JSON. Snapshot grows an additive `paper` object so an old TUI still parses. Paper WebSocket events are a **new discrete type** (not latest-wins `bar_update`). No orders, fills, or lines yet.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] First launch with empty SQLite creates one USD **paper account** (implementer-chosen default name and initial balance, visible in settings)
- [x] Multiple **paper accounts** can exist in the book; snapshot lists them and which id is **active**; the panel shows only the active one
- [x] Do **not** invent a **paper account** switching UX (TBD). Tests may select active via IPC. The panel may show the active name.
- [x] Do **not** invent the paper-panel keyboard shortcut (TBD). The panel must be togglable by *some* existing-pattern local UI control so it can hide.
- [x] Workspace JSON role unchanged: layout / charts / watchlists / indicators only. The paper book is not serialized there.
- [x] Engine restart (same SQLite path) restores accounts and the active id
- [x] `GET /v1/snapshot` includes feed, workspace, quotes, indicators, **plus** `paper` (optional/default-empty; missing key must not hard-fail the TUI)
- [x] `last_vendor_tick_ts` still present on feed; **aligned live bars** / feed-delay contract tests still pass
- [x] New paper WS event family exists and is flushed without latest-wins drop (even if this ticket only emits account/desk events)
- [x] TUI deserializes snapshot with and without the `paper` key
- [x] Togglable paper panel shows active account name and balance, with empty **Position** / **filled order history** / **balance history** tables
- [x] When the paper panel is open it owns input focus (same idea as the indicator panel); watchlist arrows and chart pan idle until it closes
- [x] Fake vendor only. No live LSE.

## Notes

- Parent spec: `.scratch/phase-b-paper-m1-nudge/spec.md`
- Language: `CONTEXT.md` — **paper account**, **paper trading**, **Watchlist vs Position**, **New York time**
- Route names are implementer-chosen in the existing `/v1/…` family
- Default numbers must be visible in settings, not code-only constants with no surface
