# LEGO Mindstorms EV3 Web Motor Control

A fast, lightweight web server in Rust for the LEGO Mindstorms EV3 hardware (Texas Instruments Sitara AM1808, ARMv5te @ 300MHz, 64MB DRAM).

| Design View (`/design`) | Run View Desktop (`/run`) | Run View Mobile (390×844) |
| :---: | :---: | :---: |
| ![Design View](docs/screenshots/screenshot_design.png) | ![Run View Desktop](docs/screenshots/screenshot_run.png) | ![Run View Mobile](docs/screenshots/screenshot_run_mobile.png) |

---

## Quickstart

### 1. Standalone Appliance Setup (1-Click Bake & Flash)
Create a git-ignored `wifi.txt` file in the repository root with your Wi-Fi name on line 1 and password on line 2, then run:
```bash
sudo ./tools/flash-image.sh -y
```
`flash-image.sh` automatically runs `bake-image.sh` (reading `wifi.txt`), generates a `.bmap` block map without slow `xz` compression, auto-detects your removable MicroSD card, flashes only allocated blocks with `bmaptool`, and verifies the installation. Insert the card and a compatible USB Wi-Fi dongle into the EV3 brick to boot directly to the ready URL (`http://<ip>/`).

### 2. Host Simulation Testing
Test the complete web interface, 2D joystick, and simulated motors on your PC without EV3 hardware:
```bash
cargo run -- --mock --port 8080
# Open http://localhost:8080/ in your browser
```

---

## Documentation Guides

Detailed guides are modularized in the [`docs/`](docs/) directory:

- **[Appliance Setup Guide](docs/setup.md):** Detailed image baking (`tools/bake-image.sh`), drive verification, command-line flashing (`tools/flash-image.sh`), and first boot.
- **[Development & Deployment Guide](docs/development.md):** Cross-compilation toolchain, host simulation, USB networking (Windows RNDIS / macOS CDC), and deployment scripts.
- **[System Architecture Guide](docs/architecture.md):** Runtime memory model, non-blocking sysfs polling, unified safety watchdog, web asset pipeline, and code organization rules.
- **[REST API Reference](docs/api.md):** Complete JSON REST API reference table, unified `/api/port` controller, payload schemas, and examples.

---

## Architectural Design & Features

- **Non-Blocking Telemetry:** Background polling thread caches kernel sysfs telemetry every 50 ms. HTTP request threads read directly from RAM without blocking on sysfs file I/O.
- **Embedded Web Assets:** Static web dashboard assets are embedded as plain text at compile time and served directly from RAM without disk I/O or compression overhead.
- **Unified Safety Watchdog:** 2,000 ms timeout across all driving controls with coast stop action to eliminate gear stress.
- **On-Brick LCD Status Display:** Native 7-row console status renderer on `/dev/tty1` displaying live IP, battery voltage, and motor telemetry.
- **Resource Targets:** Designed for ARM9 @ 300 MHz and 64 MB RAM, targeting < 3 MB RAM RSS with a statically linked musl binary.

---

## Project Structure

```
ev3_next/
├── Cargo.toml               # Cargo package configuration
├── ev3-web.service          # Systemd unit file for auto-start on boot
├── docs/*                   # Documentation guides and screenshots
├── tools/                   # Appliance automation scripts (bake, flash)
├── src/                     # Rust application source (sysfs, controller, router)
└── web_assets/              # Web dashboard source (HTML, CSS, JavaScript)
```

---

## License
MIT / Apache 2.0
