# market-engine

Local market engine for Trading Telemetry. Owns vendor I/O, hot state, and the **HTTP + WebSocket** IPC surface for the TUI.

Managed with **[uv](https://github.com/astral-sh/uv)**.

## Setup

```bash
uv sync --extra dev
```

## Run

```bash
uv run market-engine
# uv run market-engine --host 127.0.0.1 --port 8765 --vendor fake
# uv run market-engine --vendor lse   # requires LSE_API_KEY
```

**Vendor modes** (one engine, swappable adapter):

| Mode | Selection | Notes |
|------|-----------|--------|
| `fake` | **default** (`--vendor fake` or omit) | Offline/CI; known fixtures for SPY/QQQ/ES |
| `lse` | `--vendor lse` or `MARKET_ENGINE_VENDOR=lse` | London Strategic Edge; needs `LSE_API_KEY` |

```bash
export LSE_API_KEY=lse_live_xxxxxxxxxxxx   # from https://londonstrategicedge.com/data
uv run market-engine --vendor lse
```

Domain instruments stay canonical (`SPY`, not `SPY:test`). LSE symbol/resolution mapping lives only in the adapter.

**Futures wire symbols:** domain `ES` / `NQ` map to LSE continuous futures `ES.F` / `NQ.F`. Bare `ES` on LSE is the **equity** ticker (Eversource), not E-mini S&P — without this map, watchlist/chart quotes sit around ~$70 instead of index levels (~7700).

## IPC (v1)

| Endpoint | Purpose |
|----------|---------|
| `GET /v1/snapshot` | Bootstrap snapshot: `feed` + `workspace` (layout + charts + watchlists + per-chart indicator configs) + `quotes` + hot `indicators` series |
| `POST /v1/workspace` | Set `layout_mode` (`single` \| `dual-vertical`); persists to disk |
| `POST /v1/chart/interest` | Chart interest: historical OHLCV for `instrument` + `timeframe` (+ optional `chart_id`); arms live updates; persists selection; returns indicator series when configured |
| `POST /v1/indicators` | Full-replace indicator list for one `chart_id` (MA ≤3, Volume ≤1, Session VP ≤1, Fixed Range VP ≤4, Anchored VP ≤2, GEX ≤1, GARCH ≤1); rejects over-limit; persists; returns configs + series (GEX/GARCH may be `status: unavailable`) |
| `POST /v1/watchlist/active` | Switch active watchlist (`watchlist_id`); persists |
| `POST /v1/watchlist/add` | Add `symbol` to the active watchlist; persists; returns workspace + quotes |
| `POST /v1/watchlist/remove` | Remove `symbol` from the active watchlist; persists |
| `POST /v1/watchlist/rename` | Rename active watchlist display name (`name`); empty rejected; duplicate names allowed; id stable; persists; returns workspace + quotes |
| `WS /v1/ws` | Live events: `feed_status`, `heartbeat`, conflated `bar_update` + `quote_update` + `indicator_update` |

**Workspace file:** default `~/.local/share/trading-telemetry/workspace.json` (override with `--workspace` or `MARKET_ENGINE_WORKSPACE`). No Redis/Postgres.

Example snapshot:

```json
{
  "feed": {
    "status": "connected",
    "vendor_mode": "fake",
    "engine": "up"
  },
  "workspace": {
    "layout_mode": "single",
    "charts": [
      {"id": "primary", "instrument": "SPY", "timeframe": "1D", "indicators": []}
    ],
    "watchlists": [
      {"id": "core", "name": "Core", "symbols": ["ES", "NQ", "SPY", "QQQ", "SOXL"]},
      {"id": "focus", "name": "Focus", "symbols": []}
    ],
    "active_watchlist_id": "core"
  },
  "quotes": [
    {"symbol": "SPY", "status": "ok", "last": 548.0, "previous_close": 546.25, "change": 1.75, "change_pct": 0.003203}
  ],
  "indicators": {}
}
```

**Indicators:** Charts open **naked** (`indicators: []`) until configured. `POST /v1/indicators` applies a full list for one chart only (dual charts are independent). Limits are **rejected** (HTTP 422), not clamped: **MA ≤ 3** lines, **Volume ≤ 1**, **Session VP ≤ 1**, **Fixed Range VP ≤ 4**, **Anchored VP ≤ 2**, **GEX ≤ 1**, **GARCH ≤ 1**. MA lines support `ma_type` `sma`|`ema` and `length`; default stack lengths are **10 / 60 / 200**. Volume is a per-bar histogram series. **Session Volume Profile** (`type: session_vp`) is mode **All** only (one profile per day). Session clocks America/New_York: equities/ETFs **16:00 → next day 16:00**; CME **ES/NQ** **prior day 18:00 → 17:00**. Defaults: `rows` **500**, `value_area_volume` **70**, `box_width` **30**, `placement` `right`; histogram + toggleable POC/VAH/VAL styling. Series payload is `profiles[]` with `session_start`/`session_end`, equal-price `bins`, and `poc`/`vah`/`val`. **Fixed Range Volume Profile** (`type: fixed_range_vp`) uses two unix time anchors `start`/`end` (required). `extend_to_right` (default false) when **on** both accumulates bars past `end` and projects POC/VAH/VAL via `levels_end` past the original window; when **off**, only the closed `[start, end]` window counts and levels stay within it. Defaults: `rows` **200**, `value_area_volume` **70**, `box_width` **30**, `placement` `right`. Series `profiles[]` include `range_start`, `range_end`, `anchor_end`, `levels_end`, `extend_to_right`, bins, and levels. **Anchored Volume Profile** (`type: anchored_vp`) uses one unix time `anchor` (required) and always builds forward to the latest bar. Defaults: `rows` **500**, `value_area_volume` **70**, `box_width` **30**, `placement` `right`; histogram + toggleable POC/VAH/VAL styling. Series `profiles[]` include `anchor`, `range_start`, `range_end`, `levels_end`, bins, and levels. Typical anchor: cash open **09:30 America/New_York**. **GEX** (`type: gex`) and **GARCH** (`type: garch`) are optional: configs always apply, but series carry `status: "ok"` only when inputs allow a real compute (options chain for GEX; ≥50 closes for GARCH). Otherwise `status: "unavailable"` with a `reason` and **no invented values**. Fake vendor can `seed_options_chain` for GEX tests; default LSE/fake have no options → GEX unavailable. Configs restore from the workspace file; series recompute on interest and live bar updates (`indicator_update` WS frames).

```bash
curl -s -X POST http://127.0.0.1:8765/v1/indicators \
  -H 'Content-Type: application/json' \
  -d '{"chart_id":"primary","indicators":[
    {"id":"ma10","type":"ma","enabled":true,"ma_type":"sma","length":10},
    {"id":"ma60","type":"ma","enabled":true,"ma_type":"sma","length":60},
    {"id":"ma200","type":"ma","enabled":true,"ma_type":"sma","length":200},
    {"id":"vol","type":"volume","enabled":true},
    {"id":"svp","type":"session_vp","enabled":true,"mode":"all","rows":500,"value_area_volume":70,"box_width":30,"placement":"right"},
    {"id":"frvp1","type":"fixed_range_vp","enabled":true,"start":1719842400,"end":1719864000,"extend_to_right":false,"rows":200,"value_area_volume":70,"box_width":30,"placement":"right"},
    {"id":"avp1","type":"anchored_vp","enabled":true,"anchor":1719840600,"rows":500,"value_area_volume":70,"box_width":30,"placement":"right"},
    {"id":"gex1","type":"gex","enabled":true},
    {"id":"garch1","type":"garch","enabled":true}
  ]}'
```

**Watchlists:** multiple named lists (default **Core** + empty **Focus**). Core first-launch symbols: **ES, NQ, SPY, QQQ, SOXL**, plus **VIX** only when the vendor resolves it. Quote fields: `last`, `previous_close`, `change` (last − previous close), `change_pct` (change / previous close). Unavailable symbols return `status: "unavailable"` without failing the list. Live `quote_update` frames are conflated like bars.

Example chart interest (fake vendor knows **SPY** @ **1D**, **SPY** @ **1h**, **QQQ** @ **1D**, **ES** @ **1D**; unknown pairs return `status: unavailable` with empty `bars`):

```bash
curl -s -X POST http://127.0.0.1:8765/v1/chart/interest \
  -H 'Content-Type: application/json' \
  -d '{"instrument":"SPY","timeframe":"1D","chart_id":"primary"}'
```

```json
{
  "instrument": "SPY",
  "timeframe": "1D",
  "status": "ok",
  "chart_id": "primary",
  "bars": [
    {"ts": 1719792000, "open": 540.0, "high": 540.5, "low": 539.5, "close": 540.0, "volume": 50000000.0}
  ]
}
```

Instrument ids are **canonical** (`SPY`, `QQQ`, …) — never a `:test` suffix. Fake vs real is `vendor_mode`, not ticker encoding.

**Layout / selection:**
- `layout_mode`: `single` (chart id `primary`) or `dual-vertical` (ids `top`, `bottom`, equal-height stack).
- First dual open with no saved dual memory: top **QQQ** @ **1D**, bottom **SPY** @ **1D**. Single default remains **SPY** @ **1D**.
- Single and dual chart memories are independent; toggling layout restores each mode’s last instruments/timeframes.
- Multiple chart slots may hold concurrent interest (dual layout). `chart_id` defaults to `primary` (single) or `top` (dual) when omitted.
- Supported product timeframes: `1m`, `3m`, `5m`, `15m`, `30m`, `1h`, `4h`, `1D`, `1W`. Outside that set (or unknown pairs) → `status: unavailable` with empty `bars` (no invented series).

```bash
curl -s -X POST http://127.0.0.1:8765/v1/workspace \
  -H 'Content-Type: application/json' \
  -d '{"layout_mode":"dual-vertical"}'
```

### Live bar updates (WebSocket)

After chart interest succeeds, the fake vendor can emit ticks (auto random-walk in the CLI process, or `inject_tick` in tests). The engine aggregates ticks into the **last bar** (and rolls a new bar on period boundaries). Updates are **conflated** (~50ms default) so a tick burst yields fewer `bar_update` frames than raw ticks:

```json
{
  "type": "bar_update",
  "instrument": "SPY",
  "timeframe": "1D",
  "completed_bars": [],
  "bar": {"ts": 1720569600, "open": 546.25, "high": 549.25, "low": 545.75, "close": 549.25, "volume": 50910000.0}
}
```

`completed_bars` is non-empty when a period rolls (previous tip closed); `bar` is always the current series tip.

## Test

Primary suite is the **engine IPC seam** (black-box over HTTP+WS):

```bash
uv run pytest -v
# Live LSE integration (optional; skipped without credentials):
# LSE_API_KEY=... uv run pytest -v -k live_lse
```
