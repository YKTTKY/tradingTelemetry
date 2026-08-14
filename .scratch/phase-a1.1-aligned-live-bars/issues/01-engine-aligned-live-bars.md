# 01 — Engine: aligned live bars + last vendor tick time

**What to build:** Stop dropping live last prices when vendor `tick.ts` is behind the last bar. Re-bucket those prints on **wall clock**. Publish `last_vendor_tick_ts` on the feed snapshot (and a live WS field) so the TUI can show **feed delay**. Do not invent gap bars. Do not change LSE pipes (vault history + existing WS ticks).

**Blocked by:** none

**Status:** ready-for-agent

- [ ] `apply_tick` (or the live apply wrapper) keeps today’s behavior when `bar_open(tick.ts) >= last.ts`
- [ ] When `bar_open(tick.ts) < last.ts`, re-bucket on `time.time()` and update or roll; never fill skipped minutes
- [ ] Wall-clock open still `< last.ts` → still drop (clock skew)
- [ ] Every live vendor tick records raw `tick.ts` as `last_vendor_tick_ts` (even if the bar is placed on wall clock)
- [ ] Snapshot `feed` includes `last_vendor_tick_ts` (omit/null if none)
- [ ] WS exposes the same field (additive on `feed_status` and/or `heartbeat`)
- [ ] Fake-vendor contract test: last bar `T`, tick `ts = T − 30m`, frozen wall `T + 5m` → tip open is wall bucket, close is tick price
- [ ] Existing live-bar / sim-clock tests still pass (in-order sim `ts` unchanged)
- [ ] No TUI work in this ticket (see 02)

## Notes

- Parent spec: `.scratch/phase-a1.1-aligned-live-bars/spec.md`
- Language: `CONTEXT.md` — **aligned live bars**, **feed delay**
- Fake vendor auto-walk uses a history-anchored sim clock; do **not** force those ticks onto wall clock
- Optional short comment on the stale-ts branch: vendor time does not place the bar; delay uses raw `tick.ts`

## Comments
