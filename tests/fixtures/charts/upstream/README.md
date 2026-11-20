# Imported synthetic producer answers

These thirteen JSON files are copied byte for byte from the producer's published
fixture directory for its ledger series answers. Every value in them is invented
(workspaces `ws` and `ws2`, the model `mystery-9`, fitted quota weights far larger
than real ones); they hold no account data and no credential.

Provenance: all thirteen files are byte-identical at upstream commits `73a5063`
and `49f89cc`. `error-too-many-buckets.json` and `series-objects.json` are also
identical at `9a15507`; the other eleven changed after it, and `series-quota-percent.json`
first appears at `73a5063`.

`capabilities-without-quota.json` is the one derived file: `capabilities.json`
minus the four quota descriptors (`subscriptionWeeklyPercent`,
`subscriptionWeeklyUnweighted`, `subscriptionFiveHourPercent`,
`subscriptionFiveHourUnweighted`), the keys of `.series.columns` that the quota
profile's documented check looks for.

Do not edit the imported files; replace them from upstream when the producer
changes an answer.

Files:

- `capabilities-without-quota.json`
- `capabilities.json`
- `error-too-many-buckets.json`
- `series-dst-23-hour-day.json`
- `series-dst-25-hour-day.json`
- `series-empty.json`
- `series-late-correction-after.json`
- `series-late-correction-before.json`
- `series-objects.json`
- `series-open-bucket.json`
- `series-quota-percent.json`
- `series-subscription.json`
- `series-unpriced.json`
- `series-zero-only.json`
