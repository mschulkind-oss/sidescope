# Hardware sensors slice

Approved direction: read-only Linux hwmon discovery on the existing 2-second local worker; no setup, commands, privilege escalation or control writes. [Source research](../research/sensors.md) documents units and identity constraints.

Implementation boundaries:
- `src/sensors.rs` owns discovery, stable chip/device/channel selection, closed config validation, units, fault handling and per-channel last success. Never persist enumeration `hwmonN` or infer fan function from its RPM.
- Conservative defaults: labeled CPU package/control/die and GPU edge temperatures, plus labeled fans. Explicit friendly selectors replace defaults and can select other channels; an empty list selects none.
- Preserve stale values/time and append missing slots to bounded 120-slot signed raw histories. The compact card shows readings plus per-temperature-channel trend plots (stable 5 °C-step auto range, minimum 10 °C span, gaps break the line); fans stay readings-only. Raw-slot/gap context stays in row details/Info.
- Add one compact local card with synthetic fixtures. Preserve Comfortable, existing typography, graphs and all 21 themes. Extend the full Screen-fit fixture to thirteen cards and four Agy budgets; measured overflow still scrolls without hiding readings.
- Validate units/zero/missing/permissions/malformed/fault/hotplug/duplicates/identity/config; run format/check-ci/full private Sway and inspect PNGs, not just capture exit codes.

[Completed tasks](../tasks/sensors.md) · [QA/evidence](../qa/sensors.md). No threshold alerts or control/driver configuration. Memory/no-swap and host glyph rendering remain separate issues; their renderer/typography were not changed.
