# 04 — Input focus routing + bar countdown

**What to build:** Enforce focus/mode key routing from the spec: Normal ↑↓ watchlist, ←→ pan; indicator panel / prompts / pin modes own keys. Per-chart **bar countdown** in chart chrome = time remaining in the forming bar of that chart’s timeframe (independent in dual).

**Blocked by:** 03 — Chart pan + deeper history (A2)

**Status:** ready-for-agent

- [ ] Normal arrow routing matches spec (watchlist vs pan vs dual Tab)
- [ ] Indicator panel open blocks watchlist symbol nav
- [ ] Pin placement ← → moves pin, not pan
- [ ] Welcome Enter unchanged
- [ ] Each chart shows forming-bar countdown in chrome when live series exists
- [ ] Dual layout shows two countdowns
- [ ] Help popup updated for new shortcuts

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`
