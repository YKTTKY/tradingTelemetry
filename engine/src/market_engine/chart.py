"""Chart interest: history load + live last-bar updates from vendor ticks."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field

from market_engine.vendor import Bar, HistoryResult, MarketDataVendor, Tick

# Domain timeframes → bar period in seconds (unix-bucket alignment).
TIMEFRAME_SECONDS: dict[str, int] = {
    "1m": 60,
    "3m": 180,
    "5m": 300,
    "15m": 900,
    "30m": 1_800,
    "1h": 3_600,
    "4h": 14_400,
    "1D": 86_400,
    "1W": 604_800,
}

BarUpdateCallback = Callable[[str, str, list[Bar], Bar], None]


def bar_open_ts(ts: float, timeframe: str) -> int:
    period = TIMEFRAME_SECONDS.get(timeframe)
    if period is None:
        raise ValueError(f"unsupported timeframe: {timeframe}")
    return int(ts // period) * period


def apply_tick(bars: list[Bar], timeframe: str, tick: Tick) -> tuple[list[Bar], Bar] | None:
    """
    Mutate ``bars`` with ``tick``.

    Returns ``(completed_bars, current_last_bar)`` when the series changes,
    or ``None`` if there is nothing to apply (empty series).
    """
    if not bars:
        return None

    open_ts = bar_open_ts(tick.ts, timeframe)
    last = bars[-1]

    if open_ts < last.ts:
        # Out-of-order / stale tick — ignore.
        return None

    if open_ts == last.ts:
        updated = Bar(
            ts=last.ts,
            open=last.open,
            high=max(last.high, tick.price),
            low=min(last.low, tick.price),
            close=tick.price,
            volume=last.volume + tick.volume,
        )
        bars[-1] = updated
        return ([], updated)

    # Period roll: one or more bars may be skipped; only emit the immediate
    # previous tip as completed (gaps are empty — no invented middles).
    completed = [last]
    new_bar = Bar(
        ts=open_ts,
        open=tick.price,
        high=tick.price,
        low=tick.price,
        close=tick.price,
        volume=tick.volume,
    )
    bars.append(new_bar)
    return (completed, new_bar)


@dataclass
class _ChartSlot:
    instrument: str
    timeframe: str
    bars: list[Bar]


@dataclass
class ChartService:
    """Loads history per chart slot and keeps live series from vendor ticks.

    Multiple chart slots may be active (dual layout). Each ``set_interest``
    updates one ``chart_id`` without clearing the others. Layout changes call
    ``sync_active_charts`` to drop slots that are no longer visible.
    """

    vendor: MarketDataVendor
    on_bar_update: BarUpdateCallback | None = None
    _slots: dict[str, _ChartSlot] = field(default_factory=dict)
    _subscribed: set[str] = field(default_factory=set)

    def set_interest(
        self,
        instrument: str,
        timeframe: str,
        chart_id: str = "primary",
    ) -> HistoryResult:
        """Set interest for one chart slot; return history or unavailability."""
        instrument = instrument.strip().upper()
        timeframe = timeframe.strip()
        chart_id = chart_id.strip()

        if timeframe not in TIMEFRAME_SECONDS:
            self._drop_slot(chart_id)
            return HistoryResult(
                instrument=instrument,
                timeframe=timeframe,
                available=False,
                bars=(),
            )

        result = self.vendor.fetch_history(instrument, timeframe)
        result = HistoryResult(
            instrument=instrument,
            timeframe=timeframe,
            available=result.available,
            bars=result.bars,
        )

        if result.available:
            self._slots[chart_id] = _ChartSlot(
                instrument=instrument,
                timeframe=timeframe,
                bars=list(result.bars),
            )
            self._ensure_subscribed(instrument)
            self._prune_subscriptions()
        else:
            # Keep the slot's selection for workspace but no live series.
            self._drop_slot(chart_id)
            self._prune_subscriptions()

        return result

    def sync_active_charts(self, chart_ids: list[str]) -> None:
        """Drop series for chart slots not in the active layout set."""
        active = set(chart_ids)
        for chart_id in list(self._slots):
            if chart_id not in active:
                del self._slots[chart_id]
        self._prune_subscriptions()

    def active_series_keys(self) -> list[tuple[str, str, str]]:
        """Return (chart_id, instrument, timeframe) for slots with live series."""
        return [
            (cid, slot.instrument, slot.timeframe)
            for cid, slot in self._slots.items()
        ]

    def _drop_slot(self, chart_id: str) -> None:
        self._slots.pop(chart_id, None)

    def _prune_subscriptions(self) -> None:
        needed = {slot.instrument for slot in self._slots.values()}
        for instrument in list(self._subscribed):
            if instrument not in needed:
                self.vendor.unsubscribe(instrument, self._on_tick)
                self._subscribed.discard(instrument)

    def _ensure_subscribed(self, instrument: str) -> None:
        if instrument in self._subscribed:
            return
        self.vendor.subscribe(instrument, self._on_tick)
        self._subscribed.add(instrument)

    def _on_tick(self, tick: Tick) -> None:
        # Group by (instrument, timeframe) so two charts on the same pair
        # share one update event (hub keys on instrument+timeframe).
        emitted: set[tuple[str, str]] = set()
        for slot in list(self._slots.values()):
            if slot.instrument != tick.instrument:
                continue
            change = apply_tick(slot.bars, slot.timeframe, tick)
            if change is None:
                continue
            completed, last = change
            key = (slot.instrument, slot.timeframe)
            if key in emitted:
                continue
            emitted.add(key)
            if self.on_bar_update is not None:
                self.on_bar_update(slot.instrument, slot.timeframe, completed, last)
