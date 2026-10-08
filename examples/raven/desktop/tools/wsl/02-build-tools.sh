#!/usr/bin/env bash
# Fetch the build tools the kernel needs that this image does not have.
#
#     bash tools/wsl/02-build-tools.sh
#
# Two of them are missing from a plain Ubuntu install and there is no root here,
# so they are taken the way anything else is taken: the distribution's own
# packages, downloaded and unpacked into this project's directory rather than
# installed. `apt-get download` needs no privileges and `dpkg-deb -x` needs
# none either, and the result is a private `usr/bin` that `10-kernel.sh` puts at
# the front of its path.
#
# What each one is for:
#
#   m4      bison generates its parser by running m4 over a template. Nothing
#           else in this project uses it.
#   bison   the kernel's configuration language is a yacc grammar.
#   flex    and a lexer.
#   libfl2  flex's runtime, for the lexer flex generates.
#
# None of them is needed to *run* anything: this is the build of the kernel, not
# the machine.

set -euo pipefail

RAVEN_DESKTOP="${RAVEN_DESKTOP:-$HOME/raven-desktop}"
TOOLS="$RAVEN_DESKTOP/build-tools"
STAGE="$RAVEN_DESKTOP/build-tools/deb"

mkdir -p "$TOOLS" "$STAGE"
cd "$STAGE"

echo "=== fetching"
# `apt-get download` writes into the working directory and needs no root. It is
# given the packages by name and takes whatever version the release has.
for package in m4 bison flex libfl2; do
    if ls "${package}"_*.deb >/dev/null 2>&1; then
        echo "have  $package"
        continue
    fi
    echo "get   $package"
    apt-get download "$package" >/dev/null 2>&1 || echo "  (no $package; see what fails)"
done

echo
echo "=== unpacking into $TOOLS"
for deb in *.deb; do
    [ -e "$deb" ] || continue
    dpkg-deb -x "$deb" "$TOOLS"
done

echo
echo "=== what is there now"
for tool in m4 bison flex; do
    found="$(find "$TOOLS" -name "$tool" -type f 2>/dev/null | head -1)"
    printf '%-8s %s\n' "$tool" "${found:-MISSING}"
done

cat > "$RAVEN_DESKTOP/build-tools/path.sh" <<'SH'
# Source this to put the fetched build tools in front of the path.
export PATH="$(dirname "$(find "${RAVEN_DESKTOP:-$HOME/raven-desktop}/build-tools" -name bison -type f | head -1)"):$PATH"
SH
echo
echo "source $RAVEN_DESKTOP/build-tools/path.sh to use them"
