# 12 — Optional GEX / GARCH (graceful unavailable)

**What to build:** The trader may attach **GEX** and/or **GARCH** when inputs and compute succeed. If options data is missing, history is insufficient, or compute fails, the indicator is **unavailable** — charts and other indicators keep working, and **no fake values** are shown.

**Blocked by:** 07 — Indicator panel + MA + Volume (naked → restore)

**Status:** done

- [x] GEX is offered only when options data + computation succeed; otherwise unavailable without breaking the chart
- [x] GARCH is offered only when history allows a stable estimate; otherwise unavailable
- [x] Never display invented GEX/GARCH series
- [x] Indicator panel surfaces unavailable state clearly when the user tries to enable them without inputs
- [x] Success path (when fake or real inputs allow) delivers payload/render consistent with other indicators
- [x] Contract tests cover at least the unavailable path; success path tested when a deterministic fixture exists

## Notes

- Parent spec: `.scratch/phase-a-chart-terminal/spec.md`
- Optional for Phase A ship — must not block chart terminal MVP if data is hard to fixture
- Recommended last in the execution sequence
