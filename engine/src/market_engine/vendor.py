"""Market-data vendor seam: history + live ticks (fake + LSE adapters)."""

from __future__ import annotations

import asyncio
import random
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Protocol


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

# SPY @ 1h: short intraday series for timeframe-selection contract tests.
_SPY_1H_CLOSES: tuple[float, ...] = (
    540.0,
    540.2,
    540.5,
    540.1,
    540.8,
    541.0,
    541.2,
    541.0,
    541.5,
    541.8,
)

# Anchor: 2024-07-01 00:00:00 UTC.
_SPY_1D_START_TS = 1_719_792_000
_DAY_SECONDS = 86_400
_HOUR_SECONDS = 3_600

# v1 product timeframes (domain). Unknown intervals are unavailable.
V1_TIMEFRAMES: frozenset[str] = frozenset(
    {"1m", "3m", "5m", "15m", "30m", "1h", "4h", "1D", "1W"}
)

# Domain timeframe → LSE vault resolution (case-sensitive product vs lower-case API).
DOMAIN_TO_LSE_TIMEFRAME: dict[str, str] = {
    "1m": "1m",
    "3m": "3m",
    "5m": "5m",
    "15m": "15m",
    "30m": "30m",
    "1h": "1h",
    "4h": "4h",
    "1D": "1d",
    "1W": "1w",
}

def domain_to_lse_instrument(instrument: str) -> str:
    """Map domain instrument id to LSE wire symbol (identity for v1 product names).

    Mapping stays inside the adapter; IPC always uses canonical ids (SPY, ES, …).
    Override here if a future domain name diverges from the LSE catalog symbol.
    """
    return instrument.strip().upper()


def parse_lse_timestamp(value: Any) -> int | None:
    """Parse LSE candle/tick timestamp to unix seconds (UTC)."""
    if value is None:
        return None
    if isinstance(value, (int, float)):
        # Heuristic: ms vs seconds
        ts = float(value)
        if ts > 1e12:
            ts /= 1000.0
        return int(ts)
    if not isinstance(value, str):
        return None
    s = value.strip()
    if not s:
        return None
    # Numeric string
    try:
        return parse_lse_timestamp(float(s))
    except ValueError:
        pass
    try:
        dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        return int(dt.timestamp())
    except ValueError:
        return None


def bars_from_lse_candles(rows: list[dict[str, Any]]) -> tuple[Bar, ...]:
    """Convert LSE candle dicts (timestamp/open/high/low/close/volume) to Bars."""
    bars: list[Bar] = []
    for row in rows:
        ts = parse_lse_timestamp(row.get("timestamp") if "timestamp" in row else row.get("ts"))
        if ts is None:
            continue
        try:
            open_ = float(row["open"])
            high = float(row["high"])
            low = float(row["low"])
            close = float(row["close"])
            volume = float(row.get("volume") or 0.0)
        except (KeyError, TypeError, ValueError):
            continue
        bars.append(
            Bar(
                ts=ts,
                open=open_,
                high=high,
                low=low,
                close=close,
                volume=volume,
            )
        )
    bars.sort(key=lambda b: b.ts)
    return tuple(bars)


def _bars_from_closes(
    start_ts: int,
    closes: tuple[float, ...],
    *,
    period_seconds: int,
    base_volume: float = 50_000_000.0,
) -> tuple[Bar, ...]:
    bars: list[Bar] = []
    prev_close = closes[0]
    for i, close in enumerate(closes):
        open_ = prev_close if i > 0 else close
        # Deterministic wick geometry from open/close (no randomness).
        high = max(open_, close) + 0.5
        low = min(open_, close) - 0.5
        volume = base_volume + i * 100_000.0
        bars.append(
            Bar(
                ts=start_ts + i * period_seconds,
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
    ("SPY", "1D"): _bars_from_closes(
        _SPY_1D_START_TS, _SPY_1D_CLOSES, period_seconds=_DAY_SECONDS
    ),
    ("SPY", "1h"): _bars_from_closes(
        _SPY_1D_START_TS,
        _SPY_1H_CLOSES,
        period_seconds=_HOUR_SECONDS,
        base_volume=1_000_000.0,
    ),
    # QQQ ready for dual-layout defaults later; still valid fake coverage.
    ("QQQ", "1D"): _bars_from_closes(
        _SPY_1D_START_TS,
        tuple(c - 50.0 for c in _SPY_1D_CLOSES),
        period_seconds=_DAY_SECONDS,
    ),
    # ES for instrument-selection demos (distinct level from SPY/QQQ).
    ("ES", "1D"): _bars_from_closes(
        _SPY_1D_START_TS,
        tuple(c + 5000.0 for c in _SPY_1D_CLOSES),
        period_seconds=_DAY_SECONDS,
        base_volume=200_000.0,
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
        # Outside the v1 set → explicit unavailable (no invented series).
        if timeframe not in V1_TIMEFRAMES:
            return HistoryResult(
                instrument=instrument,
                timeframe=timeframe,
                available=False,
                bars=(),
            )
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


class LseVendor:
    """London Strategic Edge adapter: same history + live interest seam as fake.

    Domain instruments/timeframes stay canonical on the engine IPC boundary.
    LSE symbol/resolution mapping and credentials live only here.

    ``client`` is injectable for tests (stub with candles/subscribe/on/connect).
    Production uses ``lse.LSE`` with ``LSE_API_KEY`` (or explicit ``api_key``).
    """

    def __init__(
        self,
        client: Any | None = None,
        *,
        api_key: str | None = None,
        history_limit: int = 500,
        start_stream: bool = True,
    ) -> None:
        if client is None:
            from lse import LSE

            client = LSE(api_key=api_key)
        self._client = client
        self._history_limit = history_limit
        self._start_stream = start_stream
        self._handlers: dict[str, list[TickHandler]] = {}
        # LSE wire symbol → domain instrument for tick fan-out.
        self._lse_to_domain: dict[str, str] = {}
        self._lock = threading.Lock()
        self._stream_thread: threading.Thread | None = None
        self._stream_started = False
        # Register once; LSE client dispatches Tick-like objects.
        on = getattr(self._client, "on", None)
        if callable(on):
            on("tick", self._on_lse_tick)

    def fetch_history(self, instrument: str, timeframe: str) -> HistoryResult:
        instrument = instrument.strip().upper()
        timeframe = timeframe.strip()
        if timeframe not in V1_TIMEFRAMES:
            return HistoryResult(
                instrument=instrument,
                timeframe=timeframe,
                available=False,
                bars=(),
            )
        lse_tf = DOMAIN_TO_LSE_TIMEFRAME[timeframe]
        lse_sym = domain_to_lse_instrument(instrument)
        try:
            rows = self._client.candles(
                lse_sym,
                lse_tf,
                limit=self._history_limit,
                order="asc",
            )
        except Exception:
            # LSEError, transport, bad key, unknown symbol — explicit unavailable.
            return HistoryResult(
                instrument=instrument,
                timeframe=timeframe,
                available=False,
                bars=(),
            )
        if not rows:
            return HistoryResult(
                instrument=instrument,
                timeframe=timeframe,
                available=False,
                bars=(),
            )
        bars = bars_from_lse_candles(list(rows))
        if not bars:
            return HistoryResult(
                instrument=instrument,
                timeframe=timeframe,
                available=False,
                bars=(),
            )
        return HistoryResult(
            instrument=instrument,
            timeframe=timeframe,
            available=True,
            bars=bars,
        )

    def subscribe(self, instrument: str, handler: TickHandler) -> None:
        instrument = instrument.strip().upper()
        lse_sym = domain_to_lse_instrument(instrument)
        with self._lock:
            handlers = self._handlers.setdefault(instrument, [])
            if handler not in handlers:
                handlers.append(handler)
            self._lse_to_domain[lse_sym] = instrument
            first = len(handlers) == 1
        if first:
            try:
                self._client.subscribe([lse_sym])
            except Exception:
                pass
            self._ensure_stream()

    def unsubscribe(self, instrument: str, handler: TickHandler) -> None:
        instrument = instrument.strip().upper()
        lse_sym = domain_to_lse_instrument(instrument)
        should_unsub = False
        with self._lock:
            handlers = self._handlers.get(instrument)
            if not handlers:
                return
            if handler in handlers:
                handlers.remove(handler)
            if not handlers:
                self._handlers.pop(instrument, None)
                self._lse_to_domain.pop(lse_sym, None)
                should_unsub = True
        if should_unsub:
            try:
                self._client.unsubscribe([lse_sym])
            except Exception:
                pass

    def close(self) -> None:
        """Stop the live stream thread and disconnect the LSE client."""
        disconnect = getattr(self._client, "disconnect", None)
        if callable(disconnect):
            try:
                disconnect()
            except Exception:
                pass
        thread = self._stream_thread
        self._stream_thread = None
        self._stream_started = False
        if thread is not None and thread.is_alive() and thread is not threading.current_thread():
            thread.join(timeout=2.0)

    def _ensure_stream(self) -> None:
        if not self._start_stream:
            return
        with self._lock:
            if self._stream_started:
                return
            self._stream_started = True
        thread = threading.Thread(
            target=self._stream_loop,
            name="lse-stream",
            daemon=True,
        )
        self._stream_thread = thread
        thread.start()

    def _stream_loop(self) -> None:
        """Block on LSE connect in a side thread (own event loop; not uvicorn's)."""
        try:
            # Empty list: symbols already tracked via subscribe() / _subscriptions.
            self._client.connect([])
        except Exception:
            # Auth/network failures surface as missing live ticks; history still works.
            pass
        finally:
            with self._lock:
                self._stream_started = False

    def _on_lse_tick(self, lse_tick: Any) -> None:
        try:
            symbol = str(getattr(lse_tick, "symbol", "") or "")
            price = float(getattr(lse_tick, "price"))
        except (TypeError, ValueError, AttributeError):
            return
        volume_raw = getattr(lse_tick, "volume", None)
        try:
            volume = float(volume_raw) if volume_raw is not None else 0.0
        except (TypeError, ValueError):
            volume = 0.0
        ts_raw = getattr(lse_tick, "timestamp", None)
        if ts_raw is None:
            ts_raw = getattr(lse_tick, "ts", None)
        parsed = parse_lse_timestamp(ts_raw)
        ts = float(parsed) if parsed is not None else time.time()
        # Prefer registered domain mapping; fall back to uppercased wire symbol.
        with self._lock:
            instrument = self._lse_to_domain.get(symbol) or self._lse_to_domain.get(
                symbol.upper()
            )
            if instrument is None:
                instrument = symbol.strip().upper()
            handlers = list(self._handlers.get(instrument, ()))
        if not handlers:
            return
        tick = Tick(
            instrument=instrument,
            price=price,
            volume=volume,
            ts=ts,
        )
        for handler in handlers:
            try:
                handler(tick)
            except Exception:
                pass


def default_vendor(
    mode: str = "fake",
    *,
    auto_ticks: bool = True,
    api_key: str | None = None,
    client: Any | None = None,
) -> MarketDataVendor:
    """Build the market-data adapter for ``mode`` (``fake`` or ``lse``)."""
    if mode == "fake":
        return FakeVendor(auto_ticks=auto_ticks)
    if mode == "lse":
        return LseVendor(client=client, api_key=api_key)
    raise ValueError(f"unsupported vendor mode: {mode}")
