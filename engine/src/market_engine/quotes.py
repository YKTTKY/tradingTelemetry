"""Watchlist quote interest: last / previous close / change from vendor + ticks."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any

from market_engine.vendor import MarketDataVendor, Tick, TickHandler

QuoteUpdateCallback = Callable[[str, dict[str, Any]], None]


@dataclass
class QuoteRow:
    """One instrument's quote fields for the watchlist sidebar."""

    symbol: str
    status: str  # "ok" | "unavailable"
    last: float | None = None
    previous_close: float | None = None
    change: float | None = None
    change_pct: float | None = None

    def recompute(self) -> None:
        if (
            self.status != "ok"
            or self.last is None
            or self.previous_close is None
            or self.previous_close == 0.0
        ):
            if self.status == "ok" and self.last is not None and self.previous_close is not None:
                self.change = self.last - self.previous_close
                self.change_pct = None
            return
        self.change = self.last - self.previous_close
        self.change_pct = self.change / self.previous_close

    def to_dict(self) -> dict[str, Any]:
        return {
            "symbol": self.symbol,
            "status": self.status,
            "last": self.last,
            "previous_close": self.previous_close,
            "change": self.change,
            "change_pct": self.change_pct,
        }


def quote_from_daily_history(
    symbol: str,
    available: bool,
    closes: list[float],
) -> QuoteRow:
    """Build a row from 1D closes: last = tip, previous_close = previous day close."""
    if not available or not closes:
        return QuoteRow(symbol=symbol, status="unavailable")
    last = float(closes[-1])
    # Need at least two daily closes for a true previous-day close.
    prev = float(closes[-2]) if len(closes) >= 2 else None
    row = QuoteRow(
        symbol=symbol,
        status="ok",
        last=last,
        previous_close=prev,
    )
    row.recompute()
    return row


@dataclass
class QuoteService:
    """Loads daily quotes for watchlist symbols and updates last on live ticks."""

    vendor: MarketDataVendor
    on_quote_update: QuoteUpdateCallback | None = None
    on_live_tick: TickHandler | None = None
    _rows: dict[str, QuoteRow] = field(default_factory=dict)
    _subscribed: set[str] = field(default_factory=set)

    def sync_symbols(self, symbols: list[str]) -> list[dict[str, Any]]:
        """Refresh interest to exactly ``symbols``; return public quote rows."""
        wanted = []
        seen: set[str] = set()
        for raw in symbols:
            sym = raw.strip().upper()
            if not sym or sym in seen:
                continue
            seen.add(sym)
            wanted.append(sym)

        for sym in list(self._rows):
            if sym not in seen:
                del self._rows[sym]

        for sym in wanted:
            if sym not in self._rows:
                self._rows[sym] = self._load_quote(sym)

        self._prune_subscriptions()
        for sym in wanted:
            row = self._rows[sym]
            if row.status == "ok":
                self._ensure_subscribed(sym)

        return [self._rows[s].to_dict() for s in wanted]

    def last_price(self, symbol: str) -> float | None:
        """Last quote for ``symbol`` if this desk already has a live/ok row."""
        row = self._rows.get(symbol.strip().upper())
        if row is None or row.last is None:
            return None
        return float(row.last)

    def _load_quote(self, symbol: str) -> QuoteRow:
        result = self.vendor.fetch_history(symbol, "1D")
        closes = [b.close for b in result.bars] if result.available else []
        return quote_from_daily_history(symbol, result.available, closes)

    def _ensure_subscribed(self, instrument: str) -> None:
        if instrument in self._subscribed:
            return
        self.vendor.subscribe(instrument, self._on_tick)
        self._subscribed.add(instrument)

    def _prune_subscriptions(self) -> None:
        needed = {
            sym for sym, row in self._rows.items() if row.status == "ok"
        }
        for instrument in list(self._subscribed):
            if instrument not in needed:
                self.vendor.unsubscribe(instrument, self._on_tick)
                self._subscribed.discard(instrument)

    def _on_tick(self, tick: Tick) -> None:
        if self.on_live_tick is not None:
            self.on_live_tick(tick)
        row = self._rows.get(tick.instrument)
        if row is None or row.status != "ok":
            return
        row.last = float(tick.price)
        row.recompute()
        if self.on_quote_update is not None:
            self.on_quote_update(tick.instrument, row.to_dict())
