#!/usr/bin/env bash
# Assemble the boot ROM.
#
#     bash tools/wsl/30-rom.sh
#
# `tools/rom/*.S` is real ARM assembly and this is a real assembler: the same
# `arm-buildroot-linux-gnueabi` toolchain the kernel is built with. Nothing in
# this project hand-encodes an instruction, because an instruction written as a
# number is one nobody can read and the first version of this ROM got one wrong.
#
# The ROM has to know two things that are not code: where in the flash the
# kernel, the initramfs and the device tree are, and how big each of them is.
# The offsets and the destinations come from the board file -- the one place the
# layout is written down -- and the lengths come from `images/`, which is what
# the guest build produced. They are passed as assembler constants, so the ROM
# and the board file cannot disagree.
#
# Run this after 20-guest.sh: it needs the images to exist to know their sizes.

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

mkdir -p "$ROOT/images"

# The layout, from the board file, printed as assembler constants. Node is the
# only thing here that can read a `.mjs`, and it is the same file the build tool
# and the guest build read, so there is one answer to where anything is.
read -r KERNEL_OFFSET KERNEL_DEST INITRD_OFFSET INITRD_DEST DTB_OFFSET DTB_DEST <<EOF
$(node -e "
import('file://$ROOT/boards/versatile-pb.mjs').then((m) => {
    const l = m.flashLayout;
    const h = (n) => '0x' + n.toString(16);
    console.log([h(l.kernel.offset), h(l.kernel.dest), h(l.initrd.offset),
        h(l.initrd.dest), h(l.dtb.offset), h(l.dtb.dest)].join(' '));
});
")
EOF

# Lengths, rounded up to the block the copy loop moves. A block that runs a few
# bytes past the end of its image copies whatever is beside it in the flash, and
# nothing reads those bytes.
round_up() { echo $(( ( ($1 + 31) / 32 ) * 32 )); }
size_of() { if [ -f "$1" ]; then stat -c%s "$1"; else echo 0; fi; }

KERNEL_BYTES=$(round_up "$(size_of "$ROOT/images/kernel.img")")
INITRD_BYTES=$(round_up "$(size_of "$ROOT/images/initramfs.cpio.gz")")
DTB_BYTES=$(round_up "$(size_of "$ROOT/images/versatile-pb.dtb")")

echo "kernel  $KERNEL_OFFSET -> $KERNEL_DEST  $KERNEL_BYTES bytes"
echo "initrd  $INITRD_OFFSET -> $INITRD_DEST  $INITRD_BYTES bytes"
echo "dtb     $DTB_OFFSET -> $DTB_DEST  $DTB_BYTES bytes"

DEFINES="-DFLASH_KERNEL_OFFSET=$KERNEL_OFFSET -DFLASH_KERNEL_DEST=$KERNEL_DEST -DFLASH_KERNEL_BYTES=$KERNEL_BYTES"
DEFINES="$DEFINES -DFLASH_INITRD_OFFSET=$INITRD_OFFSET -DFLASH_INITRD_DEST=$INITRD_DEST -DFLASH_INITRD_BYTES=$INITRD_BYTES"
DEFINES="$DEFINES -DFLASH_DTB_OFFSET=$DTB_OFFSET -DFLASH_DTB_DEST=$DTB_DEST -DFLASH_DTB_BYTES=$DTB_BYTES"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

for source in "$ROOT"/tools/rom/*.S; do
    name="$(basename "$source" .S)"
    echo
    echo "assembling $name"
    # `-marm` because this core has no Thumb, and `-nostdlib` because there is
    # no library: this is the code that runs before there is anything.
    # shellcheck disable=SC2086
    "${CROSS}gcc" -c -march=armv5te -marm -nostdlib -fno-pic $DEFINES \
        -o "$WORK/$name.o" "$source"
    "${CROSS}ld" -Ttext=0x00000000 --build-id=none -o "$WORK/$name.elf" "$WORK/$name.o"
    "${CROSS}objcopy" -O binary "$WORK/$name.elf" "$ROOT/images/$name.bin"
    printf '%-10s %6s bytes\n' "$name" "$(stat -c%s "$ROOT/images/$name.bin")"
    # The alias window is thirty-two kilobytes. A ROM that does not fit under it
    # is a ROM that cannot answer at address zero, which is where the processor
    # starts.
    if [ "$(stat -c%s "$ROOT/images/$name.bin")" -gt 32768 ]; then
        echo "  the ROM is larger than the alias window" >&2
        exit 1
    fi
    "${CROSS}objdump" -d "$WORK/$name.elf" | sed -n '/<copy_block>:/,/^$/p'
done
