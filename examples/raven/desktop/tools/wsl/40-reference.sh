#!/usr/bin/env bash
# Build the reference machine's reset shim.
#
#     bash tools/wsl/40-reference.sh
#
# QEMU's Versatile PB starts the processor at address zero and runs whatever is
# there. This board's own firmware is at the bottom of its flash and answers at
# zero through the static memory controller's remap bit, which QEMU does not
# model -- so the reference run cannot use `images/bootrom.bin`, and the raw
# kernel `Image` cannot be used either: QEMU loads a `-kernel` at 0x10000, and
# `arch/arm/kernel/head.S` works PHYS_OFFSET out from where it actually is
# (`adr_l r8, _text; sub r8, r8, #TEXT_OFFSET`), so 0x10000 tells the kernel
# that RAM starts at 0x8000 and it stops before it prints anything.
#
# `tools/ref/shim.S` is the twenty bytes that put QEMU's machine into the state
# this board's boot ROM leaves it in, so that the reference runs *this* board's
# memory image at *this* board's addresses. `tools/ref/run.sh` drives it.
#
# Run this after 30-rom.sh: it needs `images/bootrom.bin` only to know the
# images exist, and it writes `images/refshim.bin`.

set -euo pipefail

RAVEN_DESKTOP="${RAVEN_DESKTOP:-$HOME/raven-desktop}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"

TOOLCHAIN_DIR="$(find "$RAVEN_DESKTOP/toolchain" -maxdepth 2 -name bin -type d | head -1)"
if [ -z "$TOOLCHAIN_DIR" ]; then
    echo "no toolchain under $RAVEN_DESKTOP/toolchain; run 00-setup.sh first" >&2
    exit 2
fi
CROSS="$(find "$TOOLCHAIN_DIR" -maxdepth 1 -name '*-gcc' | head -1)"
CROSS="${CROSS%-gcc}-"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

"${CROSS}gcc" -c -march=armv5te -marm -nostdlib -fno-pic \
    -o "$WORK/shim.o" "$ROOT/tools/ref/shim.S"
"${CROSS}ld" -Ttext=0x00000000 --build-id=none -o "$WORK/shim.elf" "$WORK/shim.o"
"${CROSS}objcopy" -O binary "$WORK/shim.elf" "$ROOT/images/refshim.bin"
printf 'refshim    %6s bytes\n' "$(stat -c%s "$ROOT/images/refshim.bin")"
"${CROSS}objdump" -d "$WORK/shim.elf" | tail -n +5
