"""SQLite paper book: accounts, working orders, positions, fills, balances."""

from __future__ import annotations

import sqlite3
import time
import uuid
from dataclasses import dataclass
from collections.abc import Callable
from pathlib import Path
from typing import Any

from market_engine.vendor import Bar

DEFAULT_ACCOUNT_NAME = "Paper"
DEFAULT_CURRENCY = "USD"
DEFAULT_INITIAL_BALANCE = 100_000.0
DEFAULT_COMMISSION_PER_FILL_USD = 1.0
DEFAULT_LEVERAGE_ENABLED = False
DEFAULT_LEVERAGE_MULTIPLE = 1.0
DEFAULT_MAINTENANCE_MARGIN_RATIO = 0.5

ASSET_EQUITIES = "equities"
ASSET_FUTURES = "futures"
VALID_ASSET_CLASSES = frozenset({ASSET_EQUITIES, ASSET_FUTURES})
# Domain futures roots (CME equity-index). ES/NQ map to LSE *.F in the vendor.
_FUTURES_ROOTS = frozenset({"ES", "NQ", "MES", "MNQ"})

PAPER_DEFAULTS: dict[str, Any] = {
    "name": DEFAULT_ACCOUNT_NAME,
    "currency": DEFAULT_CURRENCY,
    "initial_balance": DEFAULT_INITIAL_BALANCE,
    "commission_per_fill_usd": DEFAULT_COMMISSION_PER_FILL_USD,
    "leverage_enabled": DEFAULT_LEVERAGE_ENABLED,
    "leverage_multiple": DEFAULT_LEVERAGE_MULTIPLE,
    "maintenance_margin_ratio": DEFAULT_MAINTENANCE_MARGIN_RATIO,
}

_SCHEMA = """
CREATE TABLE IF NOT EXISTS paper_accounts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    currency TEXT NOT NULL,
    initial_balance REAL NOT NULL,
    balance REAL NOT NULL,
    commission_per_fill_usd REAL NOT NULL,
    leverage_enabled INTEGER NOT NULL,
    leverage_multiple REAL NOT NULL,
    asset_class_restriction TEXT,
    created_ts INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS paper_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS paper_working_orders (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    instrument TEXT NOT NULL,
    side TEXT NOT NULL,
    order_type TEXT NOT NULL,
    qty REAL NOT NULL,
    limit_price REAL,
    stop_price REAL,
    ref_price REAL NOT NULL,
    placed_ts INTEGER NOT NULL,
    bracket_id TEXT,
    role TEXT NOT NULL DEFAULT 'entry'
);
CREATE TABLE IF NOT EXISTS paper_positions (
    account_id TEXT NOT NULL,
    instrument TEXT NOT NULL,
    qty REAL NOT NULL,
    avg_price REAL NOT NULL,
    PRIMARY KEY (account_id, instrument)
);
CREATE TABLE IF NOT EXISTS paper_filled_orders (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    instrument TEXT NOT NULL,
    side TEXT NOT NULL,
    order_type TEXT NOT NULL,
    qty REAL NOT NULL,
    limit_price REAL,
    stop_price REAL,
    fill_price REAL NOT NULL,
    commission REAL NOT NULL,
    placed_ts INTEGER NOT NULL,
    filled_ts INTEGER NOT NULL,
    duration_s INTEGER NOT NULL,
    margin REAL,
    trade_mark_pair_id TEXT,
    trade_mark_kind TEXT
);
CREATE TABLE IF NOT EXISTS paper_balance_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id TEXT NOT NULL,
    ts INTEGER NOT NULL,
    balance REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS paper_trade_mark_pairs (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL,
    instrument TEXT NOT NULL,
    visible INTEGER NOT NULL
);
"""

VALID_SIDES = frozenset({"buy", "sell"})
VALID_ORDER_TYPES = frozenset({"market", "limit", "stop"})
ROLE_ENTRY = "entry"
ROLE_TP = "tp"
ROLE_SL = "sl"
CHILD_ROLES = frozenset({ROLE_TP, ROLE_SL})
TRADE_MARK_ENTRY = "entry"
TRADE_MARK_EXIT = "exit"


@dataclass
class PaperAccount:
    """One local USD paper account (rules + cash)."""

    id: str
    name: str
    currency: str
    initial_balance: float
    balance: float
    commission_per_fill_usd: float
    leverage_enabled: bool
    leverage_multiple: float
    asset_class_restriction: str | None
    created_ts: int

    def to_public(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "currency": self.currency,
            "initial_balance": self.initial_balance,
            "balance": self.balance,
            "commission_per_fill_usd": self.commission_per_fill_usd,
            "leverage_enabled": self.leverage_enabled,
            "leverage_multiple": self.leverage_multiple,
            "asset_class_restriction": self.asset_class_restriction,
        }


def _new_account_id() -> str:
    return "pa_" + uuid.uuid4().hex[:12]


def _new_working_order_id() -> str:
    return "wo_" + uuid.uuid4().hex[:12]


def _new_bracket_id() -> str:
    return "br_" + uuid.uuid4().hex[:12]


def _new_filled_order_id() -> str:
    return "fo_" + uuid.uuid4().hex[:12]


def _new_trade_mark_pair_id() -> str:
    return "tm_" + uuid.uuid4().hex[:12]


@dataclass
class WorkingOrder:
    """Accepted, unfilled paper order (resting until fill or cancel)."""

    id: str
    account_id: str
    instrument: str
    side: str
    type: str
    qty: float
    limit: float | None
    stop: float | None
    placed_ts: int
    ref_price: float
    bracket_id: str | None = None
    role: str = ROLE_ENTRY

    def to_public(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "account_id": self.account_id,
            "instrument": self.instrument,
            "side": self.side,
            "type": self.type,
            "qty": self.qty,
            "limit": self.limit,
            "stop": self.stop,
            "placed_ts": self.placed_ts,
            "bracket_id": self.bracket_id,
            "role": self.role,
        }

    def is_child(self) -> bool:
        return self.role in CHILD_ROLES


@dataclass
class Position:
    """Open holding for one instrument on one paper account (signed qty)."""

    account_id: str
    instrument: str
    qty: float
    avg_price: float

    def to_public(
        self,
        last: float | None = None,
        take_profit: float | None = None,
        stop_loss: float | None = None,
    ) -> dict[str, Any]:
        side = "long" if self.qty >= 0 else "short"
        abs_qty = abs(self.qty)
        last = float(self.avg_price if last is None else last)
        if self.qty >= 0:
            unrealized = (last - self.avg_price) * abs_qty
        else:
            unrealized = (self.avg_price - last) * abs_qty
        return {
            "symbol": self.instrument,
            "side": side,
            "qty": abs_qty,
            "avg_price": self.avg_price,
            "unrealized_pnl": unrealized,
            "take_profit": take_profit,
            "stop_loss": stop_loss,
        }


@dataclass
class FilledOrder:
    """Append-only filled order history row (one leg)."""

    id: str
    account_id: str
    instrument: str
    side: str
    type: str
    qty: float
    limit: float | None
    stop: float | None
    fill_price: float
    commission: float
    placed_ts: int
    filled_ts: int
    duration_s: int
    margin: float | None = None
    trade_mark_pair_id: str | None = None
    trade_mark_kind: str = TRADE_MARK_ENTRY

    def to_public(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "account_id": self.account_id,
            "symbol": self.instrument,
            "side": self.side,
            "type": self.type,
            "qty": self.qty,
            "limit": self.limit,
            "stop": self.stop,
            "fill_price": self.fill_price,
            "commission": self.commission,
            "placed_ts": self.placed_ts,
            "filled_ts": self.filled_ts,
            "duration_s": self.duration_s,
            "margin": self.margin,
            "trade_mark_pair_id": self.trade_mark_pair_id,
            "trade_mark_kind": self.trade_mark_kind,
        }


@dataclass
class TradeMarkPair:
    """Hideable entry+exit trade mark pair (does not own filled-history rows)."""

    id: str
    account_id: str
    instrument: str
    visible: bool = True


@dataclass
class BalanceRecord:
    """Cash snapshot after a fill for one paper account."""

    account_id: str
    ts: int
    balance: float

    def to_public(self) -> dict[str, Any]:
        return {"ts": self.ts, "balance": self.balance}


class PaperBook:
    """In-memory paper desk with optional SQLite durability."""

    def __init__(self, path: Path | str | None = None) -> None:
        self.path = Path(path) if path is not None else None
        self.accounts: dict[str, PaperAccount] = {}
        self.active_account_id: str = ""
        self.working_orders: dict[str, WorkingOrder] = {}
        self.positions: dict[tuple[str, str], Position] = {}
        self.filled_orders: list[FilledOrder] = []
        self.balance_history: list[BalanceRecord] = []
        self.trade_mark_pairs: dict[str, TradeMarkPair] = {}
        self._open_trade_pairs: dict[tuple[str, str], str] = {}
        self._load_or_seed()

    def to_public(self, last_prices: dict[str, float] | None = None) -> dict[str, Any]:
        accounts = [a.to_public() for a in self.accounts.values()]
        active = self.accounts.get(self.active_account_id)
        aid = self.active_account_id
        working = [
            o.to_public()
            for o in self.working_orders.values()
            if o.account_id == aid
        ]
        last_prices = last_prices or {}
        positions = []
        for p in self.positions.values():
            if p.account_id != aid or abs(p.qty) <= 1e-12:
                continue
            tp, sl = self._bracket_prices(p.account_id, p.instrument)
            positions.append(
                p.to_public(
                    last=last_prices.get(p.instrument),
                    take_profit=tp,
                    stop_loss=sl,
                )
            )
        filled = [f.to_public() for f in self.filled_orders if f.account_id == aid]
        balances = [b.to_public() for b in self.balance_history if b.account_id == aid]
        return {
            "active_account_id": self.active_account_id,
            "accounts": accounts,
            "active": active.to_public() if active is not None else None,
            "defaults": dict(PAPER_DEFAULTS),
            "working_orders": working,
            "positions": positions,
            "filled_order_history": filled,
            "balance_history": balances,
            "trade_marks": self._trade_marks_public(aid),
        }

    def instruments_needing_1m(self) -> list[str]:
        """Instruments with working orders or open positions (engine-owned 1m)."""
        inst: set[str] = set()
        for order in self.working_orders.values():
            inst.add(order.instrument)
        for pos in self.positions.values():
            if abs(pos.qty) > 1e-12:
                inst.add(pos.instrument)
        return sorted(inst)

    def create_account(
        self,
        *,
        name: str | None = None,
        initial_balance: float | None = None,
        commission_per_fill_usd: float | None = None,
        leverage_enabled: bool | None = None,
        leverage_multiple: float | None = None,
        asset_class_restriction: str | None = None,
    ) -> PaperAccount:
        cleaned_name = (name or DEFAULT_ACCOUNT_NAME).strip() or DEFAULT_ACCOUNT_NAME
        balance = (
            DEFAULT_INITIAL_BALANCE if initial_balance is None else float(initial_balance)
        )
        if balance < 0:
            raise ValueError("initial_balance must be >= 0")
        commission = (
            DEFAULT_COMMISSION_PER_FILL_USD
            if commission_per_fill_usd is None
            else float(commission_per_fill_usd)
        )
        if commission < 0:
            raise ValueError("commission_per_fill_usd must be >= 0")
        lev_on = (
            DEFAULT_LEVERAGE_ENABLED
            if leverage_enabled is None
            else bool(leverage_enabled)
        )
        lev_mult = (
            DEFAULT_LEVERAGE_MULTIPLE
            if leverage_multiple is None
            else float(leverage_multiple)
        )
        if lev_mult < 1.0:
            raise ValueError("leverage_multiple must be >= 1")
        restriction = _parse_asset_class_restriction(asset_class_restriction)
        account = PaperAccount(
            id=_new_account_id(),
            name=cleaned_name,
            currency=DEFAULT_CURRENCY,
            initial_balance=balance,
            balance=balance,
            commission_per_fill_usd=commission,
            leverage_enabled=lev_on,
            leverage_multiple=lev_mult,
            asset_class_restriction=restriction,
            created_ts=int(time.time()),
        )
        self.accounts[account.id] = account
        if not self.active_account_id:
            self.active_account_id = account.id
        self._save()
        return account

    def set_active(self, account_id: str) -> PaperAccount:
        aid = account_id.strip()
        account = self.accounts.get(aid)
        if account is None:
            raise ValueError(f"unknown paper account id: {aid}")
        self.active_account_id = aid
        self._save()
        return account

    def place_order(
        self,
        *,
        instrument: str,
        side: str,
        order_type: str,
        qty: float,
        limit: float | None = None,
        stop: float | None = None,
        last_price: float | None = None,
        take_profit: float | None = None,
        stop_loss: float | None = None,
    ) -> WorkingOrder:
        account = self._require_active()
        cleaned_instrument = instrument.strip().upper()
        if not cleaned_instrument:
            raise ValueError("instrument is required")
        self._assert_asset_class_allowed(account, cleaned_instrument)
        cleaned_side = side.strip().lower()
        if cleaned_side not in VALID_SIDES:
            raise ValueError("side must be buy or sell")
        cleaned_type = order_type.strip().lower()
        if cleaned_type not in VALID_ORDER_TYPES:
            raise ValueError("type must be market, limit, or stop")
        qty_val = _require_positive(qty, "qty")
        limit_val = _optional_positive(limit, "limit")
        stop_val = _optional_positive(stop, "stop")
        if cleaned_type == "limit":
            if limit_val is None:
                raise ValueError("limit is required for limit working orders")
            ref_price = limit_val
        elif cleaned_type == "stop":
            if stop_val is None:
                raise ValueError("stop is required for stop working orders")
            ref_price = stop_val
        else:
            # Market may rest without a last print (fill eval is ticket 03).
            if last_price is not None and last_price > 0 and _finite(last_price):
                ref_price = float(last_price)
            else:
                ref_price = 0.0
            limit_val = None
            stop_val = None
        tp_val, sl_val = self._optional_bracket_prices(
            cleaned_side, take_profit, stop_loss
        )
        self._assert_qty_supported(
            account, cleaned_side, qty_val, ref_price, instrument=cleaned_instrument
        )
        order = WorkingOrder(
            id=_new_working_order_id(),
            account_id=account.id,
            instrument=cleaned_instrument,
            side=cleaned_side,
            type=cleaned_type,
            qty=qty_val,
            limit=limit_val,
            stop=stop_val,
            placed_ts=int(time.time()),
            ref_price=ref_price,
            role=ROLE_ENTRY,
        )
        if tp_val is not None and sl_val is not None:
            self._drop_position_children(account.id, cleaned_instrument)
            bracket_id = _new_bracket_id()
            order.bracket_id = bracket_id
            self.working_orders[order.id] = order
            exit_side = _exit_side(cleaned_side)
            self._place_bracket_children(
                account=account,
                instrument=cleaned_instrument,
                exit_side=exit_side,
                qty=qty_val,
                take_profit=tp_val,
                stop_loss=sl_val,
                bracket_id=bracket_id,
            )
        else:
            self.working_orders[order.id] = order
        self._save()
        return order

    def modify_order(
        self,
        order_id: str,
        *,
        qty: float | None = None,
        limit: float | None = None,
        stop: float | None = None,
        last_price: float | None = None,
    ) -> WorkingOrder:
        account = self._require_active()
        oid = order_id.strip()
        order = self.working_orders.get(oid)
        if order is None or order.account_id != account.id:
            raise ValueError(f"unknown working order id: {oid}")
        new_qty = order.qty if qty is None else _require_positive(qty, "qty")
        new_limit = order.limit if limit is None else _optional_positive(limit, "limit")
        new_stop = order.stop if stop is None else _optional_positive(stop, "stop")
        if order.type == "limit":
            if new_limit is None:
                raise ValueError("limit is required for limit working orders")
            ref_price = new_limit
            new_stop = None
        elif order.type == "stop":
            if new_stop is None:
                raise ValueError("stop is required for stop working orders")
            ref_price = new_stop
            new_limit = None
        else:
            if last_price is not None and last_price > 0 and _finite(last_price):
                ref_price = float(last_price)
            else:
                ref_price = order.ref_price
            new_limit = None
            new_stop = None
        if order.is_child():
            # v1 children track full parent/position qty — no partials.
            new_qty = order.qty
        else:
            self._assert_qty_supported(
                account,
                order.side,
                new_qty,
                ref_price,
                except_id=order.id,
                instrument=order.instrument,
            )
        order.qty = new_qty
        order.limit = new_limit
        order.stop = new_stop
        order.ref_price = ref_price
        if order.role == ROLE_ENTRY and order.bracket_id:
            for child in self._bracket_children(order.bracket_id):
                child.qty = new_qty
        self._save()
        return order

    def cancel_order(self, order_id: str) -> WorkingOrder:
        account = self._require_active()
        oid = order_id.strip()
        order = self.working_orders.get(oid)
        if order is None or order.account_id != account.id:
            raise ValueError(f"unknown working order id: {oid}")
        if order.role == ROLE_ENTRY and order.bracket_id:
            self._drop_bracket(order.bracket_id)
        elif order.is_child() and order.bracket_id:
            self._drop_children(order.bracket_id)
        else:
            del self.working_orders[oid]
        self._save()
        return order

    def attach_bracket(
        self,
        *,
        instrument: str,
        take_profit: float,
        stop_loss: float,
    ) -> None:
        """Attach or replace TP/SL children on an open position."""
        account = self._require_active()
        inst = instrument.strip().upper()
        if not inst:
            raise ValueError("instrument is required")
        pos = self.positions.get((account.id, inst))
        if pos is None or abs(pos.qty) < 1e-12:
            raise ValueError(f"no open position for {inst}")
        entry_side = "buy" if pos.qty > 0 else "sell"
        tp_val, sl_val = self._require_bracket_prices(entry_side, take_profit, stop_loss)
        existing = None
        for order in self.working_orders.values():
            if (
                order.account_id == account.id
                and order.instrument == inst
                and order.is_child()
                and order.bracket_id
            ):
                existing = order.bracket_id
                break
        self._drop_position_children(account.id, inst)
        bracket_id = existing or _new_bracket_id()
        self._place_bracket_children(
            account=account,
            instrument=inst,
            exit_side=_exit_side(entry_side),
            qty=abs(pos.qty),
            take_profit=tp_val,
            stop_loss=sl_val,
            bracket_id=bracket_id,
        )
        self._save()

    def evaluate_bar(
        self,
        instrument: str,
        bar: Bar,
        now_ts: int | None = None,
        on_fill: Callable[[FilledOrder], None] | None = None,
    ) -> list[FilledOrder]:
        """Bar-touch fill against the engine-owned 1m last bar (no partials)."""
        inst = instrument.strip().upper()
        filled_ts = int(now_ts if now_ts is not None else time.time())
        fills: list[FilledOrder] = []
        # Two passes: parent entries first, then newly live TP/SL children.
        for _ in (0, 1):
            pending = [
                o
                for o in self.working_orders.values()
                if o.instrument == inst
                and o.account_id in self.accounts
                and self._order_is_live(o)
            ]
            pending.sort(key=_eval_rank)
            for order in pending:
                if order.id not in self.working_orders:
                    continue
                if not self._order_is_live(order):
                    continue
                price = bar_touch_fill_price(order, bar)
                if price is None:
                    continue
                filled = self._fill_working_order(order, price, filled_ts)
                fills.append(filled)
                if on_fill is not None:
                    on_fill(filled)
        fills.extend(
            self._margin_call_if_needed(inst, float(bar.close), filled_ts, on_fill)
        )
        return fills

    def close_position(
        self,
        instrument: str,
        fill_price: float,
        now_ts: int | None = None,
    ) -> FilledOrder:
        """Flatten the active account's position (exit filled-history leg)."""
        account = self._require_active()
        inst = instrument.strip().upper()
        pos = self.positions.get((account.id, inst))
        if pos is None or abs(pos.qty) < 1e-12:
            raise ValueError(f"no open position for {inst}")
        price = _require_positive(fill_price, "fill_price")
        filled_ts = int(now_ts if now_ts is not None else time.time())
        return self._flatten_position(
            account, inst, price, filled_ts, order_type="close"
        )

    def _flatten_position(
        self,
        account: PaperAccount,
        instrument: str,
        fill_price: float,
        filled_ts: int,
        *,
        order_type: str,
    ) -> FilledOrder:
        pos = self.positions.get((account.id, instrument))
        if pos is None or abs(pos.qty) < 1e-12:
            raise ValueError(f"no open position for {instrument}")
        side = "sell" if pos.qty > 0 else "buy"
        qty = abs(pos.qty)
        filled = self._record_fill(
            account=account,
            instrument=instrument,
            side=side,
            order_type=order_type,
            qty=qty,
            limit=None,
            stop=None,
            fill_price=fill_price,
            placed_ts=filled_ts,
            filled_ts=filled_ts,
        )
        self._drop_position_children(account.id, instrument)
        self._save()
        return filled

    def _margin_call_if_needed(
        self,
        instrument: str,
        last: float,
        filled_ts: int,
        on_fill: Callable[[FilledOrder], None] | None,
    ) -> list[FilledOrder]:
        """Close leveraged positions on this instrument when maintenance fails."""
        fills: list[FilledOrder] = []
        for account in list(self.accounts.values()):
            if not account.leverage_enabled:
                continue
            pos = self.positions.get((account.id, instrument))
            if pos is None or abs(pos.qty) <= 1e-12:
                continue
            if self._maintenance_ok(account, {instrument: last}):
                continue
            filled = self._flatten_position(
                account,
                instrument,
                last,
                filled_ts,
                order_type="liquidation",
            )
            fills.append(filled)
            if on_fill is not None:
                on_fill(filled)
        return fills

    def _maintenance_ok(
        self, account: PaperAccount, last_prices: dict[str, float]
    ) -> bool:
        equity = self._account_equity(account, last_prices)
        return equity + 1e-9 >= self._account_maintenance(account)

    def _account_equity(
        self, account: PaperAccount, last_prices: dict[str, float]
    ) -> float:
        equity = account.balance
        for pos in self.positions.values():
            if pos.account_id != account.id or abs(pos.qty) <= 1e-12:
                continue
            last = last_prices.get(pos.instrument, pos.avg_price)
            equity += self._position_initial_margin(account, pos)
            equity += _unrealized_pnl(pos, last)
        return equity

    def _account_maintenance(self, account: PaperAccount) -> float:
        total = 0.0
        for pos in self.positions.values():
            if pos.account_id != account.id:
                continue
            total += (
                self._position_initial_margin(account, pos)
                * DEFAULT_MAINTENANCE_MARGIN_RATIO
            )
        return total

    def set_trade_mark_visibility(
        self,
        *,
        pair_id: str | None = None,
        fill_id: str | None = None,
        visible: bool,
    ) -> TradeMarkPair:
        """Hide or show a trade mark pair without deleting filled history."""
        account = self._require_active()
        pid = (pair_id or "").strip()
        fid = (fill_id or "").strip()
        if not pid and fid:
            fill = next((f for f in self.filled_orders if f.id == fid), None)
            if fill is None or fill.account_id != account.id:
                raise ValueError(f"unknown filled order id: {fid}")
            pid = fill.trade_mark_pair_id or ""
        if not pid:
            raise ValueError("pair_id or fill_id is required")
        pair = self.trade_mark_pairs.get(pid)
        if pair is None or pair.account_id != account.id:
            raise ValueError(f"unknown trade mark pair id: {pid}")
        pair.visible = bool(visible)
        self._save()
        return pair

    def _fill_working_order(
        self, order: WorkingOrder, fill_price: float, filled_ts: int
    ) -> FilledOrder:
        account = self.accounts[order.account_id]
        filled = self._record_fill(
            account=account,
            instrument=order.instrument,
            side=order.side,
            order_type=order.type,
            qty=order.qty,
            limit=order.limit,
            stop=order.stop,
            fill_price=fill_price,
            placed_ts=order.placed_ts,
            filled_ts=filled_ts,
        )
        if order.is_child() and order.bracket_id:
            self._drop_children(order.bracket_id)
        else:
            self.working_orders.pop(order.id, None)
        self._sync_bracket_qty(order.account_id, order.instrument)
        self._save()
        return filled

    def _record_fill(
        self,
        *,
        account: PaperAccount,
        instrument: str,
        side: str,
        order_type: str,
        qty: float,
        limit: float | None,
        stop: float | None,
        fill_price: float,
        placed_ts: int,
        filled_ts: int,
    ) -> FilledOrder:
        commission = float(account.commission_per_fill_usd)
        signed = qty if side == "buy" else -qty
        key = (account.id, instrument)
        pos_before = self.positions.get(key)
        old_im = self._position_initial_margin(account, pos_before)
        realized = 0.0
        if account.leverage_enabled:
            realized = _realized_pnl(pos_before, signed, fill_price)
        elif side == "buy":
            account.balance -= qty * fill_price + commission
        else:
            account.balance += qty * fill_price - commission
        kind, pair_id = self._assign_trade_mark(account.id, instrument, side)
        self._apply_position(account.id, instrument, signed, fill_price)
        self._sync_open_trade_pair(account.id, instrument)
        pos_after = self.positions.get(key)
        new_im = self._position_initial_margin(account, pos_after)
        margin: float | None = None
        if account.leverage_enabled:
            account.balance += (old_im - new_im) + realized - commission
            posted = abs(new_im - old_im)
            margin = posted if posted > 1e-12 else None
        duration = max(0, int(filled_ts) - int(placed_ts))
        filled = FilledOrder(
            id=_new_filled_order_id(),
            account_id=account.id,
            instrument=instrument,
            side=side,
            type=order_type,
            qty=qty,
            limit=limit,
            stop=stop,
            fill_price=fill_price,
            commission=commission,
            placed_ts=int(placed_ts),
            filled_ts=int(filled_ts),
            duration_s=duration,
            margin=margin,
            trade_mark_pair_id=pair_id,
            trade_mark_kind=kind,
        )
        self.filled_orders.append(filled)
        self.balance_history.append(
            BalanceRecord(account_id=account.id, ts=int(filled_ts), balance=account.balance)
        )
        return filled

    def _apply_position(
        self, account_id: str, instrument: str, signed_qty: float, fill_price: float
    ) -> None:
        key = (account_id, instrument)
        pos = self.positions.get(key)
        if pos is None or abs(pos.qty) < 1e-12:
            self.positions[key] = Position(
                account_id=account_id,
                instrument=instrument,
                qty=signed_qty,
                avg_price=fill_price,
            )
            return
        new_qty = pos.qty + signed_qty
        if abs(new_qty) < 1e-12:
            del self.positions[key]
            return
        same_side = pos.qty * signed_qty > 0
        if same_side:
            abs_old = abs(pos.qty)
            abs_add = abs(signed_qty)
            pos.avg_price = (abs_old * pos.avg_price + abs_add * fill_price) / (
                abs_old + abs_add
            )
            pos.qty = new_qty
            return
        if abs(signed_qty) < abs(pos.qty) - 1e-12:
            pos.qty = new_qty
            return
        # Flipped through flat: remainder opens the opposite side at fill.
        self.positions[key] = Position(
            account_id=account_id,
            instrument=instrument,
            qty=new_qty,
            avg_price=fill_price,
        )

    def _trade_marks_public(self, account_id: str) -> list[dict[str, Any]]:
        vis = {
            p.id: p.visible
            for p in self.trade_mark_pairs.values()
            if p.account_id == account_id
        }
        marks: list[dict[str, Any]] = []
        for filled in self.filled_orders:
            if filled.account_id != account_id or not filled.trade_mark_pair_id:
                continue
            marks.append(
                {
                    "pair_id": filled.trade_mark_pair_id,
                    "fill_id": filled.id,
                    "instrument": filled.instrument,
                    "kind": filled.trade_mark_kind,
                    "price": filled.fill_price,
                    "filled_ts": filled.filled_ts,
                    "side": filled.side,
                    "visible": vis.get(filled.trade_mark_pair_id, True),
                }
            )
        return marks

    def _new_trade_mark_pair(self, account_id: str, instrument: str) -> TradeMarkPair:
        pair = TradeMarkPair(
            id=_new_trade_mark_pair_id(),
            account_id=account_id,
            instrument=instrument,
            visible=True,
        )
        self.trade_mark_pairs[pair.id] = pair
        return pair

    def _assign_trade_mark(
        self, account_id: str, instrument: str, side: str
    ) -> tuple[str, str]:
        key = (account_id, instrument)
        pos = self.positions.get(key)
        signed = 1.0 if side == "buy" else -1.0
        is_exit = (
            pos is not None and abs(pos.qty) > 1e-12 and pos.qty * signed < 0
        )
        if is_exit:
            pair_id = self._open_trade_pairs.get(key)
            if pair_id is None:
                pair_id = self._new_trade_mark_pair(account_id, instrument).id
                self._open_trade_pairs[key] = pair_id
            return TRADE_MARK_EXIT, pair_id
        if pos is None or abs(pos.qty) < 1e-12:
            pair_id = self._new_trade_mark_pair(account_id, instrument).id
            self._open_trade_pairs[key] = pair_id
            return TRADE_MARK_ENTRY, pair_id
        pair_id = self._open_trade_pairs.get(key)
        if pair_id is None:
            pair_id = self._new_trade_mark_pair(account_id, instrument).id
            self._open_trade_pairs[key] = pair_id
        return TRADE_MARK_ENTRY, pair_id

    def _sync_open_trade_pair(self, account_id: str, instrument: str) -> None:
        key = (account_id, instrument)
        pos = self.positions.get(key)
        if pos is None or abs(pos.qty) < 1e-12:
            self._open_trade_pairs.pop(key, None)

    def _load_trade_mark_pairs(self) -> None:
        if self.path is None:
            return
        with self._connect() as conn:
            conn.executescript(_SCHEMA)
            _migrate_trade_marks(conn)
            rows = conn.execute(
                "SELECT * FROM paper_trade_mark_pairs ORDER BY id"
            ).fetchall()
        for row in rows:
            account_id = str(row["account_id"])
            if account_id not in self.accounts:
                continue
            pair = TradeMarkPair(
                id=str(row["id"]),
                account_id=account_id,
                instrument=str(row["instrument"]),
                visible=bool(row["visible"]),
            )
            self.trade_mark_pairs[pair.id] = pair

    def _reconstruct_trade_marks(self) -> None:
        """Pair fills that predate the trade-mark columns (FIFO per instrument)."""
        running: dict[tuple[str, str], float] = {}
        open_pair: dict[tuple[str, str], str] = {}
        dirty = False
        for fill in self.filled_orders:
            key = (fill.account_id, fill.instrument)
            signed = fill.qty if fill.side == "buy" else -fill.qty
            prev = running.get(key, 0.0)
            is_exit = abs(prev) > 1e-12 and prev * signed < 0
            kind = TRADE_MARK_EXIT if is_exit else TRADE_MARK_ENTRY
            if fill.trade_mark_pair_id:
                pair_id = fill.trade_mark_pair_id
                if pair_id not in self.trade_mark_pairs:
                    self.trade_mark_pairs[pair_id] = TradeMarkPair(
                        id=pair_id,
                        account_id=fill.account_id,
                        instrument=fill.instrument,
                        visible=True,
                    )
                    dirty = True
                if not fill.trade_mark_kind:
                    fill.trade_mark_kind = kind
                    dirty = True
            else:
                if is_exit:
                    pair_id = open_pair.get(key)
                    if pair_id is None:
                        pair_id = self._new_trade_mark_pair(
                            fill.account_id, fill.instrument
                        ).id
                elif abs(prev) < 1e-12 or key not in open_pair:
                    pair_id = self._new_trade_mark_pair(
                        fill.account_id, fill.instrument
                    ).id
                else:
                    pair_id = open_pair[key]
                fill.trade_mark_pair_id = pair_id
                fill.trade_mark_kind = kind
                dirty = True
            new_qty = prev + signed
            running[key] = 0.0 if abs(new_qty) < 1e-12 else new_qty
            if abs(running[key]) < 1e-12:
                open_pair.pop(key, None)
            else:
                open_pair[key] = fill.trade_mark_pair_id or ""
        self._open_trade_pairs = {
            k: pid
            for k, pid in open_pair.items()
            if k in self.positions and abs(self.positions[k].qty) > 1e-12
        }
        if dirty:
            self._save()

    def _require_active(self) -> PaperAccount:
        account = self.accounts.get(self.active_account_id)
        if account is None:
            raise ValueError("no active paper account")
        return account

    def _optional_bracket_prices(
        self, entry_side: str, take_profit: float | None, stop_loss: float | None
    ) -> tuple[float | None, float | None]:
        tp_given = take_profit is not None
        sl_given = stop_loss is not None
        if not tp_given and not sl_given:
            return None, None
        if not tp_given or not sl_given:
            raise ValueError("bracket requires take_profit and stop_loss")
        return self._require_bracket_prices(entry_side, take_profit, stop_loss)

    def _require_bracket_prices(
        self, entry_side: str, take_profit: float | None, stop_loss: float | None
    ) -> tuple[float, float]:
        tp_val = _require_positive(take_profit, "take_profit")
        sl_val = _require_positive(stop_loss, "stop_loss")
        if entry_side == "buy" and tp_val <= sl_val:
            raise ValueError("buy bracket requires take_profit > stop_loss")
        if entry_side == "sell" and tp_val >= sl_val:
            raise ValueError("sell bracket requires take_profit < stop_loss")
        return tp_val, sl_val

    def _place_bracket_children(
        self,
        *,
        account: PaperAccount,
        instrument: str,
        exit_side: str,
        qty: float,
        take_profit: float,
        stop_loss: float,
        bracket_id: str,
    ) -> None:
        placed_ts = int(time.time())
        tp = WorkingOrder(
            id=_new_working_order_id(),
            account_id=account.id,
            instrument=instrument,
            side=exit_side,
            type="limit",
            qty=qty,
            limit=take_profit,
            stop=None,
            placed_ts=placed_ts,
            ref_price=take_profit,
            bracket_id=bracket_id,
            role=ROLE_TP,
        )
        sl = WorkingOrder(
            id=_new_working_order_id(),
            account_id=account.id,
            instrument=instrument,
            side=exit_side,
            type="stop",
            qty=qty,
            limit=None,
            stop=stop_loss,
            placed_ts=placed_ts,
            ref_price=stop_loss,
            bracket_id=bracket_id,
            role=ROLE_SL,
        )
        self.working_orders[tp.id] = tp
        self.working_orders[sl.id] = sl

    def _bracket_children(self, bracket_id: str) -> list[WorkingOrder]:
        return [
            o
            for o in self.working_orders.values()
            if o.bracket_id == bracket_id and o.is_child()
        ]

    def _drop_bracket(self, bracket_id: str) -> None:
        to_del = [
            oid
            for oid, order in self.working_orders.items()
            if order.bracket_id == bracket_id
        ]
        for oid in to_del:
            del self.working_orders[oid]

    def _drop_children(self, bracket_id: str) -> None:
        to_del = [
            oid
            for oid, order in self.working_orders.items()
            if order.bracket_id == bracket_id and order.is_child()
        ]
        for oid in to_del:
            del self.working_orders[oid]

    def _drop_position_children(self, account_id: str, instrument: str) -> None:
        to_del = [
            oid
            for oid, order in self.working_orders.items()
            if order.account_id == account_id
            and order.instrument == instrument
            and order.is_child()
        ]
        for oid in to_del:
            del self.working_orders[oid]

    def _bracket_prices(
        self, account_id: str, instrument: str
    ) -> tuple[float | None, float | None]:
        tp: float | None = None
        sl: float | None = None
        for order in self.working_orders.values():
            if order.account_id != account_id or order.instrument != instrument:
                continue
            if order.role == ROLE_TP:
                tp = order.limit
            elif order.role == ROLE_SL:
                sl = order.stop
        return tp, sl

    def _sync_bracket_qty(self, account_id: str, instrument: str) -> None:
        """v1 children always rest the full open position qty (no partials)."""
        pos = self.positions.get((account_id, instrument))
        if pos is None or abs(pos.qty) < 1e-12:
            self._drop_position_children(account_id, instrument)
            return
        qty = abs(pos.qty)
        for order in self.working_orders.values():
            if (
                order.account_id == account_id
                and order.instrument == instrument
                and order.is_child()
            ):
                order.qty = qty

    def _order_is_live(self, order: WorkingOrder) -> bool:
        if not order.is_child():
            return True
        if order.bracket_id:
            for sibling in self.working_orders.values():
                if sibling.bracket_id == order.bracket_id and sibling.role == ROLE_ENTRY:
                    return False
        pos = self.positions.get((order.account_id, order.instrument))
        return pos is not None and abs(pos.qty) > 1e-12

    def _position_initial_margin(
        self, account: PaperAccount, pos: Position | None
    ) -> float:
        if pos is None or abs(pos.qty) <= 1e-12:
            return 0.0
        if not account.leverage_enabled:
            return 0.0
        lev = max(float(account.leverage_multiple), 1.0)
        return abs(pos.qty) * pos.avg_price / lev

    def _opening_qty(
        self, account_id: str, instrument: str | None, side: str, qty: float
    ) -> float:
        """Qty that would open or add, after netting an opposite open position."""
        if not instrument:
            return qty
        pos = self.positions.get((account_id, instrument))
        if pos is None or abs(pos.qty) <= 1e-12:
            return qty
        pos_side = "buy" if pos.qty > 0 else "sell"
        if side == pos_side:
            return qty
        return max(0.0, qty - abs(pos.qty))

    def _required_cash(
        self,
        account: PaperAccount,
        side: str,
        qty: float,
        price: float,
        instrument: str | None = None,
    ) -> float:
        commission = account.commission_per_fill_usd
        opening = self._opening_qty(account.id, instrument, side, qty)
        if not account.leverage_enabled:
            # Sells do not spend cash at place (no position book yet). Reserve
            # commission only so a later fill can still debit the rule.
            if side == "sell" or opening <= 1e-12:
                return commission
            return opening * price + commission
        # off / 1× / leveraged: when enabled, reserve initial margin on entry.
        if opening <= 1e-12 or price <= 0 or not _finite(price):
            return commission
        lev = max(float(account.leverage_multiple), 1.0)
        return opening * price / lev + commission

    def _reserved_cash(self, account_id: str, except_id: str | None = None) -> float:
        account = self.accounts[account_id]
        total = 0.0
        for order in self.working_orders.values():
            if order.account_id != account_id:
                continue
            if except_id is not None and order.id == except_id:
                continue
            if order.is_child():
                continue
            total += self._required_cash(
                account,
                order.side,
                order.qty,
                order.ref_price,
                instrument=order.instrument,
            )
        return total

    def _assert_qty_supported(
        self,
        account: PaperAccount,
        side: str,
        qty: float,
        price: float,
        except_id: str | None = None,
        instrument: str | None = None,
    ) -> None:
        required = self._required_cash(
            account, side, qty, price, instrument=instrument
        )
        available = account.balance - self._reserved_cash(account.id, except_id)
        if required > available + 1e-9:
            raise ValueError("qty exceeds buying power")

    def _assert_asset_class_allowed(
        self, account: PaperAccount, instrument: str
    ) -> None:
        restriction = account.asset_class_restriction
        if not restriction:
            return
        actual = instrument_asset_class(instrument)
        if actual != restriction:
            raise ValueError(
                "instrument is outside the paper account asset-class allow-list"
            )

    def _seed_default(self) -> None:
        self.create_account()

    def _connect(self) -> sqlite3.Connection:
        assert self.path is not None
        self.path.parent.mkdir(parents=True, exist_ok=True)
        conn = sqlite3.connect(self.path)
        conn.row_factory = sqlite3.Row
        return conn

    def _load_or_seed(self) -> None:
        if self.path is None:
            self._seed_default()
            return
        rows: list[sqlite3.Row] = []
        meta_value: str | None = None
        with self._connect() as conn:
            conn.executescript(_SCHEMA)
            _migrate_working_orders(conn)
            _migrate_trade_marks(conn)
            rows = conn.execute(
                "SELECT * FROM paper_accounts ORDER BY created_ts, id"
            ).fetchall()
            if rows:
                meta = conn.execute(
                    "SELECT value FROM paper_meta WHERE key = ?",
                    ("active_account_id",),
                ).fetchone()
                meta_value = str(meta["value"]) if meta is not None else None
        if not rows:
            self._seed_default()
            return
        for row in rows:
            account = PaperAccount(
                id=str(row["id"]),
                name=str(row["name"]),
                currency=str(row["currency"]),
                initial_balance=float(row["initial_balance"]),
                balance=float(row["balance"]),
                commission_per_fill_usd=float(row["commission_per_fill_usd"]),
                leverage_enabled=bool(row["leverage_enabled"]),
                leverage_multiple=float(row["leverage_multiple"]),
                asset_class_restriction=row["asset_class_restriction"],
                created_ts=int(row["created_ts"]),
            )
            self.accounts[account.id] = account
        active = meta_value or ""
        if active not in self.accounts:
            active = next(iter(self.accounts))
        self.active_account_id = active
        self._load_working_orders()
        self._load_positions()
        self._load_filled_orders()
        self._load_balance_history()
        self._load_trade_mark_pairs()
        self._reconstruct_trade_marks()

    def _load_working_orders(self) -> None:
        if self.path is None:
            return
        with self._connect() as conn:
            conn.executescript(_SCHEMA)
            _migrate_working_orders(conn)
            rows = conn.execute(
                "SELECT * FROM paper_working_orders ORDER BY placed_ts, id"
            ).fetchall()
        for row in rows:
            account_id = str(row["account_id"])
            if account_id not in self.accounts:
                continue
            limit_raw = row["limit_price"]
            stop_raw = row["stop_price"]
            keys = row.keys()
            bracket_raw = row["bracket_id"] if "bracket_id" in keys else None
            role_raw = row["role"] if "role" in keys else ROLE_ENTRY
            role = str(role_raw or ROLE_ENTRY).strip().lower() or ROLE_ENTRY
            order = WorkingOrder(
                id=str(row["id"]),
                account_id=account_id,
                instrument=str(row["instrument"]),
                side=str(row["side"]),
                type=str(row["order_type"]),
                qty=float(row["qty"]),
                limit=None if limit_raw is None else float(limit_raw),
                stop=None if stop_raw is None else float(stop_raw),
                placed_ts=int(row["placed_ts"]),
                ref_price=float(row["ref_price"]),
                bracket_id=None if not bracket_raw else str(bracket_raw),
                role=role,
            )
            self.working_orders[order.id] = order

    def _load_positions(self) -> None:
        if self.path is None:
            return
        with self._connect() as conn:
            conn.executescript(_SCHEMA)
            rows = conn.execute(
                "SELECT * FROM paper_positions ORDER BY account_id, instrument"
            ).fetchall()
        for row in rows:
            account_id = str(row["account_id"])
            if account_id not in self.accounts:
                continue
            qty = float(row["qty"])
            if abs(qty) < 1e-12:
                continue
            pos = Position(
                account_id=account_id,
                instrument=str(row["instrument"]),
                qty=qty,
                avg_price=float(row["avg_price"]),
            )
            self.positions[(pos.account_id, pos.instrument)] = pos

    def _load_filled_orders(self) -> None:
        if self.path is None:
            return
        with self._connect() as conn:
            conn.executescript(_SCHEMA)
            _migrate_trade_marks(conn)
            rows = conn.execute(
                "SELECT * FROM paper_filled_orders ORDER BY filled_ts, id"
            ).fetchall()
        for row in rows:
            account_id = str(row["account_id"])
            if account_id not in self.accounts:
                continue
            limit_raw = row["limit_price"]
            stop_raw = row["stop_price"]
            margin_raw = row["margin"]
            keys = row.keys()
            pair_raw = (
                row["trade_mark_pair_id"] if "trade_mark_pair_id" in keys else None
            )
            kind_raw = row["trade_mark_kind"] if "trade_mark_kind" in keys else None
            kind = str(kind_raw or TRADE_MARK_ENTRY).strip().lower() or TRADE_MARK_ENTRY
            filled = FilledOrder(
                id=str(row["id"]),
                account_id=account_id,
                instrument=str(row["instrument"]),
                side=str(row["side"]),
                type=str(row["order_type"]),
                qty=float(row["qty"]),
                limit=None if limit_raw is None else float(limit_raw),
                stop=None if stop_raw is None else float(stop_raw),
                fill_price=float(row["fill_price"]),
                commission=float(row["commission"]),
                placed_ts=int(row["placed_ts"]),
                filled_ts=int(row["filled_ts"]),
                duration_s=int(row["duration_s"]),
                margin=None if margin_raw is None else float(margin_raw),
                trade_mark_pair_id=None if not pair_raw else str(pair_raw),
                trade_mark_kind=kind,
            )
            self.filled_orders.append(filled)

    def _load_balance_history(self) -> None:
        if self.path is None:
            return
        with self._connect() as conn:
            conn.executescript(_SCHEMA)
            rows = conn.execute(
                "SELECT * FROM paper_balance_history ORDER BY ts, id"
            ).fetchall()
        for row in rows:
            account_id = str(row["account_id"])
            if account_id not in self.accounts:
                continue
            self.balance_history.append(
                BalanceRecord(
                    account_id=account_id,
                    ts=int(row["ts"]),
                    balance=float(row["balance"]),
                )
            )

    def _save(self) -> None:
        if self.path is None:
            return
        with self._connect() as conn:
            conn.executescript(_SCHEMA)
            _migrate_working_orders(conn)
            _migrate_trade_marks(conn)
            conn.execute("DELETE FROM paper_accounts")
            conn.execute("DELETE FROM paper_meta")
            for account in self.accounts.values():
                conn.execute(
                    """
                    INSERT INTO paper_accounts (
                        id, name, currency, initial_balance, balance,
                        commission_per_fill_usd, leverage_enabled, leverage_multiple,
                        asset_class_restriction, created_ts
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        account.id,
                        account.name,
                        account.currency,
                        account.initial_balance,
                        account.balance,
                        account.commission_per_fill_usd,
                        1 if account.leverage_enabled else 0,
                        account.leverage_multiple,
                        account.asset_class_restriction,
                        account.created_ts,
                    ),
                )
            conn.execute(
                "INSERT INTO paper_meta (key, value) VALUES (?, ?)",
                ("active_account_id", self.active_account_id),
            )
            conn.execute("DELETE FROM paper_working_orders")
            for order in self.working_orders.values():
                conn.execute(
                    """
                    INSERT INTO paper_working_orders (
                        id, account_id, instrument, side, order_type, qty,
                        limit_price, stop_price, ref_price, placed_ts,
                        bracket_id, role
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        order.id,
                        order.account_id,
                        order.instrument,
                        order.side,
                        order.type,
                        order.qty,
                        order.limit,
                        order.stop,
                        order.ref_price,
                        order.placed_ts,
                        order.bracket_id,
                        order.role,
                    ),
                )
            conn.execute("DELETE FROM paper_positions")
            for pos in self.positions.values():
                if abs(pos.qty) < 1e-12:
                    continue
                conn.execute(
                    """
                    INSERT INTO paper_positions (
                        account_id, instrument, qty, avg_price
                    ) VALUES (?, ?, ?, ?)
                    """,
                    (pos.account_id, pos.instrument, pos.qty, pos.avg_price),
                )
            conn.execute("DELETE FROM paper_filled_orders")
            for filled in self.filled_orders:
                conn.execute(
                    """
                    INSERT INTO paper_filled_orders (
                        id, account_id, instrument, side, order_type, qty,
                        limit_price, stop_price, fill_price, commission,
                        placed_ts, filled_ts, duration_s, margin,
                        trade_mark_pair_id, trade_mark_kind
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (
                        filled.id,
                        filled.account_id,
                        filled.instrument,
                        filled.side,
                        filled.type,
                        filled.qty,
                        filled.limit,
                        filled.stop,
                        filled.fill_price,
                        filled.commission,
                        filled.placed_ts,
                        filled.filled_ts,
                        filled.duration_s,
                        filled.margin,
                        filled.trade_mark_pair_id,
                        filled.trade_mark_kind,
                    ),
                )
            conn.execute("DELETE FROM paper_trade_mark_pairs")
            for pair in self.trade_mark_pairs.values():
                conn.execute(
                    """
                    INSERT INTO paper_trade_mark_pairs (
                        id, account_id, instrument, visible
                    ) VALUES (?, ?, ?, ?)
                    """,
                    (
                        pair.id,
                        pair.account_id,
                        pair.instrument,
                        1 if pair.visible else 0,
                    ),
                )
            conn.execute("DELETE FROM paper_balance_history")
            for rec in self.balance_history:
                conn.execute(
                    """
                    INSERT INTO paper_balance_history (
                        account_id, ts, balance
                    ) VALUES (?, ?, ?)
                    """,
                    (rec.account_id, rec.ts, rec.balance),
                )
            conn.commit()


def instrument_asset_class(instrument: str) -> str:
    """Map a domain instrument to equities or futures."""
    root = instrument.strip().upper().split(".", 1)[0]
    if root in _FUTURES_ROOTS:
        return ASSET_FUTURES
    return ASSET_EQUITIES


def _parse_asset_class_restriction(value: str | None) -> str | None:
    if value is None:
        return None
    cleaned = str(value).strip().lower()
    if not cleaned:
        return None
    if cleaned not in VALID_ASSET_CLASSES:
        raise ValueError("asset_class_restriction must be equities or futures")
    return cleaned


def _unrealized_pnl(pos: Position, last: float) -> float:
    abs_qty = abs(pos.qty)
    if pos.qty >= 0:
        return (last - pos.avg_price) * abs_qty
    return (pos.avg_price - last) * abs_qty


def _realized_pnl(
    pos: Position | None, signed_qty: float, fill_price: float
) -> float:
    """PnL on the closing portion of a fill (0 when opening or adding)."""
    if pos is None or abs(pos.qty) <= 1e-12:
        return 0.0
    if pos.qty * signed_qty > 0:
        return 0.0
    closed = min(abs(signed_qty), abs(pos.qty))
    if pos.qty > 0:
        return (fill_price - pos.avg_price) * closed
    return (pos.avg_price - fill_price) * closed


def _exit_side(entry_side: str) -> str:
    return "sell" if entry_side == "buy" else "buy"


def _eval_rank(order: WorkingOrder) -> tuple[int, int, str]:
    if order.role == ROLE_SL:
        return (1, order.placed_ts, order.id)
    if order.role == ROLE_TP:
        return (2, order.placed_ts, order.id)
    return (0, order.placed_ts, order.id)


def _migrate_working_orders(conn: sqlite3.Connection) -> None:
    cols = {str(row[1]) for row in conn.execute("PRAGMA table_info(paper_working_orders)")}
    if "bracket_id" not in cols:
        conn.execute("ALTER TABLE paper_working_orders ADD COLUMN bracket_id TEXT")
    if "role" not in cols:
        conn.execute(
            "ALTER TABLE paper_working_orders ADD COLUMN role TEXT NOT NULL DEFAULT 'entry'"
        )


def _migrate_trade_marks(conn: sqlite3.Connection) -> None:
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS paper_trade_mark_pairs (
            id TEXT PRIMARY KEY,
            account_id TEXT NOT NULL,
            instrument TEXT NOT NULL,
            visible INTEGER NOT NULL
        )
        """
    )
    cols = {str(row[1]) for row in conn.execute("PRAGMA table_info(paper_filled_orders)")}
    if "trade_mark_pair_id" not in cols:
        conn.execute("ALTER TABLE paper_filled_orders ADD COLUMN trade_mark_pair_id TEXT")
    if "trade_mark_kind" not in cols:
        conn.execute("ALTER TABLE paper_filled_orders ADD COLUMN trade_mark_kind TEXT")


def bar_touch_fill_price(order: WorkingOrder, bar: Bar) -> float | None:
    """Return fill price when ``bar`` trades through ``order``, else None."""
    if order.type == "market":
        return float(bar.close)
    if order.type == "limit":
        if order.limit is None:
            return None
        if order.side == "buy" and bar.low <= order.limit:
            return float(order.limit)
        if order.side == "sell" and bar.high >= order.limit:
            return float(order.limit)
        return None
    if order.type == "stop":
        if order.stop is None:
            return None
        if order.side == "buy" and bar.high >= order.stop:
            return float(order.stop)
        if order.side == "sell" and bar.low <= order.stop:
            return float(order.stop)
        return None
    return None


def _finite(value: float) -> bool:
    return value == value and value not in (float("inf"), float("-inf"))


def _require_positive(value: float, field: str) -> float:
    number = float(value)
    if not _finite(number) or number <= 0:
        raise ValueError(f"{field} must be > 0")
    return number


def _optional_positive(value: float | None, field: str) -> float | None:
    if value is None:
        return None
    return _require_positive(value, field)
