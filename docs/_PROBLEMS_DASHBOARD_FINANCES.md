# Finances — Known Problems & Technical Debt

## P1 — Budget refresh formula is arbitrary

```rust
wage_budget = max(0, finance) × 6%
transfer_budget = max(0, finance) × 22%
```

Why 6% and 22%? These numbers determine the entire financial pacing of the
game. No design document explains whether they're calibrated to real esports
budgets or pure gameplay convenience.

**Fix:** Document calibration source or derive from real data.

---

## P2 — No multi-year financial planning

The game only tracks the current season's income and expenses. There's no
P&L history, no year-over-year comparison, no trend visualization.

**Fix:** Store historical seasons in financial ledger and add trend charts
to the Finances tab.

---

## P3 — Matchday revenue is a random estimate

```rust
attendance_pct = random(15.0..=30.0);
avg_ticket = random(4.0..=8.0);
```

These are pure random numbers with no relation to team performance,
reputation, stadium size, or opponent strength.

**Fix:** Derive attendance from team form, reputation, opponent, and match
importance.

---

## P4 — Sponsor offers not visible outside inbox

Pending sponsor offers live as inbox messages with `ChooseOption` actions.
There's no dedicated "Sponsors" view that shows upcoming, active, and expired
sponsors in one place. The Finances tab only shows the active sponsor.

**Fix:** Add a sponsor management section with history and pipeline.

---

## Resolved — Financial ledger visibility and coverage

The Finances tab now renders a filterable transaction ledger. The engine records
salary, staff wage, facility, transfer, sponsorship, matchday revenue, prize,
budget refresh, and academy transaction kinds. This is no longer an open issue.

---

## P6 — No debt mechanics

If `finance < 0`, the warning message says "URGENT: Club in Debt" but there
are no gameplay consequences — no interest, no board action, no forced
player sales.

**Fix:** Add interest on negative balance, board intervention at thresholds,
or forced cost-cutting measures.

---

| Prio | Issue | Effort |
|---|---|---|
| P1 | Budget % arbitrary | Small (doc) |
| P2 | No multi-year history | Medium |
| P3 | Matchday revenue random | Medium |
| P4 | Sponsor offers inbox-only | Small |
| P6 | No debt consequences | Large |
