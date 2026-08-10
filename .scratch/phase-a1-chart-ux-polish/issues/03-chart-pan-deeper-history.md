# 03 — Chart pan + deeper history (A2)

**What to build:** **← →** pans the focused chart over **loaded** bars (independent pan state per dual chart). Clamp at ends; optional soft hint at oldest edge. Raise initial history depth for short timeframes (especially **1m**) so previous session/day(s) are reachable without fetch-on-pan. **No** async edge-fetch (B) this ship.

**Blocked by:** 01 — Vendor candlestick widget + price/time axes

**Status:** ready-for-agent

- [ ] ← → pans focused chart in Normal mode; dual charts independent
- [ ] Pan does not block on network
- [ ] Clamp at oldest/newest loaded bar; optional hint at left wall
- [ ] Engine/fake history limit supports multi-day-ish 1m (exact cap documented)
- [ ] Contract or unit coverage for deeper history / pan clamp as applicable
- [ ] Live tip re-attach behavior documented and implemented

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
- Pin placement mode still owns ← → (ticket 04 input routing)
- B (edge fetch) explicitly later
