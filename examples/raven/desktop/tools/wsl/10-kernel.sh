#!/usr/bin/env bash
# Build the guest's kernel.
#
#     bash tools/wsl/10-kernel.sh
#
# The kernel is not taken on trust. `versatile_defconfig` is the kernel's own
# configuration for this board, and everything changed on top of it is changed
# for a reason that is written down here:
#
#   MODULES=n          so that nothing has to be built as a module and there is
#                      no module signing, no openssl, and no initramfs full of
#                      .ko files to load. Everything the machine needs is either
#                      built in or not there.
#   DEVTMPFS, _MOUNT   so that /dev exists and is populated without udev, which
#                      this image has no room for and no need for.
#   FRAMEBUFFER_CONSOLE, DRM_FBDEV_EMULATION
#                      so that the kernel draws its console into the LCD
#                      controller's framebuffer. This is the whole point: what
#                      the Stage shows is the kernel's own console, written into
#                      memory the controller is scanning out.
#   FONT_8x16          the console's font. Without it there is a framebuffer and
#                      nothing to put in it.
#   CMDLINE=""         because the command line belongs in the device tree the
#                      board hands over, not in the kernel. `versatile_defconfig`
#                      ships `root=1f03 mem=32M`, which is a command line for a
#                      machine with a different disk and more memory.
#
# The output is `arch/arm/boot/Image`: the uncompressed kernel, which is what a
# boot loader loads when it is going to jump straight into it rather than run a
# decompressor first.

set -euo pipefail

RAVEN_DESKTOP="${RAVEN_DESKTOP:-$HOME/raven-desktop}"
KERNEL_VERSION="${KERNEL_VERSION:-6.6.158}"
SRC="$RAVEN_DESKTOP/src/linux-$KERNEL_VERSION"

if [ ! -d "$SRC" ]; then
    echo "no kernel source at $SRC; run 00-setup.sh first" >&2
    exit 2
fi

TOOLCHAIN_DIR="$(find "$RAVEN_DESKTOP/toolchain" -maxdepth 2 -name bin -type d | head -1)"
CROSS="$(find "$TOOLCHAIN_DIR" -maxdepth 1 -name '*-gcc' | head -1)"
CROSS="${CROSS%-gcc}-"
echo "cross compiler  ${CROSS}gcc"
"${CROSS}gcc" --version | head -1

# The kernel's configuration language needs m4, bison and flex, and this
# image has none of them and no root to install them with. `02-build-tools.sh`
# fetches the distribution's own packages and unpacks them beside the
# toolchain; this puts them in front of the path, and tells bison where its
# data files went, because it looks for them where the package would have
# installed them and they are not there.
if [ -d "$RAVEN_DESKTOP/build-tools/usr/bin" ]; then
    export PATH="$RAVEN_DESKTOP/build-tools/usr/bin:$PATH"
    export BISON_PKGDATADIR="$RAVEN_DESKTOP/build-tools/usr/share/bison"
    export M4="$RAVEN_DESKTOP/build-tools/usr/bin/m4"
fi
for tool in m4 bison flex; do
    command -v "$tool" >/dev/null || echo "warning: no $tool; the config step will fail"
done

cd "$SRC"

# The tarball ships the generated Kconfig and device-tree parsers, and the
# kernel regenerates them from their `.l` and `.y` sources if it thinks they
# are older. There is no flex and no bison here and there is no reason for
# either: the generated files in the tarball are the ones the release was
# built with. Touching them is what says so.
find scripts -name "*.lex.c" -o -name "*.tab.c" -o -name "*.tab.h" | xargs -r touch

echo
echo "=== configuring"
make ARCH=arm CROSS_COMPILE="$CROSS" versatile_defconfig >/dev/null
scripts/config --file .config \
    --disable MODULES \
    --enable DEVTMPFS --enable DEVTMPFS_MOUNT \
    --enable FRAMEBUFFER_CONSOLE --enable DRM_FBDEV_EMULATION \
    --enable BACKLIGHT_CLASS_DEVICE \
    --enable FONT_8x16 \
    --disable FONT_10x18 --disable FONT_6x11 --disable FONT_7x14 \
    --set-str CMDLINE ""
make ARCH=arm CROSS_COMPILE="$CROSS" olddefconfig >/dev/null

for option in CONFIG_DEVTMPFS CONFIG_DEVTMPFS_MOUNT CONFIG_FRAMEBUFFER_CONSOLE \
              CONFIG_DRM_FBDEV_EMULATION CONFIG_FONT_8x16 CONFIG_DRM_PL111 \
              CONFIG_DRM_PANEL_ARM_VERSATILE CONFIG_SERIAL_AMBA_PL011_CONSOLE \
              CONFIG_ARCH_VERSATILE CONFIG_MMC_ARMMMCI CONFIG_SMC91X; do
    printf '%-34s %s\n' "$option" "$(grep -E "^$option=" .config || echo '(not set)')"
done

echo
echo "=== building"
make ARCH=arm CROSS_COMPILE="$CROSS" -j"$(nproc)" Image 2>&1 | tail -25

echo
ls -l arch/arm/boot/Image
