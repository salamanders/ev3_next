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
| BUG-23 | Deploy scripts fail on non-interactive `sudo` and hide the error | Critical | PENDING | CODE-CONFIRMED |
| BUG-24 | EV3 USB IP is not `192.168.2.2` by default; no static IP step | Critical | PENDING | PLATFORM-LIKELY |
| BUG-25 | ARM binary was built but never run (no QEMU / readelf check) | High | PENDING | CODE-CONFIRMED |
| BUG-26 | Unix, `cargo-zigbuild`, and `cross` build paths are broken (reopens BUG-09) | High | PENDING | CODE-CONFIRMED |
| BUG-27 | Python is a hidden prerequisite; WinGet Zig glob does not match | Medium | PENDING | CODE-CONFIRMED |

### Gate B: Fix Before Motors Move

| ID | Title | Severity | Status | Evidence |
| :--- | :--- | :--- | :--- | :--- |
| BUG-28 | Motors keep running after the server stops, crashes, or redeploys | Critical | PENDING | CODE-CONFIRMED |
| BUG-30 | CORS `*` + Private Network Access lets any website drive the robot (revert BUG-17) | High | PENDING | CODE-CONFIRMED |
| BUG-31 | Watchdog gaps (multi-client keep-alive, tab throttling, `hold` E-Stop, zero-speed run) | High | PENDING | CODE-CONFIRMED |
| BUG-46 | Real sysfs driver has zero tests; all tests use the mock | High | PENDING | CODE-CONFIRMED |
| BUG-47 | Remaining swallowed errors (Rule 3 violations) | Medium | PENDING | CODE-CONFIRMED |

### Gate C: Fix Before Phase 7 Keypad / Wi-Fi Work

| ID | Title | Severity | Status | Evidence |
| :--- | :--- | :--- | :--- | :--- |
| BUG-29 | No clean shutdown path after `brickman` is disabled | High | PENDING | PLATFORM-LIKELY |
| BUG-32 | LCD grid of 22x16 assumes an 8x8 font; the real font sets the grid | High | PENDING | PLATFORM-LIKELY |
| BUG-33 | EV3 button presses echo escape codes onto `/dev/tty1` | High | PENDING | PLATFORM-LIKELY |
| BUG-34 | Console blanking and kernel / systemd messages overwrite the LCD | Medium | PENDING | PLATFORM-LIKELY |
| BUG-35 | LCD IP and battery are read once at startup; hardcoded IP fallback | Medium | PENDING | CODE-CONFIRMED |

### Gate D: Performance, Robustness, and Docs

| ID | Title | Severity | Status | Evidence |
| :--- | :--- | :--- | :--- | :--- |
| BUG-36 | BUG-15 file descriptor cache is discarded every 2 seconds | Medium | PENDING | CODE-CONFIRMED |
| BUG-37 | `Motor::find_all()` runs on the HTTP thread for missing ports (Rule 6) | Medium | PENDING | CODE-CONFIRMED |
| BUG-38 | No HTTP request body size limit (OOM risk on 64 MB RAM) | Medium | PENDING | CODE-CONFIRMED |
| BUG-39 | `Cache-Control: max-age=3600` serves stale JS after deploy | Medium | PENDING | CODE-CONFIRMED |
| BUG-43 | Unmeasured performance claims are labeled as achieved | Medium | PENDING | CODE-CONFIRMED |
| BUG-44 | Wrong platform facts (kernel 4.4, USB 2.0 host, AP mode) | Medium | PENDING | PLATFORM-LIKELY |
| BUG-40 | `--poll-interval 0` causes a divide-by-zero panic | Low | PENDING | CODE-CONFIRMED |
| BUG-41 | Polarity is written twice, error is discarded, setting is not kept | Low | PENDING | CODE-CONFIRMED |
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

- [ ] **BUG-23: Deploy Scripts Fail on Non-Interactive `sudo` and Hide the Error**
  - **Status:** PENDING
  - **Severity:** Critical (blocks Phase 5 and all hardware work)
  - **Evidence:** CODE-CONFIRMED
  - **File:** `deploy.ps1` (lines 78-97), `deploy.sh` (lines 31-35)
  - **Description:**
    1. The scripts run `ssh host "sudo systemctl ..."` with no TTY. On stock ev3dev, `robot` must type a password for `sudo`. Without a TTY, `sudo` fails.
    2. `2>/dev/null || true` hides the stop failure (Rule 3). The service keeps running, and then `scp` fails with `Text file busy`.
    3. PowerShell `try/catch` does not catch native command exit codes. The `catch` block on line 81 never runs.
    4. No step sets up SSH keys, so each deploy asks for the password 3 times.
    5. The script prints "SUCCESS" even when the restart fails (line 95 only writes a warning).
  - **Solution (recommended):**
    1. Add a one-time setup step: install an SSH public key for `robot`.
    2. Add `/etc/sudoers.d/ev3-web` with a narrow rule, for example: `robot ALL=(root) NOPASSWD: /bin/systemctl stop ev3-web.service, /bin/systemctl restart ev3-web.service, /bin/mv /tmp/ev3-web-motor.new /home/robot/ev3-web-motor`.
    3. Upload to `/tmp/ev3-web-motor.new`, then use `mv` to replace the binary. A rename works while the old binary runs, so `Text file busy` cannot occur.
    4. Check `$LASTEXITCODE` after every native call. Stop with an error on failure.
    5. Run `systemctl is-active ev3-web.service` after the restart. Print "SUCCESS" only when it is active.
  - **Verification:** `ssh robot@<ip> sudo -n true` returns exit code 0. Two deploys in a row complete with no password prompt.

- [ ] **BUG-24: EV3 USB IP Is Not `192.168.2.2` by Default; No Static IP Step**
  - **Status:** PENDING
  - **Severity:** Critical (blocks Phase 1)
  - **Evidence:** PLATFORM-LIKELY
  - **File:** `PROJECT_PLAN.md` (Phase 1, Phase 2), `README.md`, `deploy.ps1`, `deploy.sh`, `src/sysfs/display.rs`
  - **Description:**
    1. All docs and scripts use `192.168.2.2`. Stock ev3dev-stretch gets its USB gadget address from DHCP, or from link-local (`169.254.x.x`), or from Windows ICS (`192.168.137.x`). ev3dev documents `ev3dev.local` (mDNS) as the usual way to connect.
    2. The usual way to find the IP is the brickman screen. Phase 2 disables brickman.
    3. The risk table in PROJECT_PLAN mentions "static IP setup", but no step sets a static IP.
    4. No step sets the Windows host adapter IP (`192.168.2.1/24`). Only macOS has a step.
  - **Solution (recommended):**
    1. On first boot, read the IP from the brickman screen or use `ssh robot@ev3dev.local`.
    2. **Before** you disable brickman, set a static IP: `connmanctl services` to get the gadget service ID, then `connmanctl config <gadget_service> --ipv4 manual 192.168.2.2 255.255.255.0`.
    3. Document the Windows host adapter settings (IP `192.168.2.1`, mask `255.255.255.0`, no gateway).
    4. In the deploy scripts, try `ev3dev.local` if `192.168.2.2` does not answer.
  - **Verification:** After a reboot with brickman disabled, `ping 192.168.2.2` works from Windows and macOS.

- [ ] **BUG-25: ARM Binary Was Built but Never Run**
  - **Status:** PENDING
  - **Severity:** High
  - **Evidence:** CODE-CONFIRMED (no test or log shows a run)
  - **File:** `PROJECT_PLAN.md` (Phase 0), `.cargo/config.toml`, `zig-linker.py`
  - **Description:** Phase 0 is marked "COMPLETE" because a 658 KB file exists. Nobody has run the file. Possible faults: `Illegal instruction`, wrong float ABI (VFP code on a CPU with no FPU), missing kernel atomic helpers, or wrong ELF flags from the custom `ld.lld` adapter.
  - **Solution (recommended):**
    1. Run `llvm-readelf -A -h` on the binary. Expect `Tag_CPU_arch: v5TE`, no `Tag_FP_arch`, ELF class 32, machine ARM, statically linked.
    2. Run the binary with `qemu-arm -cpu arm926 ./ev3-web-motor --mock --port 8080` (WSL or Linux) and call `curl /api/status`.
    3. Add both checks to Phase 0 and to the deploy scripts as an optional step.
  - **Verification:** The QEMU run serves `/api/status` and does not crash.

- [ ] **BUG-26: Unix, `cargo-zigbuild`, and `cross` Build Paths Are Broken**
  - **Status:** PENDING (reopens BUG-09)
  - **Severity:** High
  - **Evidence:** CODE-CONFIRMED
  - **File:** `deploy.sh` (line 18), `deploy.ps1` (lines 46-58), `.cargo/config.toml`, `Cross.toml`
  - **Description:**
    1. `deploy.sh` sets `RUSTFLAGS="-C linker=..."`. The `RUSTFLAGS` variable **replaces** the `rustflags` in `.cargo/config.toml`. This drops `linker-flavor=ld.lld` and `target-cpu=arm926ej-s`. rustc then sends gcc-style arguments to `ld.lld`, and the link fails.
    2. `cargo-zigbuild` and `cross` both still read `linker-flavor=ld.lld` and `linker = "zig-lld-arm.cmd"` from the config. The `.cmd` file does not run inside the Linux `cross` container.
    3. `cross` needs Docker. This breaks AGENTS.md Rule 7.
    4. `Cross.toml` uses the moving `:edge` image tag, so builds are not repeatable.
  - **Solution (recommended):**
    1. In `deploy.sh`, set `CARGO_TARGET_ARMV5TE_UNKNOWN_LINUX_MUSLEABI_LINKER="$SCRIPT_DIR/zig-lld-arm.sh"` instead of `RUSTFLAGS`.
    2. **[REJECTED]** the `cross` path: remove the `cross` branch and `Cross.toml`, or label them "unsupported".
    3. Remove the `cargo-zigbuild` branch, or test it with the config flags turned off.
  - **Verification:** A Linux or macOS host builds the binary with `./deploy.sh`, and the BUG-25 checks pass.

- [ ] **BUG-27: Python Is a Hidden Prerequisite; WinGet Zig Glob Does Not Match**
  - **Status:** PENDING
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `zig-lld-arm.cmd`, `zig-linker.py` (line 24), `PROJECT_PLAN.md` (Phase 0), `README.md`
  - **Description:**
    1. `zig-lld-arm.cmd` calls `python`. Phase 0 does not list Python. On a clean Windows machine, `python` can open the Microsoft Store stub, and the link fails with an unclear error.
    2. The fallback glob `Packages\*zig*\zig.exe` does not match the real nested path (`Packages\zig.zig_*\zig-windows-x86_64-*\zig.exe`). `recursive=True` has no effect without `**`.
  - **Solution (recommended):** Add "Install Python 3" to Phase 0. Try `py -3` before `python`. Change the glob to `Packages\*zig*\**\zig.exe`. Print a clear error message if Zig or Python is not found.
  - **Verification:** A clean Windows machine with only the documented prerequisites builds the binary.

### Gate B: Before Motors Move

- [ ] **BUG-28: Motors Keep Running After the Server Stops, Crashes, or Redeploys**
  - **Status:** PENDING
  - **Severity:** Critical (safety)
  - **Evidence:** CODE-CONFIRMED
  - **File:** `ev3-web.service`, `src/main.rs`, `src/controller.rs`, `deploy.ps1`, `deploy.sh`
  - **Description:**
    1. The kernel keeps `run-forever` and `run-direct` active after the process exits.
    2. `Cargo.toml` uses `panic = "abort"`, so no `Drop` code runs.
    3. The deploy scripts stop the service while a motor can be moving.
    4. `Restart=always` starts a new process with `continuous_run_active = false` and `tank_drive_active = false`. No watchdog watches the motor that is already running.
    5. The LEDs stay green after a crash, so the brick looks ready.
  - **Solution (recommended):**
    1. Add to `ev3-web.service`: `ExecStopPost=/bin/sh -c 'for m in /sys/class/tacho-motor/motor*; do echo reset > "$m/command"; done'`.
    2. Write `reset` to every motor when the program starts, before the HTTP server binds.
    3. Set the LEDs to red or off in `ExecStopPost`.
  - **Verification:** Start a motor with `run-forever`, then run `systemctl stop`, `kill -9`, and a deploy. The motor stops each time.

- [ ] **BUG-30: CORS `*` + Private Network Access Lets Any Website Drive the Robot**
  - **Status:** PENDING (revert BUG-17)
  - **Severity:** High (security and safety)
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/web/router.rs` (lines 27-35, 181-182, 192-193)
  - **Description:**
    1. Every response sends `Access-Control-Allow-Origin: *` and `Access-Control-Allow-Private-Network: true`. Any public web page that a user opens on the same network can send `POST /api/motor/...` to the brick.
    2. The dashboard is served from the brick itself (same origin), so it does not need CORS.
    3. The API has no authentication on Wi-Fi.
    4. SSH uses the default password `maker`, and `robot` has `sudo`.
  - **Solution (recommended):**
    1. Remove all CORS and PNA headers and the `OPTIONS` handler. **[REJECTED]** BUG-17.
    2. Reject `POST` requests that have an `Origin` header that does not match the `Host` header.
    3. Change the `robot` password in Phase 1. Use SSH key login (BUG-23).
    4. Optional: add a simple shared token for Wi-Fi use.
  - **Verification:** A `fetch()` from a page on a different origin cannot start a motor.

- [ ] **BUG-31: Watchdog Gaps**
  - **Status:** PENDING
  - **Severity:** High (safety)
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs` (lines 167-206, 375-390, 409-440), `web_assets/app.js`
  - **Description:**
    1. Tier 2 resets on **any** `/api/status` poll. A second open tab or phone keeps a lost client's motors running.
    2. Hidden browser tabs slow timers to about 1 Hz. This is the same as the 1000 ms timeout, so motors stop at random. A phone screen lock also stops them.
    3. Emergency Stop and the Tier 2 watchdog use stop action `hold`. `hold` keeps the motors powered and fighting the load with no time limit. This uses up the battery and heats the motors.
    4. `tank_drive(0, 0)` sends `run-forever` at speed 0 instead of `stop`. The motor stays in a powered "running" state.
    5. Each 150 ms heartbeat writes `stop_action`, `speed_sp`, and `command` for 2 motors (40 sysfs open/write/close per second). Sending `run-forever` again can restart speed regulation and cause jerky motion.
    6. The watchdog stop calls discard errors (see BUG-47).
  - **Solution (recommended):**
    1. Give each client a session ID. Tie each continuous run to the client session that started it.
    2. Use `brake` or `coast` for watchdog stops. Use `hold` only when the user asks for it.
    3. Send `stop` when both tank speeds are 0.
    4. On a heartbeat, write only `speed_sp` when the motor is already running at the same command.
    5. Document that a hidden tab stops continuous motors. **[ADOPTED]** This is the safe behavior.
  - **Verification:** Add unit tests for multi-client keep-alive, zero-speed tank drive, and watchdog stop action.

- [ ] **BUG-46: Real Sysfs Driver Has Zero Tests**
  - **Status:** PENDING
  - **Severity:** High
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/sysfs/motor.rs`, `src/sysfs/led.rs`, `src/controller.rs`
  - **Description:** All 13 tests use `MockController`. The real `Motor`, `LedController`, battery parser, port address parser (`ev3-ports:outA`), and file descriptor cache have no tests. Hardware is the first place these code paths run.
  - **Solution (recommended):** Make the sysfs base path configurable (for example, a `--sysfs-root` option or a constructor argument). Create a fake `/sys/class/tacho-motor/motor0/...` tree in a temp folder in tests. Test discovery, address parsing, clamping, command writes, polarity, `connected: false` when files are missing, and battery microvolt parsing. This follows the Host-First rule (Rule 4).
  - **Verification:** `cargo test` runs the real driver against the fake tree.

- [ ] **BUG-47: Remaining Swallowed Errors (Rule 3 Violations)**
  - **Status:** PENDING
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs` (lines 186-187, 204, 262), `src/web/router.rs` (lines 33, 183, 199), `deploy.ps1` (line 80), `deploy.sh` (lines 17, 29, 32)
  - **Description:** These lines discard errors with `let _ =` or `|| true`. This includes the watchdog safety stops, where a failure must be logged.
  - **Solution (recommended):** Log every error with `eprintln!` and the port or request path. In the scripts, stop on errors (see BUG-23).
  - **Verification:** `grep -n "let _ =" src` and `grep -n "|| true" deploy.*` return only lines that have a comment that explains why the error is safe to ignore.

### Gate C: Before Phase 7 Keypad / Wi-Fi Work

- [ ] **BUG-29: No Clean Shutdown Path After `brickman` Is Disabled**
  - **Status:** PENDING
  - **Severity:** High
  - **Evidence:** PLATFORM-LIKELY
  - **File:** `PROJECT_PLAN.md` (Phase 2, Phase 7), planned `src/sysfs/keypad.rs`, `src/web/router.rs`
  - **Description:** brickman supplies the shutdown menu. Without brickman, the only ways to turn off the brick are SSH or removing the batteries. Users will remove the batteries. This damages ext4 on the SD card, which is the failure that BUG-12 tries to reduce.
  - **Solution (recommended):**
    1. Make a long press (2 seconds) of the Back button run `systemctl poweroff`. Show "Shutting down" on the LCD and set the LEDs to amber.
    2. Add `POST /api/shutdown`, with a confirmation step in the UI.
    3. Implement this **before** you disable brickman on a brick that is used without a PC.
  - **Verification:** A long press of Back turns off the brick cleanly, and the next boot shows no fsck repairs.

- [ ] **BUG-32: LCD Grid of 22x16 Assumes an 8x8 Font**
  - **Status:** PENDING
  - **Severity:** High
  - **Evidence:** PLATFORM-LIKELY
  - **File:** `src/sysfs/display.rs` (lines 13-14), `ev3-web.service`
  - **Description:** The console grid depends on the loaded font. ev3dev uses the Terminus console fonts (for example, `Lat15-Terminus12x6` gives about 29 columns by 10 rows). Nobody has confirmed an 8x8 font. If the grid has fewer than 16 rows, the 16-row screen scrolls and the top rows disappear. The unit test only checks string lengths.
  - **Solution (recommended):**
    1. Add `ExecStartPre=/bin/setfont <chosen-font>` to the service, so the font is known.
    2. Read the real grid size at runtime with `ioctl(TIOCGWINSZ)` on `/dev/tty1`. Lay out the screen for that size.
    3. Write a layout that works on 10 rows as a fallback.
  - **Verification:** Run `stty -F /dev/tty1 size` on hardware. The ready screen fits with no scroll.

- [ ] **BUG-33: EV3 Button Presses Echo Escape Codes onto `/dev/tty1`**
  - **Status:** PENDING
  - **Severity:** High
  - **Evidence:** PLATFORM-LIKELY
  - **File:** planned `src/sysfs/keypad.rs`, `src/sysfs/display.rs`
  - **Description:** The `gpio_keys` driver also sends key events to the VT keyboard handler. `/dev/tty1` uses the default termios (ECHO on), so button presses print `^[[A`, newlines, and backspaces onto the LCD and scroll the screen.
  - **Solution (recommended):** Call `ioctl(EVIOCGRAB, 1)` on `/dev/input/by-path/platform-gpio_keys-event` after you open it. This stops the VT from getting the events. As a second layer, turn off `ECHO` and `ICANON` on `/dev/tty1`.
  - **Verification:** Press all 6 buttons many times. The LCD does not change except through the app.

- [ ] **BUG-34: Console Blanking and Kernel / systemd Messages Overwrite the LCD**
  - **Status:** PENDING
  - **Severity:** Medium
  - **Evidence:** PLATFORM-LIKELY
  - **File:** `ev3-web.service`, `src/sysfs/display.rs`, `REPORT.md`
  - **Description:**
    1. The kernel blanks the console after 10 minutes. REPORT.md says `setterm -blank 0` was "added to PROJECT_PLAN". It was not added, and the code does not do it.
    2. Kernel `printk` messages and systemd status lines also print on tty1 and write over the screen.
  - **Solution (recommended):** Write `\x1b[9;0]` to tty1 at startup (turns off blanking), or add `consoleblank=0` to the kernel command line. Run `dmesg -n 1` in `ExecStartPre`. Set `ShowStatus=no` in systemd. Redraw the full screen every few seconds.
  - **Verification:** The ready screen stays visible for 30 minutes, and a USB plug or unplug does not damage it.

- [ ] **BUG-35: LCD IP and Battery Are Read Once at Startup; Hardcoded IP Fallback**
  - **Status:** PENDING
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/main.rs` (lines 48-50), `src/sysfs/display.rs` (lines 24-30)
  - **Description:**
    1. `show_ready()` runs one time. The battery voltage on the LCD never changes.
    2. `detect_ip()` runs right after bind. `After=network.target` does not mean that USB or Wi-Fi has an address yet.
    3. If both probes fail, the LCD shows the hardcoded `192.168.2.2`. In standalone Wi-Fi mode, the user sees a URL that does not work.
    4. The LCD hint `Center: Wi-Fi Setup` names a feature that does not exist yet.
  - **Solution (recommended):** Add a display refresh loop (for example, every 5 seconds) that reads IP addresses per interface (`usb0`, `wlan0`) and the cached battery value. Show "No network" instead of a fake IP. Remove the Wi-Fi hint until Step 7.3 is done.
  - **Verification:** Plug in Wi-Fi after boot. The LCD shows the new `wlan0` URL within 10 seconds.

### Gate D: Performance, Robustness, and Docs

- [ ] **BUG-36: BUG-15 File Descriptor Cache Is Discarded Every 2 Seconds**
  - **Status:** PENDING
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs` (lines 141-145), `src/sysfs/motor.rs` (lines 68-125)
  - **Description:** The discovery cycle sets `*motors_guard = Motor::find_all()`. Each new `Motor` has a new, empty `CachedMotorFiles`, so all file descriptors close and open again every 2 seconds. Discovery also reads 6 attribute files per motor each time.
  - **Solution (recommended):** Keep an existing `Motor` when its `sysfs_path` is the same. Only add new paths and remove paths that are gone. Discovery can list the directory names and compare them first.
  - **Verification:** `ls -l /proc/<pid>/fd` shows the same descriptor numbers across 10 seconds.

- [ ] **BUG-37: `Motor::find_all()` Runs on the HTTP Thread for Missing Ports**
  - **Status:** PENDING
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs` (lines 216-223)
  - **Description:** When a port has no motor, `get_real_motor()` does a full directory scan on the HTTP thread on **every** request. This breaks Rule 6. Holding a D-Pad button on an empty port causes a scan every 150 ms.
  - **Solution (recommended):** Return "not connected" from the cache. Let only the poller thread do discovery.
  - **Verification:** Unit test with the fake sysfs tree (BUG-46): a command to an empty port does no file system access.

- [ ] **BUG-38: No HTTP Request Body Size Limit**
  - **Status:** PENDING
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/web/router.rs` (lines 171-175)
  - **Description:** `read_to_string` reads the full body into memory. One large `POST` can cause an Out-Of-Memory kill on a 64 MB device.
  - **Solution (recommended):** Reject requests with a `Content-Length` over 4 KB. Read with `.take(4096)`.
  - **Verification:** A 10 MB `POST` gets a `413` error, and RSS does not increase.

- [ ] **BUG-39: `Cache-Control: max-age=3600` Serves Stale JS After Deploy**
  - **Status:** PENDING
  - **Severity:** Medium
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/web/router.rs` (line 180)
  - **Description:** Browsers keep `index.html`, `app.js`, and `style.css` for 1 hour. After a deploy, the old JS can call API routes that changed. During fast iteration, this looks like "the fix did not work".
  - **Solution (recommended):** Use `Cache-Control: no-cache` and an `ETag` from the build version.
  - **Verification:** Deploy a visible UI change. A normal reload shows it.

- [ ] **BUG-40: `--poll-interval 0` Causes a Divide-by-Zero Panic**
  - **Status:** PENDING
  - **Severity:** Low
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs` (line 134), `src/config.rs`
  - **Description:** `2000 / self.poll_interval_ms` panics when the value is 0. With `panic = "abort"`, the process ends (see BUG-28).
  - **Solution (recommended):** Clamp the value to a range of 10 to 1000 ms in `config.rs`, and log the change.
  - **Verification:** Unit test for `--poll-interval 0`.

- [ ] **BUG-41: Polarity Is Written Twice, Error Is Discarded, Setting Is Not Kept**
  - **Status:** PENDING
  - **Severity:** Low
  - **Evidence:** CODE-CONFIRMED
  - **File:** `src/controller.rs` (lines 251-268)
  - **Description:** `set_polarity` writes sysfs on a clone, then writes it again on the cached `Motor` with `let _ =`. Polarity goes back to `normal` after a motor is plugged in again, after a reboot, or after the discovery cycle makes new `Motor` objects.
  - **Solution (recommended):** Write one time. Keep the polarity per port in the controller, and apply it again when a motor is discovered. Optional: save it to a small config file.
  - **Verification:** Set `inversed`, unplug and plug in the motor, and read the setting again.

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
