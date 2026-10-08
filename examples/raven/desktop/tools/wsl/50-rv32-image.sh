#!/bin/bash
# The RISC-V board's Linux, with a framebuffer console on the graphics card.
#
#   bash tools/wsl/50-rv32-image.sh
#
# ## Why this exists at all
#
# The board boots `../rv32ima/images/mini_image`, and that kernel has
# `# CONFIG_FB is not set` -- no framebuffer, no `simplefb`, no `fbcon`. It can
# only ever put the console on the serial port, which is why the example used
# to have to draw the console itself. This recipe builds the same machine's
# Linux from the same configuration with the framebuffer options turned on, so
# that the console reaches the card's memory and the monitor can scan it out.
#
# ## What it is built from
#
#   * the kernel source: 6.8, whose configuration
#     `../rv32ima/configs/custom_kernel_config` is the one cnlohr's buildroot
#     used, already patched for the four changes this build wants;
#   * the root filesystem: the cpio that is *inside* `mini_image`, unpacked and
#     repacked as the kernel's own `CONFIG_INITRAMFS_SOURCE`. That is what
#     makes the board's programs -- busybox, `coremark`, `duktape`, `ed` -- the
#     same programs: they are the same flat binaries, cut out of the same
#     archive.
#
# Nothing under `examples/raven/rv32ima/` is written. Its image is an input,
# the way `ref/`'s toolchains are, and this recipe only reads it.
#
# ## The one change to the guest
#
# The shell runs with its stdout on a pipe into `tee`, which writes it to the
# serial port *and* to the framebuffer console. See `rv32-console-sh` beside
# this file. Without it the shell's output would go to whichever console the
# kernel made `/dev/console`, and only one of the two would see it.
#
#   * `console=tty0` is what puts `fbcon` on the framebuffer `simplefb`
#     registers for the graphics card. Its node is in the device tree
#     `tools/build.mjs` generates from `boards/mini-rv32.mjs`.

set -euo pipefail

cd "$(dirname "$0")/../.."
ROOT=$PWD
REPO=$(cd ../../.. && pwd)
RV32=$REPO/examples/raven/rv32ima
CNLOHR=$REPO/ref/mini-rv32ima
BUILD=${BUILD:-$HOME/kbuild}
VERSION=6.8
IMAGES=$ROOT/images
OUT=$IMAGES/mini-fb-image

mkdir -p "$BUILD"

# ---------------------------------------------------------------------------
# 1. The kernel source, and cnlohr's configuration for this machine.
# ---------------------------------------------------------------------------
if [ ! -d "$BUILD/linux-$VERSION" ]; then
    [ -s "$BUILD/linux-$VERSION.tar.xz" ] || {
        echo "fetching linux-$VERSION"
        curl -fsSL -o "$BUILD/linux-$VERSION.tar.xz" \
            "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-$VERSION.tar.xz"
    }
    tar xf "$BUILD/linux-$VERSION.tar.xz" -C "$BUILD"
fi
K=$BUILD/linux-$VERSION
cp "$CNLOHR/configs/custom_kernel_config" "$K/.config"

# ---------------------------------------------------------------------------
# 1b. cnlohr's RISC-V 32-bit ABI patch.
#
# This is the one thing this recipe takes from cnlohr's buildroot rather than
# from the kernel tarball, and it is the whole difference between a guest that
# boots and one that reaches userspace and stops.
#
# A 32-bit RISC-V gets several syscalls only by asking for them.
# `asm-generic/unistd.h` puts `__NR_wait4` and the 32-bit halves of `statfs`,
# `lseek`, `sendfile` and the clock calls inside
#
#     #if defined(__ARCH_WANT_TIME32_SYSCALLS) || __BITS_PER_LONG != 32
#
# and `__NR_newfstatat`/`__NR_fstat` inside
#
#     #if defined(__ARCH_WANT_NEW_STAT) || defined(__ARCH_WANT_STAT64)
#
# while RISC-V's own `unistd.h` defines `__ARCH_WANT_NEW_STAT` and
# `__ARCH_WANT_SET_GET_RLIMIT` only `#if defined(__LP64__)`. chlohr's patch
# drops that guard and adds `__ARCH_WANT_STAT64` and
# `__ARCH_WANT_TIME32_SYSCALLS` for the 32-bit build, and the same four are
# what arm64, arc, csky, hexagon, nios2 and openrisc define for theirs.
#
# Without it, `sys_call_table[79]`, `[80]`, `[163]`, `[164]` and `[260]` are
# `__riscv_sys_ni_syscall`, and cnlohr's uclibc -- built for a kernel that had
# them -- calls every one of them. What the console shows is a boot that gets
# all the way to `/init` and then says, once per command,
#
#     sh: waitpid: Function not implemented
#     mkdir: can't create directory '/': Function not implemented
#
# and stops: the guest is healthy, and the kernel simply does not have the
# calls its libc was built against. Upstream's rv32 drops them because its
# libcs use `waitid`, `statx`, `prlimit64` and `renameat2`; this guest's does
# not, so the ABI it was built for is the one this recipe builds.
CNLOHR_PATCH=0001-Experimental-RISC-V-32-bit-No-MMU-support.patch
if ! grep -q '__ARCH_WANT_STAT64' "$K/arch/riscv/include/uapi/asm/unistd.h"; then
    if [ ! -s "$BUILD/$CNLOHR_PATCH" ]; then
        echo "fetching cnlohr's RISC-V 32-bit ABI patch"
        curl -fsSL -o "$BUILD/$CNLOHR_PATCH" \
            "https://raw.githubusercontent.com/cnlohr/buildroot/master/board/qemu/riscv32-virt/nommu/patches/linux/6.8-rc1/$CNLOHR_PATCH"
    fi
    ( cd "$K" && patch -p1 --forward < "$BUILD/$CNLOHR_PATCH" )
    grep -q '__ARCH_WANT_STAT64' "$K/arch/riscv/include/uapi/asm/unistd.h" || {
        echo 'the RISC-V 32-bit ABI patch did not apply' >&2
        exit 1
    }
    echo "  unistd: applied cnlohr's RISC-V 32-bit ABI patch"
fi

# ---------------------------------------------------------------------------
# 2. The root filesystem: the cpio inside mini_image, plus the files rv32ima's
#    own recipe keeps outside it, plus the shell wrapper above.
# ---------------------------------------------------------------------------
rm -rf "$BUILD/rootfs"
mkdir -p "$BUILD/rootfs"
python3 - "$RV32/images/mini_image" "$BUILD/rootfs" <<'PYTHON'
import os
import stat as st
import sys

image, out = sys.argv[1], sys.argv[2]
blob = open(image, 'rb').read()
i = blob.find(b'070701')
if i < 0:
    raise SystemExit('mini_image has no built-in cpio')
start = i
while True:
    header = blob[i:i + 110]
    if header[:6] != b'070701':
        raise SystemExit('ran off the end of the cpio')
    field = lambda a, b: int(header[a:b], 16)
    size, namesize, mode = field(54, 62), field(94, 102), field(14, 22)
    rmaj, rmin = field(78, 86), field(86, 94)
    name = blob[i + 110:i + 110 + namesize - 1].decode()
    data = (i + 110 + namesize + 3) & ~3
    body = blob[data:data + size]
    if name == 'TRAILER!!!':
        break
    path = os.path.join(out, name.lstrip('/'))
    kind = mode & 0o170000
    if kind == 0o040000:
        os.makedirs(path, exist_ok=True)
    elif kind == 0o120000:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        if os.path.lexists(path):
            os.unlink(path)
        os.symlink(body.decode(), path)
    elif kind in (0o020000, 0o060000, 0o010000):
        # A device node is not a file with no bytes. `/dev/console` is a
        # character device, and unpacking it as a regular file gives the
        # kernel an init whose standard output is a file in the initramfs:
        # every message userspace writes goes there and nowhere, which looks
        # exactly like a boot that stops without saying anything.
        os.makedirs(os.path.dirname(path), exist_ok=True)
        if os.path.lexists(path):
            os.unlink(path)
        os.mknod(path, mode, os.makedev(rmaj, rmin))
    else:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, 'wb') as handle:
            handle.write(body)
        os.chmod(path, mode & 0o7777)
    i = (data + size + 3) & ~3
print('  rootfs: %d bytes of cpio unpacked from %s' % (i - start, os.path.basename(image)))
PYTHON

# The programs rv32ima's recipe installs beside the kernel's own archive.
for f in "$RV32"/images/mini-rootfs/root/* "$RV32"/images/mini-rootfs/usr/bin/*; do
    rel=${f#"$RV32"/images/mini-rootfs/}
    mkdir -p "$BUILD/rootfs/$(dirname "$rel")"
    cp "$f" "$BUILD/rootfs/$rel"
    chmod 755 "$BUILD/rootfs/$rel"
done

# The shell, with its output on the serial port and the card at once.
cp tools/wsl/rv32-console-sh "$BUILD/rootfs/usr/bin/console-sh"
chmod 755 "$BUILD/rootfs/usr/bin/console-sh"

if [ "${TRACE:-0}" = 1 ]; then
    echo '  trace: instrumenting /init and the shell wrapper' >&2
    python3 - "$BUILD/rootfs/init" <<'PYTHON'
import sys
path = sys.argv[1]
text = open(path).read()
text = text.replace('#!/bin/sh\n',
                    '#!/bin/sh\necho "/init: start" >/dev/console\n', 1)
text = text.replace('exec /sbin/init "$@"',
                    'echo "/init: exec /sbin/init" >/dev/ttyS0\nexec /sbin/init "$@"', 1)
open(path, 'w').write(text)
PYTHON
    cp "$BUILD/rootfs/usr/bin/console-sh" "$BUILD/rootfs/usr/bin/console-sh.orig"
    cat > "$BUILD/rootfs/usr/bin/console-sh" <<'SCRIPT'
#!/bin/sh
S=/dev/ttyS0
echo "console-sh: start" > $S
echo "console-sh: plain pipe" > $S
echo pipedata | cat > $S
echo "console-sh: after plain pipe" > $S
echo "console-sh: tee alone" > $S
echo teedata | tee $S > /dev/null
echo "console-sh: after tee" > $S
echo "console-sh: shell to tty0" > $S
/bin/sh -c 'echo "console-sh: tty0 says hi"' > /dev/tty0 2>&1
echo "console-sh: after tty0 write" > $S
/bin/sh -l </dev/ttyS0 >/dev/tty0 2>&1
echo "console-sh: direct shell returned" > $S
SCRIPT
    chmod 755 "$BUILD/rootfs/usr/bin/console-sh"
fi

# The getty line becomes that shell. rv32ima's own recipe replaced the getty
# with `-/bin/sh` in place inside its image; this is the same line, moved to
# the wrapper, and it is a plain edit because this tree is ours.
python3 - "$BUILD/rootfs/etc/inittab" <<'PYTHON'
import sys

path = sys.argv[1]
text = open(path).read()
old = 'console::respawn:-/bin/sh'
new = 'console::respawn:/usr/bin/console-sh'
if old not in text:
    raise SystemExit('the serial shell line is not in /etc/inittab')
open(path, 'w').write(text.replace(old, new))
print('  inittab: %s -> %s' % (old, new))
PYTHON

# ---------------------------------------------------------------------------
# 3. The configuration: cnlohr's, with the framebuffer turned on.
# ---------------------------------------------------------------------------
cd "$K"
./scripts/config --file .config \
    -e FB -e FB_SIMPLE -e FRAMEBUFFER_CONSOLE -e FONT_8x16 -e FONT_SUPPORT \
    -e VT -e VT_CONSOLE \
    -d FRAMEBUFFER_CONSOLE_DEFERRED_TAKEOVER -d LOGO -d HVC_RISCV_MINIRV32 \
    --set-str INITRAMFS_SOURCE "$BUILD/rootfs" \
    --set-str LOCALVERSION '-mini-rv32ima-fb' \
    --set-str CMDLINE 'rw earlycon=uart8250,mmio,0x10000000,1000000 console=tty0 console=ttyS0'
make ARCH=riscv CROSS_COMPILE=riscv64-unknown-elf- olddefconfig >/dev/null
grep -q '^CONFIG_FB_SIMPLE=y' .config || { echo 'FB_SIMPLE did not stay on' >&2; exit 1; }
grep -q '^CONFIG_FRAMEBUFFER_CONSOLE=y' .config || { echo 'fbcon did not stay on' >&2; exit 1; }

# ---------------------------------------------------------------------------
# 4. The kernel, and the image the board boots.
# ---------------------------------------------------------------------------
make ARCH=riscv CROSS_COMPILE=riscv64-unknown-elf- -j"$(nproc)" Image
mkdir -p "$IMAGES"
cp arch/riscv/boot/Image "$OUT"
echo "  $(stat -c%s "$OUT") bytes -> images/$(basename "$OUT")"
strings "$OUT" | grep -m1 'Linux version' || true

# The console's own font, out of `vmlinux`, as a file the check can read.
#
# The guest's console is pixels and there is no text in it, so the only way
# for `tools/check-rv32.mjs` to say what the card is showing is to put the
# kernel's own glyphs back over them. `fontdata_8x16` is the table `fbcon` and
# `cfb_imageblit` draw every character from, and it is read out of the same
# kernel the guest is running rather than from a copy of a font that might not
# be the one it drew with. 4096 bytes: 256 characters of 16 rows of one byte.
# `fontdata_8x16` is not the glyphs: it is a `struct font_data`, four u32s of
# `{ width, height, charcount, ... }` and then the 4096 bytes of them, so the
# table starts sixteen bytes past the symbol.
python3 - "$K/vmlinux" "$K/System.map" "$IMAGES/mini-fb-font.bin" <<'PYTHON'
import re
import subprocess
import sys

vmlinux, sysmap, out = sys.argv[1], sys.argv[2], sys.argv[3]
addr = None
for line in open(sysmap):
    p = line.split()
    if len(p) == 3 and p[2] == 'fontdata_8x16':
        addr = int(p[0], 16) + 16
if addr is None:
    raise SystemExit('fontdata_8x16 is not in System.map')
sections = subprocess.run(['riscv64-unknown-elf-readelf', '-S', '-W', vmlinux],
                          capture_output=True, text=True).stdout
for line in sections.splitlines():
    m = re.match(r'\s*\[\s*\d+\]\s+(\S+)\s+(\S+)\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)', line)
    if not m:
        continue
    va, off, size = int(m.group(3), 16), int(m.group(4), 16), int(m.group(5), 16)
    if va and va <= addr < va + size:
        with open(vmlinux, 'rb') as handle:
            handle.seek(off + (addr - va))
            data = handle.read(4096)
        open(out, 'wb').write(data)
        print('  %s: fontdata_8x16 at %#x -> images/%s' % (out, addr, out.rsplit('/', 1)[-1]))
        break
else:
    raise SystemExit('fontdata_8x16 is not in any section')
PYTHON
