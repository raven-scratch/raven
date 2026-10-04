# rv32ima — a RISC-V machine in raven, running two guests

A raven port of the Scratch project that runs
[mini-rv32ima](https://github.com/cnlohr/mini-rv32ima) — a 32 bit RISC-V hart
with no MMU — and boots Linux on it. The terminal is drawn by
[`lib/penfont`](../../../lib/penfont) rather than by the Scratch project's own
8 by 16 bitmap font.

Three images, three `.sb3` files, one machine. One command builds all three:

| image | | built as | and it says |
| --- | --- | --- | --- |
| `baremetal.bin` | 442 B | `dist/rv32baremetal.sb3` | hello, an address, a speed, and goodbye |
| `linux_image` | 2,945,224 B | `dist/rv32linux.sb3` | a Buildroot boot log, `Welcome to Buildroot`, and a login prompt |
| the Scratch project's own | 4,263,001 B | `dist/rv32scratch.sb3` | the same kernel with the original rootfs, and a root shell |

The first two are the images from
[`bjoernQ/mini-rv32ima-rs`](https://github.com/bjoernQ/mini-rv32ima-rs), the Rust
port this is checked against. The third is the one the Scratch project itself
carries and exists nowhere else: it is a list inside that `.sb3`'s
`project.json`, 4,263,001 bytes of Linux 6.1.14 with the original project's own
rootfs, and it is the image this example was first built around.

```
Hello world from RV32 land.
main is at:
80000044
Assembly code:
I'm an assembly function.
Processor effective speed:
5 Mcyc/s
Poweroff 2147483908
```

```
[    8.373000] 10000000.uart: ttyS0 at MMIO 0x10000000 (irq = 0, base_baud = 1048576) is a XR16850
[    8.373000] printk: console [ttyS0] enabled
[    8.373000] printk: bootconsole [uart8250] disabled
[    8.718000] Freeing unused kernel image (initmem) memory: 1104K
[    8.791000] This architecture does not have kernel memory protection.
[    8.791000] Run /init as init process
mount: can't read '/proc/mounts': No such file or directory

Welcome to Buildroot
buildroot login:
```

```
[   16.111000] This architecture does not have kernel memory protection.
[   16.111000] Run /init as init process
Welcome to Linux On Scratch!
Jan  1 00:00:22 login[29]: root login on 'console'
~ #
```

```sh
# the images, the device tree and the ALU's tables, then all three projects
node tools/build.mjs

cargo run -p raven -- check -m examples/raven/rv32ima/raven.toml
cargo run -p raven -- build -m examples/raven/rv32ima/raven-baremetal.toml --debug

# the only check that is about the guests rather than about the compiler
SCRATCH_VM_ROOT=path/to/scratch-vm node tools/check.mjs
```

`tools/build.mjs` needs a checkout of the reference:

```sh
git clone https://github.com/bjoernQ/mini-rv32ima-rs ref/mini-rv32ima-rs
```

It only reads the clone, so nothing has to build; to run the reference's own
`cli` beside this — which is where the first two outputs quoted above come from —
its `Cargo.toml` wants an empty `[workspace]` table first, because `ref/` sits
inside raven's workspace and cargo will otherwise refuse it.

The third image comes out of
`ref/Linux 6.1.14-rv32ima On Scratch.sb3` instead, and that file's `project.json`
is 162 MB of which almost all is the project's own 64 MiB of *RAM* written out
one byte to an item. Reading it as JSON costs a four gigabyte heap and buys
nothing, so the tool finds the list's name in the text, walks the array after it
to its closing bracket and reads the items — and the device tree beside it is not
read at all, because it is byte for byte the one `examples/cli.rs` carries.

`tools/check.mjs` loads all three built projects into a real Scratch VM, presses
the green flag, steps the runtime, and reads the terminal's own cell buffer —
which *is* the screen — for what each guest is supposed to have printed. For the
two Linux images it also drives the keyboard, and for the bare metal image it
counts the pen lines the cursor draws and drives the console's escape sequences
by hand. On the machine this was written on:

```
=== baremetal  (dist/rv32baremetal.sb3)
executed  7000062 instructions in 16 frames, 4 s
RAM       67108672 bytes
cursor    126 pen lines, x -223.5..-217.13, y -159.73..-143.98
colours   green #00FF00, after 0 #FFFFFF, after bare m #FFFFFF, after 39 #FFFFFF
ok   the greeting ran
ok   the assembly function ran
ok   main is where the image says
ok   the processor measured itself
ok   the machine powered off
ok   the machine executed
ok   RAM reaches the device tree
ok   nothing threw
ok   the cursor is drawn as a glyph
ok   the cursor covers a cell
ok   SGR 32 is the palette green
ok   SGR 0 returns to the default ink
ok   SGR m returns to the default ink
ok   SGR 39 returns to the default ink

=== linux  (dist/rv32linux.sb3)
executed  52283605 instructions in 246 frames, 31 s
RAM       67108672 bytes
keyboard  hat sent [97], poll sent [127]
ok   the console came up
ok   the kernel mounted a rootfs
ok   the guest booted
ok   a login is waiting
ok   a key hat reached the machine
ok   the polled key reached the machine
ok   the poll asked for the key
ok   the machine executed
ok   RAM reaches the device tree
ok   nothing threw

=== scratch  (dist/rv32scratch.sb3)
executed  50226951 instructions in 210 frames, 32 s
RAM       67108672 bytes
keyboard  hat sent [97], poll sent [127]
ok   the console came up
ok   the kernel mounted a rootfs
ok   the guest booted
ok   a shell is waiting
ok   a key hat reached the machine
ok   the polled key reached the machine
ok   the poll asked for the key
ok   the machine executed
ok   RAM reaches the device tree
ok   nothing threw

PASS
```

| | |
| --- | --- |
| `src/sprites/riscv.rav` | the machine: fetch, decode, the CSRs, the CLINT, the console, the frame loop |
| `src/sprites/terminal.rav` | 64 by 20 cells of penfont glyphs, with the reference's escape sequences, scroll and cursor |
| `src/sprites/input.rav` | the keyboard, as the bytes the guest's console reads |
| `src/rv32/io.rav` | the two byte streams the machine and the terminal share |
| `src/rv32/tables.rav` | generated, committed: the ALU's three 64 KiB tables and the byte-to-character table |
| `src/rv32/image.rav` | generated, one image at a time: the guest and its device tree |
| `src/penfont/` | the library and its table, installed by `font2vm.py` — do not edit them here |
| `tools/build.mjs` | reads the reference and the Scratch project, writes the two data files, builds all three projects |
| `tools/check.mjs` | runs all three built projects in a real VM and reads the screen |
| `tools/render.mjs` | draws the stage the pen actually drew, as a PNG |
| `tools/profile.mjs` | the frame, split by thread: the guest's arithmetic against the pen's |

## The three references, and which one this is

`mini-rv32ima` is the C. `mini-rv32ima-rs` is a Rust port of it. The Scratch
project in `ref/` is a translation of the C into blocks, and this is that
translation into raven — so the shapes are the C's (`state.extraflags`, `trap`,
`OPCODE_IDK-LOL`) and the checks are the Rust's.

Where the Scratch project and the Rust disagree, this follows the Rust, and the
four places that matter are fixed here rather than ported:

**`div` and `rem` truncate toward zero.** Scratch's `mod` and `floor` do not, so
the Scratch project's `div` rounds the wrong way for a negative quotient and its
`rem` takes the sign of the divisor instead of the dividend. Both are written
out here.

**`0x137` prints eight hexadecimal digits.** That is the reference's own
extension — `csr_write(0x137) => print!("{:08x}", writeval)` — and it is how the
bare metal image prints the address of its own `main`. The Scratch project
folded `0x136` and `0x137` together and prints decimal, which is why its
`80000044` would read `2147483908`.

**A step that returns a value ends the frame.** `wfi` and a write to the system
controller are the guest asking to stop; the Scratch project's frame loop clears
the value and runs straight on, so its bare metal image never powers off. Ours
carries it out to the check that turns it into `Poweroff`.

**A string has a case, and Scratch strings do not.** `byte_chars` is ordered by
code, and Scratch compares two strings without their case, so a search for `h`
answers with the item for `H` and every letter comes out a capital. Lines the
machine writes itself mark capitals, the way penfont's text language does; the
guest is unaffected because the guest's bytes never go through a search. The
same search also answers the digit `0` with the table's *tab*, because
`scratch-vm` reads a lone tab as the number zero for compatibility with a
Scratch 2 project — that one is written out rather than searched for.

And two places where being faithful mattered more than being clever, both of
which cost a debugging session:

**The device tree stays in the last 1728 bytes of RAM.** The bytes at
`0x8000_0000` are a stub that unpacks the kernel to `0x8100_0000`, so anything
left between the image and there is written over. A tree immediately past the
image — where a loader would normally put it — has the decompressor walk through
it, and the boot dies in an `ebreak` loop inside a spinlock whose lock word was
clobbered.

**`lr.w` writes nothing back.** A load-reserved that stores what it loaded turns
Linux' spinlock — `lr.w; bne; sc.w; bne; fence; beq; ebreak` — into an `ebreak`
loop inside its own trap handler.

## The cursor, and erasing

The cursor is the font's U+2588 FULL BLOCK, showing and disappearing on the
blink. It is not drawn at the text's scale: the block is the font's *line* box
and not its character box, 63 scan rows, so it is drawn at the scale that makes
those rows a cell's 16 units — which is what the reference got from a stamped
costume and what a rectangle of pen lines got wrong.

The blink is the reference's phase test with one character changed. It wrote
`mod(seconds, 2) > 1`, and `mod` by two is never more than one, so its cursor
was steady and the blink was dead code; `>= 1` is the test a cursor wants.

Nothing is ever drawn back off the paper. A cell that has to go — an `ESC [ J`,
a character arriving where one already is, the cursor leaving a cell, the blink
turning it off — says the page has changed, and the page is then *the buffer
drawn out whole* after the pen is cleared. There is no per-cell erase and no
partial redraw anywhere; the paper can only ever be a copy of the buffer.

Three attempts at erasing a cell are why, and the third is the interesting one.
The first drew one round-capped pen line as tall as the cell, which is a *disc*
seventeen units across in a seven unit cell, and punched black holes through the
letters either side. The second drew the font's own U+2588 FULL BLOCK in black —
the right ink, in sixty-three runs a cell, which is 126 renderer calls, because a
run is a pen-down dot and then a stroke. The third drew a rectangle of the cell's
own box, seventeen rows or seven columns of strokes, and it cannot be right
either: `EM` is 14 and `BASE_DROP` is 12 in a cell 16 tall, so the font's box is
two units taller than the cell and a tall glyph — a bracket, a bar, anything
reaching its em — draws *outside* the box the erase covered, and the sliver it
leaves is what a partial redraw costs. The whole-page redraw has no such
question to answer, and it is also less code: `blank_cell`, `repaint_cell` and
the flag between them all go.

It is not free, and this is the one place in the project where the cheaper thing
was given up deliberately. `tools/profile.mjs` on the Buildroot boot, before and
after:

| | partial erase | page drawn whole |
| --- | --- | --- |
| pen, lines a frame | 1,867 | 3,524 |
| pen clears a boot | 23 | 47 |
| console's thread | 1.1 ms/frame | 1.8 ms/frame (6% of the frame) |
| guest's throughput | 1.90M instr/s | 1.88M instr/s |

So the pen's volume roughly doubles — every frame that has output on it now draws
the page instead of one cell — and it costs 0.7 ms a frame, which is inside the
frame the machine hands back either way. What it buys is that the picture is
never a guess: it is the buffer, drawn in one piece, every time.

`node tools/render.mjs linux --frames 900` is what shows this: it steps the
built project in a real VM with a renderer that records every `penLine`, and
rasterises those lines into `dist/linux-900.png` — the picture above the check,
where the check is the meaning.

## Colours

The palette is the sixteen an ANSI terminal is expected to have — the eight and
the eight bright — with `colors[8]`, the eighth of eight, as the default ink
`ESC[0m`, a bare `ESC[m` and `ESC[39m` return to. The reference's was eight inks
all white except the third, which is the one green its SGR handler could reach,
and a page that can only be white is a page no full-screen program can draw:
colour is how a TUI says what is selected, and how `screenfetch` says anything at
all. The paper is still the backdrop and still black — SGR 40–47, 49 and 7 are
ignored — so "white paper, black ink" is not something this terminal can be asked
for.

`tools/check.mjs` asks for the ink directly, writes escape sequences into the
machine's console output by hand, and then reads not only `current_fg` back but
the ink the *cell* was drawn in: `ESC[32m` is the palette's green and the cell
says `#00AA00`, `ESC[34m` says `#0000AA`. The bare `ESC[m` is the port's own fix
— the reference tested the parameter against the *number* zero, and Scratch
compares `""` with `"0"` as unequal, so `ESC[m` returned nothing.

## What a TUI can ask for, and what it cannot

The console speaks enough ANSI to run a shell and to draw a screen with. `ESC[K`
— erase in line, all three modes — is the last of it that was missing and is
there now: a line editor rewrites a row and erases the tail the shorter version
of the line does not cover, which is why the boot log's line ends are ragged
where the kernel overwrites a longer line with a shorter one.

Nerd Font works. `src/penfont/` is the font table, and `font2vm.py` is what
installs it, so it is one `--charset` away from any set the font carries; here it
is `latin` plus every `nf-` family except the two big ones:

```sh
python lib/penfont/font2vm.py --project examples/raven/rv32ima \
  --charset latin,nf-ple,nf-dev,nf-logos,nf-seti,nf-oct,nf-cod,nf-fae,nf-weather,nf-extra,nf-iec,nf-pom
```

That is 2,797 glyphs and 210,460 runs, and it costs nothing that can be measured:
the glyph lookup is a binary search, so eleven comparisons became twelve and the
console's share of a frame stayed at 1.8 ms of 31. All of Nerd Font is 11,152
glyphs and 830,307 runs — `icons` rather than the list above — which is fine too
and puts 2.5 MB into the font table instead of 0.5, so it is a size decision and
not a capability one.

What is *not* there is the last of what a full-screen editor asks for: no scroll
region (`ESC[r`) and no wrap-off (`ESC[?7l`). Both are ignored rather than
refused, so a program that asks for them draws on the one screen it has and does
not get its own. Everything else a TUI needs is implemented and checked: `ESC[K`,
cursor addressing, erase in display, the scroll commands, the 256-colour SGR
forms (`ESC[38;5;Nm` and `ESC[38;2;r;g;bm`), the alternate screen
(`ESC[?1049h` and `ESC[?1049l`) and cursor hide (`ESC[?25l`).


## What it costs

`src/rv32/image.rav` is 11 MB of source for the Buildroot image, 16 MB for the
Scratch project's, and 0.03 MB for the bare metal one. The built project is
130 MB of `project.json` and 28 MB of `.sb3` for Linux and about 2 MB for bare
metal — most of it the image written one byte to a list item, which is how the
machine reads it. RAM is not written out: a byte the guest has not touched is not
in the list at all, and Scratch reads a missing item as zero, so the 64 MiB comes
into being only up to where the guest has written.

The machine runs at about two million instructions a second in Node with no
renderer. Either Linux' boot is around 50 million of them.

The pen is where the rest of it goes. A scroll moves the buffer by a row, and
ink cannot be moved, so the page is cleared and drawn again — around thirteen
thousand pen lines for a screen of text. The scroll therefore only *says* the
page moved and the redraw happens once, at the end of the frame: a boot log or a
shell echoing a burst redraws once instead of once a line. A frame of the bare
metal image that scrolled five times used to clear the pen five times and is now
one clear and one page, and erasing cells that are already blank — most of what
`ESC [ J` covers — costs nothing at all.

**The frame is the machine's, and that is what a stutter is.** A Scratch frame is
33 ms — `FrameLoop` steps the runtime at 30 frames a second — so a frame that
takes longer than that is a frame the page did not draw. The reference gives the
machine 50 ms and lets it keep the frame for 200 ms when it has nothing to show,
and both are longer than the frame itself, so the console draws in whatever is
left over and everything on screen — the boot log, the cursor's blink, a
keystroke's echo — arrives at the machine's pace rather than the page's.

`tools/profile.mjs` is what says so, and it says it by thread:
`node tools/profile.mjs scratch --keys` boots the image, reports the frame, the
pen and the split between the machine's thread and the console's, then times a
keystroke. What it measured, with the slice at the reference's 50/200 ms:

| | machine's thread | console's thread | frame |
| --- | --- | --- | --- |
| 50/200 ms slice | 114 ms | 3.4 ms | 187 ms |
| 30 ms slice | 29.5 ms | 1.1 ms | 31 ms |

The guest's arithmetic is 96% of the frame either way and the drawing is 3 or
4%: this is not a project whose lag is its pen. What the 200 ms slice did buy was
a *frame* the machine kept, and the price of it was everything else on the page —
five frames a second instead of thirty, and a keystroke's echo 696 ms later
instead of 176. Slicing at 30 ms — just under a frame, so the console's drawing
fits in the frame the machine hands back — costs the guest nothing that can be
measured (1.9M instructions a second either side of the change) and gives both
back. Two things follow from that. Longer frames are not more throughput: the
pen is a tenth of the clock and the rest is the same arithmetic stretched over a
longer frame. And everything that waits on a frame — the echo, the blink, the
screen — waits on the machine's slice, which is why the slice is a UI decision
and not only an emulation one.

## What is not right, and what the next person should know

* **The mini image carries `ed`, and no `vi`.** busybox 1.35's `vi` run with no
  file name asks the kernel to open a NULL path -- `vi_main` does
  `argv += optind`, so with no argument the *file* it edits is `argv[0]`, which
  is NULL -- and this kernel answers that with an unhandled load access fault
  instead of `-EFAULT`: `getname_flags` reaches `strncpy_from_user` with a NULL
  source, its word-at-a-time load at NULL+1 is not covered by the exception
  table (`do_trap_error` calls `fixup_exception` for a kernel-mode fault and
  dies only when there is no entry for it), and the guest prints `Oops - load
  access fault [#1]`, `badaddr 00000001`, `cause 5` and a `Segmentation fault`
  before `vi` draws anything. `vi <file>` was never affected; the image just
  does not keep the program. `tools/mini-image.sh` splices the `bin/vi` entry
  out of the kernel's own rootfs and installs a standalone `ed` at
  `/usr/bin/ed` -- a flat binary lifted out of the Scratch project's own rootfs,
  and the one program in the image this repository did not build.
* **A character typed at the Linux login is read on the kernel's timer, not on
  an interrupt.** The keyboard reaches the machine's console input at once —
  that is what the check measures — but the kernel's 8250 driver has no interrupt
  to read it on (the device tree gives the UART no `interrupts` and mini-rv32ima
  has no PLIC), so it falls back to a polled timer and reads the byte when that
  fires. The reference has the same machine and the same tree. How long that
  takes in wall clock is the frame's business, not the driver's: the guest's
  clock advances by one frame's worth of microseconds at a time, so the poll
  interval, about six frames, is 176 ms at a 30 ms slice and was a second at
  200 ms.
* **No MMU, and therefore no `sfence.vma`.** The kernel is built for
  `riscv-minimal-nommu` — it says so in its own device tree — and an `sfence.vma`
  traps as an illegal instruction.
* **`rdtime` reads zero.** `0xC00` (`cycle`) is the cycle counter and `0xC01`
  (`time`) is wired to nothing, exactly as in the C and the Rust, so a guest that
  wants a clock reads `cycle`. The bare metal image's speed figure is therefore a
  measurement of this machine and not of the reference's, which is why the two
  numbers differ.
* **The `ESC [ J` path was wrong twice over.** In the Scratch project a `stop this
  script` sits where a `return` was meant, and it exits the routine rather than
  clearing the rest of the screen; the listing has the `stop` inside the loop
  that clears a row, and the port returns after the row, which is what the code
  around it says it means. Underneath that, both versions read the sequence's
  parameter as `num(item 1 of …)` and test it against the number zero — and the
  empty string an absent parameter is, is *not* the number zero to Scratch. The
  parameterless `ESC [ J` that busybox' line editor sends with a backspace
  therefore fell past both branches into the whole-screen clear. That clear also
  emptied the console's own output queue, so the rest of the same write — the
  character the editor was re-echoing and everything after it — was thrown away,
  and a shell with nothing more to say left the screen blank until it did. One
  backspace, a black page. `tools/check.mjs`'s colours section is where the same
  empty-string-versus-zero trap in `ESC [ m` is held down.

## Reading the machine

`riscv.rav` runs `step` (1024 instructions, or until a trap or a control
transfer), `run_frame` (one frame of the guest, in seven numbered steps that
match the Scratch project's own comments) and `run_loop` (a frame a pass).
Everything in it is `warp` except `run_loop`, which is the one loop that hands
the frame back — a plain `repeat` in Scratch yields after every turn, so an
interpreter that is not `warp` runs at thirty instructions a second.
