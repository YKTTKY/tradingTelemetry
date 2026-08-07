# 06 — Watchlist sidebar (multi-list + live quotes)

**What to build:** A **watchlist sidebar** docked on the **right**, **show/hide** togglable, with **multiple named watchlists** and a switcher for the active list. Default list (e.g. Core): **ES, NQ, SPY, QQQ, SOXL**, and **VIX** only if the vendor resolves it. Rows: **symbol**, **last**, **change**, **change%** (change = last − previous day close); up green, down red. Add/remove symbols; live **conflated** quotes. Partial unavailable symbols must not brick the whole list. No logos.

**Blocked by:** 03 — Live conflated bar updates

**Status:** ready-for-human

- [x] Sidebar docks right and can be shown/hidden
- [x] Multiple named watchlists; user can switch which list is active
- [x] First-launch default list includes ES, NQ, SPY, QQQ, SOXL; VIX included only if vendor resolves it
- [x] Rows show symbol, last, change, change%; green/red for direction
- [x] User can add a symbol to the active list and remove a symbol
- [x] Last/change fields update from live conflated quotes over IPC
- [x] Unavailable symbol is omitted or marked unavailable without failing the whole watchlist
- [x] No logos on rows
- [x] Watchlist membership persists with workspace where workspace store already exists (or is extended here)
- [x] Contract tests cover quote fields and multi-list behavior with fake vendor

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Watchlist ≠ Position (paper is out of Phase A)

## Implementation notes

- Engine IPC: snapshot includes `workspace.watchlists` + `active_watchlist_id` + top-level `quotes`; mutations via `POST /v1/watchlist/{active,add,remove}`; live `quote_update` conflated on WS.
- Default lists: **Core** (product defaults) + empty **Focus** for multi-list switcher.
- TUI: `w` toggle sidebar, `n`/`p` cycle lists, `a` add, `x`/`d` remove, ↑/↓ select row.
