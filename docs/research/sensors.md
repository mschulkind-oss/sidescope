# Read-only hardware sensors

Source contract: [Linux hwmon sysfs interface](https://docs.kernel.org/hwmon/sysfs-interface.html). `name` identifies the driver/chip; `tempN_input` is millidegrees Celsius, `fanN_input` is RPM. Channel labels and enable/fault attributes are optional. Zero RPM alone indicates a stopped reading, not a disconnected or broken fan.

Local read-only inspection confirmed hwmon sources are already exposed (NVMe, GPU, CPU and motherboard driver families). No host setup is required for exposed readable files. The existing Waybar configuration is unavailable and is not treated as a mapping oracle. No actual readings or personal device paths were committed.

Enumeration `hwmonN` can change after reboot/hotplug. Use chip name plus canonical device path plus channel; the `device` link is preferred, with the enclosing device path before `/hwmon/hwmonN` as fallback. Same-name chips at different device paths remain distinct; a duplicate complete identity is ambiguous, never first-match-selected.

Defaults are deliberately conservative: labeled CPU package/control/die and GPU edge temperatures, and labeled fan channels. Other channels require explicit selectors. Motherboard channel names/values do not prove physical placement; fan RPM does not prove a cooling role. Absent/malformed/permission/fault states must not become zero. Re-discover each 2-second local tick; retain last success per stable identity with missing raw-history slots. No sensor thresholds, control writes, install steps, `sensors` subprocess or sudo dependency.

Implementation handoff: [plan](../plans/sensors.md), [tasks](../tasks/sensors.md), [QA](../qa/sensors.md).
