# 06 — Trade marks: persist, hide/show pair

**What to build:** Each fill paints a **trade mark** (dot) at fill price/time — **entry** and **exit**. Marks **persist after the position is closed**. From **filled order history**, the trader can **hide/show a trade mark pair** without deleting fills. Marks are not working **lines**.

**Blocked by:** 03 — Bar-touch fills

**Status:** done

- [x] Entry fill → **trade mark** pin at fill price and fill time; exit fill (TP, SL, manual close, or later liquidation) → matching exit pin
- [x] Marks persist after the **position** is flat
- [x] Hide/show a **trade mark pair** from **filled order history** does not delete **filled order history** rows
- [x] Visibility flags persist in SQLite and round-trip on snapshot / paper WS
- [x] Dual layout: pins only on charts whose **instrument** matches the fill
- [x] Overlay paint order: candles → existing indicator overlays → working **lines** → **trade marks** (pins must not look like live TP/SL)
- [x] TUI tests: pins at price/time; hide pair removes pins not fills (client follows engine visibility flags)
- [x] No new candle renderer; pins use the existing overlay pin layer

## Notes

- Parent spec: `.scratch/phase-b-paper-m1-nudge/spec.md`
- Language: **trade mark**, **filled order history**
- 04/07 add more exit kinds; this ticket must accept exit legs generally so it does not wait on them

## Comments

Shipped on `grok_bot_dev`: engine assigns a **trade mark** pair on each fill (entry while opening/adding, exit on opposite-side flatten including TP/SL/`close`). Snapshot `trade_marks` plus filled-history `trade_mark_pair_id` / `trade_mark_kind`. `POST /v1/paper/trade-marks/visibility` hide/show by `pair_id` or `fill_id` without deleting fills; flags persist in SQLite and discrete `paper_update`. TUI paints `●` pins on the existing overlay pin layer after working **lines**, filtered by **instrument**; filled history `j`/`k` select + `v` toggles the pair.

Leftover: a fill that flips through flat (oversized opposite qty) still attaches to the old pair rather than splitting an exit + new entry; v1 has no partials so this is an edge. TUI pin tests lock price/time via `ts_to_x`; chart compose uses `view_ts_to_x` but that mapping is not unit-tested against `ChartView`. Liquidation exits will get marks once 07 exists (generic opposite-side exit).
