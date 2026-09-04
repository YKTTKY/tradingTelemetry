# 07 — Risk rules: asset-class, leverage, margin call / liquidation

**What to build:** A **paper account** can restrict asset class, and when leverage is on it reserves margin and can **margin call / liquidate**. Liquidation writes an exit **filled order history** leg (and a **trade mark** once 06 exists). Numeric defaults are implementer-chosen and visible in settings.

**Blocked by:** 03 — Bar-touch fills

**Status:** done

- [x] Optional asset-class restriction: place is rejected when the **instrument** is outside the account allow-list (equities-only cannot take a futures **instrument**, and the reverse)
- [x] Leverage rule can be off / 1× / leveraged; when enabled, margin is reserved on entry
- [x] If maintenance fails, **margin call / liquidation** closes the **position**
- [x] Liquidation writes an exit **filled order history** leg and cancels leftover **working** children if a **bracket** is open (04 may already exist; do not leave orphan TP/SL)
- [x] Commission continues to apply per 03; **balance history** records cash effects of liquidation
- [x] Defaults for commission / leverage / initial balance are visible in **paper account** settings
- [x] Fake-vendor: wrong asset class rejected; constructed adverse **1m** series liquidates
- [x] **Aligned live bars** tests still pass. Fake vendor only.

## Notes

- Parent spec: `.scratch/phase-b-paper-m1-nudge/spec.md`
- Language: **paper account**, **margin call / liquidation**, **filled order history**
- Switching UX still TBD — do not ship a designed switcher; settings can still create/edit accounts (01)

## Comments

Shipped on `grok_bot_dev`: place rejects instruments outside an optional `equities` / `futures` allow-list. Leverage off / 1× / N×: when enabled, initial margin (`notional / multiple`) is reserved on entry (maintenance default 50% of IM, visible in settings). Adverse engine-owned **1m** marks run a **margin call / liquidation**: exit **filled order history** type `liquidation`, commission + **balance history**, cancel leftover **bracket** children, **trade mark** exit via the 06 path, discrete `paper_update` reason `liquidation` (not `bar_update`). TUI settings show commission / leverage / restriction / maintenance; no switcher and no paper-panel shortcut.

Leftover: no TUI/IPC **edit** form for rules on an existing account (create-at-IPC still sets them; switching UX TBD). Multi-position books mark maintenance from the instrument whose 1m bar just printed (other symbols at avg). Exit-leg `margin` is the IM released, not a second reservation.
