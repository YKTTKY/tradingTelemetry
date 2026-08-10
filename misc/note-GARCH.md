# Note: GARCH indicator (Phase A)

Reference for how optional **GARCH** is implemented in the market engine.  
Source of truth: `engine/src/market_engine/indicators.py` → `compute_garch`.

## Product intent

- **Optional** when history allows a stable estimate; otherwise **unavailable**.
- Never invent volatility series when inputs are insufficient or compute fails.
- Not a hard dependency for charts or other indicators.
- Domain note: GARCH is a volatility model on returns — **not** a simple MA-style price overlay.

## Availability gates

| Condition | Series status | Reason |
|-----------|---------------|--------|
| Fewer than **50** closes (`MIN_GARCH_BARS`) | `unavailable` | `insufficient_history` |
| Non-positive closes (log return undefined) | `unavailable` | `compute_failed` |
| Sample variance ≤ 0 or non-finite | `unavailable` | `unstable_estimate` |
| Conditional variance \(h_t\) < 0 or non-finite mid-path | `unavailable` | `compute_failed` |
| Otherwise | `ok` | — |

Default fake SPY @ 1D has only **10** bars → GARCH is unavailable until longer history is loaded/seeded.

## Model: GARCH(1,1) with variance targeting

**No MLE.** \(\alpha\) and \(\beta\) are fixed; \(\omega\) is set so the long-run variance matches the sample second moment of returns.

### Constants

```text
MIN_GARCH_BARS = 50
GARCH_ALPHA    = 0.1    # α
GARCH_BETA     = 0.85   # β
# α + β = 0.95 < 1  → covariance-stationary
```

### Step 1 — Log returns

From bar closes \(C_0, C_1, \ldots, C_{n-1}\):

\[
r_t = \ln\left(\frac{C_t}{C_{t-1}}\right),\quad t = 1,\ldots,n-1
\]

### Step 2 — Unconditional (sample) variance

\[
\bar{\sigma}^2 = \frac{1}{T}\sum_{t=1}^{T} r_t^2
\]

where \(T = n - 1\) (number of returns). Stored as `params.unconditional_var`.

### Step 3 — Variance targeting for \(\omega\)

\[
\omega = \bar{\sigma}^2 \,(1 - \alpha - \beta)
\]

Long-run mean of conditional variance under the recursion is \(\bar{\sigma}^2\).

### Step 4 — GARCH(1,1) recursion

Classic form:

\[
h_t = \omega + \alpha\, r_{t-1}^{2} + \beta\, h_{t-1}
\]

Initialization in code:

- Seed \(h \leftarrow \bar{\sigma}^2\)
- For each return \(r\) in order:  
  \(h \leftarrow \omega + \alpha\, r^{2} + \beta\, h\)  
  store \(\sigma = \sqrt{h}\)

### Step 5 — Output series

- Series length matches **number of bars** (aligned to bar index).
- Index `0`: `null` (no prior return).
- Indices `1..n-1`: **conditional volatility** \(\sigma_t = \sqrt{h_t}\) (return units, **not** annualized).

## IPC payload shape

**Unavailable**

```json
{
  "type": "garch",
  "status": "unavailable",
  "reason": "insufficient_history"
}
```

**OK**

```json
{
  "type": "garch",
  "status": "ok",
  "values": [null, 0.0123, 0.0118, "..."],
  "params": {
    "omega": 0.00000123,
    "alpha": 0.1,
    "beta": 0.85,
    "unconditional_var": 0.0000246
  }
}
```

Configs still apply and restore when unavailable; only the **series** is gated.

## What this is not

| Not implemented | What we do instead |
|-----------------|--------------------|
| MLE / QMLE of \(\omega,\alpha,\beta\) | Fixed \(\alpha,\beta\); \(\omega\) via variance targeting |
| EGARCH / GJR / Student-t innovations | Plain GARCH(1,1), Gaussian-style recursion only |
| Annualized or % vol | Per-bar \(\sqrt{h_t}\) in log-return units |
| Formal stationarity / Ljung-Box diagnostics | Length gate + finite positive \(h\) checks |
| Price-pane MA-like overlay | Optional series; TUI surfaces ok tip / panel status only |

## TUI

- Indicator panel: **`g`** adds GARCH (max 1 per chart).
- Unavailable: yellow `UNAVAILABLE (insufficient history | …)` — no invented line.
- Success: panel `ok`; chart subtitle may show tip `GARCH=<σ>` when status is ok.

## Tests

Contract coverage: `engine/tests/test_ipc_gex_garch.py`

- Default short SPY history → unavailable path.
- FakeVendor long seeded closes (≥50) → independent oracle matches engine tip vol.

Independent oracle in tests recomputes the same formulas with the same \(\alpha,\beta\) and min-bar rule.

## Related

- Issue: `.scratch/phase-a-chart-terminal/issues/12-optional-gex-garch.md`
- Spec stories 46–47: optional GEX / GARCH; no fake certainty
- Glossary: `CONTEXT.md` → **GARCH**
