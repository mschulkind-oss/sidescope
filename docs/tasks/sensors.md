# Sensor tasks

- [x] Read AGENTS/design and validate existing local sampler/config/render seams; record [source research](../research/sensors.md).
- [x] Read-only per-tick hwmon discovery with stable chip/device/channel identity and ambiguity handling.
- [x] Closed optional explicit selectors, friendly labels, conservative defaults, millidegree Celsius/RPM units and stopped zero.
- [x] Per-channel last success/time and bounded 120-slot signed raw history, with missing slots on failures/hotplug.
- [x] Compact themed card; the full fixture retains all four Agy budgets and all existing readings. Preserve Comfortable as default and existing 21 themes.
- [x] Unit tests for missing/permissions/malformed/fault/disabled/zero/hotplug/duplicates/stable identity and config validation; synthetic state/render regressions.
- [x] Format/check-ci and full private Sway visual gate; directly inspect Original/Terminal and wide/narrow zero/missing/error screenshots. Capture paths and remaining limits in [QA](../qa/sensors.md).

Separate issue: the reported no-swap label wrapping is outside the unchanged memory/inline-telemetry renderer. No typography or glyph changes were made; host monospace glyph behavior remains unconfirmed.
