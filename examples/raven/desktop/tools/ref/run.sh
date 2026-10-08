#!/usr/bin/env bash
# Boot this board's guest on the reference machine.
#
#     bash tools/ref/run.sh [seconds]
#
# QEMU's Versatile PB, driven from `images/flash.bin`'s own layout and started by
# `images/refshim.bin`, so it runs exactly the memory image this project's boot
# ROM builds: the kernel at `flashLayout.kernel.dest`, the initramfs at
# `flashLayout.initrd.dest`, the device tree at `flashLayout.dtb.dest`, and the
# same value in `r2`.
#
# The point of it is to answer one question cheaply -- "is the guest good, or is
# the emulator?" -- and to give a trace to hold the emulator's own against
# (`tools/cpu-trace.mjs`). Linux reaches `/init` here in about a second, which
# is the whole reason this exists: the project's own machine is five thousand
# instructions a second and a boot is hours.
#
# QEMU is a downloaded, unpacked `qemu-system-arm`; see the README for how it is
# fetched. Nothing here writes into the project.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
QEMU_DIR="${QEMU_DIR:-$HOME/raven-desktop/qemu}"
SECONDS_TO_RUN="${1:-60}"

QEMU="$QEMU_DIR/root/usr/bin/qemu-system-arm"
if [ ! -x "$QEMU" ]; then
    echo "no qemu-system-arm at $QEMU" >&2
    echo "unpack it there, or set QEMU_DIR" >&2
    exit 2
fi

for image in kernel.img initramfs.cpio.gz versatile-pb.dtb refshim.bin; do
    if [ ! -f "$ROOT/images/$image" ]; then
        echo "missing images/$image; run the guest build and 40-reference.sh" >&2
        exit 2
    fi
done

export LD_LIBRARY_PATH="$QEMU_DIR/root/usr/lib/x86_64-linux-gnu"
export QEMU_AUDIO_DRV=none

# The three destinations come from the board file, so the reference cannot
# disagree with the machine about where anything goes.
read -r KERNEL_DEST INITRD_DEST DTB_DEST <<EOF
$(node -e "
import('file://$ROOT/boards/versatile-pb.mjs').then((m) => {
    const l = m.flashLayout;
    const h = (n) => '0x' + n.toString(16);
    console.log([h(l.kernel.dest), h(l.initrd.dest), h(l.dtb.dest)].join(' '));
});
")
EOF

exec timeout "$SECONDS_TO_RUN" "$QEMU" -M versatilepb -m 16M -nographic -no-reboot \
    -device loader,file="$ROOT/images/refshim.bin",addr=0x0,force-raw=on \
    -device loader,file="$ROOT/images/kernel.img",addr="$KERNEL_DEST",force-raw=on \
    -device loader,file="$ROOT/images/initramfs.cpio.gz",addr="$INITRD_DEST",force-raw=on \
    -device loader,file="$ROOT/images/versatile-pb.dtb",addr="$DTB_DEST",force-raw=on
