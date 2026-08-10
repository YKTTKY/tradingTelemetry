# 06 — Watchlist → chart + rename

**What to build:** Enter/Space on selected watchlist row sets **focused chart instrument** (keep timeframe + indicators). Normal mode **`r`** renames active watchlist (prompt, persist, reject empty, allow duplicate display names). Engine additive rename API + workspace file. Indicator panel open: `r` stays re-pin.

**Blocked by:** none (can parallel 01–05; coordinate key routing with 04)

**Status:** ready-for-agent

- [ ] Enter/Space load symbol on focused chart only
- [ ] Timeframe + indicators preserved on symbol change
- [ ] Dual: only focused chart changes
- [ ] Inactive during indicator panel / prompt / pin mode
- [ ] `r` rename active list in Normal; persisted across restart
- [ ] Empty name rejected; ids stable
- [ ] IPC contract tests for rename
- [ ] Help text updated

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
