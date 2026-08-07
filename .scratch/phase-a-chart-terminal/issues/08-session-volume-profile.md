# 08 — Session Volume Profile

**What to build:** The trader can add **Session Volume Profile** on a chart (max **1** per chart, mode **All** only, one profile per day). Session clocks America/New_York: US equities/ETFs **16:00 → next day 16:00**; CME ES/NQ **prior day 18:00 → 17:00**. Parameters: box width, left/right placement, number of rows (default **500**), value area volume (default **70%**), histogram styling, toggleable **POC / VAH / VAL** with color/opacity. Drawn as a horizontal histogram **overlay** on price. Settings restore with the chart.

**Blocked by:** 07 — Indicator panel + MA + Volume (naked → restore)

**Status:** ready-for-agent

- [ ] Session VP can be added via indicator panel; max 1 per chart enforced
- [ ] Mode All only; one profile per day; no pre/RTH/post session modes
- [ ] Day bounds match equities/ETF vs ES/NQ rules in America/New_York
- [ ] Number of rows = equal price buckets across profile high–low (default 500); not ticks-per-row
- [ ] Value area default 70%; POC/VAH/VAL from volume distribution; each level toggleable with style
- [ ] Box width (% of session span) and left/right placement work
- [ ] Overlay render keeps candles readable (opacity tunable)
- [ ] Snapshot/live indicator payloads and workspace restore include Session VP settings
- [ ] Contract tests with fake vendor bars assert POC/VAH/VAL structure and session window behavior

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
