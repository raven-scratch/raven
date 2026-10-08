#!/usr/bin/env bash
# Build the rest of the guest: BusyBox, an initramfs, and the device tree.
#
#     bash tools/wsl/20-guest.sh          # after 10-kernel.sh
#
# Four things come out of this, and `tools/build.mjs` puts all four into the
# machine's flash chip in the order the boot ROM expects to find them:
#
#   images/kernel.img          the kernel, from 10-kernel.sh
#   images/initramfs.cpio.gz   a root filesystem with a shell in it
#   images/versatile-pb.dtb    the machine, described to the kernel
#   images/bootrom.bin         the firmware, from 05-rom.sh
#
# The initramfs is a cpio archive the kernel unpacks into a ramfs and then runs
# `/init` from. There is no disk: this machine's storage is the flash chip, and
# the flash chip holds the kernel and the initramfs and nothing else. It is
# exactly the shape an embedded Linux has, which is what this is.
#
# The BusyBox is static. A dynamic one would need a loader, a libc and a
# `/lib` full of shared objects in the image, which is three times the bytes for
# the same shell.

set -euo pipefail

RAVEN_DESKTOP="${RAVEN_DESKTOP:-$HOME/raven-desktop}"
KERNEL_VERSION="${KERNEL_VERSION:-6.6.158}"
BUSYBOX_VERSION="${BUSYBOX_VERSION:-1.36.1}"
KERNEL="$RAVEN_DESKTOP/src/linux-$KERNEL_VERSION"
BUSYBOX="$RAVEN_DESKTOP/src/busybox-$BUSYBOX_VERSION"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
REPO="$(cd "$ROOT/../../.." && pwd)"

TOOLCHAIN_DIR="$(find "$RAVEN_DESKTOP/toolchain" -maxdepth 2 -name bin -type d | head -1)"
CROSS="$(find "$TOOLCHAIN_DIR" -maxdepth 1 -name '*-gcc' | head -1)"
CROSS="${CROSS%-gcc}-"

mkdir -p "$ROOT/images"

# ---------------------------------------------------------------------------
# The root filesystem
# ---------------------------------------------------------------------------

echo "=== busybox"
cd "$BUSYBOX"
make ARCH=arm CROSS_COMPILE="$CROSS" defconfig >/dev/null
sed -i 's/^# CONFIG_STATIC is not set/CONFIG_STATIC=y/' .config
sed -i 's/^CONFIG_TC=y/# CONFIG_TC is not set/' .config
sed -i 's/^CONFIG_FEATURE_SYNC_FANCY=y/# CONFIG_FEATURE_SYNC_FANCY is not set/' .config
make ARCH=arm CROSS_COMPILE="$CROSS" -j"$(nproc)" >/dev/null 2>&1
echo "busybox    $(stat -c%s busybox) bytes"

ROOTFS="$RAVEN_DESKTOP/rootfs"
rm -rf "$ROOTFS"
mkdir -p "$ROOTFS"/{bin,sbin,etc,proc,sys,dev,tmp,usr/bin,usr/sbin}
make ARCH=arm CROSS_COMPILE="$CROSS" CONFIG_PREFIX="$ROOTFS" install >/dev/null

# The init script. `devtmpfs` is mounted by the kernel because the guest kernel
# was built with DEVTMPFS_MOUNT; proc and sysfs are the shell's own business.
cat > "$ROOTFS/init" <<'INIT'
#!/bin/sh
#
# The first thing the kernel runs. It is a shell, and what it does is mount the
# two filesystems a shell needs and then become one -- and it prints a line
# first, so that a boot which reaches userspace says so on the console the
# Stage is showing.

mount -t proc none /proc
mount -t sysfs none /sys
mount -t tmpfs none /tmp

echo
echo "raven desktop -- the machine is up"
echo
uname -a
echo
echo "the console you are reading is the LCD controller's scanout:"
echo "the kernel drew these glyphs into the framebuffer the controller"
echo "is reading, and the Stage is the controller's output."
echo

exec /bin/sh
INIT
chmod +x "$ROOTFS/init"
ln -sf /bin/busybox "$ROOTFS/linuxrc"

# ---------------------------------------------------------------------------
# The archive
# ---------------------------------------------------------------------------

echo
echo "=== initramfs"
python3 "$HERE/mkcpio.py" "$ROOTFS" "$RAVEN_DESKTOP/initramfs.cpio"
python3 - "$RAVEN_DESKTOP/initramfs.cpio" "$ROOT/images/initramfs.cpio.gz" <<'PY'
import gzip, shutil, sys
with open(sys.argv[1], 'rb') as src, gzip.open(sys.argv[2], 'wb', 9) as dst:
    shutil.copyfileobj(src, dst)
PY
INITRD_BYTES=$(stat -c%s "$ROOT/images/initramfs.cpio.gz")
echo "initramfs  $INITRD_BYTES bytes"

# ---------------------------------------------------------------------------
# The kernel
# ---------------------------------------------------------------------------

cp "$KERNEL/arch/arm/boot/Image" "$ROOT/images/kernel.img"
echo "kernel     $(stat -c%s "$ROOT/images/kernel.img") bytes"

# ---------------------------------------------------------------------------
# The device tree
# ---------------------------------------------------------------------------

echo
echo "=== device tree"
# The initramfs's address and its end are the only things in the tree that
# depend on the build, and they come from the board file -- which is also where
# the boot ROM's copy loop reads them from. One source, so the ROM and the tree
# cannot disagree about where the initramfs is.
INITRD_START="$(node -e "import('file://$ROOT/boards/versatile-pb.mjs').then(m => console.log('0x' + m.flashLayout.initrd.dest.toString(16).padStart(8, '0')))")"
INITRD_END="$(printf '0x%08x' $(( $(printf '%d' "$INITRD_START") + INITRD_BYTES )))"
echo "initrd     $INITRD_START .. $INITRD_END"

# The sizes the tree describes are the board file's too. A tree that disagrees
# with the machine about how much memory or flash there is describes a computer
# nobody built, and the guest believes the tree.
read -r RAM_SIZE FLASH_SIZE <<EOF
$(node -e "
import('file://$ROOT/boards/versatile-pb.mjs').then((m) => {
    const h = (n) => '0x' + n.toString(16).padStart(8, '0');
    console.log(h(m.memory.sdram.size), h(m.memory.flash.size));
});
")
EOF
echo "memory     $RAM_SIZE, flash $FLASH_SIZE"

sed -e "s/@INITRD_START@/$INITRD_START/" -e "s/@INITRD_END@/$INITRD_END/" \
    -e "s/@RAM_SIZE@/$RAM_SIZE/" -e "s/@FLASH_SIZE@/$FLASH_SIZE/" \
    "$ROOT/boards/versatile-pb.dts" > "$RAVEN_DESKTOP/versatile-pb.dts"
dtc -I dts -O dtb -o "$ROOT/images/versatile-pb.dtb" \
    -Wno-unit_address_vs_reg -Wno-simple_bus_reg "$RAVEN_DESKTOP/versatile-pb.dts" 2>&1 | head -20
echo "dtb        $(stat -c%s "$ROOT/images/versatile-pb.dtb") bytes"

cd "$REPO"
echo
ls -l "$ROOT/images"
