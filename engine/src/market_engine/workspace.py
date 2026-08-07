"""File-backed workspace: layout mode + per-chart instrument/timeframe."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal

LayoutMode = Literal["single", "dual-vertical"]

LAYOUT_SINGLE: LayoutMode = "single"
LAYOUT_DUAL: LayoutMode = "dual-vertical"
VALID_LAYOUTS: frozenset[str] = frozenset({LAYOUT_SINGLE, LAYOUT_DUAL})

CHART_PRIMARY = "primary"
CHART_TOP = "top"
CHART_BOTTOM = "bottom"


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
class WorkspaceState:
    """In-memory workspace with independent single vs dual chart memories."""

    layout_mode: LayoutMode = LAYOUT_SINGLE
    primary: ChartSelection = field(default_factory=default_single)
    dual_top: ChartSelection = field(default_factory=default_dual_top)
    dual_bottom: ChartSelection = field(default_factory=default_dual_bottom)

    def active_charts(self) -> list[dict[str, str]]:
        """Charts visible for the current layout (for snapshot / TUI restore)."""
        if self.layout_mode == LAYOUT_DUAL:
            return [
                {"id": CHART_TOP, **self.dual_top.to_dict()},
                {"id": CHART_BOTTOM, **self.dual_bottom.to_dict()},
            ]
        return [{"id": CHART_PRIMARY, **self.primary.to_dict()}]

    def to_public(self) -> dict[str, Any]:
        return {
            "layout_mode": self.layout_mode,
            "charts": self.active_charts(),
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

    def to_storage(self) -> dict[str, Any]:
        """Full state for disk (includes inactive layout memory)."""
        return {
            "layout_mode": self.layout_mode,
            "primary": self.primary.to_dict(),
            "dual_top": self.dual_top.to_dict(),
            "dual_bottom": self.dual_bottom.to_dict(),
        }

    @classmethod
    def from_storage(cls, data: dict[str, Any]) -> WorkspaceState:
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

        return cls(
            layout_mode=layout,  # type: ignore[arg-type]
            primary=_sel("primary", default_single()),
            dual_top=_sel("dual_top", default_dual_top()),
            dual_bottom=_sel("dual_bottom", default_dual_bottom()),
        )


class WorkspaceStore:
    """Load/save workspace JSON. Missing/corrupt file → product defaults."""

    def __init__(self, path: Path | None = None) -> None:
        self.path = path
        self.state = WorkspaceState()
        if path is not None:
            self.load()

    def load(self) -> WorkspaceState:
        if self.path is None or not self.path.is_file():
            self.state = WorkspaceState()
            return self.state
        try:
            raw = json.loads(self.path.read_text(encoding="utf-8"))
            if not isinstance(raw, dict):
                self.state = WorkspaceState()
            else:
                self.state = WorkspaceState.from_storage(raw)
        except (OSError, json.JSONDecodeError, TypeError, ValueError):
            self.state = WorkspaceState()
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
