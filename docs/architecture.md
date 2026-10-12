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
[ HTTP Router /api/ports ] ───► [ Web Client Browser ]
```

- **Background Polling Thread:** Queries motor attributes and sensor values every 50 ms, caching results in atomic in-memory structs.
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
   - Immediately dispatches a broadcast reset to all motors and illuminates red status LEDs.

---

## 4. Web Asset Delivery & Dynamic Widget Updater

Static assets (`index.html`, `design.html`, `style.css`, `app.js`) are served directly from RAM without disk access or compression overhead. Modular widgets are delivered from disk if present or from embedded RAM baseline.

### Static Asset Serving:
```
[ HTTP Client Request ]
         │
         ├─► Core Pages (/, /run, /design, /style.css, /app.js) ──► Serve directly from RAM
         │
         └─► Widget Files (/widgets/{widget}/widget.js)
                 │
                 ├─► Disk file exists (/home/robot/web_assets/widgets/...) ──► Serve uncompressed file
                 │
                 └─► Disk file absent ──► Serve embedded compile-time baseline from RAM
```

- **RAM Asset Delivery:** Core web assets are compiled into the binary (`include_bytes!`) and served directly from RAM without disk I/O or gzip overhead.
- **Single-File Widget Architecture:** Each dynamic widget consists of a single `widget.js` file.
- **Background Widget Updater:** `src/updater.rs` performs a single non-blocking check on startup with a 5-second timeout. If the network is available, it compares remote widget manifests and downloads updated `widget.js` files atomically to `/home/robot/web_assets/widgets/<name>/`.

---

## 5. Frontend Dashboard Architecture

- **Single Top Tab Navigation:** Compact `Design` and `Run` tabs eliminate duplicate headers and maximize vertical screen space.
- **Responsive 2-Unit Grid System:**
  - Uses 2 columns on mobile devices and 4 columns on desktop displays (`grid-auto-flow: dense`).
  - **2x2 Square Widget (`.run-card-2x2`):** 2 columns by 2 rows with 1:1 aspect ratio. Used for the 2D joystick widget.
  - **2x1 Half-Height Widget (`.run-card-2x1`):** 2 columns by 1 row with 2:1 aspect ratio. Used for sliders, action buttons, and sensor displays.
  - Sized so a 2x2 joystick with a 2x1 slider forms a 2x3 mobile layout (~560 px height) that fills mobile viewports without vertical scrolling.
  - On desktop displays, two 2x1 cards stack next to the 2x2 joystick to match its total height.
- **2D Virtual Joystick:**
  - Pointer capture API isolates touch and mouse input.
  - `touch-action: none` prevents touch dragging from scrolling the page on mobile devices.
  - Dynamic radius scaling adapts to element dimensions.
  - Client-side 15 Hz throttle prevents network queue saturation.
- **Footer Status Bar:** Passive telemetry (`Battery`, `Online`, `RTT`) and brick safety actions (`STOP ALL`, `Power Off`) live in the footer, keeping primary robot controls above the fold.

---

## 6. Code Organization & Separation of Concerns

Use this guide to place new features in the correct layer:

| Subsystem | File / Directory | What Goes Here | What Does NOT Go Here |
| :--- | :--- | :--- | :--- |
| **Hardware sysfs Drivers** | `src/sysfs/` (`motor.rs`, `sensor.rs`, `led.rs`, `display.rs`, `wifi.rs`, `mock.rs`) | Direct Linux `/sys` operations, device discovery, file descriptor caching, hardware mocks. | No HTTP handling, no web routing, no JSON payload parsing. |
| **System State & Poller** | `src/controller.rs` | Background telemetry polling loop, in-memory status caching, safety watchdogs, emergency stop. | No raw HTTP socket operations, no HTML rendering. |
| **HTTP Transport & Routing** | `src/web/router.rs` | URL route dispatch, HTTP methods, security headers (Origin/Host validation), static asset delivery. | No direct sysfs file reads, no motor physics math. |
| **API Payloads & Schemas** | `src/web/handlers.rs` | Request/response structs, command normalization, parameter inference, JSON serialization. | No hardware communication, no thread management. |
| **Configuration & CLI** | `src/config.rs` | CLI arguments, port configuration, mock detection flags. | No runtime motor control, no network serving. |
| **Application Lifecycle** | `src/main.rs` | Program startup, worker thread pool initialization, LCD refresher thread, shutdown handling. | No low-level driver logic, no route definitions. |
| **Frontend Templates** | `web_assets/index.html`, `design.html` | Semantic HTML structure for Run and Design views. | No business logic, no styling rules. |
| **Frontend Logic** | `web_assets/app.js` | UI event listeners, pointer capture, client rate limiting (15 Hz), API communication, `WidgetRegistry`. | No direct sysfs references, no individual widget math or DOM layouts. |
| **Modular Widgets** | `web_assets/widgets/` (`joystick`, `tank`, `slider`, `button`, `sensor`) | Standalone native ES widget modules and baseline `manifest.json`. | No global app orchestration, no server polling loops. |
| **Catalog Updater** | `src/updater.rs` | Background thread for widget updates via curl, manifest comparisons, atomic on-disk swaps. | No UI event binding, no sysfs driver actuation. |
