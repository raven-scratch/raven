# desktop — an ARM machine in raven, built out of replaceable hardware

`rv32ima` is one hart and two devices: a console and a timer. This is the other
end of that — a *computer*. A motherboard with an address decoder and a bus, an
ARM926EJ-S with an MMU, SDRAM, a boot ROM, an interrupt controller, a timer, a
UART, a real-time clock, a keyboard controller, an LCD controller that scans a
framebuffer out of memory, an SD host, and an Ethernet controller. Linux boots
on it, and what you see on the Stage is the LCD controller's own pixel output.

Every one of those is a **module with a bus interface**, and the machine is
described by a **board file** that says which modules exist, where they are
mapped, how they are wired to the interrupt controller, and what clock each one
is driven at. Replacing the display is replacing one line of the board file.
Replacing the CPU is replacing another. That is the point of the project: not
"an ARM emulator", but a way to build a machine.

**There are two boards now, and the second one is the proof of that.** The
first is `boards/versatile-pb.mjs`: an ARM926EJ-S with an MMU and everything
listed above. The second is `boards/mini-rv32.mjs`: a 32 bit RISC-V hart with
no MMU, on mini-rv32ima's memory map — RAM at `0x8000_0000`, an 8250 console,
a CLINT, a system controller and a graphics card. It boots Linux to a root
shell and it runs coremark, duktape and `ed` typed at that shell, and it is
the same `tools/build.mjs`, the same bus module shape, the same
generated-from-the-board-file decoder, and the same monitor sprite drawing a
device's own output on the Stage. **Its console is the graphics card too**:
the kernel's `fbcon` writes it through `simplefb` into the card's own memory
and the monitor scans it out, so the Stage shows pixels the guest put there
and nothing in this project draws a character. See
[## The RISC-V board](#the-risc-v-board) at the end of this file: it says what
a different RISC-V board would change, which is the question the whole project
exists to answer.

```
boards/versatile-pb.mjs      the machine: modules, addresses, IRQ wiring, clocks
         |
         v  tools/build.mjs
src/board/decode.rav         generated: the address decoder and the wiring
src/cpu/luts.rav             generated: the ALU's three logic tables
src/mem/sdram.rav            generated: the memory, one Scratch list item a word
src/rom/bootrom.rav          generated: the firmware
         |
         v
src/board/bus.rav            the bus: one read, one write, one tick per device
src/cpu/*.rav                the CPU: ARMv5TE, CP15, MMU, exceptions
src/dev/*.rav                the devices
src/monitor/*.rav            the monitor: how a scanout becomes the Stage
```

> **Before you open it: if you press the green flag on
> `dist/desktop-arm-virt-linux.sb3` in
> Scratch, you will watch the boot ROM's coloured checkerboard for hours and
> conclude that nothing works. That is not the machine, and it is worth knowing
> before you spend the hours.**
>
> The project uses only vanilla Scratch blocks, so Scratch's editor *interprets*
> them, and the interpreter here retires about **8,400 guest instructions a
> second**. The kernel's framebuffer console takes the display over at roughly
> **90 million** instructions and the shell banner arrives at about **118
> million** — three to four hours, and a checkerboard the whole time.
> TurboWarp's VM compiles the same blocks to JavaScript and gets there in about
> two minutes, which is what `tools/check.mjs` and `tools/probe.mjs` run.
>
> If you would rather not wait for either, the machine writes the result out:
> **`dist/stage.png`** is the framebuffer the LCD controller is scanning at the
> end of a full boot, three times size, so the console font is legible. `node
> tools/probe.mjs --budget 240` and `node tools/check.mjs --budget 400`
> regenerate it.

## The machine

An ARM Versatile PB: an ARM926EJ-S (ARMv5TE, MMU, no VFP, no Thumb in this
build) on an AMBA AHB/APB motherboard, with SDRAM at `0x0000_0000` and the
PrimeCell peripherals where the board puts them. The guest is a Linux built from
source, and its console is the CLCD, so the boot log is pixels the LCD
controller produced rather than a serial stream we drew.

| module | part | what it is | at |
| --- | --- | --- | --- |
| `cpu/core` | ARM926EJ-S | ARMv5TE in ARM state, the decode tree, the exceptions | — |
| `cpu/mem` | CP15 c1–c13 | the two-level table walk, domains, permissions, faults | — |
| `cpu/alu` | the barrel shifter | shifts, the flag rules, the conditions, the logic tables | — |
| `cpu/state` | the register file | the banked `r13`/`r14`/`spsr` and the mode field | — |
| `mem/ram` | SDRAM | the memory the CPU, the LCD and the SD host all see | `0x0000_0000` |
| `dev/pl011` | PL011 ×4 | the serial ports, and the early console | `0x101F_1000` |
| `dev/sp804` | SP804 ×2 | the tick: two down-counters a block, and the kernel clock event | `0x101E_2000` |
| `dev/pl190` | PL190 | the primary interrupt controller and its sixteen priority slots | `0x1014_0000` |
| `dev/pl192` | PL192 | the secondary controller, cascaded onto VIC line 31 | `0x1000_3000` |
| `dev/versatile_sys` | the core module | the board's ID, LEDs, card detect, and `SYS_CLCD` | `0x1000_0000` |
| `rom/bootrom` | the boot ROM | reads a word at a time, aliased to zero at reset | `0x3400_0000` |
| `monitor/stage` | the Stage | scanout to pixels, 480x360 at 30 Hz | — |

The board's device list also names the parts this build of the machine does not
yet fit — the PL031 clock, the PL050 keyboard interfaces, the PL061 GPIO blocks,
the PL110 LCD controller, the PL181 SD host, the LAN91C111 Ethernet controller
and the PL080 DMA controller. They answer their addresses with zero rather than with a lie.
The board file is what says so, and `tools/build.mjs` prints it on every build.

## The bus

A device is a module that exports exactly these, and the bus knows nothing else
about it:

```rav
pub proc <dev>_read(p_off: num) -> num;             // one 32-bit register read
pub proc <dev>_write(p_off: num, p_value: num);     // one 32-bit register write
pub proc <dev>_tick(p_cycles: num);                 // advance by N cycles
pub proc <dev>_reset();                             // back to power-on state
pub proc <dev>_irq() -> num;                        // the interrupt line
```

An interrupt is not a callback. A device raises a line by answering `_irq` with
one, and the interrupt controller reads it during its own tick. That is the only
way the board lets two modules see each other, so any controller can be swapped
for any other and any device can be moved to a different line. The wiring is in
the board file and comes out in the generated `vic_line` and `sic_line` tables.

The address decode is a table and not a chain of comparisons. Every device on
this board lives inside one 2 MiB window, so the window is divided into 4 KiB
pages and `page_dev` names the device that answers each one. A page with nothing
on it reads back as zero, which is also what an unmapped address everywhere else
reads back as — so a hole in the memory map needs no special case.

## The monitor

The LCD controller is a device like any other: it has registers, it has a
framebuffer in SDRAM, and it produces a horizontal stream of pixels at a
programmed resolution and refresh. The **monitor** is what turns that stream
into the Stage, and it is the piece that is allowed to know what Scratch is.

The default monitor is 480 by 360 at 30 Hz, which is Scratch's own Stage and its
own frame. It draws what the controller scanned out, and nothing is read out of
guest memory that the controller did not itself read; a framebuffer console is
pixels here, not characters. A TurboWarp build with a different Stage is a
different monitor module, the same way `rv32ima` takes `--stage WxH`.

## Where this is

**As it stands.** The machine boots its own firmware, hands a Linux kernel the
ARM boot protocol's registers, and runs that kernel to a root shell. It brings
up its memory, its MMU, its interrupt controller, its timer, devtmpfs, an
initramfs and its own framebuffer console -- and **what the Stage shows is that
console**: the Linux logo and the kernel's text, read out of the framebuffer the
PL110 is scanning, which is not the boot ROM's pattern and not at the boot ROM's
address. `tools/check.mjs` asserts eight things about that run in a real Scratch VM and
all eight pass, and then *measures* a ninth — the Stage against the
controller's scanout:

```
ok   the boot ROM wrote its three bytes to UART0
ok   the timer reached the processor through the interrupt controller (59 ticks)
ok   the monitor drew the controller's scanout (788610254 pen lines)
ok   the pattern in the framebuffer reached the Stage (1795 pen colours)
ok   the Stage is the kernel's framebuffer console, not the boot ROM's checkerboard (82.1% background and 14.5% lit; the pattern is at most 25.0% background and 62.5% lit)
ok   the kernel moved the scanout off the boot ROM's framebuffer (LCD_UPBASE 0xe00000)
ok   the kernel booted
ok   the initramfs ran /init
```

The pen count is a whole run of whole-panel passes — 788,610,254 lines over
50,710 erases, which is about 15,600 runs a pass — and not a count of how much of
the screen changed. See "What one whole-panel pass costs" below for why it is
that large and what was done about it.

The last measurement is the invariant the whole display rests on: the Stage
against the controller's own scanout, sampled at the stage row each panel row's
stroke starts on. It is asserted now — see [the invariant the display rests
on](#the-invariant-the-display-rests-on) — and it is exact, because a pass that
erases the pen layer and repaints the whole panel leaves no older ink for the
sample to find.

**The colour count is the one assertion here that proves nothing, and it was
already there.** `the pattern in the framebuffer reached the Stage` counts the
distinct pen colours the monitor drew, and *both* pictures satisfy it: the
checkerboard with eight, the kernel's console with 1,797. A colour count cannot
tell a checkerboard from a console, because it does not look at shape, position
or proportion.

**1,797 colours is not the checkerboard. The boot ROM's pattern is 4,800 pen
lines in eight colours, each exactly 12.67% of the Stage.** That is measured and
not reasoned: run this same project with the compiler off, which is what
Scratch's own editor does, let it sit on the boot ROM's pattern, and the monitor
records 4,800 lines in eight colours; run it compiled to the handover and the
same counter records 1,317,084 lines of which 85.8% of the Stage is the
console's black background and 13.8% is its grey ink -- and the 1,797 colours
are the floating-point noise of the 179 distinct pixel values a grey console
actually contains. So the earlier reading of this section -- that 1,797 colours
*was* the checkerboard, and that the kernel never took the framebuffer over --
was wrong.

**The assertion that can tell them apart is about shape, and it is new.** The
boot ROM's pattern is a regular grid of thirty-two pixel blocks of eight
saturated colours: two of those colours are dark (black and blue, a quarter of
the area), five are lit, and none of it is background, so every pixel is ink.
A framebuffer console is the other way round -- one background colour under most
of the area with a thin minority of glyph pixels. The check rasterises the pen
lines the monitor actually issued, later ones over earlier ones, and requires
most of the Stage to be background. What that proves: the pixels the monitor put
on the Stage by the end of the run are predominantly a dark background, which
the checkerboard is not, so the console took the display over. What it does not
prove: that the ink is legible text, that the text is this kernel's, or that a
browser draws what the renderer recorded -- it is the monitor's own output and
not a screenshot. It is also a statement about the *end* of the run: a budget
too short to reach the handover leaves the pattern on the Stage and fails there,
correctly.

**The check leaves the picture behind as well as the boolean.** On every run it
writes `dist/stage.png`, the framebuffer the controller is scanning at three
times size, which is the only way to see the result without either waiting hours
for the interpreter or reading a colour count. `tools/probe.mjs` writes the same
file and prints the registers, the framebuffer and a per-pixel picture of it.

The guest reaches the shell at about 118 million instructions, and
`## The framebuffer handover` below is where the older reading of this section
is corrected in place.

Everything between here and `## The guests' storage` is the record of how it got
there -- written in the order the questions were asked, with the corrections
left in, because the corrections are the useful part.

The machine boots its own firmware, displays, ticks and takes interrupts, and
the firmware hands a Linux kernel the ARM boot protocol's registers with the
kernel, the initramfs and the device tree in place. The processor then runs that
kernel from its first instruction through CPU identification, its own page
tables, the moment it turns its MMU on, and on into the kernel proper, where it
reads its device tree, sizes its memory, brings up the interrupt controller and
the timer, and prints all of it:

```
Linux version 6.6.158 (dilemmagx@Yiming) (arm-buildroot-linux-gnueabi-gcc.br_real ...) #1
CPU: ARM926EJ-S [41069260] revision 0 (ARMv5TEJ), cr=00003137
CPU: VIVT data cache, VIVT instruction cache
OF: fdt: Machine model: ARM Versatile PB
Kernel command line: console=tty0 console=ttyAMA0,115200 rdinit=/init
Memory: 7696K/16384K available (5423K kernel code, 188K rwdata, 1160K rodata, 208K init, 154K bss, 8688K reserved, 0K cma-reserved)
SLUB: HWalign=32, Order=0-3, MinObjects=0, CPUs=1, Nodes=1
NR_IRQS: 16, nr_irqs: 16, preallocated irqs: 16
VIC @(ptrval): id 0x00000000, vendor 0x0000
FPGA IRQ chip 0 "interrupt-controller" @ (ptrval), 20 irqs, parent IRQ: 47
clocksource: arm,sp804: mask: 0xffffffff max_cycles: 0xffffffff, max_idle_ns: 1911260446275 ns
sched_clock: 32 bits at 1000kHz, resolution 1000ns, wraps every 2147483647500ns
sched_clock: 32 bits at 24MHz, resolution 41ns, wraps every 89478484971ns
```

That log is the kernel's own ring buffer. What is *not* yet true is that it is
also on the wire: `console_writes` stays at six, and six is the firmware. The
reference machine puts the same lines on its serial port from `Booting Linux`
onward, which it can only do through an early console, so the kernel has one and
this machine is not getting it -- the device tree's `stdout-path` is a path into
`/amba/serial@101f1000` and the kernel is built with `CONFIG_SERIAL_EARLYCON`,
so the distance is either the `early_ioremap` that puts the port somewhere the
kernel can write to, or the boundary between that mapping and this machine's bus.

The last line the log reaches:

```
Console: colour dummy device 80x30
printk: console [tty0] enabled
```

**Those two lines are not a stopping point, and reading them as one was a
mistake made here.** `Console: colour dummy device 80x30` is `dummycon_init` --
the *boot* console, which every kernel prints before any real console exists --
and `printk: console [tty0] enabled` is `register_console` for the VT console,
which says nothing at all about a framebuffer. On the reference machine both
lines appear as lines 29 and 30 of a 123-line boot that goes on to userspace.
They are two lines this machine failed to get past, not two lines nothing gets
past. See `## The framebuffer handover` below for what actually stops the
picture.

**Everything from here to `## What is not built yet` is the record of a round
that was already out of date when it was written, and it is left standing
because the corrections it earned are worth more than a tidy history.** The
machine does not stop in `do_initcalls`, the reference does not stop at
`/init`, and the `vsnprintf` reading below is an inference about a stop that was
not the stop. `## The three bugs that were left` is where the boot actually
ended and what it took to finish it.

What stops it now is the kernel's own `BUG()`, and by then it is a long way in:
5,650,781 instructions, through the mount caches, devtmpfs, the futex table and
into `do_initcalls`. The call site is `ptp_classifier_init`, which ends

```
c06baa64:  bl  bpf_prog_create
c06baa68:  cmp r0, #0
c06baa6c:  beq c06baa74
c06baa70:  udf #0x1f2          @ BUG()
```

and `r0` came back as `-EINVAL`. That value has exactly one source in
`bpf_prog_create`:

```c
	if (!bpf_check_basics_ok(fprog->filter, fprog->len))
		return -EINVAL;
```

which is false only when the filter pointer is null, the length is zero, or the
length exceeds `BPF_MAXINSNS`. **It is not that.** `ptp_classifier_init` sets the
length with `mov r3, #66` and the pointer from a literal that `nm` names
`ptp_filter`, so the pair handed over is `{66, 0xc06cae90}` -- valid on both
counts, and the sixty-six instructions at that address are byte-identical to the
ones in the image.

That leaves `bpf_prepare_filter`, the only other thing in `bpf_prog_create` that
can return `-EINVAL`, and inside it `bpf_check_classic`: the kernel's classic BPF
verifier refusing a filter the kernel itself compiled in. That is a real
verifier -- loops, signed comparisons and jump arithmetic -- and it is the next
thing to read, because nothing else between the filter being handed over and the
`BUG()` can produce that error.

The filter itself is not the problem, and that can be settled rather than
argued: all sixty-six entries are in the image, and checking each against every
rule `bpf_check_classic` applies -- the allowed-code table, the `ja` range, both
conditional targets, division by zero, shift widths and the sixteen memory slots
-- passes every one. So the verifier is being driven into its `-EINVAL` tail by
the machine, not by the filter.

Where it stops is now answered, and it is not the loop. The ring shows the loop
exiting exactly as it should -- `cmp r5, r1` at `c0449618`, the two increments,
`bne` not taken -- and the error coming from the check *after* it:

```
c0449628:  add  r7, r4, #48         @ r7 = insns
c044962c:  add  r3, r7, r5, lsl #3  @ r3 = one past the last entry
c0449630:  ldrh r3, [r3, #-8]       @ r3 = the last entry's code
c0449634:  bic  r3, r3, #16
c0449638:  cmp  r3, #6              @ is it a `ret`?
c044963c:  beq  c04497d8
c0449640:  mvn  r5, #21             @ -EINVAL
```

The last entry's code is `0x06`, which is `BPF_RET`, so the check should pass --
and the filter is right, so the instruction is wrong. At the error exit `r3` holds
`0xc183f240`, which is not the code but **the address the load was meant to read
from**: `ldrh` left its base register exactly as it found it.

That is what a *store* does, not a load. `ldrh r3, [r3, #-8]` is being executed as
`strh r3, [r3, #-8]`, and the reason the whole boot fails is that one halfword
load is being read as a halfword store.

The cause was one instruction earlier than the effect, and in the dispatcher
rather than the decoder. `cond 000 P U ...` with `P` set and `U` clear has bits
27:23 of `00010` -- the same five bits an `mrs` or a `bx` has -- so
`ldrh r3, [r3, #-8]` was sent to the miscellaneous space, matched nothing in it,
and fell out as a **comparison**. A load writes its destination and a comparison
does not, so the register kept the address the load was meant to read from, the
verifier saw an address where `BPF_RET` should be, and the kernel `BUG()`ed a
long way from the mistake. Bit 7 is what separates the two spaces; the extra
loads and stores are now decided first, and the outcome is:

```
Kernel panic - not syncing: GENL: Cannot register controller: -22
```

**285,140,104 instructions** -- fifty times further than the round before, past
`bpf_prog_create`, through the rest of the socket-filter setup, and into the
general netlink layer, where it panics with an `-EINVAL` of its own.

That panic is now narrowed to one kernel source line, because the console works
well enough for the kernel to say so itself:

```
WARNING: CPU: 0 PID: 1 at net/netlink/genetlink.c:470 genl_register_family+0x2e8/0x564
```

Line 470 is `WARN_ON(grp->name[0] == '\0')`, the first of the two checks over a
family's multicast groups. Everything it reads is right in the image:
`genl_ctrl.n_mcgrps` is `1` at offset 32, `genl_ctrl.mcgrps` is a pointer to
`0xc05aba98`, and the sixteen bytes there are `"notify"` and zeros. The loop runs
once, on a group whose name begins with `'n'`, and the machine reads a zero.

## A second correction

The round before this one said the machine's copy of that pointer had moved --
image `0xc05aba98`, machine `0xc05abaac`, twenty bytes along. That was wrong, and
it was wrong because of how the evidence was read rather than what it said: an
`objdump -s` line prints the address of its *first* word, and the pointer I
called `+56` was the word at `+60` of the line before it.

Dumping all twenty words of `genl_ctrl` out of the running machine and comparing
them one for one against the image, **every word is identical**, both pointer
fields included. The structure is not corrupt, and the group name is where the
kernel put it.

So the WARN is not a corrupted family. It is the machine reading an address other
than the one it should, and the way to find which is to watch the instruction
that loads the group pointer and read the address it produces -- not to infer it
from a disassembly listing and a dump whose alignment has to be tracked by eye.
That is the third time in four rounds that inference has cost a correction, and
the instruments are good enough now that there is no reason to keep doing it.

## Watched, and what it says

Watching the machine at the byte load gives three facts that cannot be argued
with:

```
r7 = c05aba98     the group pointer, and the address the load should use
word c05aba98 = 69746f6e   "noti" -- the name really is there
r3 = 0            what the load produced
```

and `r0 = 0x10`, which is the distance from `c05aba98` to `c05abaa8` -- a zero
word just past the name. So the load is producing the contents of `[r7 + r0]`
for an instruction, `ldrb r3, [r7]`, that has **no offset at all**.

The offset decoder is not the fault, and that can be checked rather than
believed: `cpu_class_load_store` takes `I` from bit 25, and with it clear the
offset is `cpu_ir % 4096`, which for `e5d73000` is zero. The address arithmetic
is right. What is left is everything after it -- the translation cache, or
`cpu_read8` itself -- and that is where the next round starts.

## And then the measurements disagreed

Watching one instruction further settles the load and moves the contradiction
into the open. At `c0464474`, immediately after `ldrb r3, [r7]`, `r3` is `0x6e`
-- the load is *correct*. At `c0464478`, immediately after `cmp r3, #0`, the
status register is `0x20000053`: the zero flag is **clear**. The branch there is
`beq`, so it must fall through, and the family should register.

It does not, and the trace says why: the pc ring runs `... 4474, 4478, 4624` --
straight past the fall-through into the warning path, which only `Z` set can
reach. The pointer is right, the memory is right, the byte is right, the flags
are right, and the branch took the other road anyway.

That is as far as this round got, and it is a better place to stop than a guess:
every value involved has been read out of the running machine, and exactly one
of them is inconsistent with the others. Either the branch or the ring is
lying, and the next round should assume neither -- watch the instruction after
the branch and read the pc, rather than trusting a ring whose entries may be
one step out.

## It was neither, and that matters more

Reading `r4` at the warning answers the question and corrects the correction. The
warned-about family *is* `genl_ctrl` (`0xc0675584`), and the line argument is
`470` -- but `r7`, the group pointer, is `0xc05abaa9`, which is the group array
plus seventeen. In the run where the branch fell through, the same register was
`0xc05aba98`. Same instruction, same boot, two values.

Neither run is lying. **The machine is not deterministic across runs**, and it
has no business being: the harness steps it against the wall clock, so an
interrupt lands after a different number of instructions each time, and the boot
takes a different path through the same code. The pc ring is honest; the two
runs were simply not the same run.

That retroactively explains the last three corrections, all of which were
"the machine did X" conclusions drawn from single runs of a machine that does X
on one run and Y on the next. **A measurement of this machine is one sample, and
a disagreement between two samples is not evidence of a bug until the machine is
made to repeat itself.** It now can:

```sh
node tools/slice.mjs --limit 5000000      # twice: 5000000 and 5000000
```

`--limit N` sets `machine_limit`, and the machine's own slice loop halts when the
*guest* has retired N instructions. The runtime's step is not a unit of anything
-- the sequencer budgets it by wall clock, so a step is a variable amount of
machine, and a fixed *number* of steps is no more reproducible than a fixed
number of seconds. The guest's instruction counter is the only counter here that
belongs to the emulated machine rather than to the host, and stopping on it stops
in the same place every time. The step count still varies between runs, because
the harness still slices by clock; the guest state does not.

It earned its keep on the first two runs. The `genl_init` panic reproduces
exactly -- same instruction count, same registers, twice:

```
node tools/slice.mjs --limit 7500000
    Kernel panic - not syncing: GENL: Cannot register controller: -22
```

So that bug is real and reachable every time, and the contradiction between the
two pc rings in rounds 30 and 31 is explained rather than mysterious: those were
wall-clock runs, the boot genuinely varies between them, and both rings were true
of their own run. Neither was lying and neither was the machine -- the two
measurements were simply of two different machines.

Which is worth stating plainly, because it is the mistake three "corrections"
were made of: **a trace is evidence about one run, and an emulator that varies
between runs turns every trace into an anecdote.** Bound the run by the guest's
own instruction count and it stops being one.

## The fix, and what it moved

The opcode field was mapped wrongly. The assembler settles the order -- `smlabb`
is op 0, `smlawb` op 1, `smlalbb` op 2 and `smulbb` op 3 (`e1003281`,
`e1203281`, `e1410382`, `e1600281`) -- and the code accumulated on op 1 alone and
treated op 3 as a top-half multiply. So every `smlabb` ran as a `smulbb`: the
product, without the pointer it was meant to add.

With that corrected the panic is gone and the boot runs on:

```
Trying to unpack rootfs image as initramfs...
jffs2: version 2.2. (NAND) 2001-2006 Red Hat, Inc.
romfs: ROMFS MTD (C) 2007 Red Hat, Inc.
io scheduler mq-deadline registered
io scheduler kyber registered
io scheduler bfq registered
```

And the display is no longer a test pattern. `tools/check.mjs` now reports:

```
pen          5760218 lines recorded
monitor      31690 frames, 17464320 pixels read, 2880109 runs drawn
ok   the pattern in the framebuffer reached the Stage (1797 colours)
ok   the kernel booted
FAIL the initramfs ran /init
```

**1797 colours and five and a quarter million pen lines** -- up from eight
colours and forty-eight hundred. The kernel is drawing its own boot log through
the PL110, glyph by glyph, from the framebuffer the controller is reading. That
is the display half of this project's whole point, and it is the first time the
Stage has shown something the *guest* put there rather than something the
firmware did.

What is left is userspace, and the log does not stop where it looks like it
does. `io scheduler bfq registered` is the last *line*, but the machine is past
that and inside `inflate_fast` -- zlib's decompression loop -- because
`Trying to unpack rootfs image as initramfs...` is followed by the gunzip of a
1.19 MB archive, and the initial ramdisk has to come apart before `/init` can be
anything. `nm` names it directly:

```
c023a084   inflate_fast
```

So nothing is stuck and nothing is wrong. The boot is at the point where the
kernel stops printing and starts decompressing, and the only thing between here
and a `raven desktop` banner is the number of instructions the check is willing
to spend.

It is worth being exact about how much, because a first attempt to run further
looked like a hang and was not. `--limit N` bounds the *guest*; it does not
replace the wall clock, and `tools/slice.mjs --limit 400000000` still stops after
the default forty seconds -- at 60,906,088 instructions, inside the same
function. The registers move between runs (`r9` and `r11` walk through
`.rodata`), so it is decompressing. A check that wants `/init` has to budget
minutes, not seconds, and `--budget` is measured on the host in the ordinary
way: the guest does about 1.5 million instructions a second here, so the
remaining distance is somewhere in the low hundreds of millions and the number
to try is in the many hundreds of seconds.

That was the guess, and a ten-minute check has now paid for it and falsified it.
`--budget 600` -- roughly nine hundred million instructions -- still ends with

```
pen           13198830 lines recorded
monitor       145122 frames, 40548800 pixels read, 6599415 runs drawn
ok   the kernel booted
FAIL the initramfs ran /init
```

so the remaining distance is not "the low hundreds of millions", and the honest
statement is that it is more than nine hundred million and unknown. The pen count
being large is not evidence of progress either: the monitor repaints the whole
framebuffer every frame, so at thirty frames a second for six hundred seconds
most of those thirteen million lines are the same pixels drawn again.

Whether the machine is still decompressing at that point or has settled into a
loop that advances slowly is not known, and the difference matters -- one is a
bill and the other is a bug wearing a bill's clothes.

**It is a bill.** Sampling the same state at two instruction counts settles it
without spending another ten minutes on the clock:

```
--limit 60000000    pc c023c034    r0=fffff800  r3=c0b10638
```

`0xc023c034` is *past* `inflate_fast`, which lives at `0xc0239db4`. The
decompressor finished, the machine moved on, and the log simply has nothing to
say in the stretch it is in. Nothing is stuck; the guest is doing work that does
not print, and the only thing between it and `/init` is time the check has not
been given.

## And the first thing it found

With the machine repeating, the `genl` warning can be examined instead of
reconstructed. Two readings, both reproducible, bracket one instruction:

```
c046446c:  smlabb r8, r8, sl, r7      @ r8 = (r8.lo * sl.lo) + r7

after it (c0464470):   r7 = c05aba98   r8 = 0x11
at the warning (4630): r7 = c05abaa9   r8 = 0x11
```

`r8` is `17`, which is what the `mov r8, #17` two instructions earlier put
there, and it is still `17` after the multiply. `r7` is the group array, and by
the warning it is the group array **plus seventeen** -- which is exactly the
number the multiply was asked to produce.

So `smlabb r8, r8, sl, r7` computes the right value and does not write it to its
destination register. The value lands in `r7`, which is `Rn` -- the accumulate
operand -- and that instruction is `cpu_halfword_multiply`, whose whole job is
to tell `Rd` from `Rn` from `Rs` from `Rm`. Everything downstream of it follows:
the group pointer is seventeen bytes into the array, the name byte read from
there is the zero padding, and the kernel panics four hundred instructions later
in a function that has nothing to do with any of it.

## A correction

The round before this one said the machine was hiding a fault because the SDRAM
is a Scratch list that grows past its size, and that the stack pointer at the
`BUG()` was a megabyte and a half past the end of memory. Both of those are
wrong, and they are worth writing down rather than quietly deleting.

`bus_write32` grows nothing. An address that is neither RAM, nor flash, nor a
peripheral is *dropped*, and `bus_read32` answers zero for it. Permissive -- and
QEMU's unassigned memory region does the same, so it is not what separates the
two machines.

And `0xc1811ed0` is not evidence of anything. The linear map does end at
`0xc1000000` for a 16 MB machine, but that is exactly where a vmalloc address
lives, and `tools/slice.mjs`'s `word` translates by subtracting `PAGE_OFFSET`,
which is the linear map's rule and not vmalloc's. It could not have read that
frame wherever the frame was, so its failure says nothing about the pointer.
**Before drawing conclusions from an address above `0xc1000000`, the tool needs
a page-table walk.**

The check sees none of this yet, which is its own small mystery and an easier
one: `tools/check.mjs` reads six bytes where `tools/slice.mjs` reads
sixty-eight, and six is what the firmware writes. The difference is that the
check never halts the machine, and sixty-eight is six plus the 56-byte `STOP`
report that the machine itself writes when `--stop` ends a run. So the check is
seeing a console that the machine has already reset -- the guest writes the
board's reset register, the trace is cleared, and the firmware starts again. The
kernel panicking and rebooting is the thing to watch for, not the trace.

It is worth saying plainly what is *not* in doubt. **The guest is good.** Booted
on a reference machine (`tools/ref/`), the same kernel, the same initramfs, the
same device tree and the same memory image reach `Run /init as init process` in
about a second. So the images, the device tree and the boot protocol are right,
and the distance that remains is the emulator's.

Every claim above is measured by running the built `.sb3` in a real Scratch VM,
not by reading the code. `node tools/check.mjs` asserts that the firmware wrote
its bytes to UART0, that the timer reached the processor through the interrupt
controller and returned through `subs pc, lr, #4`, that the monitor drew the
controller's scanout as 4800 pen lines in eight colours, and that the kernel
loaded and started. The firmware's four pass, and so does **the kernel booted**:
the kernel's own log now reaches the PL011 and `console_trace` holds it. The last
one, `the initramfs ran /init`, waits on the panic in `genl_init`.

Getting the log onto the wire needed one thing and it was not the machine. The
kernel is built with `CONFIG_SERIAL_EARLYCON` and the tree has `stdout-path`
pointing into the AMBA bus, which is what the reference machine uses -- and it
printed nothing here. Naming the console on the command line instead,
`earlycon=pl011,0x101f1000`, and the same kernel writes 4312 bytes. So the
distance was the device-tree lookup and not the mapping, which is worth knowing
because the two look identical from `console_writes`: a console that never
registered and a console whose port is mapped somewhere else both count zero.

Which VM runs those blocks is a choice the check makes. The vanilla Scratch VM
interprets them, at about three thousand instructions a second, which is a boot
measured in hours; TurboWarp's VM compiles the same blocks to JavaScript first
and is what makes a boot checkable at all. It retires a million instructions a
second in the firmware and tens of thousands in the kernel, and it is where the
number in the paragraph above comes from. `SCRATCH_VM_ROOT` picks the checkout
and the compiler is switched on when the runtime has one, so the vanilla VM
still answers for what the blocks say and TurboWarp answers for whether the
machine runs. `tools/slice.mjs` chooses the same way, because a debugging run
that is three hundred times slower is a debugging run nobody makes.

The pieces, and where each comes from:

| what | how | size |
| --- | --- | --- |
| `images/kernel.img` | `10-kernel.sh`: the kernel's own `versatile_defconfig`, plus a framebuffer console, devtmpfs and an empty command line | 7.3 MB |
| `images/initramfs.cpio.gz` | `20-guest.sh`: BusyBox 1.36.1 built static, an `/init` that mounts proc and sysfs and becomes a shell, packed by `mkcpio.py` | 1.2 MB |
| `images/versatile-pb.dtb` | `boards/versatile-pb.dts`, compiled by `dtc`, with the initramfs addresses and the memory and flash sizes substituted in from the board file | 3.9 KB |
| `images/bootrom.bin` | `30-rom.sh`: `tools/rom/bootrom.S`, assembled by the cross toolchain | 580 bytes |
| `images/flash.bin` | `tools/build.mjs`: all four, laid into the machine's flash chip -- which is what the reference machine is given | 7.4 MB |

All four are laid into the machine's flash chip by `tools/build.mjs`, in the
order the boot ROM looks for them, and the offsets and destinations are the
board file's -- the same ones the device tree is built with, so the firmware and
the tree cannot disagree about where the initramfs is.

## The guests' storage, and why the machine has a flash chip

This machine has no disk. It has a sixteen megabyte NOR chip in the socket the
static memory controller drives, holding the firmware at the bottom and the
kernel, the initramfs and the device tree above it. Its first thirty-two
kilobytes are aliased to address zero while the board's remap bit is clear,
which is where an ARM926 comes out of reset and therefore where the firmware
has to be.

The boot ROM's last act is the interesting one. It copies the three guest
blocks into memory, and then it jumps to a label in the *flash window* rather
than to the next instruction -- because the next thing it does is clear the
remap bit, which puts SDRAM at address zero and takes that code out from
under the program counter. The same bytes are still readable at
`0x3400_0000`, which is where it is running from, and that is why a Versatile
can do this at all. Then `r0` is zero, `r1` is the machine type, `r2` is the
device tree, and it is the kernel's machine.

## The one bug that was the firmware's: a stack pointer that is not the one you think

The handler saves the registers it uses, and the first version of it saved
them onto a stack. It worked: the dots appeared, the timer ticked, the
interrupt returned. What it did not do was preserve anything.

Every ARM mode has its own `r13`. The handler runs in IRQ mode, so the stack
it saves into is a *different register* from the one the reset code set up --
and its reset value is zero. So the save descended below address zero, where
nothing answers, and the restore read back the zero an unbacked address reads
back as, putting a zero in every register it had been asked to preserve. The
handler looked correct and silently corrupted its caller: a register the
interrupted code was counting with came back zero on every tick, and a loop
that should have run sixteen thousand times ran for ever.

It is the first bug in this project that was the *firmware's* rather than the
machine's, and it is the one that would have been hardest to see from the
console: the symptom was a program that did not terminate, three functions
away from the cause. The handler now saves into a fixed area of memory, which
cannot be wrong about where it is -- and interrupts are masked on entry, so
there is no second handler to collide with it.

## The framebuffer handover

**The handover happens, and this section was written before it did. Read the
first half of it as the correction and the second half as the part that is still
true -- which is about the *reference* machine and not about this one.**

What this machine's own kernel says, out of the guest's console at about 90
million instructions:

```
versatile-tft-panel 10000000.sysreg:display@0: detected: Sanyo TM38QV67A02A
drm-clcd-pl111 10120000.display: set up callbacks for Versatile PL110
drm-clcd-pl111 10120000.display: found panel on endpoint 0
[drm] Initialized pl111 1.0.0 20170317 for 10120000.display on minor 0
Console: switching to colour frame buffer device 40x30
drm-clcd-pl111 10120000.display: [drm] fb0: pl111drmfb frame buffer device
```

and no `no panel detected`, no `-EPROBE_DEFER`, no `No bridge`, no `vblank wait
timed out`. This board reports panel id `0x00`, which is SANYO_3_8 and is in the
driver's table, so the panel binds -- and it says so in as many words,
`detected: Sanyo TM38QV67A02A`. `pl111_display_enable` then programs
`LCD_UPBASE` with its own allocation's address, `0x00E00000` on the run this was
measured on, and it is the *register* the monitor reads: `src/dev/pl110.rav`
stores it at word 4 of the controller and `clcd_pixel` walks `clcd_ubas` for
every pixel `src/monitor/lcd.rav` draws. So the boot ROM's fixed frame at
`0x00D00000` (`tools/rom/bootrom.S`) stops being what the monitor sees the
moment the kernel commits a mode, and by the end of the boot the memory there is
not the pattern at all -- it is the kernel's own data.

The picture that comes out is the Linux logo and the kernel's console text, and
that is what `dist/stage.png` is: 84% of the scanout black, 12% one grey, and
179 distinct pixel values in total, in glyph shapes. The old colour assertion
could not have told anyone that; the rasterisation described in
`## Where this is` can, because it looks at how much of the Stage is background
rather than at how many colours are on it.

### What is still true, and it is about the reference

`set up callbacks for Versatile PL110` is printed by `pl111_versatile_init` from
*inside* `pl111_amba_probe`, so by the time it appears the PrimeCell id has
matched, `apb_pclk` has resolved and the `arm,versatile-sysreg` regmap has been
found. Three occurrences are three probe attempts, and three is the tell: the
driver is deferring, not failing.

The chain, on the reference:

1. `drivers/gpu/drm/panel/panel-arm-versatile.c` reads SYS_CLCD, masks the panel
   id out of bits 8-12, and matches it against `versatile_panels[]` -- SANYO_3_8
   `0x00`, SHARP_8_4 `0x01`, EPSON_2_2 `0x02`, SANYO_2_5 `0x07`, and nothing
   else. No match gives `dev_info("no panel detected")` and `-ENODEV`, so the
   panel device never binds. QEMU's `SYS_CLCD` does not name a panel this kernel
   knows, which is why the reference's log line 88 says `no panel detected`.
2. `drivers/gpu/drm/pl111/pl111_drv.c` asks for the panel, gets `-EPROBE_DEFER`
   because it has not bound, and returns `-EPROBE_DEFER` itself. That is
   permanent: the panel is not going to bind later.
3. `pl111_modeset_init` therefore never succeeds, `drm_dev_register` and
   `drm_fbdev_dma_setup` are never reached, no `fb0` exists, `fbcon` never
   registers, and `tty0` -- the first entry in this board's `bootargs` -- stays
   on the dummy console for the whole boot. The reference's 123-line log has no
   `fb0`, no `Console: switching to colour frame buffer device` and no
   `[drm] Initialized pl111`, and that is still exactly what it shows.

**The driver does not match on `compatible`.** `pl111_id_table` is an amba_id
table keyed on the PrimeCell peripheral and cell ids, which `amba_read_periphid`
reads out of the device's own id registers -- and while that read fails,
`amba_match` returns `-EPROBE_DEFER` with no message at all, which is why a
machine that answers the id registers wrongly fails silently.
`src/dev/pl110.rav` answers `0x00041110` and `0xB105F00D`, which is the table's
`arm,pl110` entry, and the reference log confirms the match.

So there are two machines here with two different problems, and only one of them
is still a problem:

* **On the reference, the fix is to make the panel always findable** rather than
  emulating an id the driver refuses. `CONFIG_DRM_PANEL_SIMPLE=y` is already in
  `versatile_defconfig`, so the panel node can be `compatible = "panel-dpi"`
  with a `panel-timing` carrying the Sanyo timings the driver already has
  written down, emitted from the board file so the machine stays the source of
  truth. `CONFIG_FB_ARMCLCD` is **not** the fix: the 6.6 driver has no Versatile
  board tables left and wants a `panel-dpi` node anyway.
* **On this machine the panel was never the problem, and neither was the
  controller.** `boards/versatile-pb.mjs` reports panel id `0x00`, which is
  SANYO_3_8 and *is* in the table, so the machine presents a panel the driver
  knows -- better than QEMU does -- and the controller binds, registers `fb0`
  and drives the console. What was stopping this machine was three decode bugs
  in the CPU and the controller's interrupt bit, which is the next section.

### The one thing this section got wrong about the picture

The bit that was fixed last is the *colour routing*, and it was found by making
the picture. `pl111_versatile_enable` picks the board's PLD mode from the
**framebuffer's** format: `DRM_FORMAT_RGB565` gets `SYS_CLCD_MODE_565_B_LSB`
(bits 1:0, value three) and `DRM_FORMAT_BGR565` gets `SYS_CLCD_MODE_565_R_LSB`
(bit 1, value two), because the kernel clears `CNTL_BGR` on this board and
leaves the routing to the PLD. `clcd_rgb565` had those two the wrong way round,
so the controller turned every sixteen-bit pixel over: the Linux logo's orange
beak and feet -- `0xf5e1` in the framebuffer, 262 of those pixels -- reached the
Stage as light blue, which is a colour the kernel never wrote. The two branches
are now the kernel's, and `tools/rom/bootrom.S` programs mode three for its own
RGB565 checkerboard, which is the same arrangement and the mode the kernel uses.

## The three bugs that were left

Everything above is the record of rounds that stopped somewhere in the kernel.
This is the round that stopped stopping, and it found three bugs, none of which
was where the round before it said to look.

### The PL110 raised the wrong interrupt bit

The controller set bit 0 of its raw status register every frame and masked on
bit 0 of its enable register. The kernel's PL111 driver uses bit **2**:
`CLCD_IRQ_NEXTBASE_UPDATE` is `BIT(2)` (`pl111_drm.h` line 99),
`pl111_display_enable_vblank` writes exactly that into `IENB`
(`pl111_display.c` line 424), and `pl111_irq` reads the masked status and tests
it against that same bit (`pl111_display.c` lines 33-42). It is not the
vertical-compare bit next to it, which the driver never asks for.

The state at the stop says so without any disassembly:

```
clcd ienb=0x4  ris=0x1
```

The enable bit is set and the raw status bit is set, in **different** bits, so
`clcd_masked()` returned zero and the line was never driven. The cost is not a
warning. `drm_atomic_helper_wait_for_vblanks` waits a hundred milliseconds for a
frame that cannot arrive, warns `[CRTC:33:crtc-0] vblank wait timed out`, and
the *next* atomic commit blocks in `drm_atomic_helper_wait_for_dependencies` ->
`drm_crtc_commit_wait`, whose `flip_done` is completed by the vblank handler, for
its ten-second timeout. Walking PID 1's stack out of the page tables -- it is a
`CONFIG_VMAP_STACK` stack, so it is not in the linear map and a plain word dump
of the kernel's memory will not find it -- gives the whole chain:

```
kernel_init -> driver_register -> __drm_fb_helper_initial_config_and_unlock
  -> register_framebuffer -> fbcon_fb_registered -> do_fbcon_takeover
  -> do_take_over_console -> do_bind_con_driver -> redraw_screen
  -> fbcon_switch -> bit_update_start -> fb_pan_display
  -> drm_fb_helper_pan_display -> drm_client_modeset_commit_locked
  -> drm_client_modeset_commit_atomic -> drm_atomic_helper_wait_for_dependencies
  -> drm_crtc_commit_wait -> __wait_for_common -> schedule_timeout
```

So the boot had not stopped. It had gone to sleep inside the framebuffer console
takeover, and the UART went quiet because that same task was holding
`console_lock` while it slept. **That is why the console's last line was a DRM
line and not a panic.** `src/dev/pl110.rav` now raises and masks
`CLCD_INT_NEXTBASE`.

### `STM(2)` was refused rather than executed

`cpu_class_block` refused a store with the user-register bit when `r15` was
*absent* from the list. That is the wrong way round, and it is the instruction
the kernel's own exception entry is built from: `stmdb r0, {sp, lr}^` in
`usr_entry` (`arch/arm/kernel/entry-armv.S` line 381) and
`stmdb r8, {sp, lr}^` in `restore_user_regs` (`entry-common.S` line 176) both
store the *user* stack pointer and link register while the processor is in a
privileged mode. The list containing `r15` is the unpredictable one, because the
program counter is not a register the bank being written is running.

With the PL110 fix in, `/init` started and the kernel said so itself:

```
Internal error: Oops - undefined instruction: 0 [#1] ARM
PC is at __pabt_usr+0x2c/0x48
```

and `0xc0009aec` is `e9406000`, which is `stmdb r0, {sp, lr}^`. The store path
already knew how to write the user bank; only the guard was inverted.
`src/cpu/core.rav` now refuses the list that contains `r15` and executes the
other one.

### A load that faulted wrote its destination

Every load in `src/cpu/core.rav` wrote the value its failed access returned --
a zero -- into the destination register, and only then looked at `mem_fault`.
ARM aborts the instruction instead of half-performing it, and the kernel's abort
handler returns to the faulting instruction and runs it again, so a register an
aborted load touched is a register the retry reads.

That is harmless until the destination is also the address. The first thing a
statically linked program executes is `_start`, and busybox's is

```
  145ec:  ldr r0, [pc, #12]     @ r0 = 0x1c
  145f0:  ldr r0, [sl, r0]      @ r0 = GOT[0x1c] = main
  145f4:  bl  0x12085c          @ __libc_start_main
```

`GOT[0x1c]` is a `.data` page, so the first read of it faults and the kernel
demand-pages it. The retry then computed its address from a destroyed `r0` and
read `GOT[0]` instead -- the word the linker reserves as the table's own address,
which is always zero -- so `main` was null, and `__libc_start_call_main` reached
`blx r3` at `0x12081c` with `r3 = 0` and branched to address zero. The kernel
turned that into the `SIGSEGV` that killed PID 1.

Three halts pin it down, each one instruction wide:

```
at 0x145f0 (before the load)  r0=0x1c  r10(sl)=0x222d1c
                              TLB slot for VA page 0x222: tag=0xc0723, want=0x223
                              -- another address space's entry, so this access
                                 walks the tables and faults
at 0x145f4 (after it)         r0=0
at 0x12081c (`blx r3`)        r3=0, and [sp,#4]=0 while GOT[0x1c] holds 0x15500
the panic's exception frame   pc=0  lr=0x120820  r3=0
```

`src/cpu/core.rav` now leaves the destination alone when `mem_fault` is set: in
the single transfer, both extra loads and the signed byte, the halfword, the
doubleword pair, and the register list.

### What it moved

| | console | the last thing the guest said |
| --- | --- | --- |
| before | 5,455 bytes | `[drm] Initialized pl111 1.0.0 20170317 for 10120000.display on minor 0`, then idle from about 97 million instructions |
| after | 6,793 bytes | `~ # ` -- and about 118 million instructions is where the banner arrives |

The guest now boots to a shell that a serial terminal on the board's connector
would show, and it is the same shell the reference boots to, line for line. The
last three lines are the point of the project:

```
the console you are reading is the LCD controller's scanout:
the kernel drew these glyphs into the framebuffer the controller
is reading, and the Stage is the controller's output.

/bin/sh: can't access tty; job control turned off
~ #
```

`node tools/check.mjs` passes all six of its assertions, `the initramfs ran
/init` among them, with 6,731 bytes on the UART, 1,317,576 pen lines and 1,797
colours on the Stage.

## What is not built yet

Nothing stands between the machine and a shell any more. It boots the kernel,
hands over to the initramfs's `/init`, prints the banner and settles at the same
prompt the reference prints, and `tools/check.mjs` passes every assertion it
makes. What is left is the rest of the board, Thumb, and the clock.

**Two things this section used to say were wrong and are recorded here so they
are not re-derived.** The boot does *not* die in `do_initcalls`; it has not since
the halfword multiply was fixed, and the `ptp_classifier_init` / `vsnprintf` /
reversed-operand account was a round's inference rather than the machine's
state -- `alu_sub`'s flags are what the round before that made them, and
`tools/check-cond.mjs` still holds the condition table. And the reference does
*not* stop at `Run /init as init process`; `bash tools/ref/run.sh` reaches a root
shell and prints the banner, which is how this round had a target to compare
against at all. See `## The three bugs that were left` for what the boot was
actually doing.

1. **Throughput, which is the binding constraint on the project.** About four
   hundred Scratch blocks an instruction at about two million blocks a second is
   five thousand guest instructions a second -- measured (`node tools/slice.mjs`
   prints instructions retired against the wall clock), not guessed. That makes
   a kernel boot hours. Inlining the bus decode, Scratch's turbo mode and a
   larger slice were each tried and none of them moved it: what an instruction
   costs is block count, not calls, so the way forward is fewer blocks in the
   decoder -- cached instruction fields, fewer dispatch layers -- and past that
   a TurboWarp-compiled run of the same project.

2. **The rest of the board.** The PL031 clock, the PL050 keyboard interfaces and
   the PL061 GPIO blocks, which the kernel probes and which answer zero today;
   then the SD host; then the network.

3. **Thumb, and the halfword multiplies.** The kernel and userspace are ARM
   state, so Thumb is not needed to boot and a swap to it faults as an undefined
   instruction rather than answering wrongly. The ARMv5TE halfword multiplies
   are decoded -- `smlabb` and its siblings -- and they are exercised on every
   boot now that the kernel gets to `genl_register_family`.

## The instruments

Three tools exist because a machine that stops is not a machine that tells you
where.

`tools/slice.mjs` runs the built project for a wall-clock budget and prints what
the machine is doing: instructions retired per step, the program counter, the
status register, CP15's fault registers, the whole register file, every mode's
banked stack pointer and `spsr`, the level one page table entries for the kernel,
its section mappings and the vector page, and the printable runs out of the
kernel's own log buffer. It is how "the kernel got as far as the MMU" is a number
rather than an opinion, and the log buffer is how a kernel that panics before it
has a console can still be read -- its reason is in RAM, not on the wire.

`tools/cpu-trace.mjs` points the processor at any address and steps it. The
kernel image is also in the flash chip, so the processor can be aimed straight
at the kernel's first instruction with no two-minute copy in the way -- and its
trace can be held against the reference machine's.

When a guest is neither faulting nor finishing -- it is simply somewhere it
should not be for a very long time -- three switches on the machine sprite ask
it instead of watching it from outside. `machine_watch_pc` stops the machine the
moment the program counter reaches a given address, so every register, the
caller's frame and the whole stack are still exactly as the caller left them;
that is how the first `__stack_chk_fail` was caught rather than the recursion it
caused. `machine_trace_on` keeps a ring of the last 256 program counters, because
a loop's shape is the addresses it repeats and a sample taken once a step lands
wherever it lands. And `tools/slice.mjs` reads the words at `WORD_AT`, the
printable text at `TEXT_AT` and the supervisor stack of the running machine --
the last of which has no unwinder, but a kernel stack is return addresses with
saved registers between them, so the words pointing into the kernel's text *are*
the call chain in order.

`tools/ref/` boots the same memory image on QEMU's Versatile PB. `-kernel`
cannot do it: QEMU loads a kernel at `0x10000`, and `head.S` works
`PHYS_OFFSET` out from where it actually is, so the kernel would believe RAM
starts at 32 KB. `tools/ref/shim.S` is the twenty bytes that put QEMU's machine
into the state this board's firmware leaves it in instead. It answers one
question in a second that takes the emulator hours: is the guest good, or is the
machine?

`tools/watch-rv32.mjs` is the RISC-V board's version of `slice.mjs`, and it is
the one that found the four bugs this board's Linux had before it would reach a
shell — a device tree that had outgrown the window the board reserves for it, a
`/dev/console` unpacked as a regular file, and a kernel whose syscall table was
missing `wait4` and `newfstatat`. It prints the guest's instruction count, its
program counter and a histogram of where it has been, and `--from N` cuts the
machine's slice to 64 cycles once the guest has retired N instructions so that
a spin is a handful of addresses rather than a blur; `--log 0x...` reads the
kernel's own printk ring out of RAM, which is where a message goes when
`console_lock` is held by the task that would print it.

It is also the instrument a *change* is measured with, because `check-rv32.mjs`
is not: that one asserts, and its totals depend on how long the host took.
Three switches make a number out of a boot:

```sh
node tools/watch-rv32.mjs --boot                          # seconds and instructions to /init
node tools/watch-rv32.mjs --boot --slice 1048576          # the same at another slice length
node tools/watch-rv32.mjs --boot --idle 20                # what the pen does at the prompt
```

`--boot` stops the run the moment the guest's console says the kernel has handed
over to `/init` — the same marker the check waits for, without the typing — and
prints the seconds of wall clock and the guest instructions it took to get there.
`--slice N` sets `machine_slice` before the machine runs, so the cost of a
shorter or a longer slice is measured rather than guessed. And `--idle N` keeps
running for N more seconds at the prompt and prints what the monitor read and
drew *in that window*, normalized by the guest instructions retired in it,
because the two builds being compared do not retire the same number of
instructions in a second and a raw per-second total would be a comparison of how
fast the host ran rather than of what the monitor cost.

`tools/probe-residue.mjs` is the instrument for the *display* rather than for the
boot. It boots a real machine and compares the pen's own raster against
`efb_words` after every Scratch frame, row by row, recording for each row how
long it stayed wrong and whether anything ever repaired it; `--runs 2` presses
the green flag twice, which resets the machine and empties the card, and
`--png-worst` writes the Stage, the card and a white-on-black map of the pixels
where they disagree, at the first moment a row has been the wrong picture for
forty frames. Under the whole-panel repaint that moment is never reached: a row
can only be wrong for the part of one pass, and a pass is not forty frames.

```sh
node tools/probe-residue.mjs --runs 2 --after 45                 # the machine, frame by frame
node tools/probe-blocks.mjs dist/desktop-rv32-linux.sb3          # the Monitor's procedures, statically
node tools/probe-blocks.mjs dist/desktop-rv32-linux.sb3 --profile # and what one pass costs in blocks
```

`tools/probe-blocks.mjs` reads the *built* project rather than the source,
because the compiler is what decides the shape. In its default mode it counts
the blocks of every procedure the Monitor sprite owns; with `--profile` it boots
the project in the compiling VM, turns the interpreter back on for a few frames,
and counts every primitive the interpreter runs on the real picture, attributed
to the sprite that ran it. The second is the one that answers "where do this
project's block executions go", because a static count cannot: `monitor_row_direct`
is one procedure and its inner loop turns two hundred and thirty-nine times
inside each of three hundred and sixty row calls.

`tools/ref/align.S` is the same idea for a *hardware rule* rather than a whole
boot, and it is the one to copy when a machine and a guest disagree about what
the processor does. It asks whether an ARM926 faults on a doubleword transfer
that is four-byte aligned but not eight, which is a question whose answer is not
in this repository and which reads as though it were obvious either way: eight
is the ARMv6 rule, and this core is an ARMv5. The program does the same `strd`
at an eight-aligned address, at one four past a multiple of eight, and at one two
past, with alignment checking on the way Linux leaves it, and prints a letter
per step. It does the first two and faults on the third.

## The bugs, and why the check is written the way it is

Every one of them was found by running the built project in a real Scratch VM
and reading the state back, and every one of them was a decode or state-machine
error rather than a design error. They are written down because they are the
argument for testing the whole path before a kernel is put on it.

| what was wrong | what it looked like |
| --- | --- |
| `msr` read as `mrs` (bits 21:20, not bit 20) | `msr cpsr_c, r0` wrote the status register into the field mask's register slot, which is 15, so the processor jumped to the value of the status register |
| `msr`'s field mask read from the top | `msr cpsr_c` wrote the flags and left the mode alone |
| the rotated immediate divided by 2048 instead of 256 | `mov r1, #16` became `mov r1, #1`; every programmed register was a plausible wrong value |
| a data-processing write to `r15` with the flags refused | the first handler to end in `subs pc, lr, #4` faulted instead of returning |
| the exception saved the status register before the mode moved | the first interrupt returned into mode 0, which runs, because mode 0 maps to the user bank |
| block transfer S and W swapped (bits 22 and 21) | `stmia rN!, {...}` decoded as a user-register store and was refused as unpredictable |
| block transfer IA and IB start addresses inverted | every multiple store landed one word along and left two pixels of each block unwritten |
| the ROM packed one pixel per 32-bit word | half the framebuffer stayed black: vertical stripes one pixel wide, which looks like a display that is nearly working |
| `CPSR_C` and `CPSR_V` written to bits 28 and 27 instead of 29 and 28 | the condition test read the same wrong bits, so for a while the two errors cancelled and everything worked. Fixing the test made every comparison read the wrong carry, and the firmware's pattern loop ran for ever |
| `tst` and `teq` set no flags, and `mov`, `orr`, `bic` and `mvn` did not take the carry from the shifter | a comparison that changes nothing and a logical that leaves a stale carry |
| `msr` with an immediate guarded by bits 27:23 equal to 2 instead of 6 | `msr cpsr_c, #0xd3` -- the first instruction of every Linux kernel -- was read as `rsb`/`rsc`, had its `S` bit forced for being in the comparison opcode range, and took ARM's exception return into a status register that had never been saved, which is zero. The kernel died on its first instruction, in user mode |
| the coprocessor number read from bits 27:24 instead of bits 11:8 | `mrc p15, 0, r9, c0, c0` asked coprocessor 14, so the kernel never read its own processor id |
| `swi` told apart from a coprocessor by bits 27:26 instead of 27:24 | `mrc` is `1110` in bits 27:24 and `swi` is `1111`, and bits 27:26 are `11` for both, so every coprocessor read was taken as a supervisor call |
| `mrs` and `msr` told apart from the comparisons by bit 20 | `teq r3, r4` is the same shape as `msr cpsr_c, r4` and is distinguished only by the fields each is required to leave as `r15`. `__lookup_processor_type` is four `teq`s in a row, so the kernel wrote its way through its own status register and looped |
| the processor id one digit out (`0x4106_a260` for `0x4106_9260`) | `__lookup_processor_type` found no processor it recognised. The failure is silent: the error path prints through a debug UART this kernel does not have, so the machine spins |
| the cache type one digit out (`0x1d15_5052` for `0x1d15_2152`) | the kernel derives a cache line length from it, which is a number it then cleans ranges by |
| the fault status and fault address kept in a second pair of cells | a fault recorded `mem_fsr`, and `mrc p15, 0, r0, c5, c0, 0` answered `cp15_fsr` -- which the walk never wrote. So every abort handed the kernel a status of zero, the encoding for "no fault at all", and the handler did the wrong thing with every fault it was given |
| the kernel loaded at `0x10000` instead of `0x8000` | `head.S` computes `PHYS_OFFSET` as its own load address minus `TEXT_OFFSET`, so the kernel believed RAM began at 32 KB. Every physical address it computed was 32 KB out, and 32 KB is not even a section. A raw `Image` is not a `zImage`: there is no decompressor to relocate it |
| the initramfs's `newc` header padded to four | a header is 110 bytes and the writer aligned every piece it wrote, so the header gained two bytes and every field after the magic landed two bytes early |
| the initramfs's names padded by their own length | padding is measured from the running offset, not from the piece: a header is two bytes past a four-byte boundary, so a name begins two bytes past one too. The archive was unreadable and had never been read |
| the device tree's flash twice the size of the machine's | the guest sized its MTD structures for a 64 MB chip inside 16 MB of RAM and ran out of memory before userspace. The tree and the board file now take the size from one place |
| a post-indexed transfer used `Rn + offset` as its address | pre-indexed, the sum *is* the address; post-indexed, the transfer is at `Rn` and the sum is only what gets written back. `str r3, [r0], #4` therefore wrote four bytes too far and advanced as well, so the kernel's loop that fills its page tables put every section mapping one entry down the table -- and the top of its own image was mapped a megabyte low, where an instruction fetch found the middle of a string. The kernel had been running for a million instructions by then |
| the decoder tagged the multiply groups by bits 11:4 instead of bits 7:4 | bits 11:8 are the `Rs` register in every multiply encoding, so a tag read from bits 11:4 is a register number with a shift amount glued to it: `lsl r2, r3, #1` has bits 7:4 of `1000` -- and bits 11:4 of eight, for the same reason `0x08` has a low nibble of eight. It was read as a *long multiply*, which multiplied two registers nobody had asked for and wrote the halves over `rdlo` and `rdhi` |
| a doubleword transfer required eight-byte alignment | an ARM926 asks for four; eight is the ARMv6 rule, and it reads as if it were always true. `tools/ref/align.S` settled it against QEMU's own ARM926 with alignment checking on. Requiring eight made Linux fault inside `setup_arch`, before it had mapped the high vector page, so the abort handler could not run and the machine looped on a vector it could not fetch |
| a post-indexed load suppressed its writeback when the load wrote `r15` | the guard was `cpu_branched == 0`, which is set by any write to the program counter -- and the instruction that writes it here is the *load itself*. `pop {pc}` is not a multiple transfer: it is `ldr pc, [sp], #4`, and the writeback is the whole of what makes a stack pointer a stack pointer. So every return from a function that saved only `lr` -- which is what gcc emits for a leaf -- left `sp` four bytes low. Millions of instructions of that walks the kernel's stack down through everything above it, and the first thing to notice was a stack-protector check comparing a canary against a `va_list` pointer |
| the extra load and store taken in the order the mnemonics sort | the four encodings are not ordered the way the names are. `arm-linux-as` is the authority: `ldrh` is `…b0`, `ldrsb` is `…d0`, `ldrsh` is `…f0`, and `ldrd`/`strd` are `…d8`/`…f8` -- so the halfword is the only pair whose direction is the load bit, and the doubleword is the only pair whose direction is the type field. Reading `f` as a doubleword made `ldrsh r3, [sp, #118]` inside `vsnprintf` a `ldrd`, which then demanded eight-byte alignment of an address that was only ever two-byte aligned |
| a doubleword's second register read from the same field as the first | `ldrd r2, [sp, #24]` loads `r2` and `r3`: the encoding names one register and implies the next. Reading the field twice loaded both halves into `r2` and left `r3` holding what it had, so `printk_sprint` handed `vsnprintf` its *argument list* as its format string, and the kernel dereferenced `0xffffffa0` |
| a subtraction's carry computed from the *signed* operands | `sub` needs two different readings of the same two numbers: the borrow is a property of the unsigned difference and the overflow flag is a property of the signed one. Doing both signed is right only while the operands are on the same side of 2^31, and `memblock_add_range`'s very first act is `min(size, ~base)` -- a comparison against a value with bit 31 set. The carry came out as a borrow that never happened, `size` collapsed to zero, `memblock` was empty, `arm_lowmem_limit` was zero, `prepare_page_table` cleared its own page tables and the kernel died inside `setup_arch`. This is the one bug in the list that a single `cmp` away from the top of the boot decided the whole run |
| every multiply took `Rs` from bits 6:3 | the multiply forms pack `Rd`, `Rn`, `Rs` and `Rm` at bits 19:16, 15:12, 11:8 and 3:0, which is not the shift-and-register layout a divide by eight finds. `mla r3, r0, r7, r3` in `__create_mapping` therefore indexed the memory-type table by whatever sat in `r2` and read a section descriptor at `0xc8b24af4`. All three multiply procedures had it, and `arm-linux-as` settles the layout: `mla r0, r1, r2, r3` is `e0203291` |
| `mrc p15, 0, r15, ...` treated as a register write | it is ARM's test-and-clean and the one coprocessor read that does not write a register: bits 31:28 of the data *are* the flags and the program counter is untouched. Writing the value to `pc` is a branch, and Linux's `__flush_whole_cache` reads the cache status into `r15` on its first pass -- so the kernel jumped to the address the cache status happened to be, which was zero |
| the cache maintenance read answered zero | `mrc p15, 0, r15, c7, c14, 3` is test, clean and invalidate, and its answer is a loop condition: bit 30 becomes `Z` and `Z` means the cache is clean. Answering zero says "still dirty" for ever, and the kernel spun 273 million instructions in a two-instruction loop. This machine has main memory behind the processor and nothing between them, so the honest answer is that it was already clean |
| the flash layout put the initramfs on top of the kernel | the layout was chosen when the kernel was smaller than four megabytes, and it grew: `kernel.img` is now 7.3 MB from flash offset `0x010000`, and the initramfs sat at `0x400000` with the device tree at `0x600000`, both inside it. Every image was written faithfully and the last one won, so the machine was booting a kernel whose tail -- everything past 4 MB, including `__do_div64` -- was a copy of the initramfs. It survived three million instructions because nothing early lives that high, and it died the first time it called something that did. `tools/ref/` loads `kernel.img` by name rather than reading the flash, which is why the reference never saw it and why "the guest is good" was true and useless: **the two machines were running different kernels.** The offsets moved and `flashImage` now refuses to lay one image over another, because it is the only place that reads both sizes |
| the S bit's amendment to the permission table not applied | CP15 control is `0x3137`, so bit 8 -- the S bit -- is set, and the one thing it changes is that a permission of `00` stops meaning "no access at all" and starts meaning "privileged read/write, user none", which is what Linux maps a kernel-only page with. Read the classic way it refuses the page to everybody, and `devicemaps_init` maps the vector **stub** page with exactly that permission -- so the processor was refused the page its own interrupt vector branches into, and the abort handler could not run because the abort handler is in that page. The two pages side by side are the diagnosis: the kuser-helper page is `0xffeaae` (bits 5:4 = `10`, privileged RW and user read-only, which is what helpers are for) and the stub page is `0xfff00e` (bits 5:4 = `00`, kernel only, which is what a vector stub is) |
| `msr` and `mrs` told apart by the `r15` alone | `mrs` names `r15` as its source and `msr` names it as its destination, so looking for the `r15` is *nearly* enough -- and `msr spsr_fsxc, r0`, whose field mask is every field, has `1111` in the source field too. The interrupt vector's own mode-setting instruction therefore read as `mrs`, which writes the saved status register into the program counter: the machine branched out of the vector into the status word. Bit 21 is the discriminator, and the assembler says which way round: `e10f0000` is `mrs r0, cpsr` and `e16ff000` is that `msr` |
| `ldm` with the user bit read one way for both of its meanings | the bit means "the User-mode registers" for a load *unless* the register list contains the program counter, and then it means the opposite: load the registers the processor is already using and restore the status register from the saved one. That second form is the exception return, and its list is `{r0 - pc}` -- which spans `r13` and `r14`, so it is also how the interrupted stack pointer comes back. Read as the user-register form, an interrupt left the *handler's* `sp` in place, and the function it interrupted returned through the handler's frame: `sched_clock_init`'s `pop {pc}` read the handler's saved status word and branched to `0x40000053`, which is what the SVC bank's `spsr` happens to be. Every interrupt corrupted the stack of whatever it interrupted |
| the PL110's interrupt raised and masked in bit 0 instead of bit 2 | the driver's `CLCD_IRQ_NEXTBASE_UPDATE` is `BIT(2)`, so the controller set a raw status bit nobody masks and masked a bit it never sets: `ienb=0x4` and `ris=0x1` at the same instant, a masked status of zero, and a vblank line that was never driven. Every atomic commit waited its timeout out -- a hundred milliseconds and a `vblank wait timed out` warning for the first, ten seconds in `drm_crtc_commit_wait` for the next -- and PID 1 went to sleep holding `console_lock` inside the framebuffer console takeover, which is why the console's last line was `[drm] Initialized pl111` and not a panic |
| a store with the user-register bit refused unless the list held `r15` | the test was the wrong way round: `STM(2)` is defined for a list *without* `r15`, and it is the instruction the kernel's exception entry is built from. `stmdb r0, {sp, lr}^` in `usr_entry` and `stmdb r8, {sp, lr}^` in `restore_user_regs` both store the user stack pointer and link register from a privileged mode, and both were refused as undefined. The kernel reported it as `Oops - undefined instruction` at `__pabt_usr+0x2c`, which is `0xc0009aec`, which is exactly that instruction |
| a load that faulted wrote its destination register | a fault aborts the instruction and the kernel returns to it and runs it again, so the register must be untouched; every load here wrote the placeholder zero its failed access returned and then noticed `mem_fault`. Harmless until the destination is also the address, and the first thing a static program runs is `ldr r0, [sl, r0]` -- the GOT entry numbered by `r0` -- against a `.data` page that needs demand-paging. The retry read `GOT[0]`, the word the linker reserves as the table's own address and always zero, so `main` was null, `blx r3` at `0x12081c` branched to address zero, and PID 1 died of the `SIGSEGV` before running one instruction of its own |

The one that is the ROM and not the machine -- the pixel packing -- is the
reason the ROM is assembly rather than a table of numbers: the emulator was
right, the pattern was wrong, and the check said so in a way that named the
difference, because the pixel at column 32 decoded to zero and the test pattern
was off by half a word rather than the scanout.
## The machine's clock, and why guest time is not wall time

A slice is a number of the processor's cycles and each device is advanced by its
share of them, so the machine is self-consistent: a timer programmed for ten
milliseconds fires after ten milliseconds of *guest* time. But thirty-five
thousand instructions a second is a processor nobody built, so guest time runs
far slower than the wall clock, and the board says so by declaring the rate —
`cpu: 4_000_000` in `boards/versatile-pb.mjs`.

That number does exactly one thing: it decides how many instructions pass
between two ticks of the kernel's timer. At 1 MHz the guest's clock and the
timer's are the same number and a 10 ms tick is ten thousand instructions; at
200 MHz it would be two million, and every delay the kernel takes would cost
hundreds of thousands of instructions. 4 MHz puts a tick every forty thousand
instructions — often enough to keep time, rare enough not to spend the machine
in its own interrupt handler.

## Building it

The guest build runs in WSL, because it needs a cross toolchain and a kernel
tree; the project build runs anywhere Node does.

```sh
bash examples/raven/desktop/tools/wsl/00-setup.sh       # toolchain, kernel, busybox
bash examples/raven/desktop/tools/wsl/02-build-tools.sh # m4, bison, flex (no root)
bash examples/raven/desktop/tools/wsl/10-kernel.sh      # the kernel's own defconfig
bash examples/raven/desktop/tools/wsl/20-guest.sh       # busybox, /init, the tree
bash examples/raven/desktop/tools/wsl/30-rom.sh         # the boot ROM, and its sizes
node examples/raven/desktop/tools/build.mjs             # tables, images, the .sb3
cargo run -p raven -- check -m examples/raven/desktop/raven.toml
SCRATCH_VM_ROOT=ref/scratch-vm/node_modules/scratch-vm \
  node examples/raven/desktop/tools/check.mjs --budget 600
```

`20-guest.sh` needs `dtc`; `02-build-tools.sh` exists because the kernel's
`scripts/kconfig` parsers are not in the kernel tarball and `flex` is not in a
plain Ubuntu image, and it fetches them the way `00-setup.sh` works -- unpacked
from `.deb`s into `~/raven-desktop/build-tools`, with no root.

The reference machine is optional and separate:

```sh
bash examples/raven/desktop/tools/wsl/40-reference.sh  # the twenty-byte shim
bash examples/raven/desktop/tools/ref/run.sh 25        # boots the same image
```

`tools/ref/run.sh` wants a `qemu-system-arm` at `~/raven-desktop/qemu/root`,
which is a `qemu-system-arm` `.deb` unpacked with `dpkg-deb -x` and the handful
of libraries it needs unpacked alongside it -- the same root-free trick, and the
only reason QEMU is not a documented dependency.

`tools/build.mjs` regenerates `src/board/decode.rav`, `src/cpu/luts.rav`,
`src/mem/sdram.rav`, `src/dev/flash_image.rav`, `src/screen.rav` and the
RISC-V board's `src/rvboard/decode.rav`, `src/rvboard/image.rav`,
`src/rvcpu/tables.rav`, `src/rvscreen.rav` from the board file and the images
every time it runs; all of them are in `.gitignore` and none of them is source.
It also writes `images/flash.bin`, which is the same bytes as a file, for the
reference machine, and the two device trees beside the board file. Everything
else under `src/` is written by hand and is source; `.gitignore` covers the
build output and nothing else.

### What each build is called

Three machines come out of this directory, and every one of them is named
`desktop-<machine>-<guest>.sb3` so that its filename says what to run it for:

| command | artifact | machine | guest |
| --- | --- | --- | --- |
| `node tools/build.mjs` | `dist/desktop-arm-virt-linux.sb3` | ARM Versatile-PB | Linux, on the PL110's console |
| `node tools/build.mjs --board boards/mini-rv32.mjs` | `dist/desktop-rv32-linux.sb3` | RISC-V mini-rv32 | Linux, on the card's `rgb565` panel |
| `… --guest doom` | `dist/desktop-rv32-doom.sb3` | RISC-V mini-rv32 | bare metal embeddedDOOM, on the card's `index8` panel |

The `<machine>` word is the board's own `name` and the `<guest>` word is the
key it has in the board's `guests` table, so a board that is renamed renames its
artifacts. The other half of the name is `[project] name` in the manifest --
which is what `raven build` names the archive after -- and `tools/build.mjs`
reads the manifest and refuses to build when the two disagree, because a rename
that reached one and not the other is a check looking for a project that nothing
writes.

The guest is built from source on Linux, because a kernel is not something to
take on trust. `00-setup.sh` pins a Bootlin `armv5-eabi` soft-float toolchain, a
Linux 6.6 LTS tarball and BusyBox, extracts them into `~/raven-desktop`, and
writes the SHA-256 of all three into `~/raven-desktop/SOURCES.txt`.

## Why the memory is a literal

`src/mem/sdram.rav` is twelve megabytes of source that says `0` four million
times, and that is not an accident of the build. Scratch refuses to add to a
list at or past 200,000 items and refuses to insert past it as well, so a list
that grows through blocks cannot be larger than 200,000 items however it is
grown. The only list that can be the size of a machine's memory is one that
arrives holding it. So the memory is written into the project as a literal of
exactly the size the board file asks for, every access is a replace into a list
that is already long enough, and a word the guest has not written reads as the
zero it was loaded with.

Two consequences are worth knowing. SDRAM is 16 MiB, which is a board parameter
and not a Scratch limit. And a warm reset leaves memory alone, because emptying
the list would leave nothing that could ever fill it again — which is what
memory does on a real board, so the machine and the runtime agree.

## The RISC-V board

The second machine in this directory is a 32 bit RISC-V hart with no MMU, on
mini-rv32ima's memory map. It is not the ARM board with a different processor
in the socket: a processor brings its own memory map with it, so the board is a
different file with a different list of parts.

```sh
node tools/build.mjs --board boards/mini-rv32.mjs               # the Linux shell guest
node tools/build.mjs --board boards/mini-rv32.mjs --guest doom  # bare metal Doom, on the card
SCRATCH_VM_ROOT=ref/turbowarp-vm node tools/check-rv32.mjs --budget 400
SCRATCH_VM_ROOT=ref/turbowarp-vm node tools/check-rv32.mjs dist/desktop-rv32-doom.sb3 --guest doom
```

and what each one writes is `dist/desktop-rv32-linux.sb3` and
`dist/desktop-rv32-doom.sb3`.

The Linux guest's image is built from source on Linux, like the ARM board's,
and it is the only part of this that is not the kernel's own tarball:

```sh
bash tools/wsl/50-rv32-image.sh     # images/mini-fb-image and images/mini-fb-font.bin
```

It needs `curl`, `patch`, `flex`, `bison`, `bc`, `libelf-dev` and a
`riscv64-unknown-elf` toolchain, and it fetches Linux 6.8 and cnlohr's kernel
patch for it. Everything else it needs is in this repository: the
configuration, the root filesystem, and the one shell script the guest runs.

### What the board file says

`boards/mini-rv32.mjs` is the machine, in the same shape as
`boards/versatile-pb.mjs`: a `devices` list, a `memory` map, a `clocks` table, a
`display` description with a `formats` table beside it, and a `guests` table
naming the images and, for a guest that wants a different panel on the card,
overriding the display. `tools/build.mjs` reads it and writes
`src/rvboard/decode.rav` — the device ids, the page-to-device table over the
whole 32 MiB MMIO window, each device's base, the RAM base and size, the
CLINT's and the console's frequencies, and the card's own id registers, its
format, its bytes a pixel and the geometry of its direct colour framebuffer. It
also generates `src/rvcpu/tables.rav` (the ALU's three 64 KiB logic tables and
the console's byte table), `src/rvscreen.rav`, `src/rvboard/image.rav` (the
guest and its device tree, as list literals) and the flattened device tree
itself, `images/mini-rv32-<guest>.dtb` and its `.dts` beside it.

| module | part | what it is | at |
| --- | --- | --- | --- |
| `rvcpu/hart` | rv32ima + Zicsr + Zifencei | fetch, decode, the register file, the CSRs, traps, the timer interrupt | — |
| `rvbus/bus` | the address decoder | one read and one write, by width, and the dispatch the page table names | — |
| `rvdev/uart8250` | an 8250/16550 | the console: `console=ttyS0`, two registers of it | `0x1000_0000` |
| `rvdev/efb` | the graphics card | two panels: 320x200 INDEX8 with a commit handshake, and 480x360 RGB565 with none | `0x1040_0000` |
| `rvdev/clint` | a CLINT | `mtime` and `mtimecmp`, and the timer interrupt | `0x1100_0000` |
| `rvdev/syscon` | a system controller | two magic writes: `0x5555` powers off, `0x7777` reboots | `0x1110_0000` |
| the hart's own list | RAM | 64 MiB at `0x8000_0000`, sparse, one item to the byte | `0x8000_0000` |
| `rvmonitor/efb` | the Stage | the card's scanout as runs, either panel, onto the Stage | — |

The board file is the only thing that says what the hart is plugged into: the
bus, the devices, the monitor, the stage and the keyboard all read the generated
tables rather than an address of their own.

The board's RAM is **sparse**, one Scratch list item to the byte, and a byte the
guest never wrote is not in the list at all — Scratch reads a missing item as
zero, which is what a fresh machine's memory is. That is the opposite of the ARM
board's SDRAM, which is one item to the *word* and present in full from the
moment the project loads, and the difference is the reset vector: an ARM926
comes out of reset executing at address zero, so something has to answer there,
while this hart comes out of reset at `0x8000_0000` with the guest image already
pushed into its memory.

### The CPU core

`src/rvcpu/hart.rav` is `examples/raven/rv32ima/src/sprites/riscv.rav` adapted to
this example's shape. The instruction set is unchanged and the port notes in
that file's header all still hold — the 16 bit split for `mulh`/`mulhu`/`mulhsu`,
the truncate-toward-zero `div` and `rem`, `lr.w` writing nothing back, and the
device tree in the last 1728 bytes of RAM because the image's own stub
decompresses over everything below `0x8100_0000`.

It is rv32ima with Zicsr and Zifencei: the base integer set, M (multiply and
divide, all six of them), A (`lr.w`, `sc.w` and the eight `amo` operations), the
six CSR instructions in both their register and immediate forms, and the
privileged behaviour a no-MMU Linux needs — `mtvec`, `mepc`, `mcause`, `mtval`,
`mstatus`, `mie`, `mip`, `mscratch`, `mret`, `wfi`, `ecall`/`ebreak`, traps and
the machine timer interrupt. `example/raven/rv32ima` and its reference
implementations are the ground truth, and the guest booting is the evidence.

Three things changed, and each is where the board shape reached in:

* **Memory that is not RAM goes through the bus.** `rvbus::bus` is reached only
  for an address outside `0x8000_0000..0x8400_0000`, so the RAM path — the access
  every instruction makes — stays inline and one list read wide. The bus passes
  the *width* the instruction asked for, because the console is a byte device
  and an eight-bit read of the data register consumes the byte.
* **`mtime` is real time, and the board file says how much of it a second is.**
  `src/rvdev/clint.rav` counts *microseconds of the host's clock*, which is what
  the reference does: `MiniRV32IMAStep` takes an `elapsedUs` and adds it to its
  `timerl`, and `GetTimeMicroseconds` is where it comes from. The heart of the
  point is that **one instruction is not one microsecond**. This machine retires
  about three and a half million instructions a second, so a `mtime` that
  counted instructions ran about three and a half times fast: the kernel's
  cursor blink, its `sleep 1`, its `uptime` and every one of its timeouts were
  three and a half times too quick, because `fbcon`'s blink and every other
  kernel timer are driven by the same counter. A CLINT on silicon is a counter
  off a timer crystal and has nothing to do with how many instructions retired;
  the device tree's `timebase-frequency` says how many counts a second holds,
  and the only way to make that true is to count the seconds.
* **The frame loop is not in the hart.** `hart_run_slice` runs one slice and
  returns; the machine sprite decides how long a slice is, exactly as the ARM
  board's machine sprite does. A slice stops at `machine_slice` instructions
  *or* `machine_slice_us` microseconds, whichever comes first.

### The console, and how it gets to the Stage

The Linux guest's console is **pixels in the graphics card's memory**, and the
Stage shows the card's scanout. Nothing in this project draws text: there is no
font table in the example, and `src/penfont/` — a 2.5 MB glyph table and an
engine that turned the serial console into pen strokes — is gone. What replaced
it is the path the bare metal Doom guest has always used, extended to the one
kind of picture a console is.

Four pieces, and each is a small edit to something that already existed:

**1. The card has two panels.** `boards/mini-rv32.mjs`'s `display` gained a
`format`, named rather than numbered, and a `formats` table beside it:

| panel | what a pixel is | memory | who wears it |
| --- | --- | --- | --- |
| `index8` | a palette index, one byte | 320x200, a 256 entry palette, a commit handshake | Doom's bare metal driver, unchanged |
| `rgb565` | a colour, two bytes | 480x360, no palette, no handshake | the Linux guest's `simple-framebuffer` |

The choice is **board data**: it is the card's entry in the board file, and a
guest that wants the other panel overrides it in its own entry in `guests`.
`tools/build.mjs` compiles it into `RV_EFB_FORMAT`, `RV_EFB_INDEXED` and the
window's geometry, so the card, the monitor and the device tree all read the
same numbers. The direct colour framebuffer is **one Scratch list item to the
32 bit word**, two pixels to an item: a byte an item would have been 345,600
items, and Scratch refuses to grow a list past 200,000 — 480 by 360 is 86,400
words. That is the card's own layout and the monitor knows it.

**2. The device tree gets a `simple-framebuffer` node.**
`tools/dtb.mjs` builds it from the card's own `pixelsOff`, `pitch`, `width`,
`height` and format, and puts it under `/chosen`, which is the one place the
kernel looks for it (`drivers/of/platform.c` creates the platform device for a
`simple-framebuffer` that is a child of `/chosen` and for nowhere else). The
node needs `#address-cells`, `#size-cells` and `ranges` on `/chosen` as well:
without them `/chosen` is not a translatable bus, the platform device is
created with no memory resource at all, and `simplefb` fails to probe with
`No memory resource`.

The generator's byte-for-byte check against the reference's own tree is still
there and still passes, and it now says what it is comparing: the tree this
board generates **for the guest the reference built its tree for** —
`referenceGuest` in the board file, which is the bootargs and the initramfs
addresses that were in the blob — with the framebuffer node this board adds
taken back out. Everything else in the tree is the reference's byte for byte.
A build whose tree does not fit the window the board reserves for it now
refuses rather than booting a kernel that is handed a blob with its last bytes
off the end of RAM.

**3. The kernel has a framebuffer console.** The reference's own image does
not: `# CONFIG_FB is not set`, and no `simplefb`, no `fbcon`, nothing to bind.
`tools/wsl/50-rv32-image.sh` builds the same machine's Linux from the same
configuration with them on:

```sh
bash examples/raven/desktop/tools/wsl/50-rv32-image.sh   # images/mini-fb-image
```

It takes Linux 6.8, cnlohr's `custom_kernel_config` for it, **cnlohr's own
`0001-Experimental-RISC-V-32-bit-No-MMU-support.patch`**, and the root
filesystem out of `mini_image`'s built-in cpio, so the programs the guest runs
are the same flat binaries. Three things in it are worth knowing.

* **The guest's shell writes to the card.** `/init`'s shell is started as
  `exec /bin/sh -l </dev/ttyS0 >/dev/tty0 2>&1` — stdin on the 8250 the check
  types into, stdout and stderr on the framebuffer console. Both ends are
  ttys, so the shell is interactive and prints a prompt. The two obvious
  alternatives are worse and both were tried: `>/dev/ttyS0` leaves the card
  showing only the kernel's boot log, and `2>&1 | tee /dev/ttyS0 /dev/tty0`
  gives the shell a *pipe* rather than a tty, at which point busybox's ash
  stops being interactive and prints no prompt at all.
* **The console's font is 8 by 16.** `fbcon=font:VGA8x16` on the command line;
  the kernel's default is the 8 by 8 one, which makes a 480 by 360 panel 60 by
  45 characters of twice the glyphs to draw. 8 by 16 is 60 by 22, which is
  what the pictures show.
* **`console=tty0 console=ttyS0`, in that order.** `printk` goes to both, and
  the last one named is the one `/dev/console` is bound to — which is what
  keeps the guest's own `/init`, its `mount` and its `mkdir` talking to the
  serial port they have always talked to.

**4. The monitor scans the card.** `src/rvmonitor/efb.rav` grew a second row
routine. The indexed panel still draws the board's latched copy of a committed
frame; the direct colour panel has no latch and no commit — it holds the
colour of every pixel, so there is nothing to copy — and the monitor reads the
memory the guest is writing, which is what a real card's scanout is. A pixel is
turned from `r5g6b5` into a pen colour by two small tables (there is no bitwise
operator in raven), and only a *run's* colour is unpacked.

Every pass erases the whole pen layer and paints the whole panel again.

**There is no partial redraw, and there is not going to be one.** Scratch's pen
draws a line and the only erase it has is `pen clear`, which takes the *whole*
layer. A scheme that redraws less than the whole panel therefore has to be
exactly right about what it leaves alone, and the pen keeps whatever it is not
told about: a row the guest changed and the monitor did not repaint keeps the old
picture for ever, and there is nothing that can ever take it back. That is
burn-in, and three attempts at it here produced three of them — a range that a
reset cleared without drawing it, a range that a wall-clock throttle deferred and
never drew, and a dirty notice that did not carry the store's width. Each was
found and each was fixed, and each fix left the next one. `monitor_draw` is now
one `pen clear` and one complete walk of the card's memory, on every turn of the
monitor's loop, whether anything changed or not. No dirty row, no dirty range, no
pending flag, no throttle, no incremental bookkeeping.

What that buys is that the picture cannot be wrong. The invariant is one
sentence and it holds by construction:

> **After the monitor has drawn, every pixel of the Stage's panel region is the
> card's memory.**

Every pass begins by making the pen layer empty and ends only when every row has
been walked, so there is no state for a pass to leave behind and nothing for the
next pass to be wrong about. What it costs is measured below and it is real; see
the block table for what was done about it.

The pen layer is the Monitor sprite's own and this build is the only thing that
draws on it — `src/sprites/machine.rav` runs the CPU and the devices and never
touches the pen, and the Stage's backdrop is a costume rather than pen work — so
`pen clear` takes exactly the monitor's picture and nothing else.

### What it does

**It boots Linux 6.8 to a root shell with no login, the shell runs programs
typed at it, and everything it prints goes to the graphics card and nowhere
else.** `node tools/check-rv32.mjs dist/desktop-rv32-linux.sb3` on the machine
this was written on:

```
file          dist\desktop-rv32-linux.sb3
vm            ref\turbowarp-vm (compiling blocks to JavaScript)
card          480x360 rgb565 (direct colour, scanned live)
boot          1614 frames in 55.1 s, 76267043 guest instructions
console       3867 bytes
card          1527232 pixel stores, 0 frames latched, ack 0
pictures      dist/screen-linux.png (the Stage's pen), dist/card-linux.png (the card's own pixels), dist/card-linux.txt (what it says)
frames        4000 runtime steps in 123.0 s
pen           50084918 lines drawn over the whole run (5544 in the last pass, which is what `pen clear` leaves)
monitor       3999 frames, 691027200 pixels read, 25042459 runs drawn
ok   the shell prompt reached the card's framebuffer and reads back out of it ("/ #")
ok   it booted to a root shell with no login
ok   the kernel booted
ok   the initramfs ran /init
ok   coremark ran and validated its results, read back out of the card
ok   duktape evaluated a program typed at the prompt
ok   duktape ran a script file from the guest
ok   ed received a real `.` keypress, wrote test.js and the shell read it back
ok   every key a real keyboard can reach arrived as its own byte, frame or no frame (44 keys: letters, digits, space, enter, the arrows, all thirty-two printable symbols, Backspace, Delete, Escape)
ok   and the rest made no byte of their own, with or without a frame (Tab, Shift, Control)
ok   a modifier a hat can see made the letter it modifies (shift+a [65], control+c [3])
ok   the guest's framebuffer console wrote 4660032 pixels into the card
ok   the monitor scanned the card out (691027200 pixels read)
ok   the monitor drew that scanout as runs (25042459 runs)
ok   the pen drew the card's picture (50084918 pen lines)
ok   the card's picture is mostly one background colour (97.1% of the panel)
ok   and a minority of it is ink (2.94%)
ok   the ink is in 23 horizontal bands, which are rows of text
ok   typing three programs scrolled the card by 13.2% of it
ok   the monitor repainted the whole panel after the guest stopped (40 pass(es) in the 40 frames of grace)
ok   the Stage is the card's picture pixel for pixel after the guest stops (0 of 172800 differ)
PASS
```

The `boot` line's 1614 frames are the kernel's own delays spent one frame each,
and that is the clock fix rather than a speedup: a slice is bounded by the clock
now, there is no 33 ms interval in a harness to spend it on, and guest
instructions to `/init` fell from 86,142,665 to 76,267,043. The wall clock is the
host's business and not the machine's — this same check has been 25.9 s on an
idle machine and 55.1 s beside another one.

The `monitor_frames` line is large because the check stops the guest before the
last two assertions and the monitor's own loop then runs without the machine's
slice to share the frame with. It is a count of passes, and every one of them is
a whole-panel repaint.

The exact keystrokes are the check's: `coremark`, then
`duktape -e 'console.log(12345)'`, then `duktape /root/fizzbuzz.js`, then an
`ed` session (`ed test.js`, `a`, the text, `.`, `w`, `q`) followed by
`cat test.js`, which prints `written-by-ed`. Every one of those sentences is
found **in the card's pixels**, by matching the kernel's own 8 by 16 glyphs
against them, because the shell writes to `/dev/tty0` and there is no other copy
of it anywhere. Every character is a **real key**: it is posted into the VM's
own keyboard device as a down and an up, with one runtime step in between, and
nothing else — no pointer, no sprite, no picture of a key. The punctuation is
the interesting half, because the editor's key dropdown has no item for it and
the block that delivers it is a hat whose field is the character itself, and the
check measures every key it can and cannot reach twice — once for a tap that
spans a frame and once for a tap that does not. What the runtime has a name for
is the other half: the same check runs on TurboWarp and on the vanilla VM, and
the rows for backspace, delete, escape and the two modifiers say the runtime's
answer rather than the board's. See
[the keyboard, key by key](#the-keyboard-key-by-key-and-what-vanilla-cannot-see).

`coremark` says, out of the card:

```
2K performance run parameters for coremark.
CoreMark Size    : 666
Total ticks      : 15803
Total time (secs): 15.803000
Iterations/Sec   : 6.960704
Iterations       : 110
Compiler version : GCC10.3.0
Compiler flags   : -march=rv32ima -mabi=ilp32 -fPIE -pie -Os -s -static
seedcrc          : 0xe9f5
[0]crclist       : 0xe714
[0]crcmatrix     : 0x1fd7
[0]crcstate      : 0x8e3a
[0]crcfinal      : 0x0134
Correct operation validated.
```

**This section used to print `CoreMark 1.0 : 0.722961` and that number is not
wrong so much as *unexplained*, which is worse.** It is a real CoreMark score —
the last line CoreMark prints, and it comes after `Correct operation validated`
rather than before it, which is why the check's own read-back stops one line
short of it. What it was never said beside is what a CoreMark score *is*: see
[the coremark score, from first principles](#the-coremark-score-from-first-principles)
below, where the 0.722961 and today's 6.96 are put on the same arithmetic and
the difference is accounted for.

**There are two pictures and they are two different things.**
`dist/card-linux.png` is the card's own framebuffer, read out of `efb_words`
after the run — what the guest wrote, pixel for pixel, at 480 by 360: the
fizzbuzz output, the `ed` session, `written-by-ed`, `/ # vi --help` and the
prompt. `dist/screen-linux.png` is the Stage, rasterised from the pen lines the
monitor actually issued — the same console, character for character, drawn as
horizontal runs. `dist/card-linux.txt` is the same picture read back out as
text by the font match the assertions make, which is also how the check knows
`coremark` validated its results: the sentence is not on the serial port, it is
in the card.

**The two files are the same file.** On the whole-panel repaint the Stage is the
card, so `dist/screen-linux.png` and `dist/card-linux.png` written by one run
compare byte for byte — identical SHA-256, and the check's own comparison of the
two rasters is `0 of 172800 differ`. That is a stronger statement than "they look
the same": it is the same raster, one of them produced by the monitor's pen and
the other by reading the card's memory.

The match needs one thing said about it. `fbcon`'s grid is an 8 by 16 cell and
the check knows that, but where the first column starts is not zero — the
kernel draws it a couple of pixels left of the panel's edge, presumably because
`vc_origin` and the glyph box are not the same rectangle. So the offset is
*fitted*: the whole picture is scored against the kernel's font at each of a
few offsets and the best one is kept. An exact fit scores about one mismatched
pixel a cell; a wrong one scores tens. That is a real caveat and it is why the
check reports the fit rather than assuming it.

**`vi` does not exist in this guest, and that is the image's decision rather
than the machine's.** `mini-image.sh` splices the `bin/vi` entry out of the
kernel's built-in cpio — the rv32ima README says why — and points `EDITOR` at
`ed`. Typing `vi --help` at the prompt gets
`sh: can't execute 'vi': No such file or directory`. So the editor this machine
runs is `ed`, and it edits.

### What the boot and the display cost

Two numbers decide whether this machine is usable, and neither of them is
`PASS`. `tools/watch-rv32.mjs` is the instrument for both, because the check
above is not: its totals depend on how long the host took, so it validates a
change rather than measuring one.

```sh
node tools/watch-rv32.mjs --boot dist/desktop-rv32-linux.sb3      # to /init
node tools/watch-rv32.mjs --boot --idle 20 dist/desktop-rv32-linux.sb3
```

The marker both of them stop at is `Run /init as init process` — the last thing
the serial port says before the shell owns the card — and the idle window is the
twenty seconds *after* it, which is a shell sitting at a prompt with a blinking
cursor. The idle figures are normalized by the guest instructions retired in the
window, because the builds compared here do not retire the same number of them
in a second and a raw per-second total would compare the host rather than the
monitor.

| | dirty-row redraw | whole panel, unoptimised | whole panel, optimised |
| --- | --- | --- | --- |
| seconds to `/init` (`--boot`) | 15.2 | 19.5 | **15.8** |
| guest instructions to `/init` | 57,559,808 | 57,559,808 | 57,559,808 |
| instructions a second | 3,780,860 | 2,945,592 | **3,641,643** |
| idle, pixels read a million guest instructions | 30,686 | 676,537 | 676,418 |
| idle, runs drawn a million guest instructions | 470 | 66,604 | 66,599 |
| idle guest instructions in a 20 s window | — | 62,322,121 | 82,259,145 |

The guest retires *exactly* the same instructions to `/init` in all three, and
the same runs are drawn to reach it — 2,302,811 — so nothing about the picture
changed. What changed is how much of the panel is painted on the way there and
what that costs. The idle pair is the price the owner accepted and it is the
whole of the difference: the monitor now walks all 172,800 pixels and draws all
of its runs once per pass instead of only the rows that moved, which is 22 times
the reads and 142 times the strokes for the same guest doing the same nothing.
That is the trade, stated rather than hidden: **the display is always exactly
right, and it costs a full scan every pass to be so.**

### What one whole-panel pass costs, and what was done about it

`tools/probe-blocks.mjs --profile` boots the project in the compiling VM, turns
the interpreter back on for two frames, and counts every primitive the
interpreter executes, attributed to the sprite that executed it. One pass over
the console, before any of the optimisation below:

```
blocks    9,089,166 block executions, 9,089,166 per pass
--- by sprite
    5,105,945/pass  Machine
    3,981,388/pass  Monitor
        1,833/pass  Input
```

and the same count after the three changes below:

```
--- by sprite
    4,687,315/pass  Machine
    2,983,741/pass  Monitor
        1,833/pass  Input
```

and the Monitor's own share, by opcode: `data_itemoflist` 1.70 M, `operator_add`
1.07 M, `control_if` 0.64 M, `operator_mod` 0.52 M, `operator_divide` 0.35 M,
`operator_mathop` 0.35 M. Ninety-three percent of the Monitor's blocks are the
row walk — three hundred and sixty calls to `monitor_row_direct`, whose inner
loop turns two hundred and thirty-nine times inside each of them — and the
remaining seven percent is the runs themselves, about eighteen thousand of them
per pass at five statements each. **The row walk is the whole problem**, and the
block count says so before anything is changed.

Three things were done, each against a number rather than a hunch.

**1. The per-pass constants moved out of the row.** The pen's thickness was a
block, the row's own `y` was two divisions and the horizontal scale was another,
and a pass was paying all three for every one of three hundred and sixty rows for
answers that differ by one subtraction. They are computed once in
`monitor_draw`.

**2. A word that cannot start or end a run is skipped whole.** A console row is
mostly background, and a background *word* has both of its pixels equal to the
run's own colour. `runword` is that colour written twice — `pixel * 65537` is
`pixel * 65536 + pixel` — so `word == runword` means neither half can change the
run and the row advances two columns on one list read and one comparison,
instead of taking the word apart into two pixels and comparing each. The fast
path is exactly equivalent: if both halves equal `pixel` then `word` *is*
`pixel * 65537`, so nothing that the slow path would have drawn is skipped.

**3. The low half of a word is a subtraction, not a remainder.** The benchmark
below measures Scratch's `%` on a value above 2^31 at about three times the cost
of the same remainder on a small number, and a framebuffer word is a 32 bit
unsigned number. `word - floor(word / 65536) * 65536` is the same value by a
divide, a multiply and a subtract, all of which are cheap, and the high half is
`floor(word / 65536)` with no remainder at all. This is the change that put the
boot back where the dirty-row redraw had it.

| | dirty-row redraw | whole panel, unoptimised | whole panel, optimised |
| --- | --- | --- | --- |
| Monitor blocks per pass | — | 3,981,388 | **2,983,741** |
| `monitor_row_direct` static blocks | 230 | — | 252 |
| seconds to `/init` | 15.2 | 19.5 | **15.8** |
| instructions a second | 3,780,860 | 2,945,592 | **3,641,643** |

The Machine's share moves with whatever the guest is doing at the moment the
counter is switched on — it was 4.7 M in the run that produced the final
Monitor number below and 5.1 M in the one above — so the Monitor's own count is
the one to compare across runs and the total is not.

The static block count *rose* while the executed count fell, which is the point:
the fast path is more code and less work, and only a count taken while the
project runs can tell the two apart. `node tools/probe-blocks.mjs` prints the
first number and `--profile` the second.

Each was measured on its own, and each is a step rather than a total. The fast
path alone took the boot from 19.5 s to **17.4 s** and 3,301,016 instructions a
second; the half-word change took it the rest of the way to **15.8 s** and
3,641,643, which is where the dirty-row redraw was. Every one of those runs
retired the same 57,559,808 instructions and drew the same 2,302,811 runs to
`/init`, so nothing but the monitor's own work moved — which is the only reason
the comparison is worth making.

### The clock, the frame, and the key that was never reported

Three things were wrong with the running machine, and none of them was the
picture — but all three were first *seen* on the picture, which is why they are
here.

**The guest's clock ran 3.87 times fast.** `mtime` was the retired instruction
count: `src/rvcpu/hart.rav`'s `step` added one to `clint_mtime_l` per
instruction, on the argument that the board file says the processor runs at one
megahertz and so one instruction is one microsecond. It is not. This machine
retires about three and a half million instructions a second, so a counter of
them ran three and a half times faster than the seconds it claimed to be
counting, and `fbcon`'s cursor blink is a timer off that counter — as is every
other kernel timeout. The reference never did this:
`MiniRV32IMAStep(state, image, 0, elapsedUs, count)` is handed the microseconds
the host's clock moved since the last block of instructions, adds *those* to
`timerl`, and counts retired instructions in a different register, `cyclel`.
`examples/raven/rv32ima` solved it the same way (`get_time_us`, `run_frame`).
This board now does too: `clint_tick` is handed the host's clock and works out
the difference from the last reading it made, which is what makes the device
tree's `timebase-frequency = <0xf4240>` true rather than nominal. Measured over
a ten second window at the shell prompt, guest microseconds against wall
microseconds:

| | before | after |
| --- | --- | --- |
| guest microseconds per wall second | 3,865,128 | **1,000,900** |
| ratio | 3.866 | **1.000** |

The reading is remembered by the CLINT and not by the slice, deliberately: the
time the machine is *not* running — the monitor's pass, the browser's own frame —
is real time too, and a clock that counted only inside its own slices would run
at 0.69 of a second per second. Both mistakes are the same mistake about what a
counter is.

**A Scratch frame is 33 milliseconds, and a slice is not.** TurboWarp's frame
loop is `setInterval(step, 1000 / 30)` with `currentStepTime` = 33.3 ms, so a
`_step()` that takes longer than that misses its interval and the display runs
at 30/n. The machine's slice was 262,144 instructions, which is 62 ms of this
host, so a step was 62 ms of machine plus 11 ms of monitor — and the display
ran at **15.1 steps a second, 66.1 ms each**, which is the owner's "10 to 15
fps" and is arithmetic rather than a browser. At the slice this README ends
with, the same measurement is **25.5 ms a step, 39.2 steps a second** in a
harness that never waits — so a browser's 33.3 ms interval is met with about
eight milliseconds to spare for its own renderer. Retired instructions were
never the right unit for a slice: what a slice has to fit inside is a frame. A
slice now stops at `machine_slice` instructions *or* `machine_slice_us`
microseconds, whichever comes first (`src/sprites/rvmachine.rav`), the machine's
thread being `warp` so nothing else can cut it short. `--slice-us N` on
`tools/watch-rv32.mjs` sets it and `--slice-us 0` turns it off.

**Half of a whole-panel pass was the pen drawing the background.** The monitor
walked the card's words, found every colour change and stroked every run,
including the forty or fifty runs a row of text makes *between* its glyphs. A
console row's paper is stroked once across the whole width before anything else
and a run of that colour is left undrawn — the band is already there.

**Which colour is the paper is decided by two pixels, not one, and that is a
correction.** The round before this one took it from the row's first pixel and
the owner reported grey and green bands across the console. Two things were done
about it and they are worth keeping apart.

The first is the measurement, because the report deserves an answer rather than
a patch. `node tools/check-rv32.mjs --shots 5` writes the Stage's raster every
five seconds of a whole run — eighteen samples, from the first lit pixel to the
last repaint after the guest stopped — and in **every one of them the Stage holds
exactly the console's two colours and no row is a full-width colour that is not
the background**:

```
shot-0.png   bg 0x000000 colours    1 rows uniform-in-a-non-bg-colour 0
shot-4.png   bg 0x000000 colours    2 rows uniform-in-a-non-bg-colour 0
shot-17.png  bg 0x000000 colours    2 rows uniform-in-a-non-bg-colour 0
```

and the end-to-end assertion beside it is exact: the Stage is the card's picture
pixel for pixel, `0 of 172800 differ`. That is what the band's shape buys and
the reason it is not a guess that has to be right: the band is drawn *first* and
every run of any other colour is drawn *over* it, so the union of the strokes is
the card whether the banded colour is the paper or not. A row banded in a
glyph's colour is a row with a wasted stroke, not a row with the wrong picture.

The second is that the objection is still right about the *browser*, which is
the one thing none of this can see. Every picture here is the monitor's own pen
output rasterised by the rules the monitor draws by; a browser antialiases, and
the band is the one construct in this project whose failure mode is a full-width
line at row pitch. So it no longer depends on one pixel: the row is banded only
when its **first and last pixels are the same colour**, because a background that
runs under a whole row is a colour the row both begins and ends with — and when
they disagree the row's content genuinely varies and **no band is used at all**,
which is the owner's own rule. The row is then drawn run by run, exactly as a row
of a picture is. The cost is one list read of the row's last word per row, which
is 360 reads a pass, and the runs it gives back are the ones it was not entitled
to.

Counted on one console picture read out of a real run, `efb_words` cached to
`tmp/efb_words.json` and the same pass timed against it:

| | before | after |
| --- | --- | --- |
| runs a pass | 16,971 | **8,672** |
| of which the row's first colour | 8,659 | — (not drawn) |
| milliseconds a pass | 10.79 / 11.44 | **5.28 / 5.22** |
| the Monitor's blocks a pass | 2,932,936 | **2,685,641** |

That last row is the point of the whole section: the *blocks* fell by 8% and the
*time* by 52%, because the blocks that went were the expensive ones. Every
stroke is five primitive calls and about 395 ns at this benchmark — a `pen run`
is 4.05 bare blocks in time and 25 blocks in count — so a count of blocks is not
a count of time, and the README's earlier conclusion that "the row walk is the
whole problem" was drawn from the count. It was half the problem; the runs were
the other half, and the cheapest thing to remove was the half of them that were
only there to repaint a background that one stroke can paint.

The pen's colour is also set only when it changes; a terminal row is one ink
colour, and `monitor_rgb565` (three list reads and six operations) and
`set pen color to` used to run tens of times a row for the same answer.

**A key hat is the only door, and it is wider than the dropdown.** A key arrives
as an *event*: `postData` upper-cases a one-character key and emits
`KEY_PRESSED`, and the runtime starts every hat whose own `KEY_OPTION` *field* is
that key, so the key travels in the hat and a tap that begins and ends between
two frames still arrives. The other door is `key pressed?`, whose argument is a
*value*: it answers for a single character and for nothing more — the runtime
upper-cases the character on both sides of the comparison, so `key pressed? "."`
is true while `.` is held, and a name longer than one character is not a key to
it at all, because `_keyArgToScratchKey` takes the first character, so
`key pressed? "backspace"` asks about the letter `B`. It is also a question about
the *present* rather than an event, so it sees a key only while that key is still
down when the block runs.

The round before this one read the keys the dropdown has no name for through
that second door — first with a once-a-frame poll of `key pressed?`, then from
the `when any key pressed` hat — and it is the bug the owner found, twice. A tap
shorter than a frame is over before any block runs, so the question is asked
about a keyboard that no longer holds the key; and the `any` hat did not fix it,
because a hat's *body* still runs at the next frame boundary even though the
event itself did not wait. Typed at that cadence `ed test.js` reached the shell
as `ed testjs`, the `ed` session never saw the `.` that ends its input, and the
console appeared to be stuck. `tools/check-rv32.mjs` never saw it because its
`press` held a key for six runtime steps. What hides it in `rv32ima` and not
here is the frame: this machine rescans a 480 by 360 panel on every frame,
`tools/probe-blocks.mjs --profile` measures 2.9 million block executions in one
pass, and the vanilla interpreter charges about half a second for it — longer
than any keystroke, so *every* symbol was lost, in every environment. `rv32ima`'s
console is a pen-drawn 64 by 20 terminal, 1.8 ms of a 31 ms frame, and its poll
always saw the tap.

The fix is the hat the dropdown does not offer. `Runtime.startHats` matches a hat
by comparing the key that went down against the value the *project* holds in the
hat's field, and never against the dropdown — the dropdown is the editor's list
— so a hat whose `KEY_OPTION` is `.` fires on the `.` key. The catalog's `KEYS`
therefore gained the thirty-two printable ASCII keys, raven names them
`Key::Exclamation` through `Key::Tilde`, and `src/sprites/rvinput.rav` has one
hat each: seventy-five hats, one for every key a vanilla runtime will admit. The
`any` hat and `symbol_keys` went with them, and with them the last thing that
needed the key to be down at a frame boundary — and the last thing that made a
second byte: the scan re-pushed each symbol that was still down whenever *any*
other key fired the hat. A TurboWarp block was the round before the last one
(`last key pressed`, with the `any` hat) and it is still gone: the catalog entry,
the `tw` extension, the purity row, the binding, the manifest's `extensions` and
the generated block reference with it.

**And the same door opens five more keys, because two runtimes disagree about
what a key is.** TurboWarp's keyboard device names twelve keys vanilla's does
not — backspace, delete, escape, tab, the lock keys, the modifiers and the
navigation keys (`ref/turbowarp-vm/src/io/keyboard.js:14-27`, and the `switch`
at `:81-93` that turns each into its name) — and it records them, where vanilla's
`postData` returns before it emits or records anything for a name longer than one
character that is not one of its own six. So on a vanilla runtime a hat naming
one of them can never fire, while on TurboWarp it fires on the key-down like any
other hat. `catalog.rs`'s `KEYS_EXTENDED` is the five this machine asks for —
`backspace`, `delete`, `escape`, `shift`, `control` — and they are hats for a
reason worth stating plainly: a hat is *inert* where it cannot work, and
`key pressed?` is not. The question is a question about whichever key starts the
name, so `key pressed? "shift"` asks about `S` and `key pressed? "control"` asks
about `C`; asking those from a vanilla runtime would corrupt every `s` and every
`c`.

`shift` and `control` are modifiers, so their hats carry no byte on a console:
they arm a gate that the letter hats consult, and the gate is asked only after
one of those hats has fired, which is what keeps the question off a vanilla
runtime entirely — there the gates can never arm. That is how the console gets
capitals and control characters: shift+a is 65, control+c is 3, and with neither
hat ever started, shift+a on vanilla is still 97. On the card the same two keys
are not modifiers at all but Doom's own `0x85` run and `0x84` fire, and `escape`
is its `0x87` menu and backspace its `0x7f`, which is what the card's contract in
`ref/emdoom-bare/bare/i_video_fb.c` asks for.

**There is no key palette and there will not be one.** A machine with a picture
of a keyboard drawn on its screen is not an emulated computer, and the round
that had one has been deleted rather than disabled: `src/rvkeys/pad.rav`,
`src/sprites/keypad.rav`, the two pictures in `assets/keys/`, the generator
`tools/keypad.mjs`, the tab and its grid, `pad_slot`, `pad_send`, `pad_shift`,
`pad_control`, the build's asset wiring and the check's click-a-cap helper are
all gone, and `tools/build.mjs` no longer writes any of them. What the keyboard
delivers, it delivers from the reader's own keyboard, and what it cannot
deliver it does not deliver.

### The keyboard, key by key, and what vanilla cannot see

The table is the whole of what a real keyboard can say to a vanilla Scratch
project. "How" is the block, "evidence" is the line in the VM that decides it,
and the last column is what one keypress makes of itself — measured by
`tools/check-rv32.mjs` out of the built console board, one real keypress at a
time, posted into `ioDevices.keyboard.postData` with the guest stopped so that
nothing drains the queue. Every bare `keyboard.js:NN` below is
`ref/scratch-vm/node_modules/scratch-vm/src/io/keyboard.js`, which is the
vanilla VM's keyboard device; `blocks-runtime-cache.js` and
`scratch3_event.js` are beside it under `src/engine/` and `src/blocks/`, and
`ref/turbowarp-vm/src/io/keyboard.js` is the other VM's, which names twelve keys
vanilla's does not — the reason the last three rows are a property of the
*runtime* and not of a key.

| key | seen? | how | evidence | one keypress makes |
| --- | --- | --- | --- | --- |
| `a`–`z`, `A`–`Z` | yes | a hat each; `key pressed? "a"` | `keyboard.js:47-67` upper-cases a one-character key, `:108` upper-cases a one-character argument, `blocks-runtime-cache.js:54-57` upper-cases the hat's field | `a` → 97 … `z` → 122, and 65…90 while shift is held, on TurboWarp |
| `0`–`9` | yes | a hat each; `key pressed? "0"` | the same three | `0` → 48 … `9` → 57 |
| space | yes | `when space pressed`; `key pressed? "space"` | `:51`, `:104-106` | 32 |
| enter | yes | `when enter pressed`; `key pressed? "enter"` | `:60` maps `'Enter'` to the recorded name `'enter'`; `:94` takes that name back out of `KEY_NAME_LIST` | 13 (carriage return) |
| the four arrows | yes | a hat each; `key pressed? "up arrow"` | `:52-59` | `ESC [ A`–`ESC [ D` = 27, 91, 65…68 |
| the thirty-two printable ASCII keys (`! " # $ % & ' ( ) * + , - . / : ; < = > ? @ [ \ ] ^ _ `` ` `` { \| } ~`) | yes | a hat each, whose `KEY_OPTION` field is the character (`Key::Period` … `Key::Tilde`) | `runtime.js:2020-2028` matches a hat on its own field whatever the field holds, and `blocks-runtime-cache.js:54-57` upper-cases it — punctuation is its own upper case | `.` → 46, `-` → 45, `/` → 47, `;` → 59, `=` → 61, `,` → 44, `(` → 40, `)` → 41, `'` → 39 |
| `any` | yes, as a key with no identity | `when any key pressed`; `key pressed? "any"` | `:138-140` answers for "is anything down"; `scratch3_event.js:15-17` starts the `any` hat for every key-down | — |
| backspace, delete, escape | **TurboWarp only** | a hat each, whose field is the key's name (`Key::Backspace`, `Key::Delete`, `Key::Escape`) | TurboWarp's `:82`, `:83`, `:88` give each a name and `:170` emits it; vanilla's `:63-65` returns `''` and `:115-118` returns before emitting | 127, `ESC [ 3 ~`, 27 — and on the card 0x7f, nothing, 0x87 |
| shift, control | **TurboWarp only** | a hat each that arms a gate; the letter hats then ask `key pressed?` | the same, `:84`, `:87` | shift+a → 65, control+c → 3; on the card 0x85 run and 0x84 fire |
| tab, insert, home, end, page up, page down, alt, caps lock, the function keys | **no** | nothing | the twelve TurboWarp names none of these are, and vanilla's six | nothing at all |

Three consequences, and each is measured rather than argued.

1. **Backspace can be typed on TurboWarp and not on vanilla.** A vanilla runtime
   drops the key before any block runs, so there is no hat, no `key pressed?`,
   and `key pressed? "backspace"` is a question about the letter `B`; a TurboWarp
   hat fires on the key-down and the shell can have its typo corrected. The check
   measures both: the row is 127 there and nothing here, and it also presses
   `s`-while-typing-`e` to prove the gate never asks a vanilla runtime the
   question that would have made it 83.
2. **Escape is the card's menu on TurboWarp, and fire and run are `control` and
   `shift`.** Doom's fire and run are 0x84 and 0x85 in the card's contract and
   its menu is 0x87, so on TurboWarp the machine's Doom can shoot and run and
   open the menu, which it could not before. `w`, `a`, `s`, `d` and the arrows
   still move and `space` still opens a door, and on vanilla the three keys are
   still nothing at all. A palette of caps could do it on either runtime, and
   there is no palette.
3. **Case can be typed on TurboWarp.** A hat cannot see the modifier: the DOM
   reports the character shift produces, and `postData` upper-cases it, so `a`
   and `A` are the *same key* to a hat. What the shift hat arms is the question
   `key pressed? "shift"`, which only TurboWarp answers as written — on vanilla
   it is a question about `S`, which is why it is asked only when that hat has
   fired, and the hat never fires there. Shift+`2` needs none of this: the DOM
   reports `@` and it is delivered as `@` on both runtimes.

**One honesty about taps.** A hat is delivered even by a tap that begins and ends
between two frames, because the key travels in the hat's own field and not in the
runtime's key state — and since the symbols got hats, *every* key the runtime
delivers is delivered that way, whatever the frame is doing. The card's movement
keys are the exception and they are meant to be: `poll_card` reads the keyboard's
*state*, and a state that is over before a frame runs is a key nobody held. The
check types the way a hand does (down, one frame, up) and then again with no
frame at all, and it *measures* the difference rather than assuming it: with the
guest stopped, every key in its table is pressed both ways. On the console board
the two columns are identical for every key in the check's console table; on the
card they differ only for the eight held ones. The rows that follow are a
TurboWarp run: on a vanilla runtime backspace, delete and escape are empty on
both sides, and shift+a and control+c are the plain letters, which the same
check asserts there:

```
keys          the built project, one real keypress per row (a frame of key-down, then no frame at all):
  "a"          [97]          [97]    ok
  "z"          [122]         [122]   ok
  "0"          [48]          [48]    ok
  " "          [32]          [32]    ok
  "Enter"      [13]          [13]    ok
  "ArrowUp"    [27,91,65]    [27,91,65]ok
  "ArrowDown"  [27,91,66]    [27,91,66]ok
  "ArrowLeft"  [27,91,68]    [27,91,68]ok
  "ArrowRight" [27,91,67]    [27,91,67]ok
  "!"          [33]          [33]    ok
  ...          the other thirty-one symbols, every one `ok`, the second column the same as the first
  "Backspace"  [127]         [127]   ok
  "Delete"     [27,91,51,126][27,91,51,126]ok
  "Escape"     [27]          [27]    ok
  "Tab"        []            []      nothing: this runtime drops the key before any block
  "Shift"      []            []      a gate, not a byte: it makes the next letter a capital
  "Control"    []            []      a gate, not a byte: it makes the next letter a control character
keys          0 of the 44 reachable keys need the key down while a frame runs: (none)
keys          shift + "a" -> [65], control + "c" -> [3] (TurboWarp gives a capital and ETX: [65] and [3])
ok   every key a real keyboard can reach arrived as its own byte, frame or no frame (44 keys: letters, digits, space, enter, the arrows, all thirty-two printable symbols, backspace, delete and escape)
ok   and the rest made no byte of their own, with or without a frame (Tab, Shift, Control)
ok   a modifier a hat can see made the letter it modifies (shift+a [65], control+c [3])
```

**And that is why the console looked locked when it was not — and why the
keyboard alone was not the whole story.** `ed` prints nothing between the command
and the `14` that says it wrote fourteen bytes, and what the *tty* echoes of the
reader's typing is written to the port the keys arrived on: stdin is `/dev/ttyS0`,
so a program's echo goes out the serial, and this machine does not draw the
serial. The shell's own typing is visible only because the shell's line editor
echoes it to its standard error, which `console-sh` points at `/dev/tty0`. So
inside `ed` the screen is blank for a reason that has nothing to do with the
keyboard — the reader is watching the card, and the card is where the guest's
*output* goes — and a reader who sees five keystrokes do nothing reasonably
concludes the keyboard is dead. What the check reads is the output: the file ed
wrote, and the shell reading it back. With the symbols arriving on a hat, the `.`
that ends an `ed` append is a real `.` key every time:

```
ok   ed received a real `.` keypress, wrote test.js and the shell read it back
ok   duktape evaluated a program typed at the prompt
ok   duktape ran a script file from the guest
ok   coremark ran and validated its results, read back out of the card
```

### The block benchmark

The three changes above were chosen from the table below, which is what one
Scratch block costs in the same VM the display runs in.
`tools/bench-blocks.mjs` builds a synthetic project per construct, runs it in
TurboWarp's VM with the compiler on, times `greenFlag` plus `_step()` until the
script says it is done, subtracts an empty loop's own cost, and reports the
median of five runs.

The unit is one `motion_changexby`, and the noise column is that row's own
`(max - min) / median`. **The box this was measured on is shared, and that
matters**: `bare_block` came back at 111.6, 138.0, 121.0 and 137.0 ns across four
full runs, and its within-run spread ranged from 5.6% to 36%. Rows whose net is
smaller than their own spread are marked `~` and should be read as "about zero",
not as the last digits.

| construct | ns/op (median) | fastest | × bare block | noise |
| --- | --- | --- | --- | --- |
| `item # of` a missing item in a 10,000 item list | 5281.6 | 4949.5 | 38.6 | 90% |
| a non-`warp` call, body moving a sprite | 1693.3 | 1273.1 | 12.4 | 28% |
| one monitor run: colour + `go to` + `pen down` + `set x` + `pen up` | **468.1** | 455.4 | 3.4 | 14% |
| **bare block** (`change x by`) | **137.0** | 121.9 | 1.000 | 23% |
| `repeat until` around a counter | 130.7 | 119.6 | 0.95 | 11% |
| `set pen color to` | 130.3 | 128.3 | 0.95 | 28% |
| a `warp` call, no parameters | 123.8 | 109.0 | 0.90 | 27% |
| `if/else` | 122.4 | 112.4 | 0.89 | 20% |
| a `warp` call, two parameters | 118.4 | 117.4 | 0.86 | 15% |
| `set x to` | 116.6 | 113.9 | 0.85 | 17% |
| `go to x: y:` | 107.7 | 101.4 | 0.79 | 18% |
| one non-`warp` loop iteration that yields | 88.2 | 79.8 | 0.64 | 10% |
| `if` with a one-block body | 60.5 | 55.3 | 0.44 | 17% |
| `join` (GC-bound) | 44.7 | 13.8 | 0.33 | **94%** |
| `efb_words[i] % 65536` | 27.0 | 21.5 | 0.20 | 29% |
| `pen down` | 17.0 | 15.9 | 0.12 | 10% |
| `add to list` | 8.4 | 8.0 | 0.061 | 17% |
| `set pen size to` | 8.0 | 7.7 | 0.059 | 16% |
| `floor(efb_words[i] / 65536) % 65536` | 7.7 | 7.5 | 0.056 | 10% |
| `timer` (`sensing_timer`) | 6.2 | 6.3 | 0.045 | 20% |
| `pen up` | 5.9 | 4.5 | 0.043 | 27% |
| a list item read, index 1 | 4.8 | 4.8 | 0.035 | 9% |
| a list item write, index 1 | 4.7 | 4.5 | 0.034 | 18% |
| a list item read, index 5,000 | 4.6 | 4.8 | 0.034 | 21% |
| a list item write, index 5,000 | 4.2 | 4.1 | 0.030 | 6% |
| `letter of` | 3.0 | 3.0 | 0.022 | 19% |
| `mod` on two numbers | 2.9 | 2.9 | 0.021 | 13% |
| `pen clear` | 2.6 | 2.5 | 0.019 | 19% |
| `change var by 1` | 1.4 | 1.2 | 0.010 | 14% |
| `+`, `−`, `×`, `÷`, `round`, `floor`, numeric `<`, `=`, `and`, `or`, `not`, `set var` | `~` | | ≈ 0 | > 30% |

Four rows are the ones that decided the work:

* **Anything that moves or paints a drawable costs about 110–140 ns**, because it
  goes through the renderer and asks for a redraw, while anything that is pure
  data or an operator costs 1–30 ns. A Scratch pen run is not a cheap thing and
  the display is pen bound; that is why the count of runs matters and why the
  fast path — which removes *words* from the walk, not runs from the picture —
  was worth doing rather than chasing the run drawing.
* **`word % 65536` on a value above 2^31 is 27 ns against 2.9 ns for the same
  remainder on two small numbers**, and 7.7 ns for `floor(word / 65536) % 65536`,
  which divides first and so never puts a large value into a remainder. That is
  change 3 above, and it is the single largest win of the three.
* **A list item read is 4.7 ns and does not depend on the index** — index 5,000
  costs the same as index 1 — so nothing anywhere in this project needs its
  lists laid out for the reader's sake. Only `item # of` on a miss is expensive,
  and nothing in the monitor uses it.
* **`timer` costs 6.2 ns**, which is why the throttle that used it is not missed.

The benchmark's own limits are stated in its header and repeated here: this is a
compiled-JavaScript VM on one shared machine, the sub-nanosecond rows cannot be
resolved at all, `join` is garbage-collector bound, and a `set variable to`
whose value nothing in the loop reads is removed by the engine entirely — which
is why every benchmark body increments a counter and every reporter feeds an
accumulator before anything is timed. `node tools/bench-blocks.mjs` prints the
table; `--json dist/bench-blocks.json` also writes the raw numbers, the `minus`
chain every derived row came from and the method block, which is what a rerun
should be compared against rather than the digits above.

### The owner's table, next to this one

The owner sent a per-opcode cost table as a scanned image (`ref/rtc-1.png`), and
it is worth having beside this one because the two agree about *order* and
disagree about *scale*. Its striking rows are the ones every Scratch programmer
knows — `touching color?` 1423.7, `color is touching color?` 5993.5, `distance
to mouse-pointer` 10422.7, `loudness` 5000.0, `switch costume to` 312.0, `stamp`
53.9, `say` 35.8, `timer` 5.0 — and none of them is in this project's machine:
there is no touching, no loudness, no costume switch and no stamp anywhere in
it. The rows it does use land where the local benchmark lands:

| opcode | owner's table (units) | this benchmark (ns) |
| --- | --- | --- |
| `pen down` | 18.1 | 17.5 |
| `pen up` | 3.5 | 6.3 |
| `timer` | 5.0 | 6.9 |
| `clear` (pen) | 6.6 | 2.4 |
| `go to x: y:` | 7.6 | 97.9 |
| `set x to` | 7.0 | 108.8 |
| `change x by` | 7.4 | 97.5 |
| a list item read | 2.8 | 6.0 |
| `+`, `-`, `*`, `/` | 4.0 | ≈ 0 |

Where they disagree the local measurement wins, because it is the same VM, the
same compiler and the same machine the display runs on, and because the owner's
table does not say what it measured on: its motion rows are seven units while a
motion block here is a hundred, and the two tables agree that all motion costs
one price and that pen and sensing are cheaper — only the unit differs. It is
also worth saying what the image actually contains: there is no
`motion_goto_menu` row in it, and no 10243, and the ten-thousand-row is the
*mouse-pointer* menu of `distance to`.

The one thing the owner's table can do that this one cannot is order the cheap
rows. `operator_add` comes back at 0.11 ns with 13% noise here and
`operator_equals` at −2.04 ns, so this benchmark cannot resolve anything below
about five nanoseconds — it is measuring the host's clock rather than the block.
The owner's table puts those at 1 to 4 units, and is the only one of the two
that can say `not` is dearer than `and`.

### The invariant the display rests on

There is one sentence the whole display is true or false by:

> **After the monitor has drawn, every pixel of the Stage's panel region is the
> card's memory.**

Scratch's pen has no erase that takes part of a line, so the Stage is the
*accumulation* of every run the pen has ever drawn. Under the whole-panel
repaint the sentence is true by construction, and the check asserts it rather
than trusting it: after the scroll-heavy sequence the check stops the guest,
gives the monitor forty passes, and compares the picture the pen drew against
the card's own memory pixel for pixel, and separately asserts that at least one
whole-panel pass ran after the guest stopped.

```
ok   the monitor repainted the whole panel after the guest stopped (40 passes in the 40 frames of grace)
ok   the Stage is the card's picture pixel for pixel after the guest stops (0 of 172800 differ)
```

The tolerance is zero columns. A row is drawn from its first pixel to its last,
so the pen's round cap bleeds half a pixel into the *next row's* own pixels,
which that row's own runs paint in the same pass, and no column of a drawn row is
left showing an older picture. What is not modelled is a browser's antialiasing
of that cap — the check compares the monitor's own output, which is why the two
pictures are written beside it for a reader to look at.

`tools/probe-residue.mjs` asks the same question *while the guest is running*,
one sample a Scratch frame, comparing the pen's own raster against `efb_words`
row by row and recording how long each row stayed wrong. Under the whole-panel
repaint a row can only be wrong for the part of one pass that has not drawn it
yet, so the probe's own burn-in trigger — a row that has been the wrong picture
for forty frames with the card quiet — is never reached. It is the instrument
that would have caught each of the three burn-ins, and it is kept for the one
that would come next.

### The ARM board, and the same change

`src/monitor/lcd.rav` is the ARM board's monitor and it now does the same thing:
one `pen clear`, then the whole of whatever the PL110 is scanning. What went
with the dirty-row redraw there is `clcd_dirty`, `clcd_row_dirty`,
`clcd_row_clean`, `clcd_dirty_all` and `clcd_note_write` in `src/dev/pl110.rav`,
`bus_fb_on`/`bus_fb_lo`/`bus_fb_hi`/`bus_fb_refresh`/`bus_touch` in
`src/board/bus.rav` and `cpu_touch` in `src/cpu/mem.rav` — the whole path that
watched where the guest stored so that the monitor could draw less. There is
nothing to watch any more: the monitor reads the controller's memory itself,
whole, every pass.

The ARM monitor also had a per-pixel problem the RISC-V one did not: it called
`clcd_pixel` for every pixel, and that call re-derived the bit depth, the stride
and the framebuffer base from the control registers and then walked a byte of
SDRAM, for every one of 76,800 pixels, several hundred times a second. At
sixteen bits a pixel — which is what both this board's firmware and its kernel
program — a row is the same shape the RISC-V card's direct colour row is:
`mem::sdram` is one Scratch list item to the 32 bit word, two pixels to an item,
the even column in the low half. So `monitor_row16` is the same walk, with the
same `runword` fast path and the same split of a word into two halves, and only a
dubious depth falls back to the general per-pixel reader. Without it the ARM
guest's boot on this host went from under four minutes to not finishing a
twenty-minute budget at all.

The ARM check's Stage-versus-scanout comparison stays a *number* rather than an
assertion, and it is the one place the two boards differ. The RISC-V check can
assert the invariant exactly because there the panel *is* the Stage: 480 by 360
of card onto 480 by 360 of Stage, one pixel to one pen unit. This board
resamples — a 320 by 240 panel on a 480 by 360 Stage makes every stroke 1.5 stage
pixels tall, so panel rows overlap and no stage pixel *is* a panel pixel — and
the sample reports about 12.7% of its pixels differing however right the monitor
is:

```
scanout       320x240 against the Stage's 480x360 raster: 9744 of 76800 sampled pixels differ (12.7%) -- measured, not asserted
```

That number is the pen's round cap and the row overlap, which the sample does not
model. What can be said about residue on this board without a mapping is said by
construction instead: every pass erases the whole pen layer and repaints the
whole panel, so there is no scheme left by which an older picture survives a
pass. Turning the number into an assertion would be a guess in the direction that
makes the check green, which is how the last three burn-ins got shipped.

**What could not be measured.** A browser. Every picture here is the monitor's
own pen output rasterised by the same rules the monitor draws by, so
antialiasing, the pen's exact cap and Scratch's own compositing are outside what
any of it can see; `dist/screen-*.png` is the closest thing to a screenshot this
project has. And the benchmark is not free of the host it runs on: it is a
compiled-JavaScript VM measured in wall-clock time on a shared machine, so it is
good to a factor, not to a digit.

### The ARM boot, measured and not yet cut

The owner asked for this machine to reach its first screen sooner. It has not
been done, and the measurement below is where a round that does it should start
rather than a claim that it has been.

`SCRATCH_VM_ROOT=ref/turbowarp-vm node tools/check.mjs --budget 600` reports, for
a whole six-hundred-second run:

```
steps         34690 runtime steps in 600.1 s
guest         284331831 instructions retired
pen           496197670 lines drawn over the whole run (34683 whole-panel erases)
monitor       34709 frames, 2663654400 pixels read, 248098835 runs drawn
picture       dist/stage.png  960x720 from 320x240 at 0xe00000, PLD mode 3
scanout       320x240 against the Stage's 480x360 raster: 9744 of 76800 sampled pixels differ (12.7%)
```

The two derived numbers are what matter. A step is **17.3 ms**, which is *inside*
a Scratch frame, so this board's display is not the thing that makes its boot
slow — unlike the RISC-V console, whose unbounded step was 64.45 ms. The guest
retires **473,846 instructions a second**, which is about a sixth of what the
RISC-V hart manages on the same host, and that is the whole of the distance: the
ARM boot is bound by blocks per retired instruction in `src/cpu/core.rav`, not by
the pen, the panel or the slice. The pen is 7,152 runs a pass against the
console's 8,984, so the monitor is a large share of a step but not the reason the
guest is slow.

What the next round should do, in the order the numbers suggest:

* **Profile the retired instructions, not the wall clock.** `tools/cpu-trace.mjs`
  and `tools/slice.mjs` both exist for it, and neither was used this round; the
  question is which of `cpu_class_load_store`, the extra transfers, the
  halfword multiplies and the dispatcher's condition tree holds the ninety
  million instructions between reset and `fbcon` taking the screen.
* **Then cut blocks per instruction** in whichever of those it is — cached
  instruction fields and fewer dispatch layers is the shape that worked for the
  RISC-V hart, which is a table-driven decoder where this one is a tree.
* **The monitor is the second lever and a smaller one.** `src/monitor/lcd.rav`
  clears and repaints the whole panel every pass and has no background band at
  all, where the RISC-V monitor's band halves the strokes of a text row. Adding
  the two-endpoint band there is a handful of lines and would give the guest
  back about a third of a step; it was not done because it could not be measured
  inside this round's budget, and an unmeasured change to the picture is how
  every burn-in in this project's history got shipped.

### The graphics card, and the honest state of Doom

The card is a device on the board and the monitor draws its output. Two guests
can be booted on it, and they are two different software stacks:

| guest | what it is | how it reaches the card |
| --- | --- | --- |
| `images/mini-fb-image` (Linux 6.8) | the shell with coremark, duktape and `ed` | **it does, through the kernel's own framebuffer console.** `simplefb` binds the card's `r5g6b5` panel from the device tree, `fbcon` puts the VT console on it, and the shell writes to `/dev/tty0`. |
| `emdoom-autostart.bin` (bare metal) | embeddedDOOM with `-warp 1 1`, no Linux | **it does, completely.** Its own video driver writes `0x1040_0000` and commits. |

The bare metal guest is the one that proves the *card*, and it is a real
picture rather than a colour count:

```
boot          700 frames in 7.1 s, 25029120 guest instructions
frames        9398 runtime steps in 259.3 s
pen           457333608 lines drawn over the whole run (54252 in the last pass, which is what `pen clear` leaves)
monitor       9397 frames, 601408000 pixels read, 228666804 runs drawn
input         40 frames to settle, then 37852 of 64000 pixels changed
              holding the up arrow; a 120-frame window with no key changed 2;
              37852 of the change is the view and 0 is the status bar;
              input_buffer 0 left
ok   holding the forward key moved the picture: 37852 pixels of 64000 changed (59.1%), against 2 in a window of the same length with no key
ok   and it is the level that moved, not the status bar: 37852 pixels above the status bar against 0 in it
ok   the guest read the queue dry, so the key was consumed exactly once
ok   the card latched 142 frames from the guest
ok   the guest's frame sequence was acknowledged (seq 140, ack 140)
ok   the monitor read a whole 320x200 frame (601408000 pixels)
ok   the monitor scanned the card out as runs (228666804 runs)
```

The four numbers that moved since the run this block used to quote are the
step's two: 2,085 steps in 195.5 s was 93.8 ms a step, and 9,398 in 259.3 s is
27.6 ms — the guest's own 8,000 µs slice, in
[a slice per guest](#a-slice-per-guest-and-what-a-step-costs). The pen's run
count rose with the frame count, which is what a whole-panel repaint per pass
means: 457,333,608 lines over 9,397 passes against 105,820,836 over 2,084.

and the guest says which device it found, out of its own UART:

```
I_InitGraphics: framebuffer at 0x10400000 id=EFB1 fmt=2 320x200 pitch=320 pixels@00010000 palette@00001000
```

`dist/card-doom.png` is the frame the card was holding when the run ended: its
latched pixels through its own palette, read out of `efb_scan_pixels` and
`efb_scan_palette`. It is E1M1 — the starting area, the status bar reading
`AMO 50`, `HEALTH 100%`, `ARMOR 0%`, and the Doom face. It is the frame *after*
the input test below, so it is the room seen from a few steps further forward
than the one the guest started in; `dist/card-doom-before.png` is where the
player was standing before the key.

**Doom's picture is on the Stage and its own driver put it there.** That is the
whole of what this example claims about Doom: the bare metal guest writes the
card's window directly and commits, and the monitor scans out the frame the board
latched. Nothing between the guest and the Stage knows what a picture is.

### The keyboard: two keyboards, and the bytes each one wants

The machine has two keyboards, and which one a guest reads is the board file's
business, not the guest's:

| guest | its keyboard | the byte an up arrow is |
| --- | --- | --- |
| `images/mini-fb-image` (Linux) | the 8250 at `0x1000_0000`, which is the kernel's `ttyS0` | `ESC [ A` — 27, 91, 65 |
| `emdoom-autostart.bin` (bare metal) | the card's `KBD_STATUS`/`KBD_DATA` at `0x1040_0034` | 128 — `0x80` |

A console wants a *terminal's* bytes and a card wants the codes its driver was
written against, and they are different bytes for the same key, so a build has
to choose. `boards/mini-rv32.mjs` says `input: 'card'` for the bare metal guest
and nothing at all for the Linux one; `tools/build.mjs` compiles the
choice into `RV_INPUT_CARD` in `src/rvboard/decode.rav` and refuses a name that
is neither; `src/sprites/rvinput.rav` makes the bytes the winning encoding
needs. There is one queue behind both — `input_buffer` in `rvio::io` — because a
board has one keyboard: the 8250's data register and the card's `KBD_DATA` both
pop its head, and the line status register and `KBD_STATUS` both only say
whether there is one.

**The bug this section exists for.** For a round the queue was filled with the
console's bytes in *every* build, so the bare metal guest was handed `ESC [ A`
for an up arrow. Its driver read those three bytes as Doom's `KEY_ESCAPE`
followed by two unbound letters: pressing an arrow opened the menu instead of
walking, and WASD, control and shift reached nothing at all. The check did not
see it, because it never pressed a key — what it asserted was the card's frame
handshake, and the frame handshake was working. Reading a register back would
not have seen it either. What saw it was the picture, measured against a window
of the same length in which no key was pressed:

* holding the up arrow for 120 Scratch frames changed **45,407 of the card's
  64,000 pixels** (70.9%), against **38** with no key pressed — and the 38 are
  the status bar's own face, which blinks whether or not anyone is playing;
* 45,334 of those pixels are above the status bar, so it is the *level* that
  moved — the view rendered from where the player is now standing — rather than
  a menu drawn over it;
* `input_buffer` was empty at the end, so the guest read the queue dry: the
  byte was delivered exactly once, which is what "reading `KBD_DATA` consumes
  it" has to mean.

`dist/card-doom-before.png` and `dist/card-doom-after.png` are the two frames,
the card's own latched pixels before and after the key, written by the check.
The second is the same room seen from further forward.

The mapping the card build makes, and what each byte is to Doom:

| what is pressed | the byte the card gets | what the guest's Doom does with it |
| --- | --- | --- |
| up arrow, `w` | 128 `0x80` | forward — `key_up` is `KEY_UPARROW` |
| down arrow, `s` | 129 `0x81` | back — `key_down` |
| left arrow, `a` | 130 `0x82` | turn left — `key_left` |
| right arrow, `d` | 131 `0x83` | turn right — `key_right` |
| space | 32 `0x20` | use, open a door — `key_use` |
| enter | 136 `0x88` | confirm a menu entry |
| `,` and `.` | 44 and 46 | strafe left and right — Doom's own defaults |
| `0`–`9` and the other thirty symbols | their ASCII | nothing; Doom has no binding for them |
| **control** (`0x84`, fire) | 132 `0x84` on TurboWarp, nothing on vanilla | fire — `key_fire` |
| **shift** (`0x85`, run) | 133 `0x85` on TurboWarp, nothing on vanilla | run — `key_speed` |
| **escape** (`0x87`, the menu) | 135 `0x87` on TurboWarp, nothing on vanilla | the menu — `key_escape` |
| **backspace** (`0x7f`) | 127 | nothing; no binding |
| **tab** (`0x89`), **alt** (`0x86`), **F1**–**F12** (`0x8a`..`0x95`) | — | **no key reaches them** |

The five arrow rows and the letter aliases are `poll_card`'s, not a hat's: the
card's keyboard is one register a driver polls, so the machine reports every key
that is *down* on each frame the guest has caught up on, and that is what
walking is. Everything else in the table is a hat, which is one report per
key-down — and fire, run and the menu are one report rather than a held key,
which is what the driver's own `FB_KEY_DOWN_TICS` is for: it holds what the board
reported for four of its tics.

The last row is the limit and it is not a mapping oversight. The card's contract
has a byte for tab, alt and the twelve function keys and the *board* could
deliver it — one hat each in `src/sprites/rvinput.rav` — but neither runtime
names them: `alt` and the function keys are longer than one character in both,
and tab is one TurboWarp does not have either. So this Doom can be walked,
turned, strafed, used and — on TurboWarp — fired, run and sent to its menu, and
on a vanilla runtime it can only be walked, turned, strafed and used. That is
what the palette used to be for, and there is no palette.

`w`, `a`, `s` and `d` are aliases for the arrows rather than letters because
*this* Doom's own default bindings are the arrows and nothing else — a letter
has no binding to reach, so the alias is made on the Scratch side or not at
all. Nothing else was re-bound, and none of it is a preference file: the guest
has no filesystem, so `m_misc.c`'s compiled-in defaults are the whole of what
the game can be played with.

Turning, walking and WASD were measured with
`tools/probe-play.mjs`, the instrument beside the check: it asserts nothing,
boots the same project, holds each key the way a player would and prints what
changed. Over the same 150-frame window, on the same run:

| held | the card's pixels that changed | where |
| --- | --- | --- |
| nothing (the control) | 75 (0.1%) | the status bar's face, 73 of them |
| up arrow | 38,463 (60.1%) | the level — 38,429 above the status bar |
| left arrow | 41,887 (65.4%) | the level |
| `w` | 43,622 (68.2%) | the level |
| `,` | 44,055 (68.8%) | the level: strafing moves the player, and the view is drawn from where the player is |
| up arrow *and* left arrow | 48,153 (75.2%) | the level — walking and turning at once |

The control is 75 pixels and 73 of them are in the status bar, which is the
face blinking whether or not anyone is playing; every key row is tens of
thousands of pixels *above* it. The last row is why the round of keys is gated
as a whole and not report by report: holding two keys has to put two reports in,
or a player cannot take a corner.

The row after `,` used to be `control (fire)`, and it was taken out when the
palette was: what that row measured — nine pistol shots, the ammo counter going
from `AMO 50` to `AMO 41` — was a measurement of the palette, because a *held*
`control` is a poll's row and a poll could not see the key at all. It is back
now and it is a hat's row: pressing control reports 0x84 once, Doom's own driver
holds it for four tics, and the shot happens. What stands where it did is `,`,
which is the only *punctuation* in the table and therefore the row that proves a
symbol key reaches the machine: `,` has no name in the editor's dropdown, its
hat's field holds the character itself, and holding it moved 44,055 pixels of the
level.

A key *held* is the card's problem and not the console's, because Doom's driver
reports an edge rather than a press and a release: after one report it holds the
key down for four of its own tics. `poll_card` therefore reports a key that is
*down* — read from the reader's real keyboard through `key pressed?`, which is a
question about the present — again as soon as the guest has read the last report
(`io_input_free`), so walking is walking rather than one step per press, and the
queue never holds a backlog of reports for a key that was let go a second
earlier. This is also why the movement keys are *not* pushed by their hats on the
card build: the hat and the poll would put two reports in for one tap, and Doom's
menu moves one entry per report. The whole round of keys is gated rather than
each report, because a player walks *and* turns: a gate per report would let the
first key that found the queue empty fill it, and the second would be starved for
as long as it was held, which is a corner taken as two separate actions. A round
therefore puts in one report for every key that is down, and leaves the queue
holding at most as many bytes as there are keys a player can hold. It is bounded
anyway — `INPUT_LIMIT`, 32 bytes — and `io_push` refuses when it is full, so a
keyboard nobody drains costs a fixed list rather than one that grows until
Scratch refuses to add to it. A power-on clears it: `Machine.powerOn` resets the
bus, and the bus resets the console, which is where `input_buffer` is emptied.

**Doom not running is the other half of the contract.** The Linux guests are
where that matters, and they are the case the build's encoding choice is for:
their kernels have no driver for `0x1040_0034` at all, so the card's two
registers answer for a guest that never asks, and a key typed at those guests is
a console byte from the moment it is made — the card's codes never reach the
8250 and `ESC [ A` never reaches the card, so neither keyboard can be handed the
other's bytes and no pending card key can corrupt a console. That the Linux
console is not corrupted by any of it is what its own check measures, and it
measures it by typing: `coremark`, `duktape`, `ed` and `vi` go in as keys and all
four come back out of the card's framebuffer.

**What the guest does not receive, plainly.** Tab, alt, caps lock and the
function keys cannot be delivered from a *keyboard* at all, and it is worth being
exact about why: no runtime names them. A vanilla runtime drops any key whose
name is longer than one character before any block sees it
(`keyboard.js:47-67`, `:115-118`), and TurboWarp's twelve extra names
(`ref/turbowarp-vm/src/io/keyboard.js:14-27`) do not include them either. So
there is no hat to write, `postData` emits nothing for them, and `key pressed?`
cannot name them: `key pressed? "escape"` asks about the letter `E` on vanilla.
Caps lock is a toggle rather than a key, and nothing polls it. That is the card's
`tab`, `alt` and `F1`–`F12` and the console's `tab`, and it is why `alt` is the
one key of the card's five that no keyboard can reach. Everything else in the
contract is delivered from the reader's own keyboard: the arrows, enter, space,
the ASCII letters, digits and the thirty-two printable symbols on both runtimes,
and backspace, delete, escape, fire, run and the menu on TurboWarp.

| what is pressed | how the console gets it | how the card gets it |
| --- | --- | --- |
| letters, digits, space, enter, the four arrows | a hat — the dropdown names them | the arrows and `w`/`a`/`s`/`d` from `poll_card`, the rest from a hat |
| `.` `/` `-` `=` `;` `,` and the other symbols | a hat each, whose field is the character (`Key::Period` … `Key::Tilde`) | the same |
| backspace, delete, escape | **TurboWarp only** — a hat each | **TurboWarp only** — 0x7f, nothing, and the menu 0x87 |
| shift, control | **TurboWarp only** — a hat each arming a gate | **TurboWarp only** — run 0x85 and fire 0x84 |
| tab, alt, caps lock, the function keys | **nothing — no runtime names them** | **nothing** |

### The ARM board's keyboard, and the same table

`src/sprites/input.rav` is the ARM board's other end of the same wire, and it
writes through `dev::console::type_byte` into the PL011's receive FIFO at
`0x101F1000`. That port is the guest's `/dev/console` — the command line in
`images/versatile-pb.dtb` ends `console=tty0 console=ttyAMA0,115200`, and the
last one wins — so what is typed there is what the shell `init` execs reads.
`dev::console::type_byte` had no caller before this.

**It is the older shape of the RISC-V keyboard and it has the older flaw.** It
still reads the thirty-two printable keys by asking `key pressed?` from the
`when any key pressed` hat, out of the table in `src/keys/ascii.rav`, so a tap
shorter than a frame is lost on this board exactly as it was on the other one.
It is left that way because the RISC-V fix is the forty-eighth line of the
sprite that was wrong and this board was not the one the bug was reported
against; the same thirty-two hats, and the same `Key::` variants, are what it
would take.

`tools/check.mjs` types at it with real keys, and waits for the console to come
back between one key and the next, because a person types at the speed the shell
echoes and the PL011's FIFO is sixteen bytes deep:

```
keys          typed "echo raven-keys" with real keypresses; the port answered "echo raven-keys\r\nraven-keys\r\n"
ok   real keypresses reach the guest and the shell runs what was typed: the port echoed "echo raven-keys" and then printed its own output for it
```

Two `raven-keys` and not one, because the first is the tty echoing the line as it
was typed — which says the bytes arrived — and the second is `echo` printing its
own argument, which says the shell read the line and ran it. It has no
`backspace`: this sprite is the older shape described above, so this shell can be
typed at and not corrected, where the RISC-V one can be corrected on TurboWarp.

### The console's colour, from the card's memory

The owner asked whether the console text's colour is right. It is, and the whole
of the distance between what the kernel asks for and what the Stage shows is the
guest's own choice of a sixteen-bit panel. Three values, and each is read rather
than reasoned:

| | value | where it was read |
| --- | --- | --- |
| what the kernel asks for | `#AAAAAA` on `#000000` | `fbcon`'s VGA palette: colour 7 is `0xAA` in all three of `default_red`, `default_green`, `default_blue` |
| what the card holds | `0xAD55` — r5 21, g6 42, b5 21 — and `0x0000` for the paper | the check's own histogram of `efb_words`: **`0xAD55` x5054**, and no other non-zero pixel anywhere in the 172,800 |
| what the pen draws | `#ADAAAD` = (173, 170, 173) | `efb_r5` / `efb_g6` in `src/rvmonitor/efb.rav`, on a direct colour card with no palette of its own |

`#AAAAAA` cannot be *stored* in `r5g6b5`: an eight bit `0xAA` is 21 of 31 reds,
42 of 63 greens and 21 of 31 blues, and that is what the guest's own
`simplefb` panel makes of it. Expanding back is the display's rounding —
`round(21 · 255/31) = 173` and `round(42 · 255/63) = 170`, which is what a
five bit channel does — and not a shift, which would give 168, 168, 168 and show
`#A8A8A8`, a colour this machine never displayed.

So the text is `#ADAAAD`: **light grey, not white, and not supposed to be.** It
is the guest's choice, the card's arithmetic is the panel's, and there is no
rendering error between them. The README's title for this console says "grey
ink" in several places and that is exactly what it is.

### The GPU: which paths there are, and why there is no local erase

**Local erase is not used anywhere in this project, and it is not a policy that
could be relaxed.** Scratch's pen draws lines and has no erase that takes part
of one back: the only erase is `pen clear`, which takes the whole layer. Both
monitors therefore clear the whole pen layer and repaint the whole panel every
pass, unconditionally — `src/rvmonitor/efb.rav`'s `monitor_draw` and
`src/monitor/lcd.rav`'s `monitor_draw` — and there is no dirty row, no dirty
span and no pending flag anywhere under `src/`.

The owner's suggestion for when an erase *is* needed — stamp a pure-black sprite
over the region and then redraw it — was measured rather than argued, and it is
not taken, for two reasons that are both facts about Scratch rather than
preferences:

* **A stamp is not cheaper than the erase it would replace.** The owner's own
  per-opcode table (`ref/rtc-1.png`) prices `stamp` at 53.9 units and `clear`
  at 6.6, and this project's own benchmark prices `pen clear` at 2.4–2.6 ns,
  which is the cheapest block in the monitor. A stage-sized stamp is one
  composited sprite per pass against one canvas clear, and it buys nothing: the
  layer has to be repainted either way.
* **A pure-black sprite is only right for a panel whose paper is black.** The
  Linux console's is black by luck; Doom's palette paper is whatever its palette
  says, and the ARM board's boot pattern is eight colours with no background at
  all. A stamp would have to be tinted to the guest's own paper, and Scratch's
  colour effects rotate hue rather than set an RGB value, so there is no costume
  that can be *made* the guest's paper. `pen clear` needs to know nothing about
  the picture it takes back, which is exactly why it is the right erase.

What is left is the *repaint*, and that is where the specialised paths are. A
path is chosen by the board file and not at run time: which panel is soldered to
the card is a board decision, `tools/build.mjs` compiles it into
`RV_EFB_FORMAT`, `RV_EFB_INDEXED`, `RV_EFB_WORDS_PER_ROW` and friends in
`src/rvboard/decode.rav`, and the monitor reads those constants rather than a
register. There are four paths and each says what it assumes:

| path | where | what it assumes | when it is chosen |
| --- | --- | --- | --- |
| indexed runs | `monitor_row_indexed`, `src/rvmonitor/efb.rav` | one list item a byte, a 256-entry palette at a fixed offset, and a guest that commits a frame rather than writing the scanout under the monitor | the card's `format` is `index8` — the bare metal guest's own driver |
| direct colour words | `monitor_row_direct` | sixteen bits a pixel in one 32 bit list item, an even pitch, and a paper the row begins and ends with; the `runword` fast path and the two-endpoint band | the card's `format` is `rgb565` — the Linux guest's `simplefb` |
| controller words | `monitor_row16`, `src/monitor/lcd.rav` | the same word walk, but over `mem::sdram` and at the base the PL110's own `LCD_UPBASE` names | the ARM board, when the controller is programmed for sixteen bits |
| controller pixels | `monitor_row_any`, `src/monitor/lcd.rav` | every depth the controller can be in, one `clcd_pixel` a pixel, which re-derives the depth, the stride and the base per pixel | the ARM board, at 1, 2, 4, 8 or 24 bits — the *general* path, and the slow one |

Two of the owner's asks are **not built**, and it is better to say so than to
describe a design as if it were code. There is no *text* path and no *3D* path,
and the reason is that neither has anything to assume: this machine's pen has no
font, so a text path could only be a run walk over pixels that happen to be
glyphs, which is what the direct colour path already is; and a 3D path would be
a property of the guest's driver and not of the card, because the card is a
framebuffer and knows nothing about what wrote the pixels. What the card *could*
carry, and does not yet, is a **dirty-rectangle report from the guest** — a
device register a driver fills with the region it changed — which would let the
monitor draw ink only where the guest said it put ink, with the paper band
already covering the rest. That is a device and a driver change together, and
nothing in this project has one.

### A slice per guest, and what a step costs

The owner asked for a per-guest slice budget **in the board file**, and that is
where it is now: `slice: { instructions, microseconds }` in
`boards/mini-rv32.mjs`, the console's at the board level and the bare metal
guest's in its own entry, compiled into `RV_SLICE` and `RV_SLICE_US` in
`src/rvboard/decode.rav` and applied by `src/sprites/rvmachine.rav` when the
machine powers on. It is a *guest's* number and not a machine's because what a
guest costs the frame is what its own panel costs the pen:

| guest | panel | slice |
| --- | --- | --- |
| `linux` | 480x360 direct colour, about 9,000 strokes a pass | 262,144 instructions or 20,000 µs |
| `doom` | 320x200 indexed, about 50,000 strokes a pass | 262,144 instructions or 8,000 µs |

Both numbers were measured on the same host, on the same build, with the time
bound the only variable — `node tools/watch-rv32.mjs --boot --idle 20` and the
same command with `--slice-us 0`, which is the machine as it was before the
bound existed. The console's pair is in the table below. **The game's 8,000 µs
is a *derived* number rather than a measured one** and is labelled that way: its
pass is about fifty thousand strokes at the 468 ns this project's benchmark
gives a run, so the pen alone is about 23 ms of a 33.3 ms frame and the machine
can have ten. Eight is that with a margin for the browser's own drawing, and the
Doom build has not been run against the bound yet — what would settle it is
`tools/watch-rv32.mjs --until "I_InitGraphics" --idle 20` on the Doom project,
which is the measurement this round did not make.

| | instruction slice only (`--slice-us 0`) | the board's 20,000 µs |
| --- | --- | --- |
| **a runtime step** | **64.45 ms, 15.5 steps a second** | **25.91 ms, 38.6 steps a second** |
| whole-panel passes in a 20 s window | 311 | 772 |
| runs a pass | 8,969 | 8,984 |
| guest instructions a step | 260,272 | 86,181 |
| guest instructions in the 20 s window | 80,944,524 | 66,531,516 |
| seconds to `Run /init as init process` | 13.6 | 17.1 |
| guest instructions to the same marker | 47,242,240 | 47,778,816 |
| instructions a second | 3,473,694 | 2,793,919 |

Three things are worth reading off that table and none of them is a surprise
once the shape of the machine is written down.

**The step is the number, and a Scratch frame is 33.3 ms.** Unbounded, a step is
64.45 ms and the display runs at 15.5 a second, which is the owner's "10 to 15
fps" and is arithmetic rather than a browser. Bounded at twenty milliseconds it
is 25.91 ms and 38.6 steps a second, which leaves TurboWarp's own renderer about
seven milliseconds of every thirty-three — the monitor's pass is the other ~5.5
of it, so most of a bounded step is a repaint that has to happen.

**The runs a pass fall by half, which is the band still doing its job.** 8,969
and 8,984 runs a pass on these two runs, against the 16,971 the same *kind* of
console picture cost with no band at all and the 8,672 it cost with the old
one-pixel band — the 16,971 and 8,672 are the earlier round's numbers for one
pinned picture, and today's two are a different scroll of the same console. The
two-endpoint rule costs nothing measurable, because a console row's paper is at
both of its ends.

**The game's own budget is measured too, and it is the largest win of the
three.** `node tools/check-rv32.mjs dist/desktop-rv32-doom.sb3` reports the
whole run's runtime steps and seconds, and the two readings are two-thirds of an
order apart:

| | Doom, 20,000 µs (the board's old default) | Doom, its own 8,000 µs |
| --- | --- | --- |
| a runtime step | 93.8 ms — 10.7 steps a second | **27.6 ms — 36.3 steps a second** |
| the check's whole run | 2,085 steps in 195.5 s | 9,398 steps in 259.3 s |
| the input test | 45,412 of 64,000 pixels moved | 37,852 of 64,000 moved |

The third row is the control that says the *picture* did not get cheaper: the
game still moves most of the card when the forward key is held, and what
improved is how often the display gets a turn. `machine_slice_us` is a budget
and not a throttle on the guest's work — the guest retires fewer instructions a
second and the frame count goes up by four and a half times, which is the trade
in the direction the owner asked for.

**A shorter slice makes the boot *slower*, and that is the honest half of the
trade.** 13.6 s unbounded against 17.1 s bounded: the guest retires about a
fifth fewer instructions a second because the monitor gets a larger share of the
frame, and it reaches the same marker 1.1% further on in its own instruction
count. What it buys is a display that redraws 38 times a second instead of 15,
which is the whole reason the bound exists. The guest is not hurt in
*throughput* by a short slice — the frame loop runs another one as soon as this
returns — only by the share of the frame the display takes.

### The coremark score, from first principles

The owner's question was whether the jump from about 1.6 to about 8 is
挂羊头卖狗肉 — a label on the wrong meat. It is not, and the whole of it is
arithmetic about what CoreMark's score *is*.

**What CoreMark measures.** One iteration is one pass over five fixed integer
workloads — a linked list, a matrix multiply, a state machine and a CRC over
each — and the score CoreMark prints is `Iterations / Total time (secs)`. The
run above says so itself:

```
Total ticks      : 15803
Total time (secs): 15.803000
Iterations/Sec   : 6.960704
Iterations       : 110
```

and 110 / 15.803 = 6.9607 exactly. The iteration count is not compiled in: with
no `-i` argument CoreMark **calibrates** it, running until its own clock says at
least ten seconds have passed. That single fact is the whole of the answer.

**Which clock the guest times itself with.** Not the host's. CoreMark's
`clock()` is the guest kernel's, the kernel's monotonic clock is the CLINT's
`mtime`, and the device tree's `timebase-frequency = <0xf4240>` says `mtime`
counts a megahertz — one count a microsecond, which is why 15,803 ticks are
15.803 seconds. `src/rvdev/clint.rav` makes that true by advancing `mtime` by
the *host's* clock rather than by retired instructions; before that fix, `mtime`
was the instruction count, and this machine retires about three and a half
million instructions a second, so a guest second was 0.259 of a real second —
the clock ran **3.866 times fast**.

**What that does to the score.** The calibration fixes the *guest* duration, not
the iteration count. If the machine does `R` iterations a second of real time
and its clock runs `k` times fast, then `N` iterations take `N/R` real seconds
and `k·N/R` guest seconds; the calibration picks `k·N/R = 10`, so `N = 10R/k`,
and the reported score is `N / 10 = R/k`.

```
                  before the fix        after
k (clock rate)    3.866                 1.000
iterations        28 (= 110 / 3.866)    110
guest seconds     10                    15.803
score = N/seconds R / 3.866             R
```

So a clock that ran fast made the benchmark's own ten-second limit arrive in a
*third of the real time*, and the machine honestly did a third of the work per
run and honestly reported a third of the score. **The clock fix multiplies the
reported score by 3.866 and does nothing else to it** — which is the owner's
own arithmetic, and 1.6 × 3.866 = **6.19**, against today's measured **6.96**,
the small excess being that the machine also got a little faster between the two
readings. The direction is the part that had to be worked out: the fix divides
the *seconds*, and the seconds are in the denominator.

**And the machine is genuinely executing the guest.** Four pieces of evidence,
none of which is a claim about the source:

* **The retired-instruction count.** `rv_instructions` is the hart's own counter
  and the check reads it either side of the command: **70,913,899 guest
  instructions** retired across the `coremark` line, in 15.803 guest-seconds,
  which is **4,487,600 instructions per guest second**. The same machine's rate
  measured at the boot (`tools/watch-rv32.mjs --boot`) is 2.6M a second with the
  monitor sharing every frame, and a tight loop with a still screen is dearer
  than a boot — the two agree, and neither is a number a short circuit could
  print.
* **CoreMark's own validation.** `seedcrc 0xe9f5`, `[0]crclist 0xe714`,
  `[0]crcmatrix 0x1fd7` and `[0]crcstate 0x8e3a` are CoreMark's documented
  values for a 2K performance run with the default seed, and `Correct operation
  validated.` is printed by CoreMark itself only when `crcfinal` matches what it
  expects for the iteration count it chose. A machine that skipped instructions
  would produce different CRCs and print `ERROR!` instead — the same program
  refuses to report a score at all.
* **The guest's clock against the host's.** The measurement in
  [the clock section](#the-clock-the-frame-and-the-key-that-was-never-reported)
  is 1,000,900 guest microseconds per wall second, ratio 1.000. The score's
  denominator is only honest if that is true, and it is measured rather than
  assumed.
* **Nothing short-circuits an instruction.** The execution path is
  `rvboard::decode` → `rvbus::bus` → `rvcpu::hart::step`, and `step` is the only
  place `rv_instructions` moves: it decodes, executes and retires, and the
  counter is not touched by any other block. There is no "skip" path anywhere
  under `src/rvcpu/` or `src/rvbus/`, and the counter is what every other
  measurement in this README is normalised by.

**Verdict.** The architecture is honest and the score is real. The 1.6 was a
correct score of a machine whose clock ran 3.866 times fast — the score was
*low* for the same reason a stopwatch that runs fast makes a job look short, and
the fix is what restored it. Nothing is short-circuited, the guest's own
validation passes against the published CRCs, and the retired-instruction count
is the right order for the guest's own seconds. The README's earlier habit of
printing the score with no account of it was the misleading part and it is
corrected here.

### What a different RISC-V board would change

This is the question the whole example exists to answer, and the answer is the
same as it is for the ARM board: **one file**.

* A board with **more RAM** is `memory.ram.size`, and nothing else — the decoder,
  the device tree's `memory` node, the device-tree address and the reset all
  read it.
* A board with a **different console** is a different `kind` on a device entry
  plus one module; the bus dispatches by the id the decoder gave, and the
  device list is what decides which module that id names.
* A board with a **different display** is the `display` table — the card's
  format, width, height, pitch, palette size and window offsets are generated
  into `rvboard::decode`, the device, the monitor and the device tree all read
  them from there, and a guest that wants a different panel overrides them in
  its own entry. A 640x480 `r5g6b5` card is seven numbers and a format name;
  the `simple-framebuffer` node, its `reg` and its `format` string are the
  generator's, not anyone's to remember.
* A board with **no graphics card** is this file without the `efb` entry; the
  bus then answers that page as an unassigned address, which is what a hole in
  a memory map reads back as, and the monitor scans out the empty card the same
  way it scans out any other — the whole panel, every pass, all of it black.
* A board with a **different guest** is the `guests` table. `tools/build.mjs
  --guest doom` builds the same machine with a different image in RAM and a
  different project manifest, and everything else — the decoder, the devices,
  the hart, the monitor — is byte for byte what the Linux build produced.
* A board whose guest reads a **different keyboard** is that guest's `input`
  entry, `console` or `card`. It is one word in the board file and one line the
  generator writes; the bytes themselves are `src/sprites/rvinput.rav`, which
  makes the encoding the build asked for and no other.
* A board with a **different processor clock** is `clocks.cpu`. It is the one
  number that is a decision rather than a datasheet value: it sets how much
  guest time a retired instruction is worth.

The board file and the device tree are checked against each other rather than
compared by eye. `tools/dtb.mjs` writes the flattened tree directly from the
board's own numbers, and it is verified by regenerating the reference's tree
byte for byte. That comparison has to say what it is comparing now that this
board's tree carries a node the reference's has no reason to: it compares the
tree generated **for the guest the reference built its tree for** —
`referenceGuest` in the board file, which is the bootargs and the initramfs
addresses that were in `mini.dtb` — with the `simple-framebuffer` node this
board adds taken back out, and everything else in it is the reference's byte
for byte. The bootargs and the root filesystem have both moved on since that
blob was written; every device address, size and frequency has not, and that
is what the check still holds.

`examples/raven/rv32ima/images/mini.dtb` is still built by `dtc` from
`mini.dts`; this one is not built by anything but `tools/build.mjs`, which is
what makes it a statement about the board rather than about the tree.

