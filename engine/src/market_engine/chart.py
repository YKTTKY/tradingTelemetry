"""Chart interest: history load + live last-bar updates from vendor ticks."""

from __future__ import annotations

import time
from collections.abc import Callable
from dataclasses import dataclass, field

from market_engine.vendor import Bar, HistoryResult, MarketDataVendor, Tick, TickHandler

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
Paper1mCallback = Callable[[str, Bar], None]


def bar_open_ts(ts: float, timeframe: str) -> int:
    period = TIMEFRAME_SECONDS.get(timeframe)
    if period is None:
        raise ValueError(f"unsupported timeframe: {timeframe}")
    return int(ts // period) * period


def align_live_tick(tick: Tick, last_bar_ts: int, timeframe: str) -> Tick | None:
    """Place a live print onto the last-bar timeline.

    In-order vendor time (bar open >= last) is unchanged so fake-vendor sim
    clocks keep today's apply_tick path.

    When vendor bar-open is behind the last bar, re-bucket on wall clock.
    Vendor time does not place the bar; delay uses raw ``tick.ts``.
    Clock skew (wall-clock open still behind last) still drops the print.
    """
    vendor_open = bar_open_ts(tick.ts, timeframe)
    if vendor_open >= last_bar_ts:
        return tick
    wall_open = bar_open_ts(time.time(), timeframe)
    if wall_open < last_bar_ts:
        return None
    return Tick(
        instrument=tick.instrument,
        price=tick.price,
        volume=tick.volume,
        ts=float(wall_open),
    )


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
    on_paper_1m_bar: Paper1mCallback | None = None
    on_live_tick: TickHandler | None = None
    _slots: dict[str, _ChartSlot] = field(default_factory=dict)
    _subscribed: set[str] = field(default_factory=set)
    # Engine-owned 1m series for paper bar-touch eval (independent of chart TF).
    _paper_1m: dict[str, list[Bar]] = field(default_factory=dict)

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

    def bars_for(self, chart_id: str) -> list[Bar] | None:
        """Return a copy of bars for a live chart slot, or None if unavailable."""
        slot = self._slots.get(chart_id)
        if slot is None:
            return None
        return list(slot.bars)

    def slot_meta(self, chart_id: str) -> tuple[str, str] | None:
        """Return (instrument, timeframe) for a live slot."""
        slot = self._slots.get(chart_id)
        if slot is None:
            return None
        return slot.instrument, slot.timeframe

    def chart_ids_for_series(self, instrument: str, timeframe: str) -> list[str]:
        """Chart slots currently holding this instrument+timeframe series."""
        return [
            cid
            for cid, slot in self._slots.items()
            if slot.instrument == instrument and slot.timeframe == timeframe
        ]

    def sync_paper_1m(self, instruments: list[str]) -> dict[str, bool]:
        """Keep an engine-owned 1m series for paper-active instruments.

        Chart timeframe does not control this series. Unavailable 1m is
        reported as False and no bars are invented.
        """
        wanted: set[str] = set()
        for raw in instruments:
            inst = raw.strip().upper()
            if inst:
                wanted.add(inst)
        for inst in list(self._paper_1m):
            if inst not in wanted:
                del self._paper_1m[inst]
        available: dict[str, bool] = {}
        for inst in wanted:
            if inst in self._paper_1m and self._paper_1m[inst]:
                available[inst] = True
                continue
            result = self.vendor.fetch_history(inst, "1m")
            if result.available and result.bars:
                self._paper_1m[inst] = list(result.bars)
                self._ensure_subscribed(inst)
                available[inst] = True
            else:
                self._paper_1m.pop(inst, None)
                available[inst] = False
        self._prune_subscriptions()
        return available

    def paper_1m_last(self, instrument: str) -> Bar | None:
        """Last engine-owned 1m bar, or None when the series is unavailable."""
        bars = self._paper_1m.get(instrument.strip().upper())
        if not bars:
            return None
        return bars[-1]

    def paper_1m_close(self, instrument: str) -> float | None:
        last = self.paper_1m_last(instrument)
        if last is None:
            return None
        return float(last.close)

    def _drop_slot(self, chart_id: str) -> None:
        self._slots.pop(chart_id, None)

    def _prune_subscriptions(self) -> None:
        needed = {slot.instrument for slot in self._slots.values()}
        needed.update(self._paper_1m.keys())
        for instrument in list(self._subscribed):
            if instrument not in needed:
                self.vendor.unsubscribe(instrument, self._on_tick)
                self._subscribed.discard(instrument)

    def _ensure_subscribed(self, instrument: str) -> None:
        if instrument in self._subscribed:
            return
        self.vendor.subscribe(instrument, self._on_tick)
        self._subscribed.add(instrument)

    def _apply_series_tick(
        self,
        bars: list[Bar],
        timeframe: str,
        tick: Tick,
        emitted: set[tuple[str, str]],
    ) -> None:
        if not bars:
            return
        placed = align_live_tick(tick, bars[-1].ts, timeframe)
        if placed is None:
            return
        change = apply_tick(bars, timeframe, placed)
        if change is None:
            return
        completed, last = change
        key = (tick.instrument, timeframe)
        if key in emitted:
            return
        emitted.add(key)
        if self.on_bar_update is not None:
            self.on_bar_update(tick.instrument, timeframe, completed, last)

    def _on_tick(self, tick: Tick) -> None:
        if self.on_live_tick is not None:
            self.on_live_tick(tick)
        # Apply engine-owned 1m before chart emits so bar-touch eval sees this print.
        paper_change: tuple[list[Bar], Bar] | None = None
        paper_bars = self._paper_1m.get(tick.instrument)
        if paper_bars:
            placed = align_live_tick(tick, paper_bars[-1].ts, "1m")
            if placed is not None:
                paper_change = apply_tick(paper_bars, "1m", placed)
        if paper_change is not None and self.on_paper_1m_bar is not None:
            self.on_paper_1m_bar(tick.instrument, paper_change[1])
        # Group by (instrument, timeframe) so two charts on the same pair
        # share one update event (hub keys on instrument+timeframe).
        emitted: set[tuple[str, str]] = set()
        for slot in list(self._slots.values()):
            if slot.instrument != tick.instrument:
                continue
            self._apply_series_tick(slot.bars, slot.timeframe, tick, emitted)
        if paper_change is not None:
            completed, last = paper_change
            key = (tick.instrument, "1m")
            if key not in emitted:
                emitted.add(key)
                if self.on_bar_update is not None:
                    self.on_bar_update(tick.instrument, "1m", completed, last)
