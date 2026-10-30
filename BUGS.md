# Critical Path Stall Analysis and Proposed Fix

## Scope

This document covers normal operation on the primary control workflow:
- **2D Joystick** ([`web_assets/widgets/joystick.js`](web_assets/widgets/joystick.js)) controlling **3 motors** (1 inverted) via `POST /api/batch`.
- **Speed Slider** ([`web_assets/widgets/slider.js`](web_assets/widgets/slider.js)) controlling **1 motor** via `POST /api/port/{port}`.
- Continuous combinations of **moving the control**, **holding still for several seconds**, and **letting go**.

With all 4 motors connected, the Rust backend and sysfs driver path ([`src/controller.rs`](src/controller.rs) and [`src/sysfs/motor.rs`](src/sysfs/motor.rs)) execute commands in under 2 ms using cached file descriptors (`MotorWriteFiles`). The 1 Hz stationary hold timer in [`web_assets/app.js`](web_assets/app.js) also refreshes the 5-second watchdog (`DEFAULT_WATCHDOG_TIMEOUT_MS`) on time.

All stalls longer than 250 ms in normal operation come from three bugs in how the browser dispatches HTTP motor commands.

---

## Identified Issues

### BUG-A: `slider.js` Sends 3 to 4 Parallel Stop Requests on Every Release

- **Location:** [`web_assets/widgets/slider.js`](web_assets/widgets/slider.js) (`initRun`, lines 68–95).
- **Cause:**
  `grab()` sets `active = true`, and `stopAndCenter()` sets `active = false`. However, `stopAndCenter()` does not check `if (!active) return;` at entry (unlike `stopMotion()` in [`web_assets/widgets/joystick.js`](web_assets/widgets/joystick.js), lines 255–257).
  Line 93 attaches `stopAndCenter` to six events: `pointerup`, `pointercancel`, `lostpointercapture`, `touchend`, `touchcancel`, and `change`.
- **Impact (> 250 ms stall):**
  On every normal touch release, the browser fires `pointerup`, `lostpointercapture`, `touchend`, and `change` in the same gesture (2 to 3 events on mouse release). Each event calls `ctx.stopBurst()`, which immediately starts a `fetch()` request before the next event cancels only the pending retry timer.
  Releasing the slider starts **3 to 4 simultaneous `POST /api/port/{port}` requests** (plus one retry 120 ms later). This immediately consumes up to 4 of the browser's 6 HTTP/1.1 connections for the EV3 origin and blocks concurrent 2D joystick batches.

### BUG-B: Un-Gated, Un-Aborted `fetch` Calls Saturate the 6-Socket Pool and Execute Out of Order

- **Location:** [`web_assets/app.js`](web_assets/app.js) (`throttle` lines 313–356, `cancelStopBurst` lines 371–378, `stopBurst` lines 381–407, `apiPost` lines 410–419) and [`src/main.rs`](src/main.rs) (8-worker HTTP pool, lines 133–165).
- **Cause:**
  `startPolling()` waits for each `GET /api/status` response to finish before scheduling the next poll. `throttle()` does not check if a command request is already in flight. It starts a new `fetch()` every 100 ms per active widget with a 3,000 ms timeout (`AbortSignal.timeout(3000)`), and neither `clearThrottle()` nor `cancelStopBurst()` aborts in-flight `fetch()` calls.
  Because concurrent `fetch()` calls travel on separate TCP sockets and run on 8 concurrent HTTP worker threads with no command sequence number, requests arrive and acquire `cmd_lock` out of order.
- **Impact (0.3 s to 5.0 s stall / overshoot):**
  1. **Browser 6-socket queue bind-up (0.3 s to 3.0 s):** Moving both controls (20 POST/s) or releasing the slider (4 simultaneous POSTs) during a normal 150–250 ms EV3 USB Wi-Fi latency spike fills all 6 browser sockets. New move and stop commands wait in the browser connection queue behind stale requests.
  2. **Release -> grab again -> hold still (~950 ms motor stall):** `cancelStopBurst()` cancels only `setTimeout`, not in-flight `STOP` HTTP requests. When the user releases a control and quickly grabs it again into a stationary hold, an in-flight `STOP` on Socket 1 can acquire `cmd_lock` *after* the new `RUN` on Socket 2. The motor stops dead and stays stalled for up to **1,000 ms** until `holdTimer` sends the next 1 Hz repeat.
  3. **Move or hold -> release (5,000 ms motor overshoot):** `clearThrottle()` cancels timers, not an in-flight `RUN` HTTP request (from dragging or from a 1 Hz `holdTimer` tick right before release). If Socket 1 lags by more than 120 ms while `stopBurst()` finishes both `STOP` packets on Socket 2, the delayed `RUN` executes *after* the stops. The motors keep running for **5,000 ms** until `check_watchdogs()` coasts them.

### BUG-C: `stopBurst` Skips Its Retry When the First Stop Request Takes >= 500 ms

- **Location:** [`web_assets/app.js`](web_assets/app.js) (`stopBurst`, lines 394–400).
- **Cause:**
  `stopBurst()` awaits `actionFn()` (`apiPost`, which has a 3,000 ms timeout) and then checks `(Date.now() - startTime) < 500` before scheduling the confirmation retry.
- **Impact (up to 5.0 s overshoot):**
  Whenever the first stop request stalls or times out after 500 ms, `Date.now() - startTime` is already `>= 500`. `stopBurst()` skips the retry completely when a retry is actually needed, leaving the motor running until the 5,000 ms backend watchdog expires.

---

## Proposed Fix

### Architectural Summary

| Option / Change | Status | Technical Rationale & Target Files |
| :--- | :--- | :--- |
| **Guard Slider Release Handler** | `[IMPLEMENTED]` | Check `if (!active && slider.value === "0") return;` in `onRelease` in [`web_assets/widgets/slider.js`](web_assets/widgets/slider.js) while keeping the explicit **Zero (Stop)** button unconditional. Eliminates duplicate stop bursts on release. |
| **Per-Key Single-In-Flight Coalescing ("Latest-Wins")** | `[IMPLEMENTED]` | Track `inFlight`, `abort`, and `nextFn` per control key in [`web_assets/app.js`](web_assets/app.js). Keeps at most 1 request in flight per widget (<= 3 sockets total including `/api/status`), preventing 6-socket pool saturation and stale queue buildup. |
| **Monotonic Command Sequence Number (`seq`)** | `[IMPLEMENTED]` | Attach an incrementing `seq: Option<u64>` to motor commands in [`web_assets/app.js`](web_assets/app.js), [`src/web.rs`](src/web.rs), and [`src/controller.rs`](src/controller.rs). Drop per-port commands with `seq < last_seq[port]` inside `cmd_lock` so stale `STOP` or `RUN` packets can never overwrite newer ones across worker threads. |
| **Tight Command Timeout & Reliable Stop Retry** | `[IMPLEMENTED]` | Lower motor command HTTP timeout from `3000 ms` to `800 ms` and remove the `< 500` ms retry cutoff in `stopBurst()` in [`web_assets/app.js`](web_assets/app.js) so a failed first stop always triggers the second stop attempt. |
| **Client-Only `AbortController` Without Backend `seq`** | `[REJECTED]` | Calling `abort()` in the browser after request bytes have already entered the TCP socket does not stop an EV3 worker thread from reading the packet and executing it out of order. |

---

### Step-by-Step Implementation Plan

#### 1. Deduplicate Release Events in `web_assets/widgets/slider.js`
- Set `active = true` inside both `grab` and the `input` listener (so keyboard or track taps also arm `active`).
- Split release handling from the manual button click:
  - Release events (`pointerup`, `pointercancel`, `lostpointercapture`, `touchend`, `touchcancel`, `change`) call `onRelease()`, which returns immediately when `!active` before calling `stopAndCenter()`.
  - `.btn-slider-zero` `click` continues to call `stopAndCenter()` unconditionally.

#### 2. Add Per-Key In-Flight Coalescing and Fix `stopBurst` in `web_assets/app.js`
- Add `this.cmdSeq = 0` on `EV3App` and include `seq: ++this.cmdSeq` in `apiBatch()` and single-port motor `apiPost()` payloads.
- Lower the `fetch` timeout for real-time motor commands (`apiPost` on `/api/batch` and `/api/port/*`) from `3000 ms` to `800 ms` (keep longer timeouts for `/api/rescan` and `/update`).
- Extend each `throttleMap` entry with `inFlight: false` and `pendingFn: null`:
  - Wrap dispatched functions in a per-key runner: if `entry.inFlight` is `true`, store the latest function in `entry.pendingFn` instead of opening another concurrent TCP request.
  - When the active request settles (`finally`), immediately execute `entry.pendingFn` (if still present and not cancelled) so the brick always receives the newest control position with zero intermediate backlog.
- In `stopBurst(key, actionFn, maxCount = 2, gapMs = 120)`:
  - Clear `entry.pendingFn` when disarming `throttle`.
  - Abort any in-flight move `AbortController` for `key` so the browser frees its socket immediately.
  - Remove `(Date.now() - startTime) < 500` so that if stop attempt 1 fails or times out at `800 ms`, stop attempt 2 still executes (bounded by `!state.cancelled` and `sent < maxCount`, retrying immediately on error or after `gapMs` on success).

#### 3. Enforce Monotonic Per-Port `seq` Ordering in `src/web.rs` and `src/controller.rs`
- Add `pub seq: Option<u64>` to `PortCommandPayload` and `BatchCommandPayload` in [`src/web.rs`](src/web.rs).
- Add `port_seq: Mutex<[u64; 4]>` (or store `[u64; 4]` directly under `cmd_lock` via `Mutex<[u64; 4]>`) on `MotorController` in [`src/controller.rs`](src/controller.rs):
  - When `seq` is `Some(s)`, compare `s` against `last_seq[port_idx]` inside `cmd_lock`:
    - If `s < last_seq[port_idx]`, skip hardware execution for that stale packet and return `"Ignored stale command"`.
    - Otherwise update `last_seq[port_idx] = s` and execute normally.
  - On `POST /api/estop` (`emergency_stop`) and watchdog `coast_all_motors`, execute unconditionally regardless of `seq`.
