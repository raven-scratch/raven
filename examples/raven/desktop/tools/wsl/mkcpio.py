#!/usr/bin/env python3
"""Write a cpio archive in the `newc` format an initramfs is.

    python3 mkcpio.py <rootfs-directory> <output>

The kernel unpacks its initramfs with the same code that unpacks any other
cpio archive, so this is the format that matters and not tar: a `newc` header
is a fixed 110 bytes of ASCII hexadecimal, then the name, then the contents.
Everything the archive needs is in this file, which is why the project does not
depend on a `cpio` program that a plain Ubuntu install does not have.

The order is the order the kernel creates the entries in, so directories are
written before the things inside them.

The padding is the part that is easy to get wrong, and getting it wrong is
silent: a header is 110 bytes, which is not a multiple of four, so a name begins
two bytes past a four-byte boundary and its padding depends on where it starts
rather than on how long it is. Padding each piece by its own length instead --
the obvious thing, and the first thing this file did -- leaves the next header
two bytes out, and the kernel reads the following entry's magic out of the
middle of its size field and reports `rootfs image is not initramfs (malformed
archive)`. Everything here therefore counts bytes.
"""

import os
import stat
import sys


class Newc:
    """A `newc` archive, padded by the offset rather than by the piece."""

    def __init__(self, out):
        self.out = out
        self.pos = 0

    def _raw(self, data):
        self.out.write(data)
        self.pos += len(data)

    def _align(self):
        pad = (-self.pos) % 4
        if pad:
            self._raw(b"\0" * pad)

    def entry(self, name, ino, mode, size, mtime, nlink=1):
        """A header and its name. The name's length includes its own NUL."""
        fields = [
            ino, mode, 0, 0, nlink, int(mtime), size,
            0, 0, 0, 0, len(name) + 1, 0,
        ]
        self._raw(("070701" + "".join("%08X" % f for f in fields)).encode())
        self._raw(name.encode() + b"\0")
        self._align()

    def data(self, blob):
        """A file's contents or a symbolic link's target."""
        self._raw(blob)
        self._align()


def main():
    root, target = sys.argv[1], sys.argv[2]
    entries = []
    for base, dirs, files in os.walk(root):
        dirs.sort()
        files.sort()
        for name in dirs + files:
            full = os.path.join(base, name)
            rel = "./" + os.path.relpath(full, root).replace(os.sep, "/")
            entries.append((rel, full))
    # A directory has to exist before anything can be put in it, so sort by
    # depth first and by name inside a depth.
    entries.sort(key=lambda e: (e[0].count("/"), e[0]))

    ino = 1
    with open(target, "wb") as out:
        archive = Newc(out)
        for rel, full in entries:
            info = os.lstat(full)
            if stat.S_ISDIR(info.st_mode):
                mode = stat.S_IFDIR | (info.st_mode & 0o7777)
                archive.entry(rel, ino, mode, 0, info.st_mtime)
            elif stat.S_ISLNK(info.st_mode):
                link = os.readlink(full).encode()
                mode = stat.S_IFLNK | 0o777
                archive.entry(rel, ino, mode, len(link), info.st_mtime)
                archive.data(link)
            elif stat.S_ISREG(info.st_mode):
                mode = stat.S_IFREG | (info.st_mode & 0o7777)
                archive.entry(rel, ino, mode, info.st_size, info.st_mtime)
                with open(full, "rb") as src:
                    archive.data(src.read())
            ino += 1

        archive.entry("TRAILER!!!", ino, 0, 0, 0)


if __name__ == "__main__":
    main()
