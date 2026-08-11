//! Overlay compose + type overlay strength for the price pane.
//!
//! Paint order (after candles/axes): VP hist → levels → MA → pins.
//! Continuous overlays may win candle cells; strength softens recoloring.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};
use tui_candlestick_chart::ChartView;

/// Default MA overlay strength (continuous lines stay readable).
pub const DEFAULT_MA_STRENGTH: f64 = 0.85;
/// Default VP histogram strength (soft so candles stay readable).
pub const DEFAULT_VP_STRENGTH: f64 = 0.35;
/// Fallback when type has no dedicated default.
pub const DEFAULT_OVERLAY_STRENGTH: f64 = 0.75;

/// Clamp overlay strength into the terminal intensity range \[0, 1\].
pub fn clamp_strength(strength: f64) -> f64 {
    if !strength.is_finite() {
        return 0.0;
    }
    strength.clamp(0.0, 1.0)
}

/// Product default overlay strength for an indicator type (type style).
pub fn default_strength_for_type(indicator_type: &str) -> f64 {
    match indicator_type {
        "ma" => DEFAULT_MA_STRENGTH,
        "session_vp" | "fixed_range_vp" | "anchored_vp" => DEFAULT_VP_STRENGTH,
        _ => DEFAULT_OVERLAY_STRENGTH,
    }
}

/// Linear blend of two RGB triples by strength (0 = under, 1 = over).
pub fn blend_rgb(under: (u8, u8, u8), over: (u8, u8, u8), strength: f64) -> (u8, u8, u8) {
    let s = clamp_strength(strength);
    let t = 1.0 - s;
    (
        (under.0 as f64 * t + over.0 as f64 * s).round() as u8,
        (under.1 as f64 * t + over.1 as f64 * s).round() as u8,
        (under.2 as f64 * t + over.2 as f64 * s).round() as u8,
    )
}

/// Best-effort RGB for a Ratatui color (named + Rgb; others fall back to gray).
pub fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::Red => (205, 49, 49),
        Color::Green => (13, 188, 121),
        Color::Yellow => (229, 229, 16),
        Color::Blue => (36, 114, 200),
        Color::Magenta => (188, 63, 188),
        Color::Cyan => (17, 168, 205),
        Color::Gray | Color::DarkGray => (118, 118, 118),
        Color::White => (229, 229, 229),
        Color::LightRed => (241, 76, 76),
        Color::LightGreen => (35, 209, 139),
        Color::LightYellow => (245, 245, 67),
        Color::LightBlue => (59, 142, 234),
        Color::LightMagenta => (214, 112, 214),
        Color::LightCyan => (41, 184, 219),
        Color::Reset | Color::Indexed(_) => (160, 160, 160),
    }
}

pub fn rgb_to_color(rgb: (u8, u8, u8)) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

/// Blend overlay color onto an under-cell color by strength.
pub fn blend_color(under: Color, over: Color, strength: f64) -> Color {
    rgb_to_color(blend_rgb(color_to_rgb(under), color_to_rgb(over), strength))
}

// --- Geometry primitives for cell paint ------------------------------------

/// Horizontal histogram bar rect in ChartView local coordinates (column/price).
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayHistBar {
    /// Local canvas x (0-based in candle area).
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: Color,
    /// Type key used to look up overlay strength (e.g. "session_vp").
    pub type_key: String,
}

/// Horizontal level segment (POC / VAH / VAL).
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayLevel {
    pub x0: f64,
    pub x1: f64,
    pub price: f64,
    pub color: Color,
    pub type_key: String,
}

/// Polyline for an MA (or similar continuous series) in local x / price.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayLine {
    pub points: Vec<(f64, f64)>,
    pub color: Color,
    pub type_key: String,
}

/// Pin / label drawn last.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayPin {
    pub x: f64,
    pub price: f64,
    pub glyph: String,
    pub color: Color,
}

/// Layers ready for the price-pane compose pass.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OverlayLayers {
    pub hist: Vec<OverlayHistBar>,
    pub levels: Vec<OverlayLevel>,
    pub lines: Vec<OverlayLine>,
    pub pins: Vec<OverlayPin>,
}

/// Resolve strength for a type: explicit map entry, else product default.
pub fn resolve_strength(type_key: &str, styles: &std::collections::HashMap<String, f64>) -> f64 {
    styles
        .get(type_key)
        .copied()
        .map(clamp_strength)
        .unwrap_or_else(|| default_strength_for_type(type_key))
}

/// Paint overlays into `buf` on the same cell grid as the candlestick widget.
///
/// Order: VP hist → levels → MA lines → pins. Continuous lines may overwrite
/// candle cells; strength blends the overlay color with the cell under it.
pub fn paint_overlays(
    buf: &mut Buffer,
    view: &ChartView,
    layers: &OverlayLayers,
    strengths: &std::collections::HashMap<String, f64>,
) {
    let area = view.candle_area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    for bar in &layers.hist {
        let s = resolve_strength(&bar.type_key, strengths);
        if s <= 0.0 {
            continue;
        }
        paint_hist_bar(buf, view, area, bar, s);
    }
    for level in &layers.levels {
        let s = resolve_strength(&level.type_key, strengths);
        if s <= 0.0 {
            continue;
        }
        paint_level(buf, view, area, level, s);
    }
    for line in &layers.lines {
        let s = resolve_strength(&line.type_key, strengths);
        if s <= 0.0 {
            continue;
        }
        paint_line(buf, view, area, line, s);
    }
    for pin in &layers.pins {
        paint_pin(buf, view, area, pin);
    }
}

fn local_x_to_col(view: &ChartView, x: f64) -> Option<u16> {
    let w = view.price_width();
    if w == 0 {
        return None;
    }
    let col = x.floor() as i64;
    if col < 0 || col >= w as i64 {
        return None;
    }
    Some(view.area.x + view.y_axis_width + col as u16)
}

fn price_to_row_clamped(view: &ChartView, price: f64) -> Option<u16> {
    view.price_to_row(price)
}

fn cell_in_area(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x && x < area.x.saturating_add(area.width) && y >= area.y && y < area.y.saturating_add(area.height)
}

fn under_fg(buf: &Buffer, x: u16, y: u16) -> Color {
    buf[(x, y)].style().fg.unwrap_or(Color::Reset)
}

fn paint_cell_fg(buf: &mut Buffer, x: u16, y: u16, over: Color, strength: f64, symbol: Option<&str>) {
    let under = under_fg(buf, x, y);
    let fg = blend_color(under, over, strength);
    let cell = &mut buf[(x, y)];
    if let Some(sym) = symbol {
        // Continuous overlays may win the glyph so the stroke stays continuous.
        cell.set_symbol(sym);
    }
    cell.set_style(Style::default().fg(fg));
}

fn paint_hist_bar(
    buf: &mut Buffer,
    view: &ChartView,
    area: Rect,
    bar: &OverlayHistBar,
    strength: f64,
) {
    // Soft hist: blend over under-cell color so candles stay readable.
    // Only plant a soft block glyph on empty cells; never force-overwrite candle glyphs.
    let x0 = bar.x;
    let x1 = bar.x + bar.width;
    let y_lo = bar.y;
    let y_hi = bar.y + bar.height;
    let Some(row_hi) = price_to_row_clamped(view, y_hi) else {
        return;
    };
    let Some(row_lo) = price_to_row_clamped(view, y_lo) else {
        return;
    };
    let (r0, r1) = if row_hi <= row_lo {
        (row_hi, row_lo)
    } else {
        (row_lo, row_hi)
    };
    let col_start = x0.floor() as i64;
    let col_end = x1.ceil() as i64;
    for col in col_start..col_end {
        let Some(cx) = local_x_to_col(view, col as f64 + 0.5) else {
            continue;
        };
        for ry in r0..=r1 {
            if !cell_in_area(area, cx, ry) {
                continue;
            }
            let under = under_fg(buf, cx, ry);
            let fg = blend_color(under, bar.color, strength);
            let cell = &mut buf[(cx, ry)];
            let sym = cell.symbol();
            if sym.is_empty() || sym == " " || sym == "\u{00a0}" {
                cell.set_symbol("░");
            }
            cell.set_style(Style::default().fg(fg));
        }
    }
}

fn paint_level(
    buf: &mut Buffer,
    view: &ChartView,
    area: Rect,
    level: &OverlayLevel,
    strength: f64,
) {
    let Some(row) = price_to_row_clamped(view, level.price) else {
        return;
    };
    let c0 = level.x0.min(level.x1).floor() as i64;
    let c1 = level.x0.max(level.x1).ceil() as i64;
    for col in c0..=c1 {
        let Some(cx) = local_x_to_col(view, col as f64 + 0.5) else {
            continue;
        };
        if !cell_in_area(area, cx, row) {
            continue;
        }
        paint_cell_fg(buf, cx, row, level.color, strength, Some("─"));
    }
}

fn paint_line(
    buf: &mut Buffer,
    view: &ChartView,
    area: Rect,
    line: &OverlayLine,
    strength: f64,
) {
    for w in line.points.windows(2) {
        let (x1, p1) = w[0];
        let (x2, p2) = w[1];
        // Skip large gaps (same policy as Phase A Canvas: adjacent columns only).
        if (x2 - x1).abs() > 1.5 + f64::EPSILON {
            continue;
        }
        let Some(c1) = local_x_to_col(view, x1) else {
            continue;
        };
        let Some(c2) = local_x_to_col(view, x2) else {
            continue;
        };
        let Some(r1) = price_to_row_clamped(view, p1) else {
            continue;
        };
        let Some(r2) = price_to_row_clamped(view, p2) else {
            continue;
        };
        for (cx, cy) in bresenham_u16(c1, r1, c2, r2) {
            if !cell_in_area(area, cx, cy) {
                continue;
            }
            // MA may win the cell glyph for continuity.
            paint_cell_fg(buf, cx, cy, line.color, strength, Some("•"));
        }
    }
}

fn paint_pin(buf: &mut Buffer, view: &ChartView, area: Rect, pin: &OverlayPin) {
    let Some(cx) = local_x_to_col(view, pin.x) else {
        return;
    };
    let Some(cy) = price_to_row_clamped(view, pin.price) else {
        return;
    };
    if !cell_in_area(area, cx, cy) {
        return;
    }
    let cell = &mut buf[(cx, cy)];
    cell.set_symbol(&pin.glyph);
    cell.set_style(
        Style::default()
            .fg(pin.color)
            .add_modifier(Modifier::BOLD),
    );
}

/// Integer Bresenham line in terminal cell coordinates.
fn bresenham_u16(x0: u16, y0: u16, x1: u16, y1: u16) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    let mut x0 = x0 as i32;
    let mut y0 = y0 as i32;
    let x1 = x1 as i32;
    let y1 = y1 as i32;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        if x0 >= 0 && y0 >= 0 {
            out.push((x0 as u16, y0 as u16));
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use std::collections::HashMap;
    use tui_candlestick_chart::Interval;

    #[test]
    fn clamp_strength_bounds_and_non_finite() {
        assert_eq!(clamp_strength(0.5), 0.5);
        assert_eq!(clamp_strength(-1.0), 0.0);
        assert_eq!(clamp_strength(2.0), 1.0);
        assert_eq!(clamp_strength(f64::NAN), 0.0);
        assert_eq!(clamp_strength(f64::INFINITY), 0.0);
    }

    #[test]
    fn default_strength_by_type() {
        assert_eq!(default_strength_for_type("ma"), DEFAULT_MA_STRENGTH);
        assert_eq!(default_strength_for_type("session_vp"), DEFAULT_VP_STRENGTH);
        assert_eq!(
            default_strength_for_type("fixed_range_vp"),
            DEFAULT_VP_STRENGTH
        );
        assert_eq!(
            default_strength_for_type("anchored_vp"),
            DEFAULT_VP_STRENGTH
        );
        assert_eq!(default_strength_for_type("gex"), DEFAULT_OVERLAY_STRENGTH);
    }

    #[test]
    fn blend_rgb_endpoints_and_mid() {
        let black = (0, 0, 0);
        let white = (255, 255, 255);
        assert_eq!(blend_rgb(black, white, 0.0), black);
        assert_eq!(blend_rgb(black, white, 1.0), white);
        assert_eq!(blend_rgb(black, white, 0.5), (128, 128, 128));
    }

    #[test]
    fn resolve_strength_prefers_map_then_default() {
        let mut styles = HashMap::new();
        styles.insert("ma".into(), 0.4);
        assert_eq!(resolve_strength("ma", &styles), 0.4);
        assert_eq!(resolve_strength("session_vp", &styles), DEFAULT_VP_STRENGTH);
        styles.insert("ma".into(), 9.0);
        assert_eq!(resolve_strength("ma", &styles), 1.0);
    }

    fn sample_view() -> ChartView {
        ChartView {
            area: Rect::new(10, 5, 30, 13),
            y_axis_width: 12,
            x_axis_height: 3,
            y_min: 100.0,
            y_max: 200.0,
            view_start_ts: 1_000_000,
            view_end_ts: 1_000_000 + 60_000 * 2,
            interval: Interval::OneMinute,
            is_live_tip: true,
            column_timestamps: vec![1_000_000, 1_060_000, 1_120_000],
            column_offset: 15,
        }
    }

    #[test]
    fn paint_order_ma_wins_over_hist_on_shared_cell() {
        let view = sample_view();
        let area = view.candle_area();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 25));
        // Seed a "candle" under-cell in the price pane.
        let seed_x = area.x + 15;
        let seed_y = view.price_to_row(150.0).unwrap();
        buf[(seed_x, seed_y)]
            .set_symbol("┃")
            .set_style(Style::default().fg(Color::Rgb(52, 208, 88)));

        let mut layers = OverlayLayers::default();
        layers.hist.push(OverlayHistBar {
            x: 15.0,
            y: 140.0,
            width: 2.0,
            height: 20.0,
            color: Color::Cyan,
            type_key: "session_vp".into(),
        });
        layers.lines.push(OverlayLine {
            points: vec![(15.5, 150.0), (16.5, 150.0)],
            color: Color::Yellow,
            type_key: "ma".into(),
        });

        let mut strengths = HashMap::new();
        strengths.insert("ma".into(), 1.0);
        strengths.insert("session_vp".into(), 1.0);
        paint_overlays(&mut buf, &view, &layers, &strengths);

        let cell = &buf[(seed_x, seed_y)];
        // MA paints last among hist/level/line and may win the glyph.
        assert_eq!(cell.symbol(), "•");
        // Yellow named → RGB approx; strength 1.0 should recolor fully.
        let fg = color_to_rgb(cell.style().fg.unwrap_or(Color::Reset));
        assert!(
            fg.0 > 150 && fg.1 > 150,
            "expected strong yellow-ish, got {fg:?}"
        );
    }

    #[test]
    fn low_strength_softens_ma_recolor_of_candle() {
        let view = sample_view();
        let area = view.candle_area();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 25));
        let seed_x = area.x + 15;
        let seed_y = view.price_to_row(150.0).unwrap();
        let candle = Color::Rgb(52, 208, 88);
        buf[(seed_x, seed_y)]
            .set_symbol("┃")
            .set_style(Style::default().fg(candle));

        let mut layers = OverlayLayers::default();
        layers.lines.push(OverlayLine {
            points: vec![(15.5, 150.0), (16.5, 150.0)],
            color: Color::Rgb(255, 0, 0),
            type_key: "ma".into(),
        });
        let mut strengths = HashMap::new();
        strengths.insert("ma".into(), 0.25);
        paint_overlays(&mut buf, &view, &layers, &strengths);

        let fg = color_to_rgb(buf[(seed_x, seed_y)].style().fg.unwrap());
        // Blend of green candle + red overlay at 0.25 → still mostly green, some red.
        assert!(fg.1 > fg.0, "low strength should keep green dominant: {fg:?}");
        assert!(fg.0 > 40, "some red should bleed in: {fg:?}");
    }

    #[test]
    fn pins_paint_last_over_ma() {
        let view = sample_view();
        let area = view.candle_area();
        let mut buf = Buffer::empty(Rect::new(0, 0, 50, 25));
        let seed_x = area.x + 15;
        let seed_y = view.price_to_row(180.0).unwrap();

        let mut layers = OverlayLayers::default();
        layers.lines.push(OverlayLine {
            points: vec![(15.5, 180.0), (16.5, 180.0)],
            color: Color::Cyan,
            type_key: "ma".into(),
        });
        layers.pins.push(OverlayPin {
            x: 15.5,
            price: 180.0,
            glyph: "▲".into(),
            color: Color::Magenta,
        });
        let strengths = HashMap::new();
        paint_overlays(&mut buf, &view, &layers, &strengths);
        assert_eq!(buf[(seed_x, seed_y)].symbol(), "▲");
        assert_eq!(
            buf[(seed_x, seed_y)].style().fg,
            Some(Color::Magenta)
        );
    }
}
