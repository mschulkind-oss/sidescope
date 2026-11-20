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
| [`agent-activity-filtered.toml`](./agent-activity-filtered.toml) | Opt-in money profile beside the legacy one: `recent` asks for no grouping, the metered channel and only the mapped columns; day sources name their columns; adds a GitHub Copilot list-price estimate. Needs a producer whose capabilities probe succeeds |
| [`agent-activity-quota.toml`](./agent-activity-quota.toml) | Opt-in Claude plan quota profile: estimated contribution cards and observed utilization cards, run through the two wrappers below. Contains no money |
| [`quota-estimate.py`](./quota-estimate.py) | Python 3 (standard library) wrapper: runs one `agent-activity ledger series` query and prints a `sidescope-chart-v1` answer of estimated quota points, exact decimals, unit `percentage points`. `--convert-stdin` converts an answer from standard input and runs nothing |
| [`quota-observed.py`](./quota-observed.py) | Python 3 wrapper: runs one `agent-activity quota series` query and prints a `sidescope-chart-v1.1` sampled gauge of one account and window, with breaks at resets and long gaps. Same `--convert-stdin` mode |

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

## Trying it on your host

Everything below uses a throwaway configuration directory, so your real
`~/.config/sidescope/config.toml` is never read, enabled, or overwritten. Setting
`XDG_CONFIG_HOME` also means your usual weather, AI, and theme settings do not apply to
these runs. Run the commands from a checkout of this repository; use `sidescope` from
`just deploy`, `cargo install`, or any release binary (in the checkout, `just dev` runs the
same application with the same environment variable).

1. **Synthetic first, no ledger needed.** Point the profile at the fake ledger and check it:

   ```bash
   try="$HOME/.cache/sidescope-charts-try"
   mkdir -p "$try/sidescope"
   sed "s|\"agent-activity\"|\"$PWD/examples/charts/fake-ledger.sh\"|" \
     examples/charts/agent-activity.toml > "$try/sidescope/config.toml"
   XDG_CONFIG_HOME="$try" sidescope doctor   # prints every command; each should say "ok"
   XDG_CONFIG_HOME="$try" sidescope          # live mode: chart cards appear below the others
   ```

   Press **R** to refresh. The fake answer ignores the time window, so the numbers are
   fixed synthetic examples.
2. **Your real collector, still in the throwaway directory.** Copy the profile unchanged
   (`cp examples/charts/agent-activity.toml "$try/sidescope/config.toml"`), confirm
   `agent-activity` is on `PATH` (or put its full path in each `command`), then run
   `XDG_CONFIG_HOME="$try" sidescope doctor` and `XDG_CONFIG_HOME="$try" sidescope`.
   Sidescope does not start, sync, or authenticate the collector; if it is stopped, the
   cards show the last answer or an unavailable banner, and **Collected through** in Info
   stays "unknown" because this CLI does not assert collection freshness.
3. **Judge the result.** Open Details and Info on each card. Look for the labels
   "list-price estimate" and "incomplete", the hatched "so far" bar for today, and the
   coverage statement in Info. Compare a few totals with the CLI's own output.
4. **Adopt it deliberately.** Only when satisfied, paste the tables you want into your real
   configuration by hand. Report problems with the exact card and Info text.

Remove the trial with `rm -r "$HOME/.cache/sidescope-charts-try"`.

## Choosing a profile, and the quota caveats

Probe by hand; Sidescope never probes, rewrites a query or falls back:

- **Legacy** `agent-activity.toml` works with any producer that has `--layout columns`.
- **Filtered** needs `agent-activity ledger capabilities --format json` to succeed. Against an older
  producer it fails visibly as a nonzero exit.
- **Quota** estimate cards need that same output to list the four quota columns
  (`.series.columns` keys `subscriptionWeeklyPercent`, `subscriptionWeeklyUnweighted`,
  `subscriptionFiveHourPercent`, `subscriptionFiveHourUnweighted`); a successful probe alone is not enough.
  Observed cards need `agent-activity quota series --format json --window five_hour` to exit 0.

The three profiles are never merged. Replace the `/REPLACE/WITH/PATH` wrapper paths and the
`REPLACE-WITH-ACCOUNT-KEY` placeholder by hand (the account key is in the output of the observed command).

- Quota is not money and an estimate is not a reading. Estimated points are fitted from one owner's
  readings, are not published by the provider, are not the bar, can sum above 100, and exclude resets
  and other clients. Observed readings are the provider's reported level, about 15 minutes apart, and
  nothing is drawn between them. The two are never overlaid; no account link is known.
- A model with no fitted weight is a partial series (a lower bound), never a complete zero.
- Claude Code used with a direct API key is classified `subscription` upstream and may be counted.
- Both wrappers refuse rather than trim: over 2048 readings or buckets, 256 series, 100000 cells,
  1 MiB of producer output, an oversized notice or invalid input exits 1 with nothing printed.
  Read the wrapper before enabling it; it runs with Sidescope's authority.
