"""Per-chart indicator configs, limits, and MA/Volume compute."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal

from market_engine.vendor import Bar

IndicatorType = Literal["ma", "volume"]
MaType = Literal["sma", "ema"]

MAX_MA_LINES = 3
MAX_VOLUME = 1
DEFAULT_MA_STACK_LENGTHS: tuple[int, int, int] = (10, 60, 200)


@dataclass
class IndicatorConfig:
    """One indicator instance attached to a chart."""

    id: str
    type: IndicatorType
    enabled: bool = True
    ma_type: MaType | None = None
    length: int | None = None

    def to_public(self) -> dict[str, Any]:
        if self.type == "ma":
            return {
                "id": self.id,
                "type": "ma",
                "enabled": self.enabled,
                "ma_type": self.ma_type or "sma",
                "length": int(self.length or 1),
            }
        return {
            "id": self.id,
            "type": "volume",
            "enabled": self.enabled,
        }

    def to_storage(self) -> dict[str, Any]:
        return self.to_public()

    @classmethod
    def from_storage(cls, data: dict[str, Any]) -> IndicatorConfig | None:
        try:
            return parse_indicator_dict(data)
        except ValueError:
            return None


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

    raise ValueError(f"unsupported indicator type: {itype!r}")


def validate_indicator_list(configs: list[IndicatorConfig]) -> None:
    """Enforce per-chart instance limits (reject — not clamp)."""
    ma_count = sum(1 for c in configs if c.type == "ma")
    vol_count = sum(1 for c in configs if c.type == "volume")
    if ma_count > MAX_MA_LINES:
        raise ValueError(
            f"ma limit exceeded: max {MAX_MA_LINES} lines per chart, got {ma_count}"
        )
    if vol_count > MAX_VOLUME:
        raise ValueError(
            f"volume limit exceeded: max {MAX_VOLUME} instance per chart, got {vol_count}"
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


def compute_series(
    configs: list[IndicatorConfig],
    bars: list[Bar] | tuple[Bar, ...],
) -> dict[str, dict[str, Any]]:
    """Compute enabled indicator series aligned to bar index."""
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

    def recompute(self, chart_id: str, bars: list[Bar] | tuple[Bar, ...]) -> dict[str, dict[str, Any]]:
        configs = self._configs.get(chart_id, [])
        series = compute_series(configs, bars)
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
        return {cid: self.public_for_chart(cid) for cid in chart_ids if cid in self._configs or cid in self._series}

    def load_from_workspace(self, chart_id: str, configs: list[IndicatorConfig]) -> None:
        self._configs[chart_id] = list(configs)
        # Series filled after bars load.
        self._series.pop(chart_id, None)
