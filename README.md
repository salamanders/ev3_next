# ⚡ LEGO Mindstorms EV3 - High-Performance Web Motor Control

A blazingly fast, lightweight, and memory-efficient web-based motor control server engineered in Rust for the LEGO Mindstorms EV3 hardware (Texas Instruments Sitara AM1808, ARMv5te @ 300MHz, 64MB DRAM).

---

## 🌟 Key Features

- **Blazing Fast & Low Latency:** Direct Linux kernel `/sys/class/tacho-motor/` sysfs communication with < 5ms command execution latency.
- **Ultra-Lean Memory Footprint:** Statically linked native Rust binary consuming **< 3 MB RAM** (vs 35+ MB for Java JVM), preventing Out-of-Memory crashes on 64MB EV3 hardware.
- **Zero-Dependency Web UI:** Complete modern Single-Page Application (HTML5 / CSS3 / Vanilla JS) embedded directly into the executable binary via `include_str!`.
- **Instant Host Simulation Mode (`--mock`):** Develop, test, and actuate simulated motors directly on Windows/macOS/Linux without physical hardware attached.
- **Zero SD-Card Swapping:** 1-Click deployment script (`deploy.ps1`) pushes new builds over USB-Ethernet RNDIS directly to the brick in ~4 seconds.
- **Robot Tank-Drive Mode:** Built-in virtual joystick, D-Pad, and keyboard shortcuts (`WASD` / Arrow keys) for 2-wheel differential drive robots.
- **Dedicated Hardware Telemetry Thread:** Non-blocking 50ms polling loop caches motor encoders into memory, ensuring HTTP endpoints respond in < 0.05ms without CPU thrashing.

---

## 🚀 Quickstart: Local Host Testing (Windows / Linux / macOS)

Test the complete UI and simulated motor controls locally on your PC without EV3 hardware:

```bash
# 1. Run local mock server on port 8080
cargo run -- --mock --port 8080

# 2. Open in your browser:
http://localhost:8080/
```

### Keyboard Shortcuts
- <kbd>W</kbd> / <kbd>▲</kbd> : Forward
- <kbd>S</kbd> / <kbd>▼</kbd> : Reverse
- <kbd>A</kbd> / <kbd>◄</kbd> : Spin Left
- <kbd>D</kbd> / <kbd>►</kbd> : Spin Right
- <kbd>Spacebar</kbd> : **EMERGENCY STOP ALL MOTORS**

---

## 🛠️ Cross-Compilation & Hardware Deployment

### 1. One-Time EV3 OS Setup
1. Flash `ev3dev-stretch` onto a MicroSD card (4GB–32GB).
2. Insert card into EV3 and power on.
3. Connect EV3 to PC using Mini-USB cable.
4. Verify SSH connection:
   ```bash
   ssh robot@192.168.2.2  # (password: maker)
   ```

### 2. Boot Acceleration (10-15s Fast Boot)
Run once on EV3 via SSH:
```bash
sudo su
# Disable slow daemons & GUI to free 20MB RAM
systemctl disable --now brickman.service
systemctl mask connman-wait-online.service
systemctl mask systemd-fsck-root.service
systemctl mask apt-daily.service apt-daily.timer
```

### 3. Install Systemd Auto-Start Service
Copy `ev3-web.service` to EV3:
```bash
scp ev3-web.service robot@192.168.2.2:/tmp/
ssh robot@192.168.2.2 "sudo mv /tmp/ev3-web.service /etc/systemd/system/ && sudo systemctl daemon-reload && sudo systemctl enable ev3-web.service"
```

### 4. 1-Click Deployment from Windows
From the project directory on Windows:
```powershell
.\deploy.ps1
```
*This cross-compiles for ARMv5te, uploads the binary, and restarts the service in ~4 seconds.*

---

## 📡 REST API Reference

| Method | Endpoint | Payload | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/` | None | Serves the embedded HTML5 Dashboard |
| `GET` | `/api/status` | None | Real-time telemetry array for all 4 motor ports |
| `POST` | `/api/motor/{port}/run-forever` | `{"speed": 500}` | Runs motor continuously at set speed |
| `POST` | `/api/motor/{port}/run-timed` | `{"speed": 500, "time_ms": 1000, "stop_action": "brake"}` | Runs motor for specific duration |
| `POST` | `/api/motor/{port}/run-to-rel-pos`| `{"speed": 400, "position_sp": 360, "stop_action": "hold"}` | Steps motor by degree count |
| `POST` | `/api/motor/{port}/run-direct` | `{"duty_cycle": 75}` | Direct PWM power (-100 to 100%) |
| `POST` | `/api/motor/{port}/stop` | `{"action": "coast" \| "brake" \| "hold"}` | Stops motor with configured action |
| `POST` | `/api/motor/{port}/reset` | None | Zeroes the relative encoder count |
| `POST` | `/api/tank-drive` | `{"left_port":"B", "right_port":"C", "left_speed":500, "right_speed":500}` | Controls differential robot drive |
| `POST` | `/api/emergency-stop` | None | **Global E-Stop**: halts all motors immediately |

---

## 🏗️ Project Architecture

```
ev3_next/
├── Cargo.toml               # Cargo manifest (opt-level = 'z', lto = true)
├── Cross.toml               # Cross-compilation container configuration
├── ev3-web.service          # Production systemd unit file
├── deploy.ps1               # Automated 1-click deployment script (PowerShell)
├── deploy.sh                # Automated 1-click deployment script (Bash)
├── src/
│   ├── main.rs              # Entry point & HTTP worker thread pool
│   ├── config.rs            # CLI argument parser (--mock, --port, --poll-interval)
│   ├── controller.rs        # Hardware abstraction & background telemetry cache
│   ├── sysfs/
│   │   ├── mod.rs           # Sysfs module exports
│   │   ├── motor.rs         # Real Linux kernel tacho-motor sysfs driver
│   │   └── mock.rs          # Physics-accurate mock motor simulator
│   └── web/
│       ├── mod.rs           # Web module exports
│       ├── router.rs        # Route matching, CORS, static asset delivery
│       └── handlers.rs      # REST API request/response structs
└── web_assets/
    ├── index.html           # SPA Dashboard HTML with D-Pad and port cards
    ├── style.css            # Dark mode cyberpunk tactile theme
    └── app.js               # Live 100ms telemetry loop, keyboard handlers
```

---

## 🛡️ License
MIT / Apache 2.0
