"""Market-data vendor seam: history fetch with fake (and later LSE) adapters."""

from __future__ import annotations

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


class MarketDataVendor(Protocol):
    """Vendor adapter: resolve history for domain instrument + timeframe ids."""

    def fetch_history(self, instrument: str, timeframe: str) -> HistoryResult: ...


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
    """In-process fake: known instruments/timeframes only; never invents the rest."""

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
        return HistoryResult(
            instrument=instrument,
            timeframe=timeframe,
            available=True,
            bars=bars,
        )


def default_vendor(mode: str = "fake") -> MarketDataVendor:
    if mode == "fake":
        return FakeVendor()
    raise ValueError(f"unsupported vendor mode: {mode}")
