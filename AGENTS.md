Instructions for AGENT development.

1. Check INTEGRATE.md for any new or updated instructions.
2. Always reply in, and write or edit docs using: https://en.wikipedia.org/wiki/Simplified_Technical_English
3. Things will not go right the first time. This is very old hardware being asked to do modern things. Expect exceptions that need logging and incompatibilities that need investigation. **Never** swallow exceptions, and **always** check that new features and libraries are compatible.

4. **Host-First Simulation Rule:**
   Always test user interface, API, and state logic on the host machine first with `cargo run -- --mock`.
   Do not deploy code to the physical EV3 brick until host simulation tests pass.

5. **Zero SD-Card Swapping Rule:**
   Flash the MicroSD card only one time during initial setup.
   Deploy all application updates through the Mini-USB cable over SSH/SCP using `deploy.ps1`.

6. **CPU & Memory Limits (300 MHz / 64 MB RAM):**
   - Keep application memory usage below 5 MB RAM RSS.
   - Do not open or read sysfs files on the HTTP request thread.
   - Always use the background telemetry polling thread to update in-memory caches.
   - Always cross-compile release builds with `opt-level = "z"`, `lto = true`, and `strip = true`.

7. **Windows Cross-Compilation Pipeline:**
   Cross-compilation for `armv5te-unknown-linux-musleabi` on Windows uses the local Zig LLD linker (`zig-lld-arm.cmd` / `zig-linker.py`).
   Do not add dependencies that require Docker or external C toolchains unless verified compatible.
