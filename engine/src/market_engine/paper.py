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

PAPER_DEFAULTS: dict[str, Any] = {
    "name": DEFAULT_ACCOUNT_NAME,
    "currency": DEFAULT_CURRENCY,
    "initial_balance": DEFAULT_INITIAL_BALANCE,
    "commission_per_fill_usd": DEFAULT_COMMISSION_PER_FILL_USD,
    "leverage_enabled": DEFAULT_LEVERAGE_ENABLED,
    "leverage_multiple": DEFAULT_LEVERAGE_MULTIPLE,
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
    placed_ts INTEGER NOT NULL
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
    margin REAL
);
CREATE TABLE IF NOT EXISTS paper_balance_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id TEXT NOT NULL,
    ts INTEGER NOT NULL,
    balance REAL NOT NULL
);
"""

VALID_SIDES = frozenset({"buy", "sell"})
VALID_ORDER_TYPES = frozenset({"market", "limit", "stop"})


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


def _new_filled_order_id() -> str:
    return "fo_" + uuid.uuid4().hex[:12]


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
        }


@dataclass
class Position:
    """Open holding for one instrument on one paper account (signed qty)."""

    account_id: str
    instrument: str
    qty: float
    avg_price: float

    def to_public(self, last: float | None = None) -> dict[str, Any]:
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
        }


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
        positions = [
            p.to_public(last=last_prices.get(p.instrument))
            for p in self.positions.values()
            if p.account_id == aid and abs(p.qty) > 1e-12
        ]
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
        restriction = None
        if asset_class_restriction is not None:
            cleaned = str(asset_class_restriction).strip()
            restriction = cleaned or None
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
    ) -> WorkingOrder:
        account = self._require_active()
        cleaned_instrument = instrument.strip().upper()
        if not cleaned_instrument:
            raise ValueError("instrument is required")
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
        self._assert_qty_supported(account, cleaned_side, qty_val, ref_price)
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
        )
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
        self._assert_qty_supported(
            account, order.side, new_qty, ref_price, except_id=order.id
        )
        order.qty = new_qty
        order.limit = new_limit
        order.stop = new_stop
        order.ref_price = ref_price
        self._save()
        return order

    def cancel_order(self, order_id: str) -> WorkingOrder:
        account = self._require_active()
        oid = order_id.strip()
        order = self.working_orders.get(oid)
        if order is None or order.account_id != account.id:
            raise ValueError(f"unknown working order id: {oid}")
        del self.working_orders[oid]
        self._save()
        return order

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
        pending = [
            o
            for o in self.working_orders.values()
            if o.instrument == inst and o.account_id in self.accounts
        ]
        pending.sort(key=lambda o: (o.placed_ts, o.id))
        fills: list[FilledOrder] = []
        for order in pending:
            if order.id not in self.working_orders:
                continue
            price = bar_touch_fill_price(order, bar)
            if price is None:
                continue
            filled = self._fill_working_order(order, price, filled_ts)
            fills.append(filled)
            if on_fill is not None:
                on_fill(filled)
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
        side = "sell" if pos.qty > 0 else "buy"
        qty = abs(pos.qty)
        filled = self._record_fill(
            account=account,
            instrument=inst,
            side=side,
            order_type="close",
            qty=qty,
            limit=None,
            stop=None,
            fill_price=price,
            placed_ts=filled_ts,
            filled_ts=filled_ts,
        )
        self._save()
        return filled

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
        self.working_orders.pop(order.id, None)
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
        if side == "buy":
            account.balance -= qty * fill_price + commission
        else:
            account.balance += qty * fill_price - commission
        signed = qty if side == "buy" else -qty
        self._apply_position(account.id, instrument, signed, fill_price)
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
            margin=None,
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

    def _require_active(self) -> PaperAccount:
        account = self.accounts.get(self.active_account_id)
        if account is None:
            raise ValueError("no active paper account")
        return account

    def _required_cash(
        self, account: PaperAccount, side: str, qty: float, price: float
    ) -> float:
        commission = account.commission_per_fill_usd
        # Sells do not spend cash at place (no position book yet). Reserve
        # commission only so a later fill can still debit the rule.
        if side == "sell":
            return commission
        notional = qty * price
        if account.leverage_enabled and account.leverage_multiple > 1.0:
            return notional / account.leverage_multiple + commission
        return notional + commission

    def _reserved_cash(self, account_id: str, except_id: str | None = None) -> float:
        account = self.accounts[account_id]
        total = 0.0
        for order in self.working_orders.values():
            if order.account_id != account_id:
                continue
            if except_id is not None and order.id == except_id:
                continue
            total += self._required_cash(
                account, order.side, order.qty, order.ref_price
            )
        return total

    def _assert_qty_supported(
        self,
        account: PaperAccount,
        side: str,
        qty: float,
        price: float,
        except_id: str | None = None,
    ) -> None:
        required = self._required_cash(account, side, qty, price)
        available = account.balance - self._reserved_cash(account.id, except_id)
        if required > available + 1e-9:
            raise ValueError("qty exceeds buying power")

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

    def _load_working_orders(self) -> None:
        if self.path is None:
            return
        with self._connect() as conn:
            conn.executescript(_SCHEMA)
            rows = conn.execute(
                "SELECT * FROM paper_working_orders ORDER BY placed_ts, id"
            ).fetchall()
        for row in rows:
            account_id = str(row["account_id"])
            if account_id not in self.accounts:
                continue
            limit_raw = row["limit_price"]
            stop_raw = row["stop_price"]
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
                        limit_price, stop_price, ref_price, placed_ts
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                        placed_ts, filled_ts, duration_s, margin
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
