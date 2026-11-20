# Chart source examples

Synthetic, opt-in examples for Sidescope's command-driven charts. Nothing here is loaded
automatically; copy what you want into your own configuration file. Commands run with
Sidescope's authority and inherited environment, **not in a sandbox**, so read a command
before enabling it and replace the example executable names with your own paths.

| File | What it is |
| :--- | :--- |
| [`synthetic-spend.sh`](./synthetic-spend.sh) | POSIX `sh` + `awk` command taking `--from --to --timezone` and printing a deterministic `sidescope-chart-v1` answer: USD interval totals for two example projects |
| [`synthetic-latency.sh`](./synthetic-latency.sh) | The same contract for an unrelated source: a millisecond gauge for two example services. Same widgets, no usage vocabulary |
| [`agent-activity.toml`](./agent-activity.toml) | Opt-in profile for the `agent-activity ledger series` CLI using the generic `columns-v1` mapper: 3 h at minute resolution, five calendar dates by project, by model, and subscription activity |
| [`fake-ledger.sh`](./fake-ledger.sh) | Prints a synthetic ledger-shaped answer from `tests/fixtures/charts/ledger/` so the profile can run end to end: swap each source's `command` for this script (it ignores the time window) |

## Reading the usage-cost profile

- The metered cards are a **list-price estimate**, not an invoice. Fixed fees, discounts, taxes
  and unobserved activity are not included, and a fresh command exit does not prove the
  collector is current.
- A response with no known price raises the matching `unpriced` counter; that cell is
  *partial* (a lower bound) and the headline says "incomplete".
- Plain subscription usage has no per-response dollar figure. It is shown only as a response
  count; `subscription_cost` is a separate provider-priced estimate and is never added to the
  metered cost.
- Omitted columns are zero because the ledger documents that rule; the profile opts in with
  `omitted = "zero"`. An explicit `null` is missing, never zero.
- "Project" is a display name for the ledger's workspace label.
- Presets: `window = "2h"` or `calendar_days = 3` with the same `--bucket`. The upstream limits
  are 400 buckets and 60,000 group x bucket values per answer, so widen the bucket, not the
  window, for long ranges; Sidescope shows a refusal rather than changing the question.
- The 3 h view is "about 3 h at 1 min": the CLI aligns bucket ends outward and Sidescope does
  not clip edge buckets. The current day bucket is provisional ("so far").
