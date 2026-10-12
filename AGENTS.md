Instructions for AGENT development.

1. Check INTEGRATE.md for any new or updated instructions.

2. Always reply in, and write or edit docs using: https://en.wikipedia.org/wiki/Simplified_Technical_English

3. Things will not go right the first time. This is very old hardware being asked to do modern things. Expect exceptions that need logging and incompatibilities that need investigation. **Never** swallow exceptions, and **always** check that new features and libraries are compatible.

4. **Host-First Simulation Rule:**
   Always test user interface, API, and state logic on the host machine first with `cargo run -- --mock` on default port `8080` (`http://localhost:8080/`).
   Never specify alternative or random port numbers.
   Do not deploy code to the physical EV3 brick until host simulation tests pass.

6. **CPU & Memory Limits (300 MHz / 64 MB RAM):**
   - Keep application memory usage low.
   - Do not open or read sysfs files on the HTTP request thread.
   - Always use the background telemetry polling thread to update in-memory caches.
   - Always cross-compile release builds with `opt-level = "z"`, `lto = true`, and `strip = true`.

7. **Cross-Compilation Pipeline:**
   Cross-compilation for `armv5te-unknown-linux-musleabi` on Linux uses Zig (`zig cc -target arm-linux-musleabi`) or standard musl toolchains.
   Do not add dependencies that require external C libraries unless verified compatible.

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
    - Always use GitHub Issues for tracking bugs, hardware investigations, and remediation.
    - Do not create or maintain local markdown bug tracker documents. Historical bug summaries are preserved in `PROJECT_PLAN.md`.

12. **Privacy & Personal Information Protection Rule:**
    - Never commit or push personal information, usernames, network names, or credentials to any remote Git repository (such as GitHub).
    - Never include personal identifiers, author names, account usernames, or personal username variations.
    - Never include Wi-Fi network names (SSIDs), passwords, home network information, or private IP addresses in committed files.
    - Always use generic placeholders (for example, `<SSID>`, `<PASSWORD>`, `<USERNAME>`) in all committed code, plan files, and documentation.

13. **Fixed Port Rule:**
    - Always use standard default port `8080` for host simulation (`http://localhost:8080/`).
    - Never select random, arbitrary, or incremented port numbers.
    - Always use port `80` on the physical EV3 brick (`http://ev3dev.local/`).

14. **No One-Off Inline Script Rule:**
    - Never execute inline scripts (`python3 -c`, `node -e`, or temporary scripts) to inspect logs, transcripts, or data. Inline scripts require manual user approval and block autonomous execution.
    - Always use standard tools (`grep`, `sed`, `awk`, `jq`, `view_file`) or existing project commands instead.

15. **Code Commenting & Consolidation Rule:**
    - Before documenting a file or function, verify that it needs to exist separately. Merge or inline single-caller wrappers and delete dead code.
    - Every file header and function comment must explain in plain Simplified Technical English: (a) what the code does in concrete terms, and (b) why it is a separate file or function (for example, shared by multiple callers, hardware performance limit, thread boundary, or unit-test seam).
    - Write these explanations as natural paragraphs. Do not repeat literal boilerplate headings such as `"What it does:"` or `"Why it is a separate function:"` in every comment.
