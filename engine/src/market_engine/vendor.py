"""Market-data vendor seam: history + live ticks (fake adapter; LSE later)."""

from __future__ import annotations

import asyncio
import random
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass
from typing import Protocol


@dataclass(frozen=True)
class Bar:
    """One OHLCV candle at a given open timestamp (unix seconds)."""

    ts: int
    open: float
    high: float
    low: float
    close: float
    volume: float

    def to_dict(self) -> dict:
        return {
            "ts": self.ts,
            "open": self.open,
            "high": self.high,
            "low": self.low,
            "close": self.close,
            "volume": self.volume,
        }


@dataclass(frozen=True)
class Tick:
    """A single last-sale (or mid) print used to advance live bars."""

    instrument: str
    price: float
    volume: float
    ts: float


@dataclass(frozen=True)
class HistoryResult:
    """Historical series for an instrument+timeframe, or explicit unavailability."""

    instrument: str
    timeframe: str
    available: bool
    bars: tuple[Bar, ...] = ()

    def to_interest_response(self) -> dict:
        return {
            "instrument": self.instrument,
            "timeframe": self.timeframe,
            "status": "ok" if self.available else "unavailable",
            "bars": [b.to_dict() for b in self.bars],
        }


TickHandler = Callable[[Tick], None]


class MarketDataVendor(Protocol):
    """Vendor adapter: history fetch + per-instrument live tick subscription."""

    def fetch_history(self, instrument: str, timeframe: str) -> HistoryResult: ...

    def subscribe(self, instrument: str, handler: TickHandler) -> None: ...

    def unsubscribe(self, instrument: str, handler: TickHandler) -> None: ...


# Deterministic daily closes for SPY @ 1D (known literals for contract tests).
# Synthetic path: slight uptrend with known OHLC geometry.
_SPY_1D_CLOSES: tuple[float, ...] = (
    540.0,
    541.5,
    539.0,
    542.25,
    544.0,
    543.0,
    545.5,
    547.0,
    546.25,
    548.0,
)

# Anchor: 2024-07-01 00:00:00 UTC, then +1 calendar day per bar.
_SPY_1D_START_TS = 1_719_792_000
_DAY_SECONDS = 86_400


def _bars_from_closes(start_ts: int, closes: tuple[float, ...]) -> tuple[Bar, ...]:
    bars: list[Bar] = []
    prev_close = closes[0]
    for i, close in enumerate(closes):
        open_ = prev_close if i > 0 else close
        # Deterministic wick geometry from open/close (no randomness).
        high = max(open_, close) + 0.5
        low = min(open_, close) - 0.5
        volume = 50_000_000.0 + i * 100_000.0
        bars.append(
            Bar(
                ts=start_ts + i * _DAY_SECONDS,
                open=open_,
                high=high,
                low=low,
                close=close,
                volume=volume,
            )
        )
        prev_close = close
    return tuple(bars)


_FAKE_HISTORY: dict[tuple[str, str], tuple[Bar, ...]] = {
    ("SPY", "1D"): _bars_from_closes(_SPY_1D_START_TS, _SPY_1D_CLOSES),
    # QQQ ready for dual-layout defaults later; still valid fake coverage.
    ("QQQ", "1D"): _bars_from_closes(
        _SPY_1D_START_TS,
        tuple(c - 50.0 for c in _SPY_1D_CLOSES),
    ),
}


class FakeVendor:
    """In-process fake: known history; live ticks via inject or optional auto-walk."""

    def __init__(
        self,
        auto_ticks: bool = False,
        auto_tick_interval_s: float = 0.25,
    ) -> None:
        self._auto_ticks = auto_ticks
        self._auto_tick_interval_s = auto_tick_interval_s
        self._handlers: dict[str, list[TickHandler]] = {}
        self._lock = threading.Lock()
        self._last_price: dict[str, float] = {}
        # Synthetic clock anchored to history tip so live ticks update the last
        # bar instead of immediately rolling a new period on wall-clock time.
        self._sim_ts: dict[str, float] = {}
        self._auto_task: asyncio.Task[None] | None = None
        self._auto_instruments: set[str] = set()

    def fetch_history(self, instrument: str, timeframe: str) -> HistoryResult:
        key = (instrument, timeframe)
        bars = _FAKE_HISTORY.get(key)
        if bars is None:
            return HistoryResult(
                instrument=instrument,
                timeframe=timeframe,
                available=False,
                bars=(),
            )
        if bars:
            self._last_price[instrument] = bars[-1].close
            # Start just inside the open last bar so the first live print updates tip.
            self._sim_ts[instrument] = float(bars[-1].ts) + 1.0
        return HistoryResult(
            instrument=instrument,
            timeframe=timeframe,
            available=True,
            bars=bars,
        )

    def subscribe(self, instrument: str, handler: TickHandler) -> None:
        with self._lock:
            handlers = self._handlers.setdefault(instrument, [])
            if handler not in handlers:
                handlers.append(handler)
            self._auto_instruments.add(instrument)
        if self._auto_ticks:
            self._ensure_auto_loop()

    def unsubscribe(self, instrument: str, handler: TickHandler) -> None:
        with self._lock:
            handlers = self._handlers.get(instrument)
            if not handlers:
                return
            if handler in handlers:
                handlers.remove(handler)
            if not handlers:
                self._handlers.pop(instrument, None)
                self._auto_instruments.discard(instrument)

    def inject_tick(
        self,
        instrument: str,
        price: float,
        volume: float = 0.0,
        ts: float | None = None,
    ) -> None:
        """Test / control-plane hook: emit one tick to current subscribers."""
        if ts is None:
            with self._lock:
                base = self._sim_ts.get(instrument)
                if base is None:
                    ts = time.time()
                else:
                    base += 1.0
                    self._sim_ts[instrument] = base
                    ts = base
        else:
            with self._lock:
                self._sim_ts[instrument] = ts
        tick = Tick(
            instrument=instrument,
            price=price,
            volume=volume,
            ts=ts,
        )
        self._last_price[instrument] = price
        self._dispatch(tick)

    def _dispatch(self, tick: Tick) -> None:
        with self._lock:
            handlers = list(self._handlers.get(tick.instrument, ()))
        for handler in handlers:
            handler(tick)

    def _ensure_auto_loop(self) -> None:
        try:
            loop = asyncio.get_running_loop()
        except RuntimeError:
            return
        if self._auto_task is None or self._auto_task.done():
            self._auto_task = loop.create_task(self._auto_tick_loop(), name="fake-auto-ticks")

    async def _auto_tick_loop(self) -> None:
        rng = random.Random(42)
        try:
            while True:
                await asyncio.sleep(self._auto_tick_interval_s)
                with self._lock:
                    instruments = list(self._auto_instruments)
                for instrument in instruments:
                    last = self._last_price.get(instrument)
                    if last is None:
                        continue
                    # Small deterministic-ish random walk for demo liveliness.
                    # Timestamp advances on the sim clock (history-anchored).
                    delta = rng.uniform(-0.15, 0.15)
                    price = max(0.01, last + delta)
                    self.inject_tick(
                        instrument,
                        price=round(price, 4),
                        volume=float(rng.randint(100, 2_000)),
                        ts=None,
                    )
        except asyncio.CancelledError:
            raise

    async def stop_auto_ticks(self) -> None:
        task = self._auto_task
        self._auto_task = None
        if task is not None:
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass


def default_vendor(mode: str = "fake", *, auto_ticks: bool = True) -> MarketDataVendor:
    if mode == "fake":
        return FakeVendor(auto_ticks=auto_ticks)
    raise ValueError(f"unsupported vendor mode: {mode}")
