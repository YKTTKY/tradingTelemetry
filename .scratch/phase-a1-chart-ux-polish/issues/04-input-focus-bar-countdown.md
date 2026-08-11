# 04 — Input focus routing + bar countdown

**What to build:** Enforce focus/mode key routing from the spec: Normal ↑↓ watchlist, ←→ pan; indicator panel / prompts / pin modes own keys. Per-chart **bar countdown** in chart chrome = time remaining in the forming bar of that chart’s timeframe (independent in dual).

**Blocked by:** 03 — Chart pan + deeper history (A2)

**Status:** done

- [x] Normal arrow routing matches spec (watchlist vs pan vs dual Tab)
- [x] Indicator panel open blocks watchlist symbol nav
- [x] Pin placement ← → moves pin, not pan
- [x] Welcome Enter unchanged
- [x] Each chart shows forming-bar countdown in chrome when live series exists
- [x] Dual layout shows two countdowns
- [x] Help popup updated for new shortcuts

## Notes

- Parent spec: `.scratch/phase-a1-chart-ux-polish/spec.md`

## Comments

- **Input focus:** Normal ↑↓ watchlist / ←→ pan / Tab dual focus already mode-routed in `main.rs` + App guards; panel/prompts/pin modes own keys. Tests: `watchlist_nav_blocked_when_indicator_panel_open`, `pin_placement_left_right_moves_pin_not_pan`, existing pan panel no-op. Welcome Enter unchanged.
- **Bar countdown:** pure helpers in `timeframe.rs` (`forming_bar_remaining_secs`, `format_bar_countdown`); `Chart::forming_bar_countdown_label` / `chrome_title`; drawn in chart block title when series Available. Dual independent via per-chart tip + TF. Hide when empty/unavailable (no invented times). Period model = unix TF buckets (1D/1W session calendar left TBD).
- **Help / status:** help popup reorganized for focus ownership + countdown note; status bar shows `←→ pan` / `↑↓ list`.
