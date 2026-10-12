#!/usr/bin/env bash
# ARMv5te cross-compilation linker wrapper for Cargo (.cargo/config.toml).
# Uses Zig's bundled LLVM linker (zig ld.lld) so cross-compiling for the EV3
# does not require a separate GNU ARM toolchain or Docker container.
# Strips '-flavor gnu' which rustc passes when linker-flavor=ld.lld is set.

set -euo pipefail

ARGS=()
SKIP_NEXT=false
for arg in "$@"; do
    if [[ "${SKIP_NEXT}" == "true" ]]; then
        SKIP_NEXT=false
        continue
    fi
    if [[ "${arg}" == "-flavor" ]]; then
        SKIP_NEXT=true
        continue
    fi
    ARGS+=("${arg}")
done

ZIG_BIN="$(command -v zig || true)"
if [[ -z "${ZIG_BIN}" ]]; then
    for candidate in /opt/homebrew/bin/zig /usr/local/bin/zig "${HOME}/.local/bin/zig"; do
        if [[ -x "${candidate}" ]]; then
            ZIG_BIN="${candidate}"
            break
        fi
    done
fi

exec "${ZIG_BIN:-zig}" ld.lld -m armelf_linux_eabi "${ARGS[@]}"
