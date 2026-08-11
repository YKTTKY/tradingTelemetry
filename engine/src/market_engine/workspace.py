"""File-backed workspace: layout mode + charts + multi-list watchlists."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal

from market_engine.indicators import IndicatorConfig, indicators_from_storage

LayoutMode = Literal["single", "dual-vertical"]

LAYOUT_SINGLE: LayoutMode = "single"
LAYOUT_DUAL: LayoutMode = "dual-vertical"
VALID_LAYOUTS: frozenset[str] = frozenset({LAYOUT_SINGLE, LAYOUT_DUAL})

CHART_PRIMARY = "primary"
CHART_TOP = "top"
CHART_BOTTOM = "bottom"

WATCHLIST_CORE_ID = "core"
WATCHLIST_FOCUS_ID = "focus"

# Always present on first-launch Core. VIX is appended only when the vendor resolves it.
CORE_DEFAULT_SYMBOLS: tuple[str, ...] = ("ES", "NQ", "SPY", "QQQ", "SOXL")
CORE_OPTIONAL_VIX = "VIX"


@dataclass(frozen=True)
class ChartSelection:
    instrument: str
    timeframe: str

    def to_dict(self) -> dict[str, str]:
        return {"instrument": self.instrument, "timeframe": self.timeframe}


def default_single() -> ChartSelection:
    return ChartSelection(instrument="SPY", timeframe="1D")


def default_dual_top() -> ChartSelection:
    return ChartSelection(instrument="QQQ", timeframe="1D")


def default_dual_bottom() -> ChartSelection:
    return ChartSelection(instrument="SPY", timeframe="1D")


@dataclass
class Watchlist:
    """Named sheet of instruments for the sidebar."""

    id: str
    name: str
    symbols: list[str] = field(default_factory=list)

    def to_public(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "symbols": list(self.symbols),
        }

    def to_storage(self) -> dict[str, Any]:
        return self.to_public()

    @classmethod
    def from_storage(cls, data: dict[str, Any]) -> Watchlist | None:
        wid = str(data.get("id", "")).strip()
        name = str(data.get("name", "")).strip()
        if not wid or not name:
            return None
        raw_syms = data.get("symbols", [])
        symbols: list[str] = []
        if isinstance(raw_syms, list):
            for s in raw_syms:
                sym = str(s).strip().upper()
                if sym and sym not in symbols:
                    symbols.append(sym)
        return cls(id=wid, name=name, symbols=symbols)


def default_core_symbols(*, include_vix: bool) -> list[str]:
    symbols = list(CORE_DEFAULT_SYMBOLS)
    if include_vix:
        symbols.append(CORE_OPTIONAL_VIX)
    return symbols


def default_watchlists(*, include_vix: bool = False) -> list[Watchlist]:
    """First-launch multi-list desk: Core (defaults) + empty Focus for switching."""
    return [
        Watchlist(
            id=WATCHLIST_CORE_ID,
            name="Core",
            symbols=default_core_symbols(include_vix=include_vix),
        ),
        Watchlist(id=WATCHLIST_FOCUS_ID, name="Focus", symbols=[]),
    ]


def _clamp_strength(value: Any) -> float:
    try:
        s = float(value)
    except (TypeError, ValueError):
        return 0.0
    if s != s:  # NaN
        return 0.0
    return max(0.0, min(1.0, s))


def parse_type_styles(raw: Any) -> dict[str, dict[str, float]]:
    """Parse type style map: { indicator_type: { overlay_strength: float } }."""
    if not isinstance(raw, dict):
        return {}
    out: dict[str, dict[str, float]] = {}
    for key, val in raw.items():
        tkey = str(key).strip()
        if not tkey or not isinstance(val, dict):
            continue
        strength = _clamp_strength(val.get("overlay_strength", 0.75))
        out[tkey] = {"overlay_strength": strength}
    return out


def type_styles_public(styles: dict[str, dict[str, float]]) -> dict[str, dict[str, float]]:
    """Public shape; empty map omitted by callers when desired."""
    return {k: {"overlay_strength": float(v["overlay_strength"])} for k, v in styles.items()}


@dataclass
class WorkspaceState:
    """In-memory workspace with independent single vs dual chart memories."""

    layout_mode: LayoutMode = LAYOUT_SINGLE
    primary: ChartSelection = field(default_factory=default_single)
    dual_top: ChartSelection = field(default_factory=default_dual_top)
    dual_bottom: ChartSelection = field(default_factory=default_dual_bottom)
    primary_indicators: list[IndicatorConfig] = field(default_factory=list)
    dual_top_indicators: list[IndicatorConfig] = field(default_factory=list)
    dual_bottom_indicators: list[IndicatorConfig] = field(default_factory=list)
    primary_type_styles: dict[str, dict[str, float]] = field(default_factory=dict)
    dual_top_type_styles: dict[str, dict[str, float]] = field(default_factory=dict)
    dual_bottom_type_styles: dict[str, dict[str, float]] = field(default_factory=dict)
    watchlists: list[Watchlist] = field(default_factory=lambda: default_watchlists())
    active_watchlist_id: str = WATCHLIST_CORE_ID

    def _chart_public(
        self,
        chart_id: str,
        selection: ChartSelection,
        indicators: list[IndicatorConfig],
        type_styles: dict[str, dict[str, float]],
    ) -> dict[str, Any]:
        body: dict[str, Any] = {
            "id": chart_id,
            **selection.to_dict(),
            "indicators": [c.to_public() for c in indicators],
        }
        if type_styles:
            body["type_styles"] = type_styles_public(type_styles)
        return body

    def active_charts(self) -> list[dict[str, Any]]:
        """Charts visible for the current layout (for snapshot / TUI restore)."""
        if self.layout_mode == LAYOUT_DUAL:
            return [
                self._chart_public(
                    CHART_TOP,
                    self.dual_top,
                    self.dual_top_indicators,
                    self.dual_top_type_styles,
                ),
                self._chart_public(
                    CHART_BOTTOM,
                    self.dual_bottom,
                    self.dual_bottom_indicators,
                    self.dual_bottom_type_styles,
                ),
            ]
        return [
            self._chart_public(
                CHART_PRIMARY,
                self.primary,
                self.primary_indicators,
                self.primary_type_styles,
            )
        ]

    def to_public(self) -> dict[str, Any]:
        return {
            "layout_mode": self.layout_mode,
            "charts": self.active_charts(),
            "watchlists": [wl.to_public() for wl in self.watchlists],
            "active_watchlist_id": self.active_watchlist_id,
        }

    def set_layout(self, layout_mode: str) -> None:
        if layout_mode not in VALID_LAYOUTS:
            raise ValueError(f"unsupported layout_mode: {layout_mode}")
        self.layout_mode = layout_mode  # type: ignore[assignment]

    def set_chart(self, chart_id: str, instrument: str, timeframe: str) -> None:
        selection = ChartSelection(
            instrument=instrument.strip().upper(),
            timeframe=timeframe.strip(),
        )
        if chart_id == CHART_PRIMARY:
            self.primary = selection
        elif chart_id == CHART_TOP:
            self.dual_top = selection
        elif chart_id == CHART_BOTTOM:
            self.dual_bottom = selection
        else:
            raise ValueError(f"unknown chart_id: {chart_id}")

    def resolve_chart_id(self, chart_id: str | None) -> str:
        """Default chart_id for the current layout when the client omits it."""
        if chart_id is not None and chart_id.strip():
            return chart_id.strip()
        if self.layout_mode == LAYOUT_DUAL:
            return CHART_TOP
        return CHART_PRIMARY

    def validate_chart_id_for_layout(self, chart_id: str) -> None:
        if self.layout_mode == LAYOUT_DUAL:
            if chart_id not in (CHART_TOP, CHART_BOTTOM):
                raise ValueError(
                    f"chart_id {chart_id!r} invalid for dual-vertical "
                    f"(use {CHART_TOP!r} or {CHART_BOTTOM!r})"
                )
        else:
            if chart_id != CHART_PRIMARY:
                raise ValueError(
                    f"chart_id {chart_id!r} invalid for single "
                    f"(use {CHART_PRIMARY!r})"
                )

    def active_chart_ids(self) -> list[str]:
        if self.layout_mode == LAYOUT_DUAL:
            return [CHART_TOP, CHART_BOTTOM]
        return [CHART_PRIMARY]

    def selection_for(self, chart_id: str) -> ChartSelection:
        if chart_id == CHART_PRIMARY:
            return self.primary
        if chart_id == CHART_TOP:
            return self.dual_top
        if chart_id == CHART_BOTTOM:
            return self.dual_bottom
        raise ValueError(f"unknown chart_id: {chart_id}")

    def indicators_for(self, chart_id: str) -> list[IndicatorConfig]:
        if chart_id == CHART_PRIMARY:
            return list(self.primary_indicators)
        if chart_id == CHART_TOP:
            return list(self.dual_top_indicators)
        if chart_id == CHART_BOTTOM:
            return list(self.dual_bottom_indicators)
        raise ValueError(f"unknown chart_id: {chart_id}")

    def set_indicators(self, chart_id: str, configs: list[IndicatorConfig]) -> None:
        if chart_id == CHART_PRIMARY:
            self.primary_indicators = list(configs)
        elif chart_id == CHART_TOP:
            self.dual_top_indicators = list(configs)
        elif chart_id == CHART_BOTTOM:
            self.dual_bottom_indicators = list(configs)
        else:
            raise ValueError(f"unknown chart_id: {chart_id}")

    def type_styles_for(self, chart_id: str) -> dict[str, dict[str, float]]:
        if chart_id == CHART_PRIMARY:
            return dict(self.primary_type_styles)
        if chart_id == CHART_TOP:
            return dict(self.dual_top_type_styles)
        if chart_id == CHART_BOTTOM:
            return dict(self.dual_bottom_type_styles)
        raise ValueError(f"unknown chart_id: {chart_id}")

    def set_type_styles(
        self, chart_id: str, styles: dict[str, dict[str, float]]
    ) -> None:
        cleaned = parse_type_styles(styles)
        if chart_id == CHART_PRIMARY:
            self.primary_type_styles = cleaned
        elif chart_id == CHART_TOP:
            self.dual_top_type_styles = cleaned
        elif chart_id == CHART_BOTTOM:
            self.dual_bottom_type_styles = cleaned
        else:
            raise ValueError(f"unknown chart_id: {chart_id}")

    def all_chart_indicator_slots(self) -> list[tuple[str, list[IndicatorConfig]]]:
        """Every persisted chart slot (including inactive layout memory)."""
        return [
            (CHART_PRIMARY, list(self.primary_indicators)),
            (CHART_TOP, list(self.dual_top_indicators)),
            (CHART_BOTTOM, list(self.dual_bottom_indicators)),
        ]

    def active_watchlist(self) -> Watchlist:
        for wl in self.watchlists:
            if wl.id == self.active_watchlist_id:
                return wl
        if self.watchlists:
            self.active_watchlist_id = self.watchlists[0].id
            return self.watchlists[0]
        # Should not happen; re-seed.
        self.watchlists = default_watchlists()
        self.active_watchlist_id = WATCHLIST_CORE_ID
        return self.watchlists[0]

    def all_watchlist_symbols(self) -> list[str]:
        seen: list[str] = []
        for wl in self.watchlists:
            for sym in wl.symbols:
                if sym not in seen:
                    seen.append(sym)
        return seen

    def set_active_watchlist(self, watchlist_id: str) -> None:
        wid = watchlist_id.strip()
        if not any(wl.id == wid for wl in self.watchlists):
            raise ValueError(f"unknown watchlist_id: {wid}")
        self.active_watchlist_id = wid

    def add_symbol(self, symbol: str) -> None:
        sym = symbol.strip().upper()
        if not sym:
            raise ValueError("symbol is required")
        wl = self.active_watchlist()
        if sym not in wl.symbols:
            wl.symbols.append(sym)

    def remove_symbol(self, symbol: str) -> None:
        sym = symbol.strip().upper()
        if not sym:
            raise ValueError("symbol is required")
        wl = self.active_watchlist()
        wl.symbols = [s for s in wl.symbols if s != sym]

    def to_storage(self) -> dict[str, Any]:
        """Full state for disk (includes inactive layout memory)."""
        return {
            "layout_mode": self.layout_mode,
            "primary": self.primary.to_dict(),
            "dual_top": self.dual_top.to_dict(),
            "dual_bottom": self.dual_bottom.to_dict(),
            "primary_indicators": [c.to_storage() for c in self.primary_indicators],
            "dual_top_indicators": [c.to_storage() for c in self.dual_top_indicators],
            "dual_bottom_indicators": [
                c.to_storage() for c in self.dual_bottom_indicators
            ],
            "primary_type_styles": type_styles_public(self.primary_type_styles),
            "dual_top_type_styles": type_styles_public(self.dual_top_type_styles),
            "dual_bottom_type_styles": type_styles_public(
                self.dual_bottom_type_styles
            ),
            "watchlists": [wl.to_storage() for wl in self.watchlists],
            "active_watchlist_id": self.active_watchlist_id,
        }

    @classmethod
    def from_storage(
        cls,
        data: dict[str, Any],
        *,
        include_vix: bool = False,
    ) -> WorkspaceState:
        layout = data.get("layout_mode", LAYOUT_SINGLE)
        if layout not in VALID_LAYOUTS:
            layout = LAYOUT_SINGLE

        def _sel(key: str, default: ChartSelection) -> ChartSelection:
            raw = data.get(key)
            if not isinstance(raw, dict):
                return default
            inst = str(raw.get("instrument", default.instrument)).strip().upper()
            tf = str(raw.get("timeframe", default.timeframe)).strip()
            if not inst or not tf:
                return default
            return ChartSelection(instrument=inst, timeframe=tf)

        watchlists = _watchlists_from_storage(data, include_vix=include_vix)
        active = str(data.get("active_watchlist_id", "")).strip()
        if not active or not any(wl.id == active for wl in watchlists):
            active = watchlists[0].id

        return cls(
            layout_mode=layout,  # type: ignore[arg-type]
            primary=_sel("primary", default_single()),
            dual_top=_sel("dual_top", default_dual_top()),
            dual_bottom=_sel("dual_bottom", default_dual_bottom()),
            primary_indicators=indicators_from_storage(
                data.get("primary_indicators")
            ),
            dual_top_indicators=indicators_from_storage(
                data.get("dual_top_indicators")
            ),
            dual_bottom_indicators=indicators_from_storage(
                data.get("dual_bottom_indicators")
            ),
            primary_type_styles=parse_type_styles(data.get("primary_type_styles")),
            dual_top_type_styles=parse_type_styles(data.get("dual_top_type_styles")),
            dual_bottom_type_styles=parse_type_styles(
                data.get("dual_bottom_type_styles")
            ),
            watchlists=watchlists,
            active_watchlist_id=active,
        )


def _watchlists_from_storage(
    data: dict[str, Any],
    *,
    include_vix: bool,
) -> list[Watchlist]:
    raw = data.get("watchlists")
    if not isinstance(raw, list) or not raw:
        return default_watchlists(include_vix=include_vix)
    parsed: list[Watchlist] = []
    for item in raw:
        if not isinstance(item, dict):
            continue
        wl = Watchlist.from_storage(item)
        if wl is not None:
            parsed.append(wl)
    if not parsed:
        return default_watchlists(include_vix=include_vix)
    # Ensure at least two lists so the switcher is usable after corrupt trim.
    if len(parsed) == 1:
        if parsed[0].id != WATCHLIST_FOCUS_ID:
            parsed.append(Watchlist(id=WATCHLIST_FOCUS_ID, name="Focus", symbols=[]))
        else:
            parsed.insert(
                0,
                Watchlist(
                    id=WATCHLIST_CORE_ID,
                    name="Core",
                    symbols=default_core_symbols(include_vix=include_vix),
                ),
            )
    return parsed


class WorkspaceStore:
    """Load/save workspace JSON. Missing/corrupt file → product defaults."""

    def __init__(
        self,
        path: Path | None = None,
        *,
        include_vix: bool = False,
    ) -> None:
        self.path = path
        self.include_vix = include_vix
        self.state = WorkspaceState(
            watchlists=default_watchlists(include_vix=include_vix),
            active_watchlist_id=WATCHLIST_CORE_ID,
        )
        if path is not None:
            self.load()

    def load(self) -> WorkspaceState:
        if self.path is None or not self.path.is_file():
            self.state = WorkspaceState(
                watchlists=default_watchlists(include_vix=self.include_vix),
            )
            return self.state
        try:
            raw = json.loads(self.path.read_text(encoding="utf-8"))
            if not isinstance(raw, dict):
                self.state = WorkspaceState(
                    watchlists=default_watchlists(include_vix=self.include_vix),
                )
            else:
                self.state = WorkspaceState.from_storage(
                    raw, include_vix=self.include_vix
                )
        except (OSError, json.JSONDecodeError, TypeError, ValueError):
            self.state = WorkspaceState(
                watchlists=default_watchlists(include_vix=self.include_vix),
            )
        return self.state

    def save(self) -> None:
        if self.path is None:
            return
        self.path.parent.mkdir(parents=True, exist_ok=True)
        payload = json.dumps(self.state.to_storage(), indent=2, sort_keys=True)
        self.path.write_text(payload + "\n", encoding="utf-8")

    def set_layout(self, layout_mode: str) -> dict[str, Any]:
        self.state.set_layout(layout_mode)
        self.save()
        return self.state.to_public()

    def set_chart(self, chart_id: str, instrument: str, timeframe: str) -> None:
        self.state.set_chart(chart_id, instrument, timeframe)
        self.save()

    def set_indicators(
        self, chart_id: str, configs: list[IndicatorConfig]
    ) -> dict[str, Any]:
        self.state.set_indicators(chart_id, configs)
        self.save()
        return self.state.to_public()

    def set_type_styles(
        self, chart_id: str, styles: dict[str, dict[str, float]]
    ) -> dict[str, Any]:
        self.state.set_type_styles(chart_id, styles)
        self.save()
        return self.state.to_public()

    def set_active_watchlist(self, watchlist_id: str) -> dict[str, Any]:
        self.state.set_active_watchlist(watchlist_id)
        self.save()
        return self.state.to_public()

    def add_symbol(self, symbol: str) -> dict[str, Any]:
        self.state.add_symbol(symbol)
        self.save()
        return self.state.to_public()

    def remove_symbol(self, symbol: str) -> dict[str, Any]:
        self.state.remove_symbol(symbol)
        self.save()
        return self.state.to_public()
