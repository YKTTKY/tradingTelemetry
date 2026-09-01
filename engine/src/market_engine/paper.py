"""SQLite paper book: paper accounts, working orders, and the active id."""

from __future__ import annotations

import sqlite3
import time
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any

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


class PaperBook:
    """In-memory paper desk with optional SQLite durability."""

    def __init__(self, path: Path | str | None = None) -> None:
        self.path = Path(path) if path is not None else None
        self.accounts: dict[str, PaperAccount] = {}
        self.active_account_id: str = ""
        self.working_orders: dict[str, WorkingOrder] = {}
        self._load_or_seed()

    def to_public(self) -> dict[str, Any]:
        accounts = [a.to_public() for a in self.accounts.values()]
        active = self.accounts.get(self.active_account_id)
        working = [
            o.to_public()
            for o in self.working_orders.values()
            if o.account_id == self.active_account_id
        ]
        return {
            "active_account_id": self.active_account_id,
            "accounts": accounts,
            "active": active.to_public() if active is not None else None,
            "defaults": dict(PAPER_DEFAULTS),
            "working_orders": working,
            "positions": [],
            "filled_order_history": [],
            "balance_history": [],
        }

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
            conn.commit()


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
