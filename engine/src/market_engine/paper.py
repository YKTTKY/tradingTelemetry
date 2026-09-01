"""SQLite paper book: paper accounts and the active id (hot copy in memory)."""

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
"""


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


class PaperBook:
    """In-memory paper desk with optional SQLite durability."""

    def __init__(self, path: Path | str | None = None) -> None:
        self.path = Path(path) if path is not None else None
        self.accounts: dict[str, PaperAccount] = {}
        self.active_account_id: str = ""
        self._load_or_seed()

    def to_public(self) -> dict[str, Any]:
        accounts = [a.to_public() for a in self.accounts.values()]
        active = self.accounts.get(self.active_account_id)
        return {
            "active_account_id": self.active_account_id,
            "accounts": accounts,
            "active": active.to_public() if active is not None else None,
            "defaults": dict(PAPER_DEFAULTS),
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
            conn.commit()
