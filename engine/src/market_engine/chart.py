"""Chart interest: express instrument+timeframe interest and load history."""

from __future__ import annotations

from dataclasses import dataclass

from market_engine.vendor import HistoryResult, MarketDataVendor


@dataclass
class ChartService:
    """Loads history for chart interest via the vendor seam."""

    vendor: MarketDataVendor

    def set_interest(self, instrument: str, timeframe: str) -> HistoryResult:
        """Return historical bars for the interest, or explicit unavailability."""
        return self.vendor.fetch_history(instrument, timeframe)
