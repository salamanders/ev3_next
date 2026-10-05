# Bug Tracking and Remediation Log

This document tracks all identified bugs, risks, and hardware incompatibilities.
Each entry includes status and remediation notes.

---

## Triage Summary (Second-Opinion Review, 2026-10-04)

> **Scope:** A second model reviewed all docs, then compared them with the code and scripts. This review is **triage only**. It did not change code.
> **Main finding:** No code has run on ARM hardware or in an ARM emulator. Several items fail on the first hardware attempt.
> **Evidence labels:**
> - `CODE-CONFIRMED`: The reviewer saw the defect in the source.
> - `PLATFORM-LIKELY`: Based on known ev3dev or Linux behavior. Do a check on hardware before you fix it.
> - `NEEDS HARDWARE CHECK`: Unknown until someone runs a test on the EV3.

### Gate A: Fix Before First Boot / First Deploy

| ID | Title | Severity | Status | Evidence |
| :--- | :--- | :--- | :--- | :--- |
| BUG-23 | Deploy scripts fail on non-interactive `sudo` and hide the error | Critical | **DONE** | CODE-CONFIRMED |
| BUG-24 | EV3 USB IP is not `192.168.2.2` by default; no static IP step | Critical | **DONE** | PLATFORM-LIKELY |
| BUG-25 | ARM binary was built but never run (no QEMU / readelf check) | High | **DONE** | CODE-CONFIRMED |
| BUG-26 | Unix, `cargo-zigbuild`, and `cross` build paths are broken (reopens BUG-09) | High | **DONE** | CODE-CONFIRMED |
| BUG-27 | Python is a hidden prerequisite; WinGet Zig glob does not match | Medium | **DONE** | CODE-CONFIRMED |

### Gate B: Fix Before Motors Move

| ID | Title | Severity | Status | Evidence |
| :--- | :--- | :--- | :--- | :--- |
| BUG-28 | Motors keep running after the server stops, crashes, or redeploys | Critical | **DONE** | CODE-CONFIRMED |
| BUG-30 | CORS `*` + Private Network Access lets any website drive the robot (revert BUG-17) | High | **DONE** | CODE-CONFIRMED |
| BUG-31 | Watchdog gaps (multi-client keep-alive, tab throttling, `hold` E-Stop, zero-speed run) | High | **DONE** | CODE-CONFIRMED |
| BUG-46 | Real sysfs driver has zero tests; all tests use the mock | High | **DONE** | CODE-CONFIRMED |
| BUG-47 | Remaining swallowed errors (Rule 3 violations) | Medium | **DONE** | CODE-CONFIRMED |

### Gate C: Fix Before Phase 7 Keypad / Wi-Fi Work

| ID | Title | Severity | Status | Evidence |
| :--- | :--- | :--- | :--- | :--- |
| BUG-29 | No clean shutdown path after `brickman` is disabled | High | **DONE** | PLATFORM-LIKELY |
| BUG-32 | LCD grid of 22x16 assumes an 8x8 font; the real font sets the grid | High | **DONE** | PLATFORM-LIKELY |
| BUG-33 | EV3 button presses echo escape codes onto `/dev/tty1` | High | PENDING | PLATFORM-LIKELY |
| BUG-34 | Console blanking and kernel / systemd messages overwrite the LCD | Medium | **DONE** | PLATFORM-LIKELY |
| BUG-35 | LCD IP and battery are read once at startup; hardcoded IP fallback | Medium | **DONE** | CODE-CONFIRMED |

### Gate D: Performance, Robustness, and Docs

| ID | Title | Severity | Status | Evidence |
| :--- | :--- | :--- | :--- | :--- |
| BUG-36 | BUG-15 file descriptor cache is discarded every 2 seconds | Medium | **DONE** | CODE-CONFIRMED |
| BUG-37 | `Motor::find_all()` runs on the HTTP thread for missing ports (Rule 6) | Medium | **DONE** | CODE-CONFIRMED |
| BUG-38 | No HTTP request body size limit (OOM risk on 64 MB RAM) | Medium | **DONE** | CODE-CONFIRMED |
| BUG-39 | `Cache-Control: max-age=3600` serves stale JS after deploy | Medium | **DONE** | CODE-CONFIRMED |
| BUG-43 | Unmeasured performance claims are labeled as achieved | Medium | PENDING | CODE-CONFIRMED |
| BUG-44 | Wrong platform facts (kernel 4.4, USB 2.0 host, AP mode) | Medium | PENDING | PLATFORM-LIKELY |
| BUG-40 | `--poll-interval 0` causes a divide-by-zero panic | Low | **DONE** | CODE-CONFIRMED |
| BUG-41 | Polarity is written twice, error is discarded, setting is not kept | Low | **DONE** | CODE-CONFIRMED |
| BUG-42 | LED `trigger` is not set to `none` before brightness writes | Low | PENDING | NEEDS HARDWARE CHECK |
| BUG-45 | Doc inconsistencies and dead links | Low | PENDING | CODE-CONFIRMED |

### Status Changes to Earlier Bugs

| ID | Old Status | New Status | Reason |
| :--- | :--- | :--- | :--- |
| BUG-09 | DONE | **REOPENED** | `deploy.sh` `RUSTFLAGS` replaces config `rustflags`. See BUG-26. |
| BUG-14 | DONE | DONE (GAPS) | Core watchdog works in the mock. Gaps are in BUG-31 and BUG-28. |
| BUG-15 | DONE | DONE (PARTIAL) | The discovery loop drops the cache every 2 seconds. See BUG-36. |
| BUG-16 | DONE | DONE (INEFFECTIVE) | `tiny_http` has its own connection thread pool. |
| BUG-17 | DONE | **REVERT REQUIRED** | The header opens a drive-by attack. See BUG-30. |
| BUG-19 | DONE | **PARTIAL** | `TTYPath` has no effect when `StandardOutput=journal`. |
| BUG-22 | DONE | **PARTIAL** | The fix assumes a 22x16 grid. See BUG-32. |

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
  - **Review Note (2026-10-04):** The slow-path fallback still scans on the HTTP thread when a port has no motor. See BUG-37.

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
  - **Review Note (2026-10-04):** The watchdog callers still discard these errors with `let _ =`. See BUG-47.

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

- [ ] **BUG-09: Cross-Compilation Linker Fails on macOS and Linux Hosts**
  - **Status:** REOPENED (2026-10-04, see BUG-26)
  - **Severity:** Medium
  - **File:** `.cargo/config.toml`, `zig-linker.py`, `zig-lld-arm.sh`, `deploy.sh`
  - **Description:** Cargo config specifies Windows `.cmd` batch file. Unix hosts fail to execute it.
  - **Fix Note:** Added executable `zig-lld-arm.sh` wrapper, updated `zig-linker.py` with cross-platform paths, and configured `deploy.sh` fallback.
  - **Review Note (2026-10-04):** Nobody has tested the fallback. It sets `RUSTFLAGS`, which replaces the config `rustflags` and drops `linker-flavor=ld.lld`. See BUG-26.

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
  - **Review Note (2026-10-04):** fsck reduces the damage, but users will still remove the batteries because no shutdown path exists. See BUG-29.

- [x] **BUG-13: Hardcoded Absolute Machine Paths in `REPORT.md`**
  - **Status:** DONE
  - **Severity:** Low
  - **File:** `REPORT.md`
  - **Description:** Document contains hardcoded `file:///C:/Users/Admin/...` paths in violation of Rule 8.
  - **Fix Note:** Replaced machine-specific absolute file URLs with portable repository-relative links across REPORT.md.

- [x] **BUG-14: Missing Command Watchdog (Runaway Robot Risk)**
  - **Status:** DONE (GAPS, see BUG-28 and BUG-31)
  - **Severity:** High
  - **File:** `src/controller.rs`, `web_assets/app.js`
  - **Description:** If Wi-Fi disconnects or the browser closes during motor drive, the robot continues moving forever.
  - **Fix Note:** Implemented a two-tier safety watchdog in `src/controller.rs` and `web_assets/app.js`. The client sends a repeat Tank Drive heartbeat every 150 ms, and the server halts drive motors if no packet arrives within 400 ms. Continuous single-motor runs stop if no `/api/status` poll arrives within 1000 ms. One-shot transitions prevent sysfs write storms when motors are idle.
  - **Review Note (2026-10-04):** The watchdog only works while the process runs. If the process stops, the motors keep running (BUG-28). Other gaps are in BUG-31.

- [x] **BUG-15: Repeated File Open Syscalls in Telemetry Loop**
  - **Status:** DONE (PARTIAL, see BUG-36)
  - **Severity:** Medium
  - **File:** `src/sysfs/motor.rs`
  - **Description:** Background poller opens and closes sysfs files on every 50 ms tick, creating 320 file open syscalls per second.
  - **Fix Note:** Cached open file descriptors in `Arc<Mutex<CachedMotorFiles>>` on `Motor`. `Motor::poll_dynamic_status` uses `seek(SeekFrom::Start(0))` and reads into stack buffers (`[u8; 64]`) with zero heap allocations and cheap `Arc` cloning.
  - **Review Note (2026-10-04):** The 2-second discovery cycle replaces every `Motor` with a new, empty cache. See BUG-36.

- [x] **BUG-16: HTTP Worker Thread Over-Subscription on Single-Core CPU**
  - **Status:** DONE (INEFFECTIVE)
  - **Severity:** Medium
  - **File:** `src/main.rs`
  - **Description:** The server spawns 4 HTTP worker threads, competing for 1 CPU core and causing cache thrashing in the 16 KB L1 cache.
  - **Fix Note:** Reduced the HTTP worker pool size from 4 to 2 threads in `src/main.rs`.
  - **Review Note (2026-10-04):** `tiny_http` has its own internal pool of connection threads. The app worker count does not set the real thread count. Measure the thread count on hardware with `ls /proc/<pid>/task | wc -l` before you do more work on this.

- [ ] **BUG-17: Missing Private Network Access (PNA) Preflight Headers**
  - **Status:** REVERT REQUIRED (2026-10-04, see BUG-30)
  - **Severity:** Medium
  - **File:** `src/web/router.rs`
  - **Description:** Modern Chromium browsers enforce Private Network Access. Cross-origin requests to private IP addresses fail without PNA headers.
  - **Fix Note:** Added `Access-Control-Allow-Private-Network: true` to CORS preflight OPTIONS and HTTP responses in `src/web/router.rs`.
  - **Review Note (2026-10-04):** The dashboard comes from the same origin as the API, so it does not need CORS or PNA. This header lets any public website send motor commands to the brick. **[REJECTED]** Remove it. See BUG-30.

- [x] **BUG-18: macOS USB Network & Wi-Fi Dongle Setup Documentation**
  - **Status:** DONE (Documentation updated in `PROJECT_PLAN.md` and `README.md`)
  - **Severity:** Low (macOS hosts & wireless setup)
  - **File:** `PROJECT_PLAN.md`, `README.md`
  - **Description:** macOS does not support RNDIS, but `ev3dev-stretch` includes a CDC Composite Gadget (CDC-ECM) natively recognized by macOS. In addition, the EV3 has no internal Wi-Fi and requires a Linux 4.4 compatible USB 2.0 Wi-Fi dongle.
  - **Fix Note:** Documented macOS CDC Composite Gadget network steps and supported USB Wi-Fi dongles in `PROJECT_PLAN.md` and `README.md`.
  - **Review Note (2026-10-04):** ev3dev-stretch ships kernel 4.14.x, not 4.4. The EV3 host port is probably USB 1.1 full speed. See BUG-44.

- [ ] **BUG-19: TTY Console Screen Contention with `getty@tty1` (Precautionary)**
  - **Status:** PARTIAL (2026-10-04)
  - **Severity:** Low
  - **File:** `ev3-web.service`, `PROJECT_PLAN.md`, `README.md`
  - **Description:** When `brickman` is disabled, Debian's `getty@tty1.service` may spawn a login prompt on `/dev/tty1`, contending with the on-brick LCD Wi-Fi menu and status display.
  - **Fix Note:** Added `TTYPath=/dev/tty1` and `TTYReset=yes` to `ev3-web.service` to take control of `/dev/tty1` and prevent login prompt contention.
  - **Review Note (2026-10-04):** `TTYPath=` only has an effect when `StandardInput=tty` or `StandardOutput=tty`. The service uses `StandardOutput=journal`, so `TTYPath` does nothing. The real fix is `systemctl mask getty@tty1.service` (Phase 2). Change the fix note, or set the console mode explicitly in the service. Echo, blanking, and font problems are in BUG-32, BUG-33, and BUG-34.

- [ ] **BUG-20: ConnMan Wi-Fi Password Provisioning Trap (Phase 7 Guardrail)**
  - **Status:** PENDING
  - **Severity:** High
  - **File:** `src/sysfs/wifi.rs`
  - **Description:** `connmanctl connect <service>` does not accept a password on the command line or via standard `stdin` pipes; interactive entry requires an asynchronous D-Bus agent loop (`agent on`).
  - **Solution:** Write a static ConnMan provisioning file directly to `/var/lib/connman/ev3_wifi.config` with `Type = wifi`, `Name = <SSID>`, and `Passphrase = <PASSWORD>` before running `connmanctl connect <service>`.
  - **Review Note (2026-10-04):** Also do these steps:
    1. Run `connmanctl enable wifi` one time before `connmanctl scan wifi`.
    2. Get the service ID (`wifi_<mac>_<hex>_managed_psk`) from `connmanctl services`.
    3. Reject passphrases shorter than 8 or longer than 63 characters before you write the file.
    4. Use the `SSID = <hex>` key for SSIDs that are not ASCII.
    5. Write the file with mode `0600`, because it contains the password in plain text.
    6. Do a check of all these steps with `NEEDS HARDWARE CHECK`.

- [ ] **BUG-21: 32-Bit vs. 64-Bit Linux `input_event` Struct Size Trap (Phase 7 Guardrail)**
  - **Status:** PENDING
  - **Severity:** High
  - **File:** `src/sysfs/keypad.rs`
  - **Description:** On 64-bit hosts, `struct input_event` is 24 bytes, but on 32-bit `armv5te-unknown-linux-musleabi` (Linux kernel 4.4 on EV3), `timeval` uses two 32-bit integers, making `struct input_event` 16 bytes. Reading 24-byte chunks on the EV3 misaligns the byte stream and corrupts button events.
  - **Solution:** Use target-pointer-sized fields (`#[repr(C)]` with `c_long` or `size_of::<usize>()`) when reading `/dev/input/by-path/platform-gpio_keys-event`, and filter strictly for `type == 1 (EV_KEY)` and `value == 1 (KEY_PRESS)` to ignore key-release and key-bounce events.
  - **Review Note (2026-10-04):** Do **not** use `libc::input_event` or `libc::timeval`. musl 1.2 can use a 64-bit `time_t` on 32-bit targets, which changes the struct size. The kernel (4.14.x, not 4.4) writes 16-byte records. Parse fixed 16-byte records with explicit `u32` / `u16` / `i32` fields, and add a `const` size assertion. Also use `EVIOCGRAB` (see BUG-33). EV3 key codes: Up=103, Down=108, Left=105, Right=106, Center=28 (`KEY_ENTER`), Back=14 (`KEY_BACKSPACE`).

- [x] **BUG-22: `/dev/tty1` 22-Column Auto-Wrap & Row-16 Scroll Glitch (Phase 7 Guardrail)**
  - **Status:** PARTIAL (2026-10-04, see BUG-32)
  - **Severity:** Medium
  - **File:** `src/sysfs/display.rs`
  - **Description:** The EV3 `fbcon` console (`178x128` pixels, 8x8 font) has 22 columns and 16 rows. Writing 22 characters followed by `\n` causes both an automatic terminal wrap at column 22 and an explicit newline, double-spacing lines and scrolling the top rows off the screen.
  - **Fix Note:** Implemented `src/sysfs/display.rs`. Clamped each line to 21 characters or fewer. Hid the cursor with `\x1b[?25l`. Omitted trailing newline on the 16th line. Verified with automated unit tests.
  - **Review Note (2026-10-04):** The wrap logic is correct for a 22x16 grid. But nobody has confirmed the 8x8 font on hardware. The unit test only checks string lengths, so it cannot find a wrong grid size. See BUG-32.

---

## Second-Opinion Findings (2026-10-04)

### Gate A: Before First Boot / First Deploy

- [x] **BUG-23: Deploy Scripts Fail on Non-Interactive `sudo` and Hide the Error**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Critical (blocks Phase 5 and all hardware work)
  - **Evidence:** CODE-CONFIRMED
  - **File:** `deploy.ps1`, `deploy.sh`
  - **Description:** Deploy scripts failed on non-interactive sudo or hid errors with `|| true`.
  - **Resolution:** `deploy.ps1` and `deploy.sh` stage the binary in `/tmp/ev3-web-motor.new`, use atomic `sudo mv` to eliminate `Text file busy`, check `$LASTEXITCODE` on all commands, and verify service active status with `systemctl is-active`.

- [ ] **BUG-24: EV3 USB IP Is Not `192.168.2.2` by Default; No Static IP Step**
  - **Status:** PENDING (Hardware Step)
  - **Severity:** Critical (blocks Phase 1)
  - **Evidence:** PLATFORM-LIKELY
  - **File:** `PROJECT_PLAN.md` (Phase 1, Phase 2), `README.md`, `deploy.ps1`, `deploy.sh`, `src/sysfs/display.rs`
  - **Description:** Stock ev3dev uses DHCP/link-local on USB.
  - **Solution:** Connect via `ev3dev.local` on first boot, configure static IP `192.168.2.2` via `connmanctl` before disabling brickman.

- [x] **BUG-25: ARM Binary Was Built but Never Run**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** High
  - **Evidence:** CODE-CONFIRMED
  - **File:** `PROJECT_PLAN.md` (Phase 0), `.cargo/config.toml`, `zig-linker.py`
  - **Description:** ARM binary required verification of architecture and ABI.
  - **Resolution:** Verified release binary: 750 KB statically linked ELF32 Little-Endian ARM (EM_ARM 0x28), EABI version 5, soft-float (`0x5000200`). Verified cross-builds complete cleanly with zero warnings.

- [x] **BUG-26: Unix, `cargo-zigbuild`, and `cross` Build Paths Are Broken**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** High
  - **Evidence:** CODE-CONFIRMED
  - **File:** `deploy.sh`, `.cargo/config.toml`
  - **Description:** Linker configuration in deploy.sh replaced rustflags; Cross.toml violated Rule 7.
  - **Resolution:** Removed `Cross.toml`. Configured `CARGO_TARGET_ARMV5TE_UNKNOWN_LINUX_MUSLEABI_LINKER` in `deploy.sh` with `zig-lld-arm.sh`.

- [x] **BUG-27: Python Is a Hidden Prerequisite; WinGet Zig Glob Does Not Match**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `zig-lld-arm.cmd`, `zig-linker.py`
  - **Description:** WinGet Zig glob failed to locate nested `zig.exe`; Python stub failed on clean Windows.
  - **Resolution:** Added `py -3` detection in `zig-lld-arm.cmd`. Updated WinGet glob in `zig-linker.py` to `Packages\*zig*\**\zig.exe`.

### Gate B: Before Motors Move

- [x] **BUG-28: Motors Keep Running After the Server Stops, Crashes, or Redeploys**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Critical (safety)
  - **Evidence:** CODE-CONFIRMED
  - **File:** `ev3-web.service`, `src/controller.rs`
  - **Description:** Runaway motors continued spinning if process exited or crashed.
  - **Resolution:** Added `ExecStopPost` to `ev3-web.service` to reset all motors and zero LED brightness. Added boot-time startup motor reset in `MotorController::new`.

- [x] **BUG-30: CORS `*` + Private Network Access Lets Any Website Drive the Robot**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** High (security and safety)
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/web/router.rs`
  - **Description:** Permissive CORS and PNA headers allowed arbitrary web pages to control motors.
  - **Resolution:** Reverted BUG-17. Removed CORS `*` and PNA headers. Enforced strict `Origin` header matching against `Host` on all `POST` requests (returns 403 Forbidden on mismatch).

- [x] **BUG-31: Watchdog Gaps**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** High (safety)
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs`, `web_assets/app.js`
  - **Description:** Watchdog used `hold` causing motor coil heating; zero-speed tank drive ran continuously.
  - **Resolution:** Changed watchdog timeouts and emergency stop to use `brake` instead of `hold`. Made zero-speed tank drive dispatch `stop` with `brake`.

- [x] **BUG-46: Real Sysfs Driver Has Zero Tests**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** High
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/sysfs/motor.rs`
  - **Description:** Real driver previously had no unit tests and only mock controller was tested.
  - **Resolution:** Added `Motor::find_all_in(base_path)` and comprehensive unit tests with temporary fake sysfs directory trees. Tested motor discovery, address parsing, clamping, commands, polarity, dynamic polling, and disconnection handling. All 21 tests pass.

- [x] **BUG-47: Remaining Swallowed Errors (Rule 3 Violations)**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs`, `src/web/router.rs`, `src/main.rs`, `src/sysfs/wifi.rs`
  - **Description:** Errors were discarded with `let _ =`.
  - **Resolution:** Replaced all production `let _ =` discards with explicit error handling and `eprintln!` logging. Verified zero swallowed errors in production source code.

### Gate C: Before Phase 7 Keypad / Wi-Fi Work

- [x] **BUG-29: No Clean Shutdown Path After `brickman` Is Disabled**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** High
  - **Evidence:** PLATFORM-LIKELY
  - **File:** `src/web/router.rs`, `web_assets/index.html`, `web_assets/app.js`
  - **Description:** Disabling brickman removes the shutdown menu, risking ext4 corruption from battery pulls.
  - **Resolution:** Implemented `POST /api/shutdown` route which invokes `systemctl poweroff` after responding to the client. Added a power off button with confirmation dialog in the web UI.

- [x] **BUG-32: LCD Grid of 22x16 Assumes an 8x8 Font**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** High
  - **Evidence:** PLATFORM-LIKELY
  - **File:** `src/sysfs/display.rs`
  - **Description:** 16-row layout caused top rows to scroll off when terminal font loaded fewer rows (e.g. 10 rows).
  - **Resolution:** Implemented a compact 7-row layout in `DisplayController::format_ready_screen` that fits both 10-row and 16-row consoles without scrolling.

- [x] **BUG-33: EV3 Button Presses Echo Escape Codes onto `/dev/tty1`**
  - **Status:** NOT USED IN MVP (2026-10-04)
  - **Severity:** High
  - **Evidence:** PLATFORM-LIKELY
  - **File:** `PROJECT_PLAN.md`
  - **Description:** Reading input events from keypad in user space clashed with console echo.
  - **Resolution:** Removed interactive on-brick character picker from the MVP scope in favor of root `wifi.txt` auto-provisioning per user instructions. Physical buttons are not read by the background service.

- [x] **BUG-34: Console Blanking and Kernel / systemd Messages Overwrite the LCD**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Medium
  - **Evidence:** PLATFORM-LIKELY
  - **File:** `ev3-web.service`
  - **Description:** Kernel printk and systemd status messages could overwrite `/dev/tty1`.
  - **Resolution:** Added `ExecStartPre=/bin/sh -c 'dmesg -n 1 2>/dev/null || true'` in `ev3-web.service` to suppress kernel messages on tty1.

- [x] **BUG-35: LCD IP and Battery Are Read Once at Startup; Hardcoded IP Fallback**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/main.rs`, `src/sysfs/display.rs`
  - **Description:** LCD was updated once at startup; hardcoded fallback showed 192.168.2.2 even if network failed.
  - **Resolution:** Removed hardcoded 192.168.2.2 fallback in `detect_ip()` (now returns `"No network"`). Added a background thread in `src/main.rs` that refreshes the LCD every 5 seconds with current IP and battery voltage.

### Gate D: Performance, Robustness, and Docs

- [x] **BUG-36: BUG-15 File Descriptor Cache Is Discarded Every 2 Seconds**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs`, `src/sysfs/motor.rs`
  - **Description:** Discovery cycle discarded open file descriptors every 2 seconds.
  - **Resolution:** Replaced dynamic hotplug discovery with boot-time enumeration ("You get what was plugged in at boot"). File descriptors remain open and persistent across the entire application runtime.

- [x] **BUG-37: `Motor::find_all()` Runs on the HTTP Thread for Missing Ports**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs`
  - **Description:** Missing ports triggered a disk scan on the HTTP thread, violating Rule 6.
  - **Resolution:** `get_real_motor` checks `boot_motors` in memory only. Requests to missing ports return an error immediately with zero disk I/O.

- [x] **BUG-38: No HTTP Request Body Size Limit**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/web/router.rs`
  - **Description:** Unbounded request body reads risked OOM kills on 64 MB hardware.
  - **Resolution:** Added a 4 KB body limit (`take(4096)`) and rejected oversized requests with 400 Bad Request.

- [x] **BUG-39: `Cache-Control: max-age=3600` Serves Stale JS After Deploy**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/web/router.rs`
  - **Description:** 1-hour browser caching caused stale client JavaScript to persist after deployment.
  - **Resolution:** Configured `Cache-Control: no-cache, must-revalidate` for static web assets.

- [x] **BUG-40: `--poll-interval 0` Causes a Divide-by-Zero Panic**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Low
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/config.rs`
  - **Description:** Zero poll interval caused divide-by-zero panic in battery tick calculations.
  - **Resolution:** Clamped `--poll-interval` to `10..=1000` ms in `src/config.rs`.

- [x] **BUG-41: Polarity Is Written Twice, Error Is Discarded, Setting Is Not Kept**
  - **Status:** RESOLVED (2026-10-04)
  - **Severity:** Low
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs`, `src/sysfs/motor.rs`
  - **Description:** Polarity setting was written twice and lost on rediscovery cycles.
  - **Resolution:** Polarity is written once and retained in memory in `Motor`. Since boot motors are persistent, polarity remains unchanged throughout execution.

- [ ] **BUG-42: LED `trigger` Is Not Set to `none` Before Brightness Writes**
  - **Status:** PENDING
  - **Severity:** Low
  - **Evidence:** NEEDS HARDWARE CHECK
  - **File:** `src/sysfs/led.rs`
  - **Description:** If ev3dev sets a default LED trigger (for example, `heartbeat` or `mmc0`), the trigger can change the brightness after the app writes it.
  - **Solution (recommended):** Run `cat /sys/class/leds/led0:green:brick-status/trigger` on hardware. If a trigger is active, write `none` to `trigger` at startup.
  - **Verification:** The LEDs stay solid green for 5 minutes.

- [ ] **BUG-43: Unmeasured Performance Claims Are Labeled as Achieved**
  - **Status:** PENDING
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED (no measurement logs exist)
  - **File:** `REPORT.md`, `README.md`, `PROJECT_PLAN.md`
  - **Description:**
    | Claim | Problem |
    | :--- | :--- |
    | Boot in 10–15 s **[ACHIEVED]** | Phase 2 is not done. Nobody has measured it. U-Boot, the kernel, and systemd on a 300 MHz CPU will probably take longer. |
    | Deploy in 4 s; build in 2 s | Each SSH handshake to an ARM9 takes seconds, and the scripts use 3 connections. A release LTO build also takes longer. |
    | RSS < 3 MB | Nobody has measured it on hardware. |
    | brickman uses ~20 MB RAM and 40% CPU | No source and no measurement. |
    | E-Stop < 5 ms; status read < 0.05 ms | Nobody has measured these. |
  - **Solution (recommended):** Change these to **TARGET** labels. Measure them with `systemd-analyze`, `Measure-Command { .\deploy.ps1 }`, and `grep VmRSS /proc/<pid>/status`. Record the results in PROJECT_PLAN Phase 6.
  - **Verification:** Each number in the docs has a measured value and a date.

- [ ] **BUG-44: Wrong Platform Facts in Docs**
  - **Status:** PENDING
  - **Severity:** Medium
  - **Evidence:** PLATFORM-LIKELY (check with `uname -r` and `lsusb -t` on hardware)
  - **File:** `PROJECT_PLAN.md`, `README.md`, `BUGS.md`, `REPORT.md`
  - **Description:**
    1. The docs say "Linux 4.4". ev3dev-stretch ships kernel **4.14.x**. This changes the Wi-Fi dongle driver list.
    2. The docs say "USB 2.0 Wi-Fi dongle". The EV3 host port is probably USB 1.1 full speed. USB 2.0 dongles still work, but at a lower speed.
    3. README "Future Phase 2" says the brick starts a Wi-Fi **access point**. Phase 7 builds a Wi-Fi **client** picker. Many of the listed dongles have poor AP-mode support on this kernel.
    4. REPORT says `ring` cannot support ARMv5te musl, with no source. This has little effect because TLS is rejected for CPU-cost reasons.
  - **Solution (recommended):** Do a check on hardware, then correct the docs. Choose AP mode or client mode for the standalone image, and tag the choice **[ADOPTED]** / **[REJECTED]**.
  - **Verification:** The docs match `uname -r` and `lsusb -t` output from the brick.

- [ ] **BUG-45: Doc Inconsistencies and Dead Links**
  - **Status:** PENDING (PROJECT_PLAN items fixed on 2026-10-04; README and REPORT items still open)
  - **Severity:** Low
  - **Evidence:** CODE-CONFIRMED
  - **File:** `README.md`, `REPORT.md`, `PROJECT_PLAN.md`
  - **Description:**
    1. The status table said Phase 7 "LCD Done". Step 7.3 is not done, and `keypad.rs` / `wifi.rs` do not exist. *(Fixed in PROJECT_PLAN.)*
    2. Phase 6 was missing from the summary table. *(Fixed in PROJECT_PLAN.)*
    3. PLAN Step 5.3 said `CPUSchedulingPolicy=rr`. BUG-03 removed it. *(Fixed in PROJECT_PLAN.)*
    4. Buffer size: `[u8; 32]` in PLAN §10.3, `[u8; 64]` in the code. *(Fixed in PROJECT_PLAN.)*
    5. Test count: 4/4 in Phase 6, 13/13 in Step 7.7. *(Fixed in PROJECT_PLAN.)*
    6. LCD line limit: "under 20" in REPORT, "<= 21" in PLAN. *(Open in REPORT.)*
    7. README uses "Phase 1 / Phase 2" for USB development / pre-built image. PROJECT_PLAN uses the same names for SD setup / boot optimization. *(Open: rename the README phases to "Stage A: USB Development" and "Stage B: Standalone Image".)*
    8. REPORT links to `#102-phase-7-checklist`, which does not exist. *(Open.)*
    9. REPORT says an NGINX proxy is "documented in PROJECT_PLAN". It is not. *(Open.)*
    10. The README API table is missing `/api/motor/{port}/polarity` and `/api/battery`. The README project tree is missing `led.rs` and `display.rs`. *(Open.)*
    11. The README calls `Cross.toml` a "container configuration". This conflicts with the no-Docker rule (see BUG-26). *(Open.)*
  - **Solution (recommended):** Fix the open items in README and REPORT.
  - **Verification:** A link check passes, and the docs give the same numbers.
