# Hardware sensor QA

## Gates

Final commands passed:
- `PATH=/bin:$PATH just format`
- `PATH=/bin:$PATH just check-ci`: fmt/clippy clean; 67 library + 15 interaction + 3 CLI tests, plus dev-tool/watch/release shell regressions.
- `PATH=/bin:$PATH just test-visual`: complete private Sway suite, all 21 themes at 640/378 px, full density/control/update/popover regressions and three new sensor-state scenarios.
- `git diff --check`; `test -z "$(git diff --cached --name-only)"`: clean whitespace and no staged files. No project commits/tags/staging were made (the release regression uses its own temporary repository).

Regression-first evidence: `sensor_fixture_and_states_keep_every_reading_and_context` failed on the missing visual scenario before implementation; `empty_selection_and_missing_hwmon_have_intentional_distinct_states` failed when empty selection incorrectly reported missing sysfs, then passed after the localized fix. Initial full-fit capture exposed actual overflow when the synthetic card was ordered before Disk; placing it after Disk, matching live/source order and preserving the existing order, solved the fixture without altering other cards' styling. Theme measurement waits now use bounded polling rather than a fragile 0.85-second sleep.

Unit coverage in `src/sensors.rs`: millidegree Celsius/RPM, negative temperatures, stopped zero, malformed/overflowing/negative fan inputs, optional fault/enable flags, permission/missing failures, no-data versus empty selection, conservative defaults excluding unlabeled motherboard/Core/junction channels, explicit temperatures/fans, stable renumbering and device-link/fallback identity, hotplug recovery, 120-slot eviction/gaps, duplicate chips versus ambiguous complete identities, closed/duplicate/unsafe config validation.

## Visual evidence (ignored, synthetic)

The fixture at that check held thirteen cards across six providers — all four Agy budgets, twelve CPU cores, RAM/swap, network, disk, four sensor readings, weather and air; a seventh reported agent would add a fourteenth tile rather than being listed here. Final `screenshots/stripe-density-screen-fit-original-geometry.log` contains all 21 themes at actual GTK 636×1413 allocation (640×1440 compositor rectangle), each `upper=1264 page=1264 no_scroll=true cards=13 horizontal_clip=false`. At 640×1360, Original honestly scrolls (`upper=1203 page=1184`); narrow, long-error and high-core fixtures also retain scrolling.

Directly inspected:
- `screenshots/stripe-density-theme-{original,terminal,blueprint}.png`
- `screenshots/stripe-theme-{original,terminal}-wide.png` (Comfortable)
- `screenshots/stripe-theme-original-narrow.png`
- `screenshots/stripe-density-fit-sensors-{zero,missing,error}-original-wide.png`
- `screenshots/stripe-density-fit-sensors-{zero,missing,error}-original-narrow-bottom.png`
- `screenshots/stripe-density-screen-fit-original-narrow-bottom.png`

Readings are legible, including `0 RPM · stopped`; unavailable rows show an em dash and specific reason rather than zero. Stale rows keep their prior reading with a visible per-row STALE reason. Units, labels and source context remain available in row tooltips / keyboard Info. No change to theme registry, typography, CPU plots or RAM/network rendering.

## Limits / separate issues

- Driver labels can be sparse. Unlabeled fans and extra probes need explicit stable selectors; RPM never identifies a fan's physical function. No thresholds or sensor controls.
- Device topology paths can change after hardware moves; selectors then intentionally report missing rather than matching a different chip. Duplicate complete identities are intentionally unavailable/stale.
- Histories are signed raw session slots with gaps and bounded eviction; temperature channels plot them with a stable auto range (outward 5 °C steps, minimum 10 °C span, in-range samples never move the Y scale). Fans stay readings-only.
- Permission errors are unit-tested through the exact reader seam, not by changing host sysfs permissions. Live collection is read-only and uses already exposed sources; no host setup or personal reading capture is committed.
- The previously reported no-swap character wrapping is flagged separately: the memory/inline-telemetry renderer was not changed by this slice. Host monospace glyph rendering remains unconfirmed; no blind typography fix.
- Acceptance still requires the parent session's independent review gate.
