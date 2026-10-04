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
  - **Description:** If Wi-Fi disconnects or the browser closes during motor drive, the robot continues moving forever. Momentary Tank Drive currently sends only one start packet on press.
  - **Solution:** Implement a two-tier watchdog in `src/controller.rs` and `web_assets/app.js`:
    1. **Tank Drive Watchdog (400 ms):** The browser sends a repeat `/api/tank-drive` heartbeat every 150 ms while a D-Pad button or drive key is held. The server stops tank drive motors if no tank-drive command arrives within 400 ms.
    2. **Client Connection Watchdog (1000 ms):** Individual motor card **Fwd** and **Rev** (`run-forever`) buttons run continuously while the browser is connected, and stop all motors if no `/api/status` poll arrives for 1000 ms.
    3. **Guardrail Against Sysfs Write Storm:** Track explicit `tank_drive_active` and `continuous_run_active` flags in `MotorController`. Trigger `stop` only one time when transitioning from active to timed-out. Never write `stop` to sysfs on every 50 ms tick while the robot is already idle.

- [ ] **BUG-15: Repeated File Open Syscalls in Telemetry Loop**
  - **Status:** PENDING
  - **Severity:** Medium
  - **File:** `src/sysfs/motor.rs`
  - **Description:** The background poller opens and closes 16 sysfs files every 50 ms. This generates 320 file open syscalls per second. While exact CPU percentage is unbenchmarked, persistent file descriptors eliminate VFS lookup and allocation overhead.
  - **Solution:** Cache open file descriptors inside an `Arc<Mutex<CachedMotorFiles>>` on `Motor` so `#[derive(Clone)]` remains a cheap pointer clone without calling `File::try_clone()` (`dup()` syscall). Use `seek(SeekFrom::Start(0))` and read into a fixed stack buffer (`[u8; 32]`) to avoid heap allocations on every 50 ms read.

- [ ] **BUG-16: HTTP Worker Thread Over-Subscription on Single-Core CPU**
  - **Status:** PENDING
  - **Severity:** Medium
  - **File:** `src/main.rs`
  - **Description:** The server spawns 4 HTTP worker threads. 7 total threads compete for 1 CPU core and cause cache thrashing in the 16 KB L1 cache.
  - **Solution:** Reduce the HTTP worker pool size from 4 to 2 threads.

- [ ] **BUG-17: Missing Private Network Access (PNA) Preflight Headers**
  - **Status:** PENDING
  - **Severity:** Medium
  - **File:** `src/web/router.rs`
  - **Description:** Modern Chromium browsers enforce Private Network Access. Cross-origin requests to private IP addresses fail without PNA headers.
  - **Solution:** Add `Access-Control-Allow-Private-Network: true` to CORS preflight responses in `Router::handle_request`.

- [x] **BUG-18: macOS USB Network & Wi-Fi Dongle Setup Documentation**
  - **Status:** DONE (Documentation updated in `PROJECT_PLAN.md` and `README.md`)
  - **Severity:** Low (macOS hosts & wireless setup)
  - **File:** `PROJECT_PLAN.md`, `README.md`
  - **Description:** macOS does not support RNDIS, but `ev3dev-stretch` includes a CDC Composite Gadget (CDC-ECM) natively recognized by macOS. In addition, the EV3 has no internal Wi-Fi and requires a Linux 4.4 compatible USB 2.0 Wi-Fi dongle.
  - **Fix Note:** Documented macOS CDC Composite Gadget network steps and supported USB Wi-Fi dongles in `PROJECT_PLAN.md` and `README.md`.

- [ ] **BUG-19: TTY Console Screen Contention with `getty@tty1` (Precautionary)**
  - **Status:** PENDING
  - **Severity:** Low
  - **File:** `ev3-web.service`, `PROJECT_PLAN.md`, `README.md`
  - **Description:** When `brickman` is disabled, Debian's `getty@tty1.service` may spawn a login prompt on `/dev/tty1`, contending with the on-brick LCD Wi-Fi menu and status display.
  - **Solution:** Mask or disable `getty@tty1.service` during boot optimization, and configure `ev3-web.service` with `TTYPath=/dev/tty1` and `TTYReset=yes`.

- [ ] **BUG-20: ConnMan Wi-Fi Password Provisioning Trap (Phase 7 Guardrail)**
  - **Status:** PENDING
  - **Severity:** High
  - **File:** `src/sysfs/wifi.rs`
  - **Description:** `connmanctl connect <service>` does not accept a password on the command line or via standard `stdin` pipes; interactive entry requires an asynchronous D-Bus agent loop (`agent on`).
  - **Solution:** Write a static ConnMan provisioning file directly to `/var/lib/connman/ev3_wifi.config` with `Type = wifi`, `Name = <SSID>`, and `Passphrase = <PASSWORD>` before running `connmanctl connect <service>`.

- [ ] **BUG-21: 32-Bit vs. 64-Bit Linux `input_event` Struct Size Trap (Phase 7 Guardrail)**
  - **Status:** PENDING
  - **Severity:** High
  - **File:** `src/sysfs/keypad.rs`
  - **Description:** On 64-bit hosts, `struct input_event` is 24 bytes, but on 32-bit `armv5te-unknown-linux-musleabi` (Linux kernel 4.4 on EV3), `timeval` uses two 32-bit integers, making `struct input_event` 16 bytes. Reading 24-byte chunks on the EV3 misaligns the byte stream and corrupts button events.
  - **Solution:** Use target-pointer-sized fields (`#[repr(C)]` with `c_long` or `size_of::<usize>()`) when reading `/dev/input/by-path/platform-gpio_keys-event`, and filter strictly for `type == 1 (EV_KEY)` and `value == 1 (KEY_PRESS)` to ignore key-release and key-bounce events.

- [ ] **BUG-22: `/dev/tty1` 22-Column Auto-Wrap & Row-16 Scroll Glitch (Phase 7 Guardrail)**
  - **Status:** PENDING
  - **Severity:** Medium
  - **File:** `src/sysfs/display.rs`
  - **Description:** The EV3 `fbcon` console (`178x128` pixels, 8x8 font) has 22 columns and 16 rows. Writing 22 characters followed by `\n` causes both an automatic terminal wrap at column 22 and an explicit newline, double-spacing lines and scrolling the top rows off the screen.
  - **Solution:** Restrict every printed line to **21 characters or fewer**, hide the blinking cursor (`\x1b[?25l`), and never write a trailing `\n` on the 16th row.


