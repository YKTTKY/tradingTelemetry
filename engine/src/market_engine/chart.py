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
class ChartService:
    """Loads history for chart interest and keeps live series from vendor ticks."""

    vendor: MarketDataVendor
    on_bar_update: BarUpdateCallback | None = None
    _series: dict[tuple[str, str], list[Bar]] = field(default_factory=dict)
    _subscribed: set[str] = field(default_factory=set)

    def set_interest(self, instrument: str, timeframe: str) -> HistoryResult:
        """Return historical bars for the interest, or explicit unavailability."""
        result = self.vendor.fetch_history(instrument, timeframe)
        key = (instrument, timeframe)
        if result.available:
            self._series[key] = list(result.bars)
            self._ensure_subscribed(instrument)
        else:
            self._series.pop(key, None)
        return result

    def _ensure_subscribed(self, instrument: str) -> None:
        if instrument in self._subscribed:
            return
        self.vendor.subscribe(instrument, self._on_tick)
        self._subscribed.add(instrument)

    def _on_tick(self, tick: Tick) -> None:
        for (instrument, timeframe), bars in list(self._series.items()):
            if instrument != tick.instrument:
                continue
            change = apply_tick(bars, timeframe, tick)
            if change is None:
                continue
            completed, last = change
            if self.on_bar_update is not None:
                self.on_bar_update(instrument, timeframe, completed, last)
