# 07 — Risk rules: asset-class, leverage, margin call / liquidation

**What to build:** A **paper account** can restrict asset class, and when leverage is on it reserves margin and can **margin call / liquidate**. Liquidation writes an exit **filled order history** leg (and a **trade mark** once 06 exists). Numeric defaults are implementer-chosen and visible in settings.

**Blocked by:** 03 — Bar-touch fills

**Status:** ready-for-agent

- [ ] Optional asset-class restriction: place is rejected when the **instrument** is outside the account allow-list (equities-only cannot take a futures **instrument**, and the reverse)
- [ ] Leverage rule can be off / 1× / leveraged; when enabled, margin is reserved on entry
- [ ] If maintenance fails, **margin call / liquidation** closes the **position**
- [ ] Liquidation writes an exit **filled order history** leg and cancels leftover **working** children if a **bracket** is open (04 may already exist; do not leave orphan TP/SL)
- [ ] Commission continues to apply per 03; **balance history** records cash effects of liquidation
- [ ] Defaults for commission / leverage / initial balance are visible in **paper account** settings
- [ ] Fake-vendor: wrong asset class rejected; constructed adverse **1m** series liquidates
- [ ] **Aligned live bars** tests still pass. Fake vendor only.

## Notes

- Parent spec: `.scratch/phase-b-paper-m1-nudge/spec.md`
- Language: **paper account**, **margin call / liquidation**, **filled order history**
- Switching UX still TBD — do not ship a designed switcher; settings can still create/edit accounts (01)
