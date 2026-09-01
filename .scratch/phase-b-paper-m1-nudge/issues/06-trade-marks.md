# 06 — Trade marks: persist, hide/show pair

**What to build:** Each fill paints a **trade mark** (dot) at fill price/time — **entry** and **exit**. Marks **persist after the position is closed**. From **filled order history**, the trader can **hide/show a trade mark pair** without deleting fills. Marks are not working **lines**.

**Blocked by:** 03 — Bar-touch fills

**Status:** ready-for-agent

- [ ] Entry fill → **trade mark** pin at fill price and fill time; exit fill (TP, SL, manual close, or later liquidation) → matching exit pin
- [ ] Marks persist after the **position** is flat
- [ ] Hide/show a **trade mark pair** from **filled order history** does not delete **filled order history** rows
- [ ] Visibility flags persist in SQLite and round-trip on snapshot / paper WS
- [ ] Dual layout: pins only on charts whose **instrument** matches the fill
- [ ] Overlay paint order: candles → existing indicator overlays → working **lines** → **trade marks** (pins must not look like live TP/SL)
- [ ] TUI tests: pins at price/time; hide pair removes pins not fills (client follows engine visibility flags)
- [ ] No new candle renderer; pins use the existing overlay pin layer

## Notes

- Parent spec: `.scratch/phase-b-paper-m1-nudge/spec.md`
- Language: **trade mark**, **filled order history**
- 04/07 add more exit kinds; this ticket must accept exit legs generally so it does not wait on them
