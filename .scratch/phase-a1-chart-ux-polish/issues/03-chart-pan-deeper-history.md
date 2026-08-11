# 03 — Chart pan + deeper history (A2)

**What to build:** **← →** pans the focused chart over **loaded** bars (independent pan state per dual chart). Clamp at ends; optional soft hint at oldest edge. Raise initial history depth for short timeframes (especially **1m**) so previous session/day(s) are reachable without fetch-on-pan. **No** async edge-fetch (B) this ship.

**Blocked by:** 01 — Vendor candlestick widget + price/time axes

**Status:** done

- [x] ← → pans focused chart in Normal mode; dual charts independent
- [x] Pan does not block on network
- [x] Clamp at oldest/newest loaded bar; optional hint at left wall
- [x] Engine/fake history limit supports multi-day-ish 1m (exact cap documented)
- [x] Contract or unit coverage for deeper history / pan clamp as applicable
- [x] Live tip re-attach behavior documented and implemented

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
- Pin placement mode still owns ← → (ticket 04 input routing)
- B (edge fetch) explicitly later

## Comments

- **TUI pan:** per-chart `pan_cursor_ts` / `pan_at_oldest`; `App::pan_focused_chart` steps by bar index over loaded series only. Normal mode ← →; pin/prompt/panel modes leave pan alone. Soft hint `· oldest loaded` in chart subtitle; `· panned` when away from tip.
- **Live tip re-attach:** pan back to newest bar clears cursor (`None`) so right edge follows live bar updates; documented on `Chart` + `pan_focused_chart`. Instrument/TF reload resets pan.
- **A2 history caps** (`market_engine.vendor.HISTORY_LIMIT_BY_TIMEFRAME`): 1m=3900, 3m=2000, 5m=1500, 15m=1000, 30m=750, 1h/4h/1D=500, 1W=260. LSE uses per-TF limit (newest page). Fake returns full seeded series; IPC test seeds 800×1m.
- Tests: engine `test_lse_history_limit_deeper_for_1m_than_1d`, `test_chart_interest_seeded_deep_1m_returns_full_loaded_series`; TUI pan clamp / dual independence / tip re-attach / panel no-op.
