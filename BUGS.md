# Bug Tracking and Remediation Log

This document tracks all identified bugs, risks, and hardware incompatibilities.
Each entry includes status and remediation notes.

---

## Issue Checklist

- [x] **BUG-01: Excessive Sysfs Polling I/O on 300 MHz ARM CPU**
  - **Status:** DONE
  - **Severity:** High
  - **File:** `src/controller.rs`, `src/sysfs/motor.rs`
  - **Description:** Background loop executes ~37 sysfs file reads every 50 ms. This starves the single-core 300 MHz CPU.
  - **Fix Note:** Cached static motor attributes in `Motor` and restricted 50 ms polling to 4 dynamic files with a 2 s discovery cycle.

- [x] **BUG-02: Synchronous Sysfs Directory Scanning on HTTP Threads**
  - **Status:** DONE
  - **Severity:** High
  - **File:** `src/controller.rs`
  - **Description:** Motor commands call `Motor::find_all()` on the HTTP worker thread. This causes synchronous file I/O and latency spikes.
  - **Fix Note:** Cached discovered motor instances in `cached_motors` so HTTP command handlers look up motors in memory without directory scans.

- [x] **BUG-03: Real-Time Scheduling Policy `rr:50` Starves System Processes**
  - **Status:** DONE
  - **Severity:** High
  - **File:** `ev3-web.service`
  - **Description:** Real-time priority 50 can starve SSH and networking processes on single-core hardware.
  - **Fix Note:** Removed `CPUSchedulingPolicy=rr` and `CPUSchedulingPriority=50` and replaced with standard priority `Nice=-5`.

- [x] **BUG-04: Premature Service Start and Crash Loop on Boot**
  - **Status:** DONE
  - **Severity:** Medium
  - **File:** `ev3-web.service`
  - **Description:** Service starts at `basic.target` before network interface exists. The server crashes on socket bind.
  - **Fix Note:** Removed `DefaultDependencies=no`, configured `After=network.target`, and attached to `multi-user.target`.

- [x] **BUG-05: Host Simulation vs. Real Hardware Speed Clamping Discrepancy**
  - **Status:** DONE
  - **Severity:** Medium
  - **File:** `src/controller.rs`, `src/sysfs/motor.rs`
  - **Description:** Host simulation clamps motor speed. Real hardware writes unvalidated speed to sysfs, causing `-EINVAL`.
  - **Fix Note:** Added automatic speed clamping to `[-max_speed, max_speed]` in `Motor::set_speed_sp` before writing to sysfs.

- [x] **BUG-06: Swallowed Exceptions on Motor Stop & Emergency Stop**
  - **Status:** DONE
  - **Severity:** Medium
  - **File:** `src/controller.rs`
  - **Description:** `stop` and `emergency_stop` discard I/O errors with `let _ =`. This violates Rule 3.
  - **Fix Note:** Propagated `set_stop_action` errors in `stop()` and collected all port errors in `emergency_stop()` to return explicit error messages.

- [x] **BUG-07: Unplugged Motors Report `connected: true`**
  - **Status:** DONE
  - **Severity:** Medium
  - **File:** `src/sysfs/motor.rs`
  - **Description:** `read_status()` returns `connected: true` even when sysfs file reads fail.
  - **Fix Note:** Validated sysfs read success in `poll_dynamic_status` and returned `connected: false` on read failures or missing nodes.

- [x] **BUG-08: Default Port 80 Permission Denied on Non-Windows Hosts**
  - **Status:** DONE
  - **Severity:** Medium
  - **File:** `src/config.rs`
  - **Description:** Non-Windows hosts default to privileged port 80. Running host simulation without root fails.
  - **Fix Note:** Changed port default logic to select 8080 whenever mock mode is active, regardless of host operating system.

- [x] **BUG-09: Cross-Compilation Linker Fails on macOS and Linux Hosts**
  - **Status:** DONE
  - **Severity:** Medium
  - **File:** `.cargo/config.toml`, `zig-linker.py`, `zig-lld-arm.sh`, `deploy.sh`
  - **Description:** Cargo config specifies Windows `.cmd` batch file. Unix hosts fail to execute it.
  - **Fix Note:** Added executable `zig-lld-arm.sh` wrapper, updated `zig-linker.py` with cross-platform paths, and configured `deploy.sh` fallback.

- [x] **BUG-10: Mobile D-Pad Buttons Lack Release Handler**
  - **Status:** DONE
  - **Severity:** Medium
  - **File:** `web_assets/app.js`
  - **Description:** D-Pad buttons trigger `run-forever` on click with no release event. Robot drives continuously.
  - **Fix Note:** Replaced click listeners on D-Pad with momentary pointerdown and pointerup/cancel/leave release handlers.

- [x] **BUG-11: Sticky Keyboard Drive on Window Blur**
  - **Status:** DONE
  - **Severity:** Low
  - **File:** `web_assets/app.js`
  - **Description:** Losing window focus while pressing driving keys leaves keys active in memory.
  - **Fix Note:** Added a window blur event listener that clears activeKeys and halts the drive motors.

- [x] **BUG-12: Masking `systemd-fsck-root.service` Risks Read-Only Filesystem Lockout**
  - **Status:** DONE
  - **Severity:** Medium
  - **File:** `PROJECT_PLAN.md`, `README.md`
  - **Description:** Masking root fsck on MicroSD cards causes read-only mounts after unclean shutdowns.
  - **Fix Note:** Removed `systemd-fsck-root.service` masking from boot optimization guides to preserve ext4 recovery on unclean power cuts.

- [x] **BUG-13: Hardcoded Absolute Machine Paths in `REPORT.md`**
  - **Status:** DONE
  - **Severity:** Low
  - **File:** `REPORT.md`
  - **Description:** Document contains hardcoded `file:///C:/Users/Admin/...` paths in violation of Rule 8.
  - **Fix Note:** Replaced machine-specific absolute file URLs with portable repository-relative links across REPORT.md.

- [ ] **BUG-14: Missing Command Watchdog (Runaway Robot Risk)**
  - **Status:** PENDING
  - **Severity:** High
  - **File:** `src/controller.rs`, `web_assets/app.js`
  - **Description:** If Wi-Fi disconnects or browser closes during motor drive, the robot continues moving forever.
  - **Solution:** Add a 400ms command timeout watchdog in `src/controller.rs`. Automatically stop motors if no heartbeat or command arrives.

- [ ] **BUG-15: Repeated File Open Syscalls in Telemetry Loop**
  - **Status:** PENDING
  - **Severity:** Medium
  - **File:** `src/sysfs/motor.rs`
  - **Description:** The background poller opens and closes 16 sysfs files every 50 ms. This generates 320 file open syscalls per second. While exact CPU percentage is unbenchmarked, persistent file descriptors eliminate VFS lookup and allocation overhead.
  - **Solution:** Cache open file descriptors. Use `seek(SeekFrom::Start(0))` before each read to eliminate open/close overhead.

- [ ] **BUG-16: HTTP Worker Thread Over-Subscription on Single-Core CPU**
  - **Status:** PENDING
  - **Severity:** Medium
  - **File:** `src/main.rs`
  - **Description:** The server spawns 4 HTTP worker threads. 7 total threads compete for 1 CPU core and cause cache thrashing in the 16 KB L1 cache.
  - **Solution:** Reduce the HTTP worker pool size from 4 to 1 or 2 threads.

- [ ] **BUG-17: Missing Private Network Access (PNA) Preflight Headers**
  - **Status:** PENDING
  - **Severity:** Medium
  - **File:** `src/web/router.rs`
  - **Description:** Modern Chromium browsers enforce Private Network Access. Cross-origin requests to private IP addresses fail without PNA headers.
  - **Solution:** Add `Access-Control-Allow-Private-Network: true` to CORS preflight responses in `Router::handle_request`.

- [ ] **BUG-18: macOS USB Network Setup Verification (CDC Composite Gadget)**
  - **Status:** PENDING
  - **Severity:** Low (macOS hosts)
  - **File:** `PROJECT_PLAN.md`, `deploy.sh`, `README.md`
  - **Description:** macOS does not support RNDIS, but ev3dev-stretch includes a CDC Composite Gadget (CDC-ECM) natively recognized by macOS. macOS requires adding the CDC interface in Network Settings.
  - **Solution:** Document the macOS CDC Composite Gadget configuration steps in `PROJECT_PLAN.md` and `README.md`.

- [ ] **BUG-19: TTY Console Screen Contention with `getty@tty1` (Precautionary)**
  - **Status:** PENDING
  - **Severity:** Low
  - **File:** `ev3-web.service`, `PROJECT_PLAN.md`
  - **Description:** When `brickman` is disabled, Debian's `getty@tty1.service` may spawn a login prompt on `/dev/tty1`, potentially contending with application console output.
  - **Solution:** Mask or disable `getty@tty1.service` during boot optimization, and configure `ev3-web.service` with `TTYReset=yes`.


