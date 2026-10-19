# EV3 System Architecture

This document describes the runtime architecture, memory model, safety watchdogs, and self-updating asset pipeline.

---

## 1. Hardware Constraints & Resource Budget

The LEGO Mindstorms EV3 runs on a single-core Texas Instruments Sitara AM1808 (ARM926EJ-S @ 300 MHz) with 64 MB of system RAM.

| Resource | Target Limit | Design Implementation |
| :--- | :--- | :--- |
| **Process Memory (RSS)** | < 3 MB RAM | Statically linked native Rust executable compiled with `opt-level = "z"` and `lto = true`. |
| **HTTP Response Time** | < 1 ms | Pre-cached in-memory telemetry reads; zero sysfs file opens on request threads. |
| **SD Card Write Wear** | Zero in operation | All dynamic data served from RAM; system logs buffered in systemd journal. |
| **Network Latency** | < 15 ms | Rate-limited client messaging (15 Hz) and compact JSON payloads. |

---

## 2. In-Memory Telemetry & Non-Blocking Polling

Reading Linux sysfs files (`/sys/class/tacho-motor/` and `/sys/class/lego-sensor/`) takes 2–8 ms per file on an ARM9 processor. Polling four motors and sensors directly on the HTTP request thread would cause 100% CPU usage and dropped connections.

### Polling Architecture:
```
[ Linux Kernel sysfs ]
         │
         │  (Background thread: 50 ms polling loop)
         ▼
[ In-Memory Controller State (RAM) ]
         ▲
         │  (HTTP request thread: non-blocking RAM read)
         │
[ HTTP Router GET /api/status ] ───► [ Web Client Browser ]
```

- **Background Polling Thread:** Queries motor attributes and sensor values every 50 ms, caching results in in-memory structs.
- **HTTP Request Thread:** Reads cached state directly from RAM and serializes JSON without sysfs I/O.
- **Motor Writes:** Only active movement commands write directly to sysfs on the request thread.

---

## 3. Unified Safety Watchdog

To prevent runaway robots when network connections drop:

1. **Unified Safety Watchdog (2000 ms default cutoff):**
   - Applies equally to all driving controls (joystick, tank drive, sliders, single-port commands, and batch API requests).
   - Any client communication (such as telemetry polling or motor commands) refreshes the watchdog activity timer.
   - If client communication ceases for more than 2.0 seconds (2000 ms) while motion is active, the supervisor thread automatically stops and coasts all motors (`coast_all_motors`) to eliminate mechanical gear stress.
2. **Emergency Stop (`POST /api/estop`):**
   - Immediately stops all motors with brake action and sets status LEDs to red.

---

## 4. Web Asset Delivery & On-Demand Widget Sync

Core web files (`index.html`, `style.css`, `app.js`) and modular widgets (`widgets/<name>.js`) are served directly from `web_assets/` on disk (`/home/robot/web_assets` on the EV3 brick, or `./web_assets` in host mock mode).

### Static Asset & Widget Serving:
```
[ HTTP Client Request ]
         │
         ├─► Core App (/, /run, /design) ──► Serve web_assets/index.html
         ├─► Static Assets (/style.css, /app.js) ──► Serve from web_assets/
         ├─► Widget Discovery (GET /api/widgets) ──► Scan web_assets/widgets/*.js
         ├─► Widget Modules (GET /widgets/{name}.js) ──► Serve web_assets/widgets/{name}.js
         └─► On-Demand Sync (POST /update) ──► Sync web_assets/widgets/*.js from GitHub
```

- **Single HTML Shell (`index.html`):** Serves `/`, `/run`, and `/design` from one HTML file so switching between Run View and Design View happens in place in memory without losing session state (such as recorded keyframes).
- **Flat Single-File Widgets:** Each widget is a standalone ES module (`web_assets/widgets/<name>.js`). `GET /api/widgets` scans `web_assets/widgets/*.js` dynamically so new widget files appear in Design View without changing server or core frontend code.
- **On-Demand GitHub Updater (`POST /update`):** Triggered by the **Update** button in Design View. `src/updater.rs` queries the GitHub Contents API, downloads new or changed `.js` widget files to `.tmp` staging paths, atomically renames them into `web_assets/widgets/`, and deletes widget files removed upstream.

---

## 5. Frontend Dashboard Architecture

- **Single Top Tab Navigation:** Compact `Design` and `Run` tabs switch views in place without reloading the page.
- **Responsive 2-Unit Grid System:**
   - Uses 2 columns on mobile devices and 4 columns on desktop displays (`grid-auto-flow: dense`).
   - **2x2 Square Widget (`.run-card-2x2`):** 2 columns by 2 rows with 1:1 aspect ratio. Used for 2D Joystick and Tank Drive widgets.
   - **2x1 Half-Height Widget (`.run-card-2x1`):** 2 columns by 1 row. Used for Speed Slider, Action Button, Sensor Display, and Keyframe Recorder widgets.
- **2D Virtual Joystick:**
   - Pointer capture API isolates touch and mouse input.
   - `touch-action: none` prevents touch dragging from scrolling the page on mobile devices.
   - Supports single-motor per axis or multi-motor axis assignment with per-motor inversion.
   - Client-side 15 Hz throttle prevents network queue saturation.
- **Footer Status Bar:** Passive telemetry (`Battery`, `Online`, `RTT`) and system actions (`STOP ALL`, `Power Off`, `Update`, `Rescan`) live in the footer and top bar, keeping primary robot controls above the fold.

---

## 6. Code Organization & Separation of Concerns

Use this guide to place new features in the correct layer:

| Subsystem | File / Directory | What Goes Here | What Does NOT Go Here |
| :--- | :--- | :--- | :--- |
| **Hardware sysfs Drivers** | `src/sysfs/` (`mod.rs`, `motor.rs`, `sensor.rs`, `led.rs`, `display.rs`, `wifi.rs`, `mock.rs`) | Direct Linux `/sys` operations, port address normalization, device discovery, file descriptor caching, hardware mocks. | No HTTP handling, no web routing, no JSON payload parsing. |
| **System State & Poller** | `src/controller.rs` | Background telemetry polling loop, in-memory status caching, safety watchdogs, emergency stop. | No raw HTTP socket operations, no HTML rendering. |
| **HTTP Router & Handlers** | `src/web.rs` | URL route dispatch, Origin/Host CSRF validation, request/response structs, command resolution, static asset and widget delivery. | No direct sysfs file reads, no motor physics math. |
| **Configuration & CLI** | `src/config.rs` | CLI arguments, port configuration, mock detection flags. | No runtime motor control, no network serving. |
| **Application Lifecycle** | `src/main.rs` | Program startup, 4-thread HTTP worker pool initialization, LCD refresher thread, shutdown handling. | No low-level driver logic, no route definitions. |
| **Frontend Shell & Styles** | `web_assets/index.html`, `web_assets/style.css` | Semantic HTML shell for Run and Design views and responsive layout styling. | No business logic or inline scripts. |
| **Frontend Controller** | `web_assets/app.js` | Telemetry polling (4 Hz), view switching, client rate limiting, shared form field helpers, `WidgetRegistry`. | No direct sysfs references, no widget-specific control math. |
| **Modular Widgets** | `web_assets/widgets/*.js` (`button.js`, `joystick.js`, `record.js`, `sensor.js`, `slider.js`, `tank.js`) | Standalone ES widget modules implementing `createDefault`, `renderDesign`, `attachDesign`, `renderRun`, `initRun`, `onTelemetry`, and `onEmergencyStop`. | No global app orchestration, no server polling loops. |
| **Widget Updater** | `src/updater.rs` | On-demand GitHub Contents API synchronization via `curl`, `.tmp` file staging, and atomic renames. | No UI event binding, no sysfs driver actuation. |
