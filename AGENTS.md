Instructions for AGENT development.

1. Check INTEGRATE.md for any new or updated instructions.

2. Always reply in, and write or edit docs using: https://en.wikipedia.org/wiki/Simplified_Technical_English

3. Things will not go right the first time. This is very old hardware being asked to do modern things. Expect exceptions that need logging and incompatibilities that need investigation. **Never** swallow exceptions, and **always** check that new features and libraries are compatible.

4. **Host-First Simulation Rule:**
   Always test user interface, API, and state logic on the host machine first with `cargo run -- --mock` on default port `8080` (`http://localhost:8080/`).
   Never specify alternative or random port numbers.
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

8. **Generic Environment & Paths Rule:**
   - Never write hardcoded user profiles or machine-specific absolute paths (for example, `C:\Users\Admin`).
   - Always use dynamic environment variables and discovery tools (for example, `$env:LOCALAPPDATA`, `%LOCALAPPDATA%`, `shutil.which()`, `$PSScriptRoot`, `%~dp0`).

9. **Plan Checklists & Decision Tagging Rule:**
   - Always maintain `PROJECT_PLAN.md` with explicit checklists (`[x]` completed vs `[ ]` pending) and a summary table so any agent can immediately resume work without losing context.
   - Tag all architectural options in technical reports with clear status labels: `[ADOPTED]`, `[IMPLEMENTED]`, `[REJECTED]`, or `[NOT USED in Phase X]` with exact file links and technical rationales.

10. **External Link & Asset Verification Rule:**
    - Never assume external download URLs, release tag names, or file extensions from legacy notes or memory.
    - Always verify that external links and asset names are live and correct before documenting them.

11. **GitHub Issue & Bug Tracking Rule:**
    - Always use GitHub Issues for tracking bugs, hardware investigations, and remediation moving forward.
    - Do not create or maintain local markdown bug tracker documents. Closed historical bugs are preserved in `archive/BUGS.md`.

12. **Privacy & Personal Information Protection Rule:**
    - Never commit or push personal information, usernames, network names, or credentials to any remote Git repository (such as GitHub).
    - Never include personal identifiers, author names, account usernames, or personal username variations.
    - Never include Wi-Fi network names (SSIDs), passwords, home network information, or private IP addresses in committed files.
    - Always use generic placeholders (for example, `<SSID>`, `<PASSWORD>`, `<USERNAME>`) in all committed code, plan files, and documentation.

13. **Fixed Port Rule:**
    - Always use standard default port `8080` for host simulation (`http://localhost:8080/`).
    - Never select random, arbitrary, or incremented port numbers.
    - Always use port `80` on the physical EV3 brick (`http://ev3dev.local/`).

