# COSMIC Temperature Monitor

A native Rust temperature monitor for the COSMIC desktop.

COSMIC Temperature Monitor watches temperatures exposed by lm-sensors, presents a live sensor dashboard, and highlights thermal trouble without assuming every machine exposes identical telemetry.

## Colour language

- **Cyan — Cool:** comfortably below the sensor warning threshold.
- **Blue — Operational:** within the reported operating range.
- **Red — Hot:** at or above the reported warning threshold, or a CPU throttle counter has increased.

Sensor-provided maximum and critical limits are preferred. Conservative device-family fallbacks are used when firmware does not publish limits. These are heuristics, not manufacturer guarantees.

## Throttling detection

The application samples Linux CPU thermal-throttle counters under /sys/devices/system/cpu. It reports a throttle event only when a counter increases between scans. Some processors and drivers do not expose these counters; in that case temperature monitoring remains available, but active throttling cannot be confirmed.

## Requirements

- Linux with COSMIC
- Rust toolchain (edition 2024)
- lm-sensors
- Native build dependencies required by libcosmic

On Debian/Pop!_OS-based systems:

    sudo apt install lm-sensors
    sudo sensors-detect
    sensors

## Build and run

    cargo run

Release build:

    cargo build --release

## Notes

- Polling interval: two seconds.
- Sensor history is kept in memory for the current session.
- No network access, telemetry upload, or privileged hardware writes.
- The application reads sensor output and kernel counters only.

## Official COSMIC references

- [libcosmic](https://github.com/pop-os/libcosmic)
- [Official COSMIC application template](https://github.com/pop-os/cosmic-app-template)
- [libcosmic API documentation](https://pop-os.github.io/libcosmic/cosmic/)
- [libcosmic book](https://pop-os.github.io/libcosmic-book/)

## Licence

GPL-3.0-or-later. See LICENSE.
