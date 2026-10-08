#!/usr/bin/env bash
# Fetch everything the guest is built from, into $RAVEN_DESKTOP (default
# ~/raven-desktop). Nothing here needs root: the toolchain from Bootlin is
# relocatable, and the kernel and BusyBox are ordinary tarballs.
#
#     bash tools/wsl/00-setup.sh
#
# It reads nothing from the repository and writes nothing to it. 10-build.sh is
# what turns what this leaves behind into the images the project boots.
#
# Every version below is pinned. Where a hash is not known yet it is not
# trusted: the script records what it downloaded and its SHA-256 in
# $RAVEN_DESKTOP/SOURCES.txt, which is the file to check a rebuild against.

set -euo pipefail

RAVEN_DESKTOP="${RAVEN_DESKTOP:-$HOME/raven-desktop}"
DL="$RAVEN_DESKTOP/dl"
SRC="$RAVEN_DESKTOP/src"
TC="$RAVEN_DESKTOP/toolchain"

BOOTLIN_VERSION="2024.02-1"
TOOLCHAIN="armv5-eabi--glibc--stable-$BOOTLIN_VERSION"
TOOLCHAIN_URL="https://toolchains.bootlin.com/downloads/releases/toolchains/armv5-eabi/tarballs/$TOOLCHAIN.tar.bz2"

# The kernel's minor release is chosen at download time: the highest 6.6.x the
# archive lists. 6.6 is an LTS with a full Versatile PB device tree, an
# ARM926EJ-S that is armv5te, and an AMBA CLCD the framebuffer console can use.
KERNEL_SERIES="6.6"
KERNEL_MIRROR="https://cdn.kernel.org/pub/linux/kernel/v6.x"

BUSYBOX_VERSION="1.36.1"
BUSYBOX_URL="https://busybox.net/downloads/busybox-$BUSYBOX_VERSION.tar.bz2"

mkdir -p "$DL" "$SRC" "$TC"

say() { printf '\n=== %s\n' "$*"; }

# ---------------------------------------------------------------------------
# One download, kept, and never trusted twice
# ---------------------------------------------------------------------------

fetch() {
    local url="$1" out="$DL/$(basename "$1")"
    if [ -s "$out" ]; then
        echo "have $(basename "$out")" >&2
    else
        echo "get  $url" >&2
        curl -fL --retry 3 --retry-delay 2 -o "$out.part" "$url" >&2
        mv "$out.part" "$out"
    fi
    printf '%s' "$out"
}

# Python extracts both formats; the image has bzip2's command line but not
# necessarily the library's, and a tarball that unpacks differently on two
# machines is not a pinned tarball.
untar() {
    local archive="$1" dest="$2" marker="$3"
    if [ -e "$dest/$marker" ]; then
        echo "have $marker" >&2
        return
    fi
    mkdir -p "$dest"
    python3 - "$archive" "$dest" <<'PY'
import sys, tarfile
tarfile.open(sys.argv[1]).extractall(sys.argv[2], filter='data')
PY
}

# ---------------------------------------------------------------------------
# The three sources
# ---------------------------------------------------------------------------

say "toolchain: $TOOLCHAIN"
untar "$(fetch "$TOOLCHAIN_URL")" "$TC" "$TOOLCHAIN"

say "kernel: $KERNEL_SERIES"
if [ -z "${KERNEL_TARBALL:-}" ]; then
    KERNEL_TARBALL="$(
        curl -fsS "$KERNEL_MIRROR/" |
            grep -oE "linux-$KERNEL_SERIES\.[0-9]+\.tar\.xz" |
            sed "s/linux-$KERNEL_SERIES\.//; s/\.tar\.xz//" |
            sort -n | tail -1
    )"
    KERNEL_TARBALL="linux-$KERNEL_SERIES.$KERNEL_TARBALL.tar.xz"
fi
untar "$(fetch "$KERNEL_MIRROR/$KERNEL_TARBALL")" "$SRC" "${KERNEL_TARBALL%.tar.xz}"

say "busybox: $BUSYBOX_VERSION"
untar "$(fetch "$BUSYBOX_URL")" "$SRC" "busybox-$BUSYBOX_VERSION"

# ---------------------------------------------------------------------------
# The record
# ---------------------------------------------------------------------------

say "the record"
{
    echo "# What this checkout was built from. Written by tools/wsl/00-setup.sh."
    echo "# Check a rebuild against these, and put the toolchain and the two"
    echo "# tarballs back by name if a hash has moved."
    echo
    echo "toolchain  $TOOLCHAIN"
    echo "kernel     $KERNEL_TARBALL"
    echo "busybox    busybox-$BUSYBOX_VERSION.tar.bz2"
    echo
    echo "# sha256  bytes  file"
    for f in "$DL/$TOOLCHAIN.tar.bz2" "$DL/$KERNEL_TARBALL" "$DL/busybox-$BUSYBOX_VERSION.tar.bz2"; do
        printf '%s  %12s  %s\n' "$(sha256sum "$f" | cut -d' ' -f1)" "$(stat -c%s "$f")" "$(basename "$f")"
    done
} > "$RAVEN_DESKTOP/SOURCES.txt"
cat "$RAVEN_DESKTOP/SOURCES.txt"

say "done"
echo "toolchain  $TC/$TOOLCHAIN"
echo "kernel     $SRC/${KERNEL_TARBALL%.tar.xz}"
echo "busybox    $SRC/busybox-$BUSYBOX_VERSION"
CROSS_GCC="$(find "$TC/$TOOLCHAIN/bin" -maxdepth 1 -name '*-gcc' | head -1)"
echo "cross gcc  $CROSS_GCC"
"$CROSS_GCC" --version | head -1
