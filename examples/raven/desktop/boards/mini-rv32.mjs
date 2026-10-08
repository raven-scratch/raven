// The machine, as data: a 32 bit RISC-V hart with no MMU, on mini-rv32ima's
// memory map.
//
// This is the second board the desktop example is built with, and it is the
// same kind of file `versatile-pb.mjs` is: it says which parts the motherboard
// is built with, where each one answers on the bus, and what clock each one is
// driven at. Nothing in `src/rvcpu/`, `src/rvdev/` or `src/rvmonitor/` knows
// what else is plugged in -- they read the tables `tools/build.mjs` generates
// from this file, and the tables are the whole of the board's knowledge.
//
//     node tools/build.mjs --board boards/mini-rv32.mjs
//
// The machine is *not* an ARM board with a different processor in the socket.
// A processor brings its own memory map with it, so the board is a different
// file with a different list of parts: where the Versatile PB has SDRAM at
// zero, a UART at `0x101f1000` and an LCD controller at `0x10120000`, this
// board has RAM at `0x8000_0000`, an 8250 at `0x1000_0000` and a CLINT at
// `0x1100_0000`, because those are the addresses the guest images target and a
// board that disagrees with its own guest does not boot.
//
// The devices are mini-rv32ima's, which is the machine its guest images were
// written for:
//
//   uart0   0x1000_0000   the console, and the only way the guest talks
//   efb     0x1040_0000   the graphics card, a 320x200 INDEX8 framebuffer
//   clint   0x1100_0000   mtime and mtimecmp, and the timer interrupt
//   syscon  0x1110_0000   two magic writes: 0x5555 powers off, 0x7777 reboots
//
// `0x1000_0000` to `0x1200_0000` is one MMIO window -- 32 MiB of it -- and a
// store outside it is a fault, which is exactly what mini-rv32ima does. The
// graphics card is deliberately *inside* that window: `0x1040_0000` is a
// little over four megabytes into it, so a guest that writes the card writes
// ordinary MMIO and nothing about the card needed the window widened.

/** Device ids. A device's id is its index in `devices` plus one, and zero --
 *  the value an unmapped page reads back as -- means "nothing here". */
export const arch = 'riscv';

export const PERIPH_BASE = 0x1000_0000;
export const PAGE_SIZE = 0x1000;
/** The whole MMIO window. mini-rv32ima answers every address in
 *  `0x1000_0000..0x1200_0000` and faults outside it, so the decoder is built
 *  over the same 32 MiB: a page the board file did not put a device on reads
 *  back as the zero an unassigned address reads, which is what the reference
 *  does for the window's holes. */
export const PERIPH_SIZE = 0x0200_0000;

export const devices = [
    // ---- the console --------------------------------------------------
    // The 8250/16550 the kernel's `console=ttyS0` names. Its data register is
    // a byte wide; the line status register at +5 says whether a byte is
    // waiting, and reading the data register takes that byte away.
    { name: 'uart0', base: 0x1000_0000, size: 0x1000, irq: null, clock: 'pclk', kind: 'uart8250' },

    // ---- the graphics card --------------------------------------------
    // 512 KiB of window: registers, a 256 entry palette at +0x1000 and
    // 320*200 bytes of pixels at +0x10000. The guest only ever learns what a
    // frame *is* by reading the registers back, so they are the contract.
    { name: 'efb', base: 0x1040_0000, size: 0x0008_0000, irq: null, clock: 'pclk', kind: 'efb' },

    // ---- the timer ----------------------------------------------------
    // `mtime` at +0xbff8, `mtimecmp` at +0x4000. The addresses are the real
    // CLINT's and the reference's, odd as they look, and the kernel reads
    // `mtime` to keep time.
    { name: 'clint', base: 0x1100_0000, size: 0x0001_0000, irq: null, clock: 'cpu', kind: 'clint' },

    // ---- the system controller ----------------------------------------
    { name: 'syscon', base: 0x1110_0000, size: 0x1000, irq: null, clock: 'pclk', kind: 'syscon' },
];

/** The parts of the machine that are not on the peripheral window.
 *
 *  RAM is 64 MiB at `0x8000_0000`, which is where a RISC-V Linux is linked to
 *  expect it. It is *sparse*: one Scratch list item to the byte, and a byte
 *  the guest never wrote is not in the list at all, so the machine costs what
 *  the guest has touched rather than what the map says it could touch. The
 *  ARM board's SDRAM is the other way round -- one item per word, the whole
 *  sixteen megabytes present from the moment the project loads -- because an
 *  ARM926 comes out of reset executing at address zero and something has to
 *  answer there. This hart comes out of reset at `0x8000_0000` with an image
 *  already in its memory, so the image itself is what brings the list into
 *  being.
 */
export const memory = {
    ram: { base: 0x8000_0000, size: 64 * 1024 * 1024 },
    /** The device tree is handed to the kernel in the last 4096 bytes of RAM.
     *  Not chosen: the image's own stub unpacks the kernel over everything up
     *  to `0x8100_0000`, so a tree anywhere below that is a tree full of code
     *  by the time the kernel looks at it, and the top of RAM is where the
     *  reference puts it.
     *
     *  The reference's own bound is 1728 bytes, which is what its tree
     *  measures. This board's tree is 1754, because it carries a
     *  `simple-framebuffer` node the reference's has no reason to, so the
     *  window is a page rather than the reference's number -- the tree has to
     *  fit or the kernel is handed a blob whose last bytes fell off the end of
     *  RAM and reads it as a tree that stops mid-property, which is a boot
     *  with no console at all and nothing said about why. `tools/build.mjs`
     *  refuses a tree that does not fit. */
    dtbBytes: 4096,
    /** The top of RAM the guest's own memory node excludes. The reference's
     *  tree says `reg = <0x00 0x80000000 0x00 0x3ffc000>`, which is this
     *  board's 64 MiB less sixteen kilobytes: the window the loader and the
     *  device tree live in, rounded up to a page. The kernel is told about the
     *  memory it may use and the tree sits outside it. */
    reservedTop: 0x4000,
};

/** What the tree tells the kernel about the console's clock and the timer's
 *  rate. Both are the reference's numbers and both are also the board's: the
 *  UART node's `clock-frequency` is the crystal its divisor is counted from,
 *  and the timebase is how many `mtime` counts a second holds. `mtime` is a
 *  count of real time -- the CLINT module is what counts it -- so this is the
 *  number the kernel divides the host's wall clock by, and it is not the
 *  processor's instruction rate.
 */
export const uartClockFrequency = 0x1000000;
export const timebaseFrequency = 1_000_000;

/** How much of a Scratch frame a guest may have before the monitor gets the
 *  rest of it.
 *
 *  A slice stops at `instructions` retired *or* `microseconds` of wall clock,
 *  whichever comes first, and the time bound is the one that matters: the
 *  machine's thread is `warp`, so a slice cannot be cut short once it starts,
 *  and a slice longer than a Scratch frame is a display that cannot redraw
 *  thirty times a second however cheap the redraw is.
 *
 *  It is a *guest's* number rather than a machine's because what a guest costs
 *  the frame is what its own panel costs the pen. The console below is a
 *  480x360 direct colour panel whose pass is a few thousand strokes; the bare
 *  metal game's is a 320x200 indexed panel that draws every wall, every sprite
 *  and its own status bar as runs, and it is tens of thousands. This is the
 *  board's default -- the console's -- and a guest that costs more overrides it
 *  in its own entry, which is board-as-data the same way the panel and the
 *  keyboard are. */
export const slice = { instructions: 262144, microseconds: 20000 };

/** The two magic writes the system controller answers. */
export const poweroffValue = 0x5555;
export const rebootValue = 0x7777;

/** Clocks, in Hz. The board has one oscillator and divides it. */
export const clocks = {
    /** The processor's clock, and the one number here that is not a hardware
     *  divider: it is the CLINT's timebase, the counts a second `mtime` holds,
     *  and the kernel's `timebase-frequency`. The machine makes it true by
     *  counting real microseconds -- `src/rvdev/clint.rav` -- because the
     *  instruction rate is whatever the host manages, which is four million a
     *  second rather than one. A machine that retired one instruction a
     *  microsecond would not need this distinction; this one does. */
    cpu: 1_000_000,
    pclk: 24_000_000,
};

/** The two pictures the card can hold, by name.
 *
 *  `code` is the number the card's own FORMAT register answers with -- the
 *  bare metal guest's video driver reads it and knows INDEX8, so that one may
 *  not move. `bpp` is the bytes one pixel takes, and `dt` is what a device
 *  tree calls the same format: `simplefb` has no indexed format at all, so
 *  only a direct colour mode has one. */
export const formats = {
    index8: { code: 2, bpp: 1, dt: null },
    rgb565: { code: 1, bpp: 2, dt: 'r5g6b5' },
};

/** The card the graphics adapter drives. It is a framebuffer rather than a
 *  signal: the guest writes palette and pixels into the window and then says
 *  "commit", and the board latches what it finds. The monitor draws the latch,
 *  never the guest's buffer -- so a frame the guest is halfway through writing
 *  can never reach the Stage.
 *
 *  This is the card's power-on panel, and it is the bare metal guest's: 320 by
 *  200 of palette indices, which is what the driver that comes with the guest
 *  image asks for. A guest that wants the other panel says so in its own entry
 *  in `guests` below, because which panel is soldered to the card is a board
 *  decision and not a runtime one. */
export const display = {
    kind: 'efb',
    id: 'EFB1',
    version: 2,
    format: 'index8',
    width: 320,
    height: 200,
    pitch: 320,
    paletteEntries: 256,
    pixelsOff: 0x10000,
    palOff: 0x1000,
};

/** Where each guest lives, and how it is loaded.
 *
 *  Two guests, and they are two different machines' worth of software on the
 *  same board. `linux` is the one this board is for: a kernel whose initramfs
 *  has busybox with coremark, duktape and ed installed and a root shell with
 *  no login, so the owner can run programs by typing at the prompt. `doom` is
 *  the bare metal embeddedDOOM that drives the graphics card directly: it is a
 *  separate image because a project boots one guest, and this one is a program
 *  where the other is an operating system.
 *
 *  `load` is where the image is placed, which for both is the bottom of RAM:
 *  a flat RISC-V Image is position dependent, and the reference's loader puts
 *  it at `0x8000_0000` and starts the hart there.
 *
 *  `input` says which end of the machine a key typed on the Stage goes to, and
 *  it is a property of the guest because the two guests read two different
 *  keyboards: `console` is the 8250 the kernel's `ttyS0` is, and `card` is the
 *  graphics card's own `KBD_STATUS`/`KBD_DATA` pair. They want different bytes
 *  for the same key -- an up arrow is `ESC [ A` to a terminal and `0x80` to the
 *  card -- so a build has to choose, and the default is the console every
 *  Linux guest reads. */
export const guests = {
    linux: {
        name: 'linux',
        image: 'images/mini-fb-image',
        dtb: true,
        load: 0x8000_0000,
        /** The card's direct colour panel, which this guest is the reason for.
         *  It is exactly Scratch's own Stage, so the monitor scans it out one
         *  pixel to one pen unit and nothing is resampled -- and it is 60 by 22
         *  characters of the kernel's 8 by 16 console font, which is a console
         *  a shell can be used on where the card's 320 by 200 indexed panel
         *  would be 40 by 12.
         *
         *  It costs more than the indexed panel and the board pays it: the
         *  guest writes two bytes a pixel instead of one, and the monitor reads
         *  two pixels out of every 32 bit word it fetches. What it buys is the
         *  only path this machine has from the guest's console to the Stage
         *  that is the guest's own: `simplefb` writes these bytes, and no font
         *  table anywhere in this project draws them. */
        display: {
            format: 'rgb565',
            width: 480,
            height: 360,
            pitch: 960,
            paletteEntries: 0,
        },
        /** The tree the reference built for this machine, which `tools/build.mjs`
         *  compares its own against. The comparison is against the guest the
         *  reference built its tree *for* -- the bootargs and the initramfs
         *  addresses below -- because this board's guest has moved on from both:
         *  its rootfs is inside the kernel rather than in a second cpio, and its
         *  bootargs name a second console. What the comparison still proves is
         *  everything else in the tree: every device address, every size and
         *  every frequency, byte for byte against the tree a working guest was
         *  handed. */
        referenceDtb: '../rv32ima/images/mini.dtb',
        referenceGuest: {
            bootargs: 'earlycon=uart8250,mmio,0x10000000,1000000 console=ttyS0',
            initrdStart: 0x80400000,
            initrdEnd: 0x804873bc,
        },
        /** Three consoles and one shell, and the order matters for exactly one
         *  of them. `console=tty0` puts the kernel's own `printk` on the
         *  framebuffer `simplefb` registers for the graphics card -- and the
         *  last one named is the one `/dev/console` is bound to, so
         *  `console=ttyS0` comes second and the guest's standard output stays
         *  on the 8250 this board has always had. The shell writes to both
         *  anyway: see `tools/wsl/rv32-console-sh`.
         *
         *  `fbcon=font:VGA8x16` is the console's font. The kernel's default is
         *  the 8 by 8 one, which makes a 480 by 360 panel 60 by 45 characters
         *  of twice the glyphs to draw; 8 by 16 is 60 by 22, which is a console
         *  a shell can be read on and half the work for the monitor's card. */
        bootargs: 'earlycon=uart8250,mmio,0x10000000,1000000 console=tty0 console=ttyS0 fbcon=font:VGA8x16',
    },
    doom: {
        name: 'doom',
        image: '../../../ref/emdoom-bare/bare/emdoom-autostart.bin',
        dtb: true,
        load: 0x8000_0000,
        bootargs: '',
        /** This guest's keyboard is the *card's*, not the console's. Its driver
         *  polls `KBD_STATUS`/`KBD_DATA` at `0x1040_0034` and translates the
         *  card's own codes into Doom's key numbers, so a byte typed here is one
         *  of `0x80..0x95` or an ASCII character -- not the console's `ESC [ A`
         *  and `0x0a`, which mean nothing to it. `tools/build.mjs` compiles the
         *  choice into `RV_INPUT_CARD`, and the console guests leave it zero.
         *
         *  The card's contract is `ref/emdoom-bare/bare/README.md`, and the
         *  translation is `fb_key_to_doom` in that directory's `i_video_fb.c`. */
        input: 'card',
        /** And a shorter slice than the console's, because this guest's panel
         *  costs the frame four or five times what the console's does.
         *
         *  The monitor repaints the whole panel every pass and a pass costs one
         *  stroke a run, so the panel is a *budget* and not a picture: this
         *  game's full screen with its status bar is about fifty thousand runs
         *  a pass where the console's is eight thousand. At the board's twenty
         *  milliseconds for the machine plus twenty-four for the pen, a step
         *  was ninety-four milliseconds and the game ran at ten frames a
         *  second; eight leaves the pen its twenty-four and the frame is
         *  thirty-three. The guest retires fewer instructions a second and the
         *  picture is not slower, which is the trade a display always makes. */
        slice: { instructions: 262144, microseconds: 8000 },
        // A second project because a project boots one guest: the same board,
        // the same devices, the same monitor, and a different image in RAM.
        // The artifact is `dist/desktop-rv32-doom.sb3`, which is
        // `desktop-<machine>-<guest>` -- the naming rule `tools/build.mjs`
        // holds every build to.
        manifest: 'raven-rv32-doom.toml',
    },
};

export const board = {
    name: 'mini-rv32',
    arch,
    compatible: ['riscv-mini-rv32'],
    PERIPH_BASE, PERIPH_SIZE, PAGE_SIZE, devices, memory, clocks, display, formats, guests, slice,
};

export default board;
