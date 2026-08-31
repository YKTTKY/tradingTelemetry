# Phase A.1.1 — Aligned live bars + feed clocks

Status: done

## Problem Statement

Phase A.1 shipped a usable chart desk. Live **NQ** (and other futures) **candles** still sit minutes behind **QQQ** even though both track Nasdaq-100 and the LSE **websocket last price** is moving.

Root cause (measured 2026-08-14 on `--vendor lse`):

- **History** is LSE **vault** REST (`/vault/candles`). QQQ 1m tip is current; NQ.F 1m tip can sit 15–25+ minutes behind wall.
- **Ticks** are already LSE **WebSocket** (`wss://data-ws.londonstrategicedge.com`), not vault. Last price updates.
- Vendor `tick.ts` is ~25 minutes behind wall on **both** QQQ and NQ.F.
- `apply_tick` **drops** a print when `bar_open(tick.ts) < last_bar.ts`. The NQ forming candle never moves. Countdown clamps at `0:00`.
- Feed status shows raw unix **heartbeat**, not a **wall clock**. There is no **feed delay**, so the tape clock can lie unnoticed.
- The “candle jump” is **missing minutes + dense 1-bar-1-column**, not a bad renderer. No new paint method.

Paper trading remains **out of scope** (Phase B). Fixing LSE vault / `tick.ts` is **later**.

## Solution

Ship **Phase A.1.1 — Aligned live bars**: paint the live tip from websocket **last price** on **New York wall-clock** minutes when vendor time is stale, and show clocks so the remaining feed bug is visible.

1. **Aligned live bars:** if a live print would be dropped because vendor `tick.ts` is behind the last bar, **still apply the price**. Bucket that print on **wall clock** (unix `now` → timeframe open). Do **not** invent the skipped minutes. One hop from last vault bar → current minute is expected.
2. **Wall clock** on Feed status, next to the existing unix heartbeat, in **New York time** (`America/New_York`; label `ET` / `EDT` / `EST` as tzdata reports). Heartbeat stays unix.
3. **Feed delay** on Feed status: `wall − last vendor tick timestamp` (latest `tick.ts` seen on the live socket). Hide when delay is under **5 seconds**. This is how we see the lying tape while candles stay aligned.
4. **Bar countdown** keeps using last painted bar open + timeframe vs wall. After (1), the tip is *now*, so countdown ticks again.
5. **No new candle renderer.** No invented gap bars. No frozen Y-scale that hides API highs/lows.

Product language: `CONTEXT.md` (**aligned live bars**, **wall clock**, **feed delay**, **New York time**, **last bar time**, **feed status**).

## User Stories

1. As a day trader, I want NQ and QQQ live tips to print on the **same wall-clock minutes** from last price, so that two Nasdaq-100 instruments do not look like different tapes.
2. As a day trader, I want a **New York wall clock** on Feed status, so that I can compare the desk to session time without decoding a unix heartbeat.
3. As a day trader, I want **feed delay** when vendor tick time is behind wall, so that I know the tape clock is wrong even though candles are aligned.
4. As a day trader, I want **bar countdown** to keep counting after a timeframe change once a live tip exists, so that `0:00` means the period actually ended — not “we dropped every tick.”
5. As an implementer, I want this behavior under **fake vendor** tests (stale `tick.ts` + controlled wall), so that CI does not need live LSE.
6. As an implementer, I do **not** need to fix LSE vault freshness or vendor `tick.ts` in this ship — only to stop dropping live prices and to expose delay.

## Implementation Decisions

### Scope and phasing

- **In:** wall-clock apply when vendor bar-open is behind last bar; last vendor tick time on feed snapshot/events; TUI wall clock + feed delay; countdown unchanged formula (benefits from new tip).
- **Out:** new candlestick renderer; interpolating missing minutes; frozen/stable Y that ignores API range; per-chart delay chrome (global Feed status is enough); changing LSE symbol map; new vendor; paper trading; async history edge-fetch (B); Phase B.

### Two LSE pipes (do not conflate)

| Pipe | Transport | Role this ship |
|---|---|---|
| History | HTTPS vault `/candles` | Unchanged. Still the left-hand series. May be stale for NQ.F. |
| Ticks | WebSocket | Already subscribed (`NQ` → `NQ.F`). Last price already updates. **This ship applies those prints to the forming bar** when vendor time is stale. |

Do not add a second futures websocket. We are already on it.

### Aligned live bars (engine)

Keep today’s `apply_tick` when `bar_open(tick.ts) >= last.ts` (fake-vendor sim clock and in-order prints stay identical).

When `bar_open(tick.ts) < last.ts` (**today: drop**):

1. Re-bucket on **wall clock**: `open_ts = bar_open_ts(time.time(), timeframe)`.
2. If that `open_ts == last.ts`, update OHLC/close/volume as today.
3. If that `open_ts > last.ts`, roll: complete last, append one new bar at `open_ts` with this print as OHLC. **No** filler bars for the gap.
4. If wall-clock `open_ts < last.ts` (clock skew), still drop.

Record **last vendor tick timestamp** from the raw `tick.ts` on every accepted socket tick (even when the bar is placed on wall clock). That value is **feed delay**, not last bar time.

Fake vendor: existing `inject_tick` / auto-walk with sim `ts` ≥ last bar must keep passing. Add a contract test: last bar at `T`, tick `ts = T − 30m`, freeze wall at `T + 5m` → series tip open is the wall-clock bucket, close is the tick price, **no** drop.

### Feed snapshot / IPC (additive)

Extend feed snapshot (and `feed_status` and/or `heartbeat` — implementer pick, no protocol break) with:

- `last_vendor_tick_ts`: unix seconds of the latest vendor `tick.ts`, or omit/`null` if none yet.

Do not replace heartbeat. Do not send wall clock from the engine (TUI has `now`).

### TUI Feed status

One line, New York time:

`feed: connected  vendor=lse  heartbeat=<unix>  11:17:44 EDT  delay 26m`

- **Wall clock** updates every draw (~100ms) from local now in `America/New_York`.
- **Feed delay** from `now − last_vendor_tick_ts`. Format compact (`26m`, `5s`, `1h 02m`). Hide if `< 5s` or no vendor tick yet.
- Dual layout: still **one** delay (latest vendor tick across the desk).
- Heartbeat remains the engine unix `ts` (connectivity), not a clock.

### Last bar time and countdown

- **Last bar time** = open of the newest **painted** bar (existing rightmost X label, including `*` on live tip). Do not add a third clock for “last print.”
- Countdown formula unchanged: `last_bar_open + timeframe − wall`. After aligned apply, a live tip is the current period, so it counts down instead of sitting on `0:00`.

### Render

No widget/renderer change. Dense columns, API OHLC, empty gaps. The one hop after a stale vault tip is accepted.

### Testing

- Engine: fake-vendor IPC test for stale-`ts` apply + `last_vendor_tick_ts` on snapshot/WS.
- TUI: format helpers for NY wall clock and delay (unit tests). Visual/manual: dual QQQ+NQ 1m tips share the current minute when last prices move.
- CI stays on fake vendor.

## Out of Scope

- Repairing LSE vault NQ.F freshness or vendor `tick.ts`
- A second websocket or different futures symbol
- Invented gap candles; new renderer; locked Y-scale
- Per-instrument delay on chart chrome
- Phase B paper trading
- Async older-bar edge fetch (B)

## Further Notes

- Grill: aligned candles + visible delay; New York time (DST **not** abolished as of 2026-08; House Sunshine Protection Act not law); no new render method.
- ADR: optional when implement lands — *why wall-clock place when vendor ts is stale* (surprising without this spec). Skip if the code comment + this spec are enough.
- Later feed fix: make vault tip and `tick.ts` honest; delay should then collapse toward hidden.
