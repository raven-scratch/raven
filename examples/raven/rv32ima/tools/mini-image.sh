#!/bin/bash
# Pack the small Linux image: the reference's kernel, with the rootfs it carries
# left alone except for the three things this recipe owns -- the line that
# decides how it asks you to log in, the `EDITOR` it names, and the editor
# itself.
#
#   bash tools/mini-image.sh            # in WSL or any Linux, with dtc + python3
#
# ## What the image is
#
# It is the reference image and five programs, and it boots to a root shell the
# way an embedded Linux does: no login, no banner of ours, nothing running that
# the rootfs did not come with. You get a prompt, and then you type.
#
# It has no `vi`. The rootfs carries one as a symlink to busybox, and busybox'
# `vi` run with no file name asks the kernel to open a NULL path -- which a
# kernel with an MMU answers with `-EFAULT` and this one answers with an
# unhandled load access fault (the README says where). So that entry is *taken
# out* of the kernel's own archive rather than left to fail, and what the image
# has instead is a standalone `ed`.
#
# ## How the rootfs is changed without rebuilding the kernel
#
# The reference image is a RISC-V `Image` with its rootfs *built into* it -- a
# plain cpio in the kernel's `.init.ramfs` -- so a file cannot be added by
# unpacking something on top: `populate_rootfs` also unpacks the initramfs the
# device tree names, and that is where the *added* files go (the tools below),
# but the kernel's own archive is unpacked first and an existing file is not
# replaced. So the two files that have to change -- `/etc/inittab`, whose getty
# line is what stops at `buildroot login:`, and `/etc/profile`, which names the
# editor -- are changed *in the archive*, in place, at exactly their own length:
# the cpio entry's size does not move, the archive's size does not move, and the
# kernel's `__initramfs_size` stays true. That is the whole trick, and it is why
# this needs no cross toolchain, no `elf2flt`, and no kernel build.
#
# Removing a file is the other half of the same trick and is why the archive is
# written back as a whole: the `bin/vi` entry is *spliced out* of the blob and
# the blob is padded with zeros back to the length the kernel was linked with.
# The kernel walks the archive entry by entry and stops at its trailer, so the
# padding is never read.
#
# the line replaced is, verbatim from the rootfs:
#
#   console::respawn:/sbin/getty -L  console 0 vt100 # GENERIC_SERIAL
#
# and what replaces it is busybox init's own way of running a shell in a getty's
# place, with the leading `-` that makes it a login shell, so the rootfs's own
# `/etc/profile` still runs and `/etc/init.d/rcS` has already started everything
# it normally starts:
#
#   console::respawn:-/bin/sh
#
# The other in-place edit is `/etc/profile`'s `export EDITOR='/bin/vi'`, which
# becomes `export EDITOR=ed #no vi` -- the same 23 bytes, because `/usr/bin/ed`
# is 11 and the shell looks a bare name up in the PATH.
#
# ## Why the programs are not compiled here
#
# They cannot be. The reference kernel has `binfmt_flat` and **no `binfmt_elf`**
# -- `Corrupted ELF file` is nowhere in it while `Unable to allocate RAM for
# process` is -- so an ELF, whatever it was built with, cannot run on this
# machine: the only userland format is the flat one, and producing *that* needs
# `elf2flt` out of a Buildroot cross toolchain. So the one program that needs
# compiling is taken from cnlohr's own prebuilt image, which was built from his
# `packages/` with exactly that toolchain for exactly this machine. `coremark`
# is the reference rootfs's own, at `/root/coremark` inside the kernel's cpio:
# this recipe lifts that file out and installs it under its own name next to the
# others, which is why the image carries no symlink and no program of ours is
# anywhere but `/usr/bin`. `ed` is a flat binary too, and the only one this
# repository did not build: it came out of the Scratch project's own rootfs, cut
# out of its cpio the same way `coremark` is, and kept in `images/mini-src/` so
# that this recipe needs neither that 162 MB file nor a toolchain to run.
#
# ## Layout, in the guest's addresses
#
#   0x80000000  the kernel image (the reference's, with the three changes this
#               file makes to the rootfs inside it)
#   0x80400000  our initramfs cpio          (INITRD_OFFSET below)
#   0x83ff940   the device tree, placed by the machine, not by this file

set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
REPO=$(cd ../../.. && pwd)
KERNEL=$REPO/ref/mini-rv32ima-rs/linux_image
IMAGES=$ROOT/images
ROOTFS=$IMAGES/mini-rootfs
MINISRC=$IMAGES/mini-src
DOWNLOAD=$IMAGES/mini-download
PATCHED=$IMAGES/mini-kernel
INITRD_OFFSET=4194304          # 4 MiB: 0x80400000 in the guest
INITRD_ADDRESS=0x80400000
FETCH=${FETCH:-1}

if [ ! -f "$KERNEL" ]; then
    echo "no $KERNEL -- the reference's image is an input, see the README" >&2
    exit 1
fi
# `root/`, `usr/` and `bin/` are this recipe's to own: what is in them is
# fetched or copied, never edited, so they are emptied and rebuilt every run.
# `bin/` is emptied rather than used -- the three programs belong at their own
# names in `/usr/bin`, where the rootfs's PATH already looks.
rm -rf "$ROOTFS/root" "$ROOTFS/usr" "$ROOTFS/bin"
mkdir -p "$ROOTFS/root" "$ROOTFS/usr/bin"

# ---------------------------------------------------------------------------
# 1. duktape, out of cnlohr's image. It lands at `/usr/bin/duktape`, which is
#    where it is asked for: an installed file, not a link to one.
# ---------------------------------------------------------------------------
if [ "$FETCH" = 1 ] && [ ! -s "$ROOTFS/usr/bin/duktape" ]; then
    mkdir -p "$DOWNLOAD"
    if [ ! -s "$DOWNLOAD/cnl.zip" ]; then
        echo "fetching cnlohr's prebuilt image"
        curl -fsSL -o "$DOWNLOAD/cnl.zip" \
            https://raw.githubusercontent.com/cnlohr/mini-rv32ima-images/master/images/linux-6.1.14-rv32nommu-cnl-1.zip
    fi
    python3 - "$DOWNLOAD/cnl.zip" "$ROOTFS" <<'PYTHON'
import sys, zipfile

archive, rootfs = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(archive) as zip_:
    name = [n for n in zip_.namelist() if 'Image' in n][0]
    image = zip_.read(name)

# The image carries its rootfs as a plain cpio, so the files are cut out of it
# rather than out of a running system.
wanted = {'root/duktapetest': 'usr/bin/duktape', 'root/fizzbuzz.js': 'root/fizzbuzz.js'}
i = image.find(b'070701')
while wanted:
    header = image[i:i+110]
    if header[:6] != b'070701':
        raise SystemExit('ran off the end of the cpio looking for %s' % list(wanted))
    field = lambda a, b: int(header[a:b], 16)
    size, namesize = field(54, 62), field(94, 102)
    entry = image[i+110:i+110+namesize-1].decode()
    start = (i + 110 + namesize + 3) & ~3
    if entry in wanted:
        data = image[start:start+size]
        with open('%s/%s' % (rootfs, wanted[entry]), 'wb') as handle:
            handle.write(data)
        print('  %s -> %s (%d bytes)' % (entry, wanted[entry], size))
        del wanted[entry]
    i = (start + size + 3) & ~3
PYTHON
    chmod +x "$ROOTFS/usr/bin/duktape"
fi

# ---------------------------------------------------------------------------
# 2. screenfetch, clear and ed, which are files here and not downloads.
# ---------------------------------------------------------------------------
#
# Upstream screenfetch is bash and uses `awk`; this guest has busybox's ash and
# no awk, so what is installed is the same fetch written for POSIX sh. Read
# `mini-src/screenfetch` for what that does and does not copy.
cp "$MINISRC/screenfetch" "$ROOTFS/usr/bin/screenfetch"
chmod +x "$ROOTFS/usr/bin/screenfetch"

# `clear` is two escape sequences, because the applet is not in this busybox and
# there is no terminfo to ask: `mini-src/clear` is the whole program.
cp "$MINISRC/clear" "$ROOTFS/usr/bin/clear"
chmod +x "$ROOTFS/usr/bin/clear"

# `ed` is a flat riscv binary and not a script: it is `usr/bin/ed` in the
# Scratch project's own rootfs, and being flat it needs no library, no
# interpreter and no emulation, which is what this kernel can load. It goes
# where the rootfs' PATH already looks, next to the other four.
cp "$MINISRC/ed" "$ROOTFS/usr/bin/ed"
chmod +x "$ROOTFS/usr/bin/ed"

# ---------------------------------------------------------------------------
# 3. coremark, out of the kernel's own rootfs.
# ---------------------------------------------------------------------------
#
# The reference rootfs carries coremark at `/root/coremark`, inside the kernel's
# own cpio, and this recipe lifts that very file out and installs it at its own
# name next to the other two. Step 4 does it, because that is the step that
# already walks that archive.

# ---------------------------------------------------------------------------
# 4. The rootfs's /etc/inittab, with its getty replaced by a root shell, its
#    /etc/profile with the editor it names, the one file lifted out of the same
#    archive, and the `vi` entry taken out of it.
# ---------------------------------------------------------------------------
python3 - "$KERNEL" "$PATCHED" "$ROOTFS" <<'PYTHON'
import os, sys

kernel, out, rootfs = sys.argv[1], sys.argv[2], sys.argv[3]
image = bytearray(open(kernel, 'rb').read())

OLD = b'console::respawn:/sbin/getty -L  console 0 vt100 # GENERIC_SERIAL'
NEW = b'console::respawn:-/bin/sh'
# Same length, which is what an in-place edit of a file inside the archive
# needs: `/bin/vi` is 7 bytes long and `/usr/bin/ed` is 11, so the line names
# the program rather than its path and the shell looks it up in PATH. The
# trailing comment is the padding.
EDITOR_OLD = b"export EDITOR='/bin/vi'"
EDITOR_NEW = b'export EDITOR=ed #no vi'
assert len(EDITOR_OLD) == len(EDITOR_NEW) == 23

def field(header, a, b):
    return int(bytes(header[a:b]), 16)

archive = image.find(b'070701')
kept = []
trailer = None
found = {'inittab': False, 'profile': False, 'coremark': False, 'vi': False}
i = archive
while i >= 0 and bytes(image[i:i+6]) == b'070701':
    header = bytes(image[i:i+110])
    size, namesize = field(header, 54, 62), field(header, 94, 102)
    name = bytes(image[i+110:i+110+namesize-1]).decode()
    start = (i + 110 + namesize + 3) & ~3
    end = (start + size + 3) & ~3
    if name == 'TRAILER!!!':
        trailer = (i, end)
        break
    if name == 'etc/inittab':
        body = bytes(image[start:start+size])
        if OLD not in body:
            raise SystemExit('the getty line is not in /etc/inittab')
        # The replacement is shorter than what it replaces and the rest is a
        # comment: the entry has to stay exactly the size it was, because the
        # kernel was linked with the archive's size inside it.
        line = NEW + b' # autologin root on the serial console'
        body = body.replace(OLD, line)
        body += b'\n' + b'#' * (size - len(body) - 1)
        assert len(body) == size, 'the patched inittab changed size'
        image[start:start+size] = body
        found['inittab'] = True
        print('  inittab: %s -> %s' % (OLD[:32].decode(), NEW.decode()))
    elif name == 'etc/profile':
        body = bytes(image[start:start+size])
        at = body.find(EDITOR_OLD)
        if at < 0:
            raise SystemExit('the EDITOR line is not in /etc/profile')
        image[start+at:start+at+len(EDITOR_OLD)] = EDITOR_NEW
        found['profile'] = True
        print('  profile: %s -> %s' % (EDITOR_OLD.decode(), EDITOR_NEW.decode()))
    elif name == 'root/coremark':
        target = os.path.join(rootfs, 'usr/bin/coremark')
        with open(target, 'wb') as handle:
            handle.write(bytes(image[start:start+size]))
        os.chmod(target, 0o755)
        found['coremark'] = True
        print('  coremark: root/coremark -> usr/bin/coremark (%d bytes)' % size)
    elif name == 'bin/vi':
        # Dropped rather than patched: busybox' `vi` is what the README's vi
        # note is about, and this image has `ed` instead. The entry's bytes
        # leave the blob; the zeros put back at the end of it keep the archive
        # the length the kernel was linked with.
        found['vi'] = True
        print('  vi: bin/vi removed (busybox symlink, %d bytes)' % size)
        i = end
        continue
    kept.append((i, end))
    i = end

for what, ok in found.items():
    if not ok:
        raise SystemExit('the kernel archive has no %s' % what)
if trailer is None:
    raise SystemExit('the kernel archive has no TRAILER!!!')

blob = b''.join(bytes(image[a:b]) for a, b in kept) + bytes(image[trailer[0]:trailer[1]])
span = trailer[1] - archive
assert len(blob) <= span, 'the archive grew'
blob += b'\0' * (span - len(blob))
image[archive:trailer[1]] = blob
print('  archive: %d bytes in, %d bytes of entries out, padded to %d' %
      (span, len(blob.rstrip(b'\0')), span))

open(out, 'wb').write(bytes(image))
PYTHON

# ---------------------------------------------------------------------------
# 5. The initramfs: everything under images/mini-rootfs, as a newc cpio.
# ---------------------------------------------------------------------------
#
# Packed by hand rather than by `cpio -o` so the bytes do not depend on the host
# tool's idea of times, owners or inode numbers -- the same tree packs to the
# same image on any machine, which is what makes the committed image checkable.
python3 - "$ROOTFS" "$IMAGES/mini.cpio" <<'PYTHON'
import os, stat, sys

source, out = sys.argv[1], sys.argv[2]

def pad(blob, size, fill):
    return blob + fill * ((size - len(blob) % size) % size)

entries = []
for base, dirs, files in os.walk(source):
    dirs.sort()
    rel = os.path.relpath(base, source)
    if rel != '.':
        entries.append((rel, stat.S_IFDIR | 0o755, b''))
    for name in sorted(files):
        full = os.path.join(base, name)
        # `rel` is the directory and must stay the directory: an earlier version
        # of this loop rebound it to the file it had just packed, so the *second*
        # file in every directory was written under the first one's name --
        # `usr/bin/duktape` became `usr/bin/coremark/duktape` and `screenfetch`
        # became `usr/bin/coremark/duktape/screenfetch`, neither of which the
        # kernel could unpack, so two of the three programs were simply absent.
        path = name if rel == '.' else os.path.join(rel, name)
        if os.path.islink(full):
            entries.append((path, stat.S_IFLNK | 0o777, os.readlink(full).encode()))
        else:
            # The mode is decided here and not read off the host: this recipe is
            # usually run from a Windows drive, where every file reports 0777,
            # so a host mode would make the image depend on where it was built.
            # Executable if the host says any execute bit, a plain 0644 if not.
            mode = 0o755 if os.stat(full).st_mode & 0o111 else 0o644
            with open(full, 'rb') as handle:
                entries.append((path, stat.S_IFREG | mode, handle.read()))
entries.sort(key=lambda e: (e[0].count('/'), e[0]))

def header(name, mode, size, ino):
    name = name.encode()
    fields = [ino, mode, 0, 0, 1, 0, size, 0, 0, 0, 0, len(name) + 1, 0]
    return pad(('070701' + ''.join('%08x' % f for f in fields)).encode() + name + b'\0', 4, b'\0')

blob = b''
for ino, (name, mode, data) in enumerate(entries, start=1):
    blob += header(name, mode, len(data), ino) + pad(data, 4, b'\0')
blob += header('TRAILER!!!', 0, 0, 0)
with open(out, 'wb') as handle:
    handle.write(blob)
print('  %d entries, %d bytes -> %s' % (len(entries), len(blob), out))
PYTHON

# ---------------------------------------------------------------------------
# 6. The device tree, with the cpio's address written into /chosen.
# ---------------------------------------------------------------------------
SIZE=$(stat -c%s "$IMAGES/mini.cpio")
END=$(printf '0x%08x' $((INITRD_ADDRESS + SIZE)))
python3 - "$IMAGES/base.dts" "$IMAGES/mini.dts" "$INITRD_ADDRESS" "$END" <<'PYTHON'
import sys
template, out, start, end = sys.argv[1:5]
text = open(template).read()
# The initramfs goes in /chosen, where the kernel looks for `linux,initrd-*`.
text = text.replace(
    'bootargs = "',
    '\t\tlinux,initrd-start = <%s>;\n\t\tlinux,initrd-end = <%s>;\n\t\tbootargs = "' % (start, end), 1)
open(out, 'w').write(text)
PYTHON
dtc -I dts -O dtb -S 1536 "$IMAGES/mini.dts" -o "$IMAGES/mini.dtb" 2>/dev/null
echo "  initrd $INITRD_ADDRESS..$END ($SIZE bytes), dtb $(stat -c%s "$IMAGES/mini.dtb") bytes"

# ---------------------------------------------------------------------------
# 7. The image: the patched kernel, pad to the initrd's address, cpio.
# ---------------------------------------------------------------------------
python3 - "$PATCHED" "$IMAGES/mini.cpio" "$IMAGES/mini_image" "$INITRD_OFFSET" <<'PYTHON'
import sys
kernel, cpio, out, offset = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
image = open(kernel, 'rb').read()
extra = open(cpio, 'rb').read()
assert len(image) <= offset, 'the kernel is longer than the initrd offset'
blob = image + b'\0' * (offset - len(image)) + extra
open(out, 'wb').write(blob)
print('  %s: %d bytes (kernel %d, pad %d, initramfs %d)' %
      (out, len(blob), len(image), offset - len(image), len(extra)))
PYTHON
