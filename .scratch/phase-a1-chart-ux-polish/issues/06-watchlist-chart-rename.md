# 06 — Watchlist → chart + rename

**What to build:** Enter/Space on selected watchlist row sets **focused chart instrument** (keep timeframe + indicators). Normal mode **`r`** renames active watchlist (prompt, persist, reject empty, allow duplicate display names). Engine additive rename API + workspace file. Indicator panel open: `r` stays re-pin.

**Blocked by:** none (can parallel 01–05; coordinate key routing with 04)

**Status:** done

- [x] Enter/Space load symbol on focused chart only
- [x] Timeframe + indicators preserved on symbol change
- [x] Dual: only focused chart changes
- [x] Inactive during indicator panel / prompt / pin mode
- [x] `r` rename active list in Normal; persisted across restart
- [x] Empty name rejected; ids stable
- [x] IPC contract tests for rename
- [x] Help text updated

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
