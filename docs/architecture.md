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
         │  (HTTP request thread: < 0.05 ms read)
         │
[ HTTP Router /api/ports ] ───► [ Web Client Browser ]
```

- **Background Polling Thread:** Queries motor attributes and sensor values every 50 ms, caching results in atomic in-memory structs.
- **HTTP Request Thread:** Reads directly from memory and serializes JSON in less than 0.05 ms.
- **Motor Writes:** Only active movement commands write directly to sysfs on the request thread.

---

## 3. Two-Tier Safety Watchdogs

To prevent runaway robots when network connections drop:

1. **Heartbeat Watchdog (400 ms):**
   - Active during continuous drive operations (tank-drive and virtual joystick).
   - If the client fails to send a renewal packet within 400 ms, the server automatically stops both drive motors with brake action.
2. **Client Disconnect Watchdog (1,000 ms):**
   - Monitors active browser polling.
   - If the browser tab closes, drops Wi-Fi, or crashes while continuous motor commands are running, the server halts all ports.
3. **Emergency Stop (`POST /api/estop`):**
   - Immediately dispatches a broadcast reset to all motors and illuminates red status LEDs.

---

## 4. Self-Updating Web Asset Pipeline

The web dashboard updates automatically from the Git repository on boot without requiring SD card re-flashing or binary updates.

```
[ Power On / Boot ]
         │
         ▼
[ ev3-update-assets.service ]
         │
         ├─► [ Check GitHub API for latest commit SHA ]
         │       │
         │       ├─► (No change or offline): Exit cleanly
         │       │
         │       └─► (New commit found):
         │               ├─ Download index.html, style.css, app.js
         │               ├─ Pre-compress with gzip -9 into /tmp
         │               ├─ Atomically move to /home/robot/web_assets/
         │               └─ Restart ev3-web.service
         ▼
[ ev3-web-motor Server Start ]
         │
         ├─► Load /home/robot/web_assets/*.gz into RAM (~12 KB)
         │
         └─► (If missing/corrupted): Restore compile-time embedded baseline
```

- **Zero SD Card Wear:** The server loads pre-compressed `.gz` files into RAM once at startup. HTTP requests serve directly from memory with `Content-Encoding: gzip`.
- **Self-Healing Fallback:** The binary includes baseline assets compiled in via `include_bytes!`. If local files are damaged or missing, the server writes and serves the baseline automatically.

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
| **Frontend Styles** | `web_assets/style.css` | CSS variables, layout grid rules, mobile viewport media queries. | No application state, no dynamic DOM manipulation. |
| **Frontend Logic** | `web_assets/app.js` | UI event listeners, pointer capture, client rate limiting (15 Hz), API communication. | No direct sysfs references, no HTML templates larger than single cards. |
