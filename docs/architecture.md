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
- **2D Virtual Joystick:**
  - Pointer capture API isolates touch and mouse input.
  - `touch-action: none` prevents touch dragging from scrolling the page on mobile devices.
  - Client-side 15 Hz throttle prevents network queue saturation.
- **Footer Status Bar:** Passive telemetry (`Battery`, `Online`, `RTT`) and brick safety actions (`STOP ALL`, `Power Off`) live in the footer, keeping primary robot controls above the fold.
