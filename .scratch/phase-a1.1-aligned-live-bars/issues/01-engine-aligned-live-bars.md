# 01 — Engine: aligned live bars + last vendor tick time

**What to build:** Stop dropping live last prices when vendor `tick.ts` is behind the last bar. Re-bucket those prints on **wall clock**. Publish `last_vendor_tick_ts` on the feed snapshot (and a live WS field) so the TUI can show **feed delay**. Do not invent gap bars. Do not change LSE pipes (vault history + existing WS ticks).

**Blocked by:** none

**Status:** done

- [x] `apply_tick` (or the live apply wrapper) keeps today’s behavior when `bar_open(tick.ts) >= last.ts`
- [x] When `bar_open(tick.ts) < last.ts`, re-bucket on `time.time()` and update or roll; never fill skipped minutes
- [x] Wall-clock open still `< last.ts` → still drop (clock skew)
- [x] Every live vendor tick records raw `tick.ts` as `last_vendor_tick_ts` (even if the bar is placed on wall clock)
- [x] Snapshot `feed` includes `last_vendor_tick_ts` (omit/null if none)
- [x] WS exposes the same field (additive on `feed_status` and/or `heartbeat`)
- [x] Fake-vendor contract test: last bar `T`, tick `ts = T − 30m`, frozen wall `T + 5m` → tip open is wall bucket, close is tick price
- [x] Existing live-bar / sim-clock tests still pass (in-order sim `ts` unchanged)
- [x] No TUI work in this ticket (see 02)

## Notes

- Parent spec: `.scratch/phase-a1.1-aligned-live-bars/spec.md`
- Language: `CONTEXT.md` — **aligned live bars**, **feed delay**
- Fake vendor auto-walk uses a history-anchored sim clock; do **not** force those ticks onto wall clock
- Optional short comment on the stale-ts branch: vendor time does not place the bar; delay uses raw `tick.ts`

## Comments

- **Aligned live bars:** `align_live_tick` in `chart.py` is the live apply wrapper. In-order vendor time still goes to unchanged `apply_tick` (fake sim clock). Stale vendor bar-open re-buckets on `time.time()`; vendor time does not place the bar; delay uses raw `tick.ts`. One hop, no filler bars. Clock skew (wall open still behind last) still drops.
- **last_vendor_tick_ts:** raw `tick.ts` on every live chart/watchlist tick (`FeedState.note_vendor_tick`), including wall-clock place and clock-skew drop. Snapshot `feed.last_vendor_tick_ts` is `null` until the first tick. Same field on `feed_status` (connect) and `heartbeat` (live). Engine does not send wall clock.
- **Tests:** `test_ipc_live_bars.py` — hop at T+5m, same-bucket update, clock-skew drop, raw ts on snapshot+heartbeat. Existing in-order live-bar tests unchanged.
