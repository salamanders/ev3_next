@echo off
where py >nul 2>nul
if %ERRORLEVEL% EQU 0 (
    py -3 "%~dp0zig-linker.py" %*
) else (
    python "%~dp0zig-linker.py" %*
)
