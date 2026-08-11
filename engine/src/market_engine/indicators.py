"""Per-chart indicator configs, limits, and MA/Volume/VP/GEX/GARCH compute."""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from typing import Any, Literal
from zoneinfo import ZoneInfo

from market_engine.vendor import Bar, OptionsChainResult

IndicatorType = Literal[
    "ma", "volume", "session_vp", "fixed_range_vp", "anchored_vp", "gex", "garch"
]
MaType = Literal["sma", "ema"]
SessionClock = Literal["equity", "cme_equity_index"]
Placement = Literal["left", "right"]

MAX_MA_LINES = 3
MAX_VOLUME = 1
MAX_SESSION_VP = 1
MAX_FIXED_RANGE_VP = 4
MAX_ANCHORED_VP = 2
MAX_GEX = 1
MAX_GARCH = 1
DEFAULT_MA_STACK_LENGTHS: tuple[int, int, int] = (10, 60, 200)

# GARCH(1,1): fixed alpha/beta with variance targeting. Need enough closes for a
# stable unconditional variance; short series → explicit unavailable (no fakes).
MIN_GARCH_BARS = 50
GARCH_ALPHA = 0.1
GARCH_BETA = 0.85
# Options contract multiplier (equity/ETF standard) for GEX dollar gamma.
GEX_CONTRACT_MULTIPLIER = 100.0

DEFAULT_SESSION_VP_ROWS = 500
DEFAULT_FIXED_RANGE_VP_ROWS = 200
DEFAULT_ANCHORED_VP_ROWS = 500
DEFAULT_VALUE_AREA_VOLUME = 70.0
DEFAULT_BOX_WIDTH = 30.0
DEFAULT_PLACEMENT: Placement = "right"
DEFAULT_HISTOGRAM: dict[str, Any] = {"color": "steelblue", "opacity": 0.35}
# Product palette: POC blue, VAH green, VAL red (TUI maps names → distinct RGB).
DEFAULT_POC: dict[str, Any] = {"enabled": True, "color": "blue", "opacity": 1.0}
DEFAULT_VAH: dict[str, Any] = {"enabled": True, "color": "lime", "opacity": 1.0}
DEFAULT_VAL: dict[str, Any] = {"enabled": True, "color": "red", "opacity": 1.0}

# CME equity-index futures roots use the overnight session clock.
_CME_EQUITY_INDEX_ROOTS: frozenset[str] = frozenset({"ES", "NQ", "MES", "MNQ"})
_NY = ZoneInfo("America/New_York")


@dataclass
class IndicatorConfig:
    """One indicator instance attached to a chart."""

    id: str
    type: IndicatorType
    enabled: bool = True
    ma_type: MaType | None = None
    length: int | None = None
    # Session / Fixed Range VP shared styling
    mode: str | None = None
    box_width: float | None = None
    placement: str | None = None
    rows: int | None = None
    value_area_volume: float | None = None
    histogram: dict[str, Any] | None = None
    poc: dict[str, Any] | None = None
    vah: dict[str, Any] | None = None
    val: dict[str, Any] | None = None
    # Fixed Range VP anchors
    start: int | None = None
    end: int | None = None
    extend_to_right: bool | None = None
    # Anchored VP single time anchor (forward to now)
    anchor: int | None = None

    def to_public(self) -> dict[str, Any]:
        if self.type == "ma":
            return {
                "id": self.id,
                "type": "ma",
                "enabled": self.enabled,
                "ma_type": self.ma_type or "sma",
                "length": int(self.length or 1),
            }
        if self.type == "volume":
            return {
                "id": self.id,
                "type": "volume",
                "enabled": self.enabled,
            }
        if self.type == "gex":
            return {
                "id": self.id,
                "type": "gex",
                "enabled": self.enabled,
            }
        if self.type == "garch":
            return {
                "id": self.id,
                "type": "garch",
                "enabled": self.enabled,
            }
        if self.type == "fixed_range_vp":
            return {
                "id": self.id,
                "type": "fixed_range_vp",
                "enabled": self.enabled,
                "start": int(self.start if self.start is not None else 0),
                "end": int(self.end if self.end is not None else 0),
                "extend_to_right": bool(self.extend_to_right)
                if self.extend_to_right is not None
                else False,
                "box_width": float(
                    self.box_width if self.box_width is not None else DEFAULT_BOX_WIDTH
                ),
                "placement": self.placement or DEFAULT_PLACEMENT,
                "rows": int(
                    self.rows if self.rows is not None else DEFAULT_FIXED_RANGE_VP_ROWS
                ),
                "value_area_volume": float(
                    self.value_area_volume
                    if self.value_area_volume is not None
                    else DEFAULT_VALUE_AREA_VOLUME
                ),
                "histogram": dict(self.histogram or DEFAULT_HISTOGRAM),
                "poc": dict(self.poc or DEFAULT_POC),
                "vah": dict(self.vah or DEFAULT_VAH),
                "val": dict(self.val or DEFAULT_VAL),
            }
        if self.type == "anchored_vp":
            return {
                "id": self.id,
                "type": "anchored_vp",
                "enabled": self.enabled,
                "anchor": int(self.anchor if self.anchor is not None else 0),
                "box_width": float(
                    self.box_width if self.box_width is not None else DEFAULT_BOX_WIDTH
                ),
                "placement": self.placement or DEFAULT_PLACEMENT,
                "rows": int(
                    self.rows if self.rows is not None else DEFAULT_ANCHORED_VP_ROWS
                ),
                "value_area_volume": float(
                    self.value_area_volume
                    if self.value_area_volume is not None
                    else DEFAULT_VALUE_AREA_VOLUME
                ),
                "histogram": dict(self.histogram or DEFAULT_HISTOGRAM),
                "poc": dict(self.poc or DEFAULT_POC),
                "vah": dict(self.vah or DEFAULT_VAH),
                "val": dict(self.val or DEFAULT_VAL),
            }
        # session_vp
        return {
            "id": self.id,
            "type": "session_vp",
            "enabled": self.enabled,
            "mode": self.mode or "all",
            "box_width": float(
                self.box_width if self.box_width is not None else DEFAULT_BOX_WIDTH
            ),
            "placement": self.placement or DEFAULT_PLACEMENT,
            "rows": int(self.rows if self.rows is not None else DEFAULT_SESSION_VP_ROWS),
            "value_area_volume": float(
                self.value_area_volume
                if self.value_area_volume is not None
                else DEFAULT_VALUE_AREA_VOLUME
            ),
            "histogram": dict(self.histogram or DEFAULT_HISTOGRAM),
            "poc": dict(self.poc or DEFAULT_POC),
            "vah": dict(self.vah or DEFAULT_VAH),
            "val": dict(self.val or DEFAULT_VAL),
        }

    def to_storage(self) -> dict[str, Any]:
        return self.to_public()

    @classmethod
    def from_storage(cls, data: dict[str, Any]) -> IndicatorConfig | None:
        try:
            return parse_indicator_dict(data)
        except ValueError:
            return None


def _parse_style_level(raw: Any, default: dict[str, Any]) -> dict[str, Any]:
    out = dict(default)
    if not isinstance(raw, dict):
        return out
    if "enabled" in raw:
        out["enabled"] = bool(raw["enabled"])
    if "color" in raw and raw["color"] is not None:
        out["color"] = str(raw["color"])
    if "opacity" in raw and raw["opacity"] is not None:
        try:
            opacity = float(raw["opacity"])
        except (TypeError, ValueError) as exc:
            raise ValueError("level opacity must be a number") from exc
        if opacity < 0.0 or opacity > 1.0:
            raise ValueError("level opacity must be between 0 and 1")
        out["opacity"] = opacity
    return out


def _parse_histogram_style(raw: Any) -> dict[str, Any]:
    out = dict(DEFAULT_HISTOGRAM)
    if not isinstance(raw, dict):
        return out
    if "color" in raw and raw["color"] is not None:
        out["color"] = str(raw["color"])
    if "opacity" in raw and raw["opacity"] is not None:
        try:
            opacity = float(raw["opacity"])
        except (TypeError, ValueError) as exc:
            raise ValueError("histogram opacity must be a number") from exc
        if opacity < 0.0 or opacity > 1.0:
            raise ValueError("histogram opacity must be between 0 and 1")
        out["opacity"] = opacity
    return out


def parse_indicator_dict(raw: dict[str, Any]) -> IndicatorConfig:
    """Parse one indicator config dict; raises ValueError on invalid shape."""
    itype = str(raw.get("type", "")).strip().lower()
    iid = str(raw.get("id", "")).strip()
    if not iid:
        raise ValueError("indicator id is required")
    enabled = bool(raw.get("enabled", True))

    if itype == "ma":
        ma_type = str(raw.get("ma_type", "sma")).strip().lower()
        if ma_type not in ("sma", "ema"):
            raise ValueError(f"ma_type must be 'sma' or 'ema', got {ma_type!r}")
        try:
            length = int(raw.get("length"))
        except (TypeError, ValueError) as exc:
            raise ValueError("ma length must be a positive integer") from exc
        if length < 1:
            raise ValueError("ma length must be >= 1")
        return IndicatorConfig(
            id=iid,
            type="ma",
            enabled=enabled,
            ma_type=ma_type,  # type: ignore[arg-type]
            length=length,
        )

    if itype == "volume":
        return IndicatorConfig(id=iid, type="volume", enabled=enabled)

    if itype == "gex":
        return IndicatorConfig(id=iid, type="gex", enabled=enabled)

    if itype == "garch":
        return IndicatorConfig(id=iid, type="garch", enabled=enabled)

    if itype == "session_vp":
        mode = str(raw.get("mode", "all")).strip().lower()
        if mode != "all":
            raise ValueError(
                "session_vp mode must be 'all' (pre/RTH/post session modes are out of v1)"
            )
        placement = str(raw.get("placement", DEFAULT_PLACEMENT)).strip().lower()
        if placement not in ("left", "right"):
            raise ValueError("session_vp placement must be 'left' or 'right'")
        try:
            rows = int(
                raw["rows"] if "rows" in raw and raw["rows"] is not None else DEFAULT_SESSION_VP_ROWS
            )
        except (TypeError, ValueError) as exc:
            raise ValueError("session_vp rows must be a positive integer") from exc
        if rows < 1:
            raise ValueError("session_vp rows must be >= 1")
        try:
            box_width = float(
                raw["box_width"]
                if "box_width" in raw and raw["box_width"] is not None
                else DEFAULT_BOX_WIDTH
            )
        except (TypeError, ValueError) as exc:
            raise ValueError("session_vp box_width must be a number") from exc
        if box_width <= 0 or box_width > 100:
            raise ValueError("session_vp box_width must be in (0, 100]")
        try:
            va = float(
                raw["value_area_volume"]
                if "value_area_volume" in raw and raw["value_area_volume"] is not None
                else DEFAULT_VALUE_AREA_VOLUME
            )
        except (TypeError, ValueError) as exc:
            raise ValueError("session_vp value_area_volume must be a number") from exc
        if va <= 0 or va > 100:
            raise ValueError("session_vp value_area_volume must be in (0, 100]")
        return IndicatorConfig(
            id=iid,
            type="session_vp",
            enabled=enabled,
            mode="all",
            box_width=box_width,
            placement=placement,
            rows=rows,
            value_area_volume=va,
            histogram=_parse_histogram_style(raw.get("histogram")),
            poc=_parse_style_level(raw.get("poc"), DEFAULT_POC),
            vah=_parse_style_level(raw.get("vah"), DEFAULT_VAH),
            val=_parse_style_level(raw.get("val"), DEFAULT_VAL),
        )

    if itype == "fixed_range_vp":
        if "start" not in raw or raw["start"] is None:
            raise ValueError("fixed_range_vp start (time anchor) is required")
        if "end" not in raw or raw["end"] is None:
            raise ValueError("fixed_range_vp end (time anchor) is required")
        try:
            start = int(raw["start"])
            end = int(raw["end"])
        except (TypeError, ValueError) as exc:
            raise ValueError("fixed_range_vp start and end must be unix timestamps") from exc
        if start > end:
            raise ValueError("fixed_range_vp start must be <= end")
        extend = bool(raw.get("extend_to_right", False))
        placement = str(raw.get("placement", DEFAULT_PLACEMENT)).strip().lower()
        if placement not in ("left", "right"):
            raise ValueError("fixed_range_vp placement must be 'left' or 'right'")
        try:
            rows = int(
                raw["rows"]
                if "rows" in raw and raw["rows"] is not None
                else DEFAULT_FIXED_RANGE_VP_ROWS
            )
        except (TypeError, ValueError) as exc:
            raise ValueError("fixed_range_vp rows must be a positive integer") from exc
        if rows < 1:
            raise ValueError("fixed_range_vp rows must be >= 1")
        try:
            box_width = float(
                raw["box_width"]
                if "box_width" in raw and raw["box_width"] is not None
                else DEFAULT_BOX_WIDTH
            )
        except (TypeError, ValueError) as exc:
            raise ValueError("fixed_range_vp box_width must be a number") from exc
        if box_width <= 0 or box_width > 100:
            raise ValueError("fixed_range_vp box_width must be in (0, 100]")
        try:
            va = float(
                raw["value_area_volume"]
                if "value_area_volume" in raw and raw["value_area_volume"] is not None
                else DEFAULT_VALUE_AREA_VOLUME
            )
        except (TypeError, ValueError) as exc:
            raise ValueError("fixed_range_vp value_area_volume must be a number") from exc
        if va <= 0 or va > 100:
            raise ValueError("fixed_range_vp value_area_volume must be in (0, 100]")
        return IndicatorConfig(
            id=iid,
            type="fixed_range_vp",
            enabled=enabled,
            start=start,
            end=end,
            extend_to_right=extend,
            box_width=box_width,
            placement=placement,
            rows=rows,
            value_area_volume=va,
            histogram=_parse_histogram_style(raw.get("histogram")),
            poc=_parse_style_level(raw.get("poc"), DEFAULT_POC),
            vah=_parse_style_level(raw.get("vah"), DEFAULT_VAH),
            val=_parse_style_level(raw.get("val"), DEFAULT_VAL),
        )

    if itype == "anchored_vp":
        if "anchor" not in raw or raw["anchor"] is None:
            raise ValueError("anchored_vp anchor (time anchor) is required")
        try:
            anchor = int(raw["anchor"])
        except (TypeError, ValueError) as exc:
            raise ValueError("anchored_vp anchor must be a unix timestamp") from exc
        placement = str(raw.get("placement", DEFAULT_PLACEMENT)).strip().lower()
        if placement not in ("left", "right"):
            raise ValueError("anchored_vp placement must be 'left' or 'right'")
        try:
            rows = int(
                raw["rows"]
                if "rows" in raw and raw["rows"] is not None
                else DEFAULT_ANCHORED_VP_ROWS
            )
        except (TypeError, ValueError) as exc:
            raise ValueError("anchored_vp rows must be a positive integer") from exc
        if rows < 1:
            raise ValueError("anchored_vp rows must be >= 1")
        try:
            box_width = float(
                raw["box_width"]
                if "box_width" in raw and raw["box_width"] is not None
                else DEFAULT_BOX_WIDTH
            )
        except (TypeError, ValueError) as exc:
            raise ValueError("anchored_vp box_width must be a number") from exc
        if box_width <= 0 or box_width > 100:
            raise ValueError("anchored_vp box_width must be in (0, 100]")
        try:
            va = float(
                raw["value_area_volume"]
                if "value_area_volume" in raw and raw["value_area_volume"] is not None
                else DEFAULT_VALUE_AREA_VOLUME
            )
        except (TypeError, ValueError) as exc:
            raise ValueError("anchored_vp value_area_volume must be a number") from exc
        if va <= 0 or va > 100:
            raise ValueError("anchored_vp value_area_volume must be in (0, 100]")
        return IndicatorConfig(
            id=iid,
            type="anchored_vp",
            enabled=enabled,
            anchor=anchor,
            box_width=box_width,
            placement=placement,
            rows=rows,
            value_area_volume=va,
            histogram=_parse_histogram_style(raw.get("histogram")),
            poc=_parse_style_level(raw.get("poc"), DEFAULT_POC),
            vah=_parse_style_level(raw.get("vah"), DEFAULT_VAH),
            val=_parse_style_level(raw.get("val"), DEFAULT_VAL),
        )

    raise ValueError(f"unsupported indicator type: {itype!r}")


def validate_indicator_list(configs: list[IndicatorConfig]) -> None:
    """Enforce per-chart instance limits (reject — not clamp)."""
    ma_count = sum(1 for c in configs if c.type == "ma")
    vol_count = sum(1 for c in configs if c.type == "volume")
    svp_count = sum(1 for c in configs if c.type == "session_vp")
    frvp_count = sum(1 for c in configs if c.type == "fixed_range_vp")
    avp_count = sum(1 for c in configs if c.type == "anchored_vp")
    gex_count = sum(1 for c in configs if c.type == "gex")
    garch_count = sum(1 for c in configs if c.type == "garch")
    if ma_count > MAX_MA_LINES:
        raise ValueError(
            f"ma limit exceeded: max {MAX_MA_LINES} lines per chart, got {ma_count}"
        )
    if vol_count > MAX_VOLUME:
        raise ValueError(
            f"volume limit exceeded: max {MAX_VOLUME} instance per chart, got {vol_count}"
        )
    if svp_count > MAX_SESSION_VP:
        raise ValueError(
            f"session_vp limit exceeded: max {MAX_SESSION_VP} instance per chart, got {svp_count}"
        )
    if frvp_count > MAX_FIXED_RANGE_VP:
        raise ValueError(
            f"fixed_range_vp limit exceeded: max {MAX_FIXED_RANGE_VP} instances per chart, got {frvp_count}"
        )
    if avp_count > MAX_ANCHORED_VP:
        raise ValueError(
            f"anchored_vp limit exceeded: max {MAX_ANCHORED_VP} instances per chart, got {avp_count}"
        )
    if gex_count > MAX_GEX:
        raise ValueError(
            f"gex limit exceeded: max {MAX_GEX} instance per chart, got {gex_count}"
        )
    if garch_count > MAX_GARCH:
        raise ValueError(
            f"garch limit exceeded: max {MAX_GARCH} instance per chart, got {garch_count}"
        )
    ids = [c.id for c in configs]
    if len(ids) != len(set(ids)):
        raise ValueError("indicator ids must be unique per chart")


def parse_indicators_payload(raw_list: Any) -> list[IndicatorConfig]:
    if raw_list is None:
        return []
    if not isinstance(raw_list, list):
        raise ValueError("indicators must be a list")
    configs: list[IndicatorConfig] = []
    for i, item in enumerate(raw_list):
        if not isinstance(item, dict):
            raise ValueError(f"indicators[{i}] must be an object")
        # Auto-id when client omits id (panel convenience).
        if not str(item.get("id", "")).strip():
            item = dict(item)
            itype = str(item.get("type", "")).strip().lower()
            item["id"] = f"{itype}_{i}" if itype else f"ind_{i}"
        configs.append(parse_indicator_dict(item))
    validate_indicator_list(configs)
    return configs


def indicators_from_storage(raw: Any) -> list[IndicatorConfig]:
    if not isinstance(raw, list):
        return []
    out: list[IndicatorConfig] = []
    for item in raw:
        if not isinstance(item, dict):
            continue
        cfg = IndicatorConfig.from_storage(item)
        if cfg is not None:
            out.append(cfg)
    try:
        validate_indicator_list(out)
    except ValueError:
        # Corrupt store: drop extras by type rather than failing load.
        trimmed: list[IndicatorConfig] = []
        ma_n = 0
        vol_n = 0
        svp_n = 0
        frvp_n = 0
        avp_n = 0
        gex_n = 0
        garch_n = 0
        seen: set[str] = set()
        for c in out:
            if c.id in seen:
                continue
            if c.type == "ma":
                if ma_n >= MAX_MA_LINES:
                    continue
                ma_n += 1
            elif c.type == "volume":
                if vol_n >= MAX_VOLUME:
                    continue
                vol_n += 1
            elif c.type == "session_vp":
                if svp_n >= MAX_SESSION_VP:
                    continue
                svp_n += 1
            elif c.type == "fixed_range_vp":
                if frvp_n >= MAX_FIXED_RANGE_VP:
                    continue
                frvp_n += 1
            elif c.type == "anchored_vp":
                if avp_n >= MAX_ANCHORED_VP:
                    continue
                avp_n += 1
            elif c.type == "gex":
                if gex_n >= MAX_GEX:
                    continue
                gex_n += 1
            elif c.type == "garch":
                if garch_n >= MAX_GARCH:
                    continue
                garch_n += 1
            seen.add(c.id)
            trimmed.append(c)
        return trimmed
    return out


def sma(closes: list[float], length: int) -> list[float | None]:
    n = len(closes)
    values: list[float | None] = [None] * n
    if length < 1:
        return values
    running = 0.0
    for i, close in enumerate(closes):
        running += close
        if i >= length:
            running -= closes[i - length]
        if i >= length - 1:
            values[i] = running / length
    return values


def ema(closes: list[float], length: int) -> list[float | None]:
    n = len(closes)
    values: list[float | None] = [None] * n
    if length < 1 or n < length:
        return values
    seed = sum(closes[:length]) / length
    values[length - 1] = seed
    k = 2.0 / (length + 1)
    prev = seed
    for i in range(length, n):
        prev = closes[i] * k + prev * (1.0 - k)
        values[i] = prev
    return values


def session_clock_for_instrument(instrument: str) -> SessionClock:
    """Map instrument root to Session VP day-bound rules."""
    root = instrument.strip().upper().split(".", 1)[0]
    if root in _CME_EQUITY_INDEX_ROOTS:
        return "cme_equity_index"
    return "equity"


def session_window_for_ts(ts: int | float, clock: SessionClock) -> tuple[int, int] | None:
    """Return [session_start, session_end) unix seconds for a bar open, or None if in break."""
    dt = datetime.fromtimestamp(float(ts), tz=_NY)
    if clock == "equity":
        # US equities/ETFs: 16:00 → next calendar day 16:00.
        # Before 16:00 → session started previous calendar day 16:00; else today 16:00.
        if (dt.hour, dt.minute, dt.second, dt.microsecond) < (16, 0, 0, 0):
            start_date = (dt - timedelta(days=1)).date()
        else:
            start_date = dt.date()
        start = datetime(
            start_date.year, start_date.month, start_date.day, 16, 0, 0, tzinfo=_NY
        )
        end = start + timedelta(days=1)
        return int(start.timestamp()), int(end.timestamp())

    # CME ES/NQ: prior calendar day 18:00 → 17:00 (break ~17:00–18:00).
    t = (dt.hour, dt.minute, dt.second, dt.microsecond)
    if t >= (17, 0, 0, 0) and t < (18, 0, 0, 0):
        return None  # maintenance break
    if t >= (18, 0, 0, 0):
        # Session opened today 18:00, ends tomorrow 17:00 (= start + 23h).
        start = datetime(dt.year, dt.month, dt.day, 18, 0, 0, tzinfo=_NY)
        end = start + timedelta(hours=23)
        return int(start.timestamp()), int(end.timestamp())
    # Before 17:00: session started yesterday 18:00, ends today 17:00.
    end = datetime(dt.year, dt.month, dt.day, 17, 0, 0, tzinfo=_NY)
    start = end - timedelta(hours=23)
    return int(start.timestamp()), int(end.timestamp())


def _bucket_index(price: float, price_low: float, row_height: float, rows: int) -> int:
    if row_height <= 0:
        return 0
    idx = int((price - price_low) / row_height)
    if idx < 0:
        return 0
    if idx >= rows:
        return rows - 1
    return idx


def build_volume_profile(
    bars: list[Bar] | tuple[Bar, ...],
    *,
    rows: int,
    value_area_volume: float,
) -> dict[str, Any] | None:
    """Build one profile from bars: equal price buckets, POC / VAH / VAL."""
    if not bars or rows < 1:
        return None
    price_low = min(b.low for b in bars)
    price_high = max(b.high for b in bars)
    if price_high < price_low:
        return None
    if price_high == price_low:
        # Degenerate single price: still one usable bucket span.
        pad = max(abs(price_low) * 1e-6, 1e-6)
        price_low -= pad
        price_high += pad
    row_height = (price_high - price_low) / rows
    volumes = [0.0] * rows

    for bar in bars:
        lo = min(bar.low, bar.high)
        hi = max(bar.low, bar.high)
        i0 = _bucket_index(lo, price_low, row_height, rows)
        i1 = _bucket_index(hi, price_low, row_height, rows)
        if i1 < i0:
            i0, i1 = i1, i0
        n_bins = i1 - i0 + 1
        share = float(bar.volume) / n_bins if n_bins else float(bar.volume)
        for i in range(i0, i1 + 1):
            volumes[i] += share

    total = sum(volumes)
    if total <= 0:
        return None

    poc_idx = max(range(rows), key=lambda i: (volumes[i], -i))
    target = total * (value_area_volume / 100.0)
    included = {poc_idx}
    accumulated = volumes[poc_idx]
    low_i = high_i = poc_idx
    while accumulated < target and (low_i > 0 or high_i < rows - 1):
        down_vol = volumes[low_i - 1] if low_i > 0 else -1.0
        up_vol = volumes[high_i + 1] if high_i < rows - 1 else -1.0
        # Prefer the side with more volume; ties expand upward (classic TV-style).
        if up_vol > down_vol:
            high_i += 1
            included.add(high_i)
            accumulated += volumes[high_i]
        elif down_vol > up_vol:
            low_i -= 1
            included.add(low_i)
            accumulated += volumes[low_i]
        elif high_i < rows - 1:
            high_i += 1
            included.add(high_i)
            accumulated += volumes[high_i]
        else:
            low_i -= 1
            included.add(low_i)
            accumulated += volumes[low_i]

    bins: list[dict[str, float]] = []
    for i in range(rows):
        b_low = price_low + i * row_height
        b_high = price_low + (i + 1) * row_height
        bins.append(
            {
                "price_low": b_low,
                "price_high": b_high,
                "volume": volumes[i],
            }
        )

    poc_low = price_low + poc_idx * row_height
    poc_high = price_low + (poc_idx + 1) * row_height
    val_low = price_low + low_i * row_height
    vah_high = price_low + (high_i + 1) * row_height

    return {
        "high": price_high,
        "low": price_low,
        "poc": (poc_low + poc_high) / 2.0,
        "val": val_low,
        "vah": vah_high,
        "total_volume": total,
        "bins": bins,
    }


def compute_session_vp(
    bars: list[Bar] | tuple[Bar, ...],
    *,
    instrument: str,
    rows: int,
    value_area_volume: float,
) -> list[dict[str, Any]]:
    """One profile per session day for the instrument's session clock."""
    clock = session_clock_for_instrument(instrument)
    by_session: dict[tuple[int, int], list[Bar]] = {}
    order: list[tuple[int, int]] = []
    for bar in bars:
        window = session_window_for_ts(bar.ts, clock)
        if window is None:
            continue
        if window not in by_session:
            by_session[window] = []
            order.append(window)
        by_session[window].append(bar)

    profiles: list[dict[str, Any]] = []
    for window in order:
        built = build_volume_profile(
            by_session[window],
            rows=rows,
            value_area_volume=value_area_volume,
        )
        if built is None:
            continue
        session_start, session_end = window
        profiles.append(
            {
                "session_start": session_start,
                "session_end": session_end,
                **built,
            }
        )
    return profiles


def compute_fixed_range_vp(
    bars: list[Bar] | tuple[Bar, ...],
    *,
    start: int,
    end: int,
    extend_to_right: bool,
    rows: int,
    value_area_volume: float,
) -> list[dict[str, Any]]:
    """One profile between two time anchors; optional live build past end.

    When extend_to_right is off: only bars with open in [start, end] count, and
    POC/VAH/VAL levels end at the user end anchor (no projection past the window).

    When on: bars from start forward (including past end) accumulate into the
    profile, and levels_end projects to the latest contributing bar so confluence
    remains visible on newer candles.
    """
    if start > end:
        return []

    if extend_to_right:
        window_bars = [b for b in bars if int(b.ts) >= start]
    else:
        window_bars = [b for b in bars if start <= int(b.ts) <= end]

    if not window_bars:
        return []

    built = build_volume_profile(
        window_bars,
        rows=rows,
        value_area_volume=value_area_volume,
    )
    if built is None:
        return []

    last_ts = max(int(b.ts) for b in window_bars)
    if extend_to_right:
        range_end = max(end, last_ts)
        levels_end = range_end
    else:
        range_end = end
        levels_end = end

    return [
        {
            "range_start": start,
            "range_end": range_end,
            "anchor_end": end,
            "levels_end": levels_end,
            "extend_to_right": extend_to_right,
            **built,
        }
    ]


def compute_anchored_vp(
    bars: list[Bar] | tuple[Bar, ...],
    *,
    anchor: int,
    rows: int,
    value_area_volume: float,
) -> list[dict[str, Any]]:
    """One profile from a single time anchor forward to the latest bar (now).

    All bars with open ts >= anchor contribute. POC/VAH/VAL levels project to the
    latest contributing bar so confluence stays live as new bars print.
    """
    window_bars = [b for b in bars if int(b.ts) >= anchor]
    if not window_bars:
        return []

    built = build_volume_profile(
        window_bars,
        rows=rows,
        value_area_volume=value_area_volume,
    )
    if built is None:
        return []

    last_ts = max(int(b.ts) for b in window_bars)
    return [
        {
            "anchor": anchor,
            "range_start": anchor,
            "range_end": last_ts,
            "levels_end": last_ts,
            **built,
        }
    ]


def compute_garch(
    closes: list[float] | tuple[float, ...],
) -> dict[str, Any]:
    """GARCH(1,1) conditional volatility, or explicit unavailable.

    Uses variance targeting with fixed alpha/beta. When history is shorter than
    ``MIN_GARCH_BARS`` or variance is non-positive, returns status unavailable
    with no invented values.
    """
    n = len(closes)
    if n < MIN_GARCH_BARS:
        return {
            "type": "garch",
            "status": "unavailable",
            "reason": "insufficient_history",
        }
    try:
        returns: list[float] = []
        for i in range(1, n):
            prev = float(closes[i - 1])
            cur = float(closes[i])
            if prev <= 0 or cur <= 0:
                return {
                    "type": "garch",
                    "status": "unavailable",
                    "reason": "compute_failed",
                }
            returns.append(math.log(cur / prev))
        if not returns:
            return {
                "type": "garch",
                "status": "unavailable",
                "reason": "insufficient_history",
            }
        uvar = sum(r * r for r in returns) / len(returns)
        if uvar <= 0 or not math.isfinite(uvar):
            return {
                "type": "garch",
                "status": "unavailable",
                "reason": "unstable_estimate",
            }
        alpha = GARCH_ALPHA
        beta = GARCH_BETA
        if alpha + beta >= 1.0:
            return {
                "type": "garch",
                "status": "unavailable",
                "reason": "unstable_estimate",
            }
        omega = uvar * (1.0 - alpha - beta)
        values: list[float | None] = [None] * n
        h = uvar
        for i, r in enumerate(returns, start=1):
            h = omega + alpha * (r * r) + beta * h
            if h < 0 or not math.isfinite(h):
                return {
                    "type": "garch",
                    "status": "unavailable",
                    "reason": "compute_failed",
                }
            values[i] = math.sqrt(h)
        return {
            "type": "garch",
            "status": "ok",
            "values": values,
            "params": {
                "omega": omega,
                "alpha": alpha,
                "beta": beta,
                "unconditional_var": uvar,
            },
        }
    except (ValueError, ZeroDivisionError, OverflowError):
        return {
            "type": "garch",
            "status": "unavailable",
            "reason": "compute_failed",
        }


def compute_gex(options: OptionsChainResult | None) -> dict[str, Any]:
    """Net + per-strike GEX from an options chain, or explicit unavailable.

    Call gamma exposure is signed positive; put exposure negative (dealer-long
    customer convention for net GEX sign). Scale is OI × gamma × 100 × spot² × 1%.
    Never invents levels when options data is missing or empty.
    """
    if options is None or not options.available:
        return {
            "type": "gex",
            "status": "unavailable",
            "reason": "options_data_missing",
            "net_gex": None,
            "spot": None,
            "levels": [],
            "values": [],
        }
    spot = options.spot
    contracts = options.contracts
    if spot is None or spot <= 0 or not contracts:
        return {
            "type": "gex",
            "status": "unavailable",
            "reason": "options_data_missing",
            "net_gex": None,
            "spot": None,
            "levels": [],
            "values": [],
        }
    try:
        by_strike: dict[float, float] = {}
        net = 0.0
        scale = GEX_CONTRACT_MULTIPLIER * (float(spot) ** 2) * 0.01
        for c in contracts:
            right = str(c.right).strip().upper()
            if right.startswith("C"):
                sign = 1.0
            elif right.startswith("P"):
                sign = -1.0
            else:
                continue
            oi = float(c.open_interest)
            gamma = float(c.gamma)
            if not math.isfinite(oi) or not math.isfinite(gamma):
                continue
            gex_i = sign * oi * gamma * scale
            if not math.isfinite(gex_i):
                continue
            strike = float(c.strike)
            by_strike[strike] = by_strike.get(strike, 0.0) + gex_i
            net += gex_i
        if not by_strike or not math.isfinite(net):
            return {
                "type": "gex",
                "status": "unavailable",
                "reason": "compute_failed",
                "net_gex": None,
                "spot": None,
                "levels": [],
                "values": [],
            }
        levels = [
            {"strike": strike, "gex": gex}
            for strike, gex in sorted(by_strike.items(), key=lambda kv: kv[0])
        ]
        return {
            "type": "gex",
            "status": "ok",
            "spot": float(spot),
            "net_gex": net,
            "levels": levels,
        }
    except (TypeError, ValueError, OverflowError):
        return {
            "type": "gex",
            "status": "unavailable",
            "reason": "compute_failed",
            "net_gex": None,
            "spot": None,
            "levels": [],
            "values": [],
        }


def compute_series(
    configs: list[IndicatorConfig],
    bars: list[Bar] | tuple[Bar, ...],
    *,
    instrument: str = "",
    options: OptionsChainResult | None = None,
) -> dict[str, dict[str, Any]]:
    """Compute enabled indicator series aligned to bar index (or VP/GEX/GARCH)."""
    closes = [b.close for b in bars]
    volumes = [b.volume for b in bars]
    series: dict[str, dict[str, Any]] = {}
    for cfg in configs:
        if not cfg.enabled:
            continue
        if cfg.type == "ma":
            length = int(cfg.length or 1)
            ma_type = cfg.ma_type or "sma"
            values = (
                ema(closes, length) if ma_type == "ema" else sma(closes, length)
            )
            series[cfg.id] = {
                "type": "ma",
                "ma_type": ma_type,
                "length": length,
                "values": values,
            }
        elif cfg.type == "volume":
            series[cfg.id] = {
                "type": "volume",
                "values": list(volumes),
            }
        elif cfg.type == "session_vp":
            rows = int(cfg.rows if cfg.rows is not None else DEFAULT_SESSION_VP_ROWS)
            va = float(
                cfg.value_area_volume
                if cfg.value_area_volume is not None
                else DEFAULT_VALUE_AREA_VOLUME
            )
            profiles = compute_session_vp(
                bars,
                instrument=instrument,
                rows=rows,
                value_area_volume=va,
            )
            series[cfg.id] = {
                "type": "session_vp",
                "profiles": profiles,
            }
        elif cfg.type == "fixed_range_vp":
            rows = int(
                cfg.rows if cfg.rows is not None else DEFAULT_FIXED_RANGE_VP_ROWS
            )
            va = float(
                cfg.value_area_volume
                if cfg.value_area_volume is not None
                else DEFAULT_VALUE_AREA_VOLUME
            )
            start = int(cfg.start if cfg.start is not None else 0)
            end = int(cfg.end if cfg.end is not None else 0)
            extend = bool(cfg.extend_to_right) if cfg.extend_to_right is not None else False
            profiles = compute_fixed_range_vp(
                bars,
                start=start,
                end=end,
                extend_to_right=extend,
                rows=rows,
                value_area_volume=va,
            )
            series[cfg.id] = {
                "type": "fixed_range_vp",
                "profiles": profiles,
            }
        elif cfg.type == "anchored_vp":
            rows = int(
                cfg.rows if cfg.rows is not None else DEFAULT_ANCHORED_VP_ROWS
            )
            va = float(
                cfg.value_area_volume
                if cfg.value_area_volume is not None
                else DEFAULT_VALUE_AREA_VOLUME
            )
            anchor = int(cfg.anchor if cfg.anchor is not None else 0)
            profiles = compute_anchored_vp(
                bars,
                anchor=anchor,
                rows=rows,
                value_area_volume=va,
            )
            series[cfg.id] = {
                "type": "anchored_vp",
                "profiles": profiles,
            }
        elif cfg.type == "gex":
            series[cfg.id] = compute_gex(options)
        elif cfg.type == "garch":
            series[cfg.id] = compute_garch(closes)
    return series


@dataclass
class IndicatorService:
    """Hot per-chart indicator configs + last computed series."""

    _configs: dict[str, list[IndicatorConfig]] = field(default_factory=dict)
    _series: dict[str, dict[str, dict[str, Any]]] = field(default_factory=dict)

    def get_configs(self, chart_id: str) -> list[IndicatorConfig]:
        return list(self._configs.get(chart_id, []))

    def set_configs(self, chart_id: str, configs: list[IndicatorConfig]) -> None:
        validate_indicator_list(configs)
        self._configs[chart_id] = list(configs)

    def recompute(
        self,
        chart_id: str,
        bars: list[Bar] | tuple[Bar, ...],
        *,
        instrument: str = "",
        options: OptionsChainResult | None = None,
    ) -> dict[str, dict[str, Any]]:
        configs = self._configs.get(chart_id, [])
        series = compute_series(
            configs, bars, instrument=instrument, options=options
        )
        self._series[chart_id] = series
        return series

    def clear_series(self, chart_id: str) -> None:
        self._series.pop(chart_id, None)

    def drop_chart(self, chart_id: str) -> None:
        """Drop hot series when chart leaves layout; configs stay in workspace."""
        self._series.pop(chart_id, None)

    def prune_series_to(self, chart_ids: list[str]) -> None:
        """Keep only hot series for the given chart ids."""
        active = set(chart_ids)
        for chart_id in list(self._series):
            if chart_id not in active:
                del self._series[chart_id]

    def public_for_chart(self, chart_id: str) -> dict[str, Any]:
        return {
            "indicators": [c.to_public() for c in self.get_configs(chart_id)],
            "series": dict(self._series.get(chart_id, {})),
        }

    def public_all(self, chart_ids: list[str]) -> dict[str, dict[str, Any]]:
        return {
            cid: self.public_for_chart(cid)
            for cid in chart_ids
            if cid in self._configs or cid in self._series
        }

    def load_from_workspace(self, chart_id: str, configs: list[IndicatorConfig]) -> None:
        self._configs[chart_id] = list(configs)
        # Series filled after bars load.
        self._series.pop(chart_id, None)
