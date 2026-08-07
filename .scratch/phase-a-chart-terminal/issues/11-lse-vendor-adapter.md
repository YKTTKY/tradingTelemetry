# 11 — LSE primary vendor adapter

**What to build:** Production market data uses **London Strategic Edge (LSE)** behind the **same vendor seam** as the fake adapter. The trader (or implementer) can run one engine entrypoint and select vendor mode — **fake** (default for CI/offline) or **lse** (credentials/env required) — without a second forked application. Domain instruments stay canonical (`SPY`, not `SPY:test`). History + live interest work against LSE when selected; TUI still never calls LSE. Feed status reports the active vendor.

**Blocked by:** 02 — History candles for default workspace (fake vendor)

**Status:** done

- [x] LSE adapter implements the same vendor interface as the fake adapter (history + live interest + unavailable)
- [x] Single engine CLI/entrypoint supports vendor selection (e.g. `--vendor fake|lse` and/or env); **fake is default**
- [x] Optional thin wrappers are OK only if they call the same binary with different flags — no divergent codepaths
- [x] Domain/IPC instruments remain canonical symbols; LSE mapping stays inside the adapter
- [x] With `--vendor lse` and valid credentials, chart interest can load history and receive live updates for supported instruments
- [x] Unavailable LSE symbols/timeframes surface as **Data Currently not Available** (same product behavior as fake)
- [x] Feed status reports vendor mode **lse** (or equivalent) when active
- [x] Default CI stays green on **fake**; LSE integration tests are env/credential-gated
- [x] TUI never imports or calls LSE directly

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Recommended execution order after 04 (or after 03 for live confidence): prove LSE early on the same IPC contract
- Not two separate products — one engine, swappable adapter
- Implementation: `LseVendor` in `engine/src/market_engine/vendor.py`; CLI `--vendor` + `MARKET_ENGINE_VENDOR`; live tests skip without `LSE_API_KEY`
