// The machine, as data.
//
// This file *is* the computer. It says which parts the motherboard is built
// with, where each one answers on the bus, which interrupt line answers reach
// the controller on, and what clock each one is driven at. `tools/build.mjs`
// reads it and writes `src/board/decode.rav` (the address decoder), and
// `tools/wsl/10-guest.sh` reads it for the device tree the guest is given.
//
// So "build a different machine" is "write another one of these". A board with
// no network card is this file without the `lan91c111` entry and its IRQ line.
// A board with a different display is this file with a different `display`.
// Nothing in `src/devices/` knows what else is on the bus.
//
// The addresses and the interrupt numbers are not invented: they are what the
// Linux device tree for an ARM Versatile PB says, because the guest probes the
// machine through that tree and a board that disagrees with its own tree does
// not boot.

/** The peripheral window the address decoder is built over. Every device
 *  below is inside it, which is what makes the decoder one table lookup
 *  rather than a chain of comparisons. */
export const PERIPH_BASE = 0x1000_0000;
export const PERIPH_SIZE = 0x0020_0000;
export const PAGE_SIZE = 0x1000;

/** Device ids. A device's id is its index in `devices` plus one, so zero --
 *  the value an unmapped page reads back as -- means "nothing here". */
export const devices = [
    // ---- the core module, at 0x1000_0000 --------------------------------
    // SYS_ID, SYS_SW, SYS_LED, SYS_100HZ, the flags, and SYS_CLCD, which is
    // how the display driver learns which panel is plugged in.
    { name: 'sysctl', base: 0x1000_0000, size: 0x1000, irq: null, clock: 'pclk', inst: 0 },

    // ---- the FPGA block, at 0x1000_xxx ----------------------------------
    { name: 'sic', base: 0x1000_3000, size: 0x1000, irq: null, clock: 'pclk', cascades_to: { vic: 31 } },
    { name: 'mmc0', base: 0x1000_5000, size: 0x1000, irq: [22, 23], controller: 'sic', clock: 'mclk', inst: 0 },
    { name: 'kmi0', base: 0x1000_6000, size: 0x1000, irq: [3], controller: 'sic', clock: 'pclk', inst: 0 },
    { name: 'kmi1', base: 0x1000_7000, size: 0x1000, irq: [4], controller: 'sic', clock: 'pclk', inst: 1 },
    { name: 'sysreg', base: 0x1000_8000, size: 0x1000, irq: null, clock: 'pclk', inst: 0 },
    { name: 'uart3', base: 0x1000_9000, size: 0x1000, irq: [6], controller: 'sic', clock: 'pclk', inst: 3 },
    { name: 'mmc1', base: 0x1000_b000, size: 0x1000, irq: [1, 2], controller: 'sic', clock: 'mclk', inst: 1 },
    { name: 'lan91c111', base: 0x1001_0000, size: 0x10000, irq: [25], controller: 'vic', clock: 'pclk' },

    // ---- the AMBA APB peripherals, at 0x1010_xxxx -----------------------
    { name: 'smc', base: 0x1010_0000, size: 0x1000, irq: null, clock: 'pclk' },
    { name: 'mpmc', base: 0x1011_0000, size: 0x1000, irq: null, clock: 'pclk' },
    { name: 'clcd', base: 0x1012_0000, size: 0x1000, irq: [16], controller: 'vic', clock: 'clcdclk' },
    { name: 'dma', base: 0x1013_0000, size: 0x1000, irq: [17], controller: 'vic', clock: 'pclk' },
    { name: 'vic', base: 0x1014_0000, size: 0x1000, irq: null, clock: 'pclk' },
    { name: 'sctl', base: 0x101e_0000, size: 0x1000, irq: null, clock: 'pclk' },
    { name: 'watchdog', base: 0x101e_1000, size: 0x1000, irq: [0], controller: 'vic', clock: 'pclk' },
    { name: 'timer0', base: 0x101e_2000, size: 0x1000, irq: [4], controller: 'vic', clock: 'timclk', inst: 0 },
    { name: 'timer1', base: 0x101e_3000, size: 0x1000, irq: [5], controller: 'vic', clock: 'timclk', inst: 1 },
    { name: 'gpio0', base: 0x101e_4000, size: 0x1000, irq: [6], controller: 'vic', clock: 'pclk', inst: 0 },
    { name: 'gpio1', base: 0x101e_5000, size: 0x1000, irq: [7], controller: 'vic', clock: 'pclk', inst: 1 },
    { name: 'gpio2', base: 0x101e_6000, size: 0x1000, irq: [8], controller: 'vic', clock: 'pclk', inst: 2 },
    { name: 'gpio3', base: 0x101e_7000, size: 0x1000, irq: [9], controller: 'vic', clock: 'pclk', inst: 3 },
    { name: 'rtc', base: 0x101e_8000, size: 0x1000, irq: [10], controller: 'vic', clock: 'pclk', inst: 0 },
    { name: 'sci', base: 0x101f_0000, size: 0x1000, irq: [15], controller: 'vic', clock: 'pclk' },
    { name: 'uart0', base: 0x101f_1000, size: 0x1000, irq: [12], controller: 'vic', clock: 'pclk', inst: 0 },
    { name: 'uart1', base: 0x101f_2000, size: 0x1000, irq: [13], controller: 'vic', clock: 'pclk', inst: 1 },
    { name: 'uart2', base: 0x101f_3000, size: 0x1000, irq: [14], controller: 'vic', clock: 'pclk', inst: 2 },
    { name: 'spi', base: 0x101f_4000, size: 0x1000, irq: [11], controller: 'vic', clock: 'pclk' },
];

/** The parts of the machine that are not on the peripheral window. */
export const memory = {
    // SDRAM. One Scratch list item to the 32-bit word, so this is sixteen
    // million items: 64 MiB is what a small Linux wants and four times less
    // than the 128 MiB the real board takes, because a list item is not free
    // and `mem=` in the guest's own command line can only make it smaller.
    sdram: { base: 0x0000_0000, size: 16 * 1024 * 1024 },
    // The boot ROM, in the socket the board puts its NOR flash in. It is also
    // aliased to address zero while the board's remap bit is clear, which is
    // where an ARM926 comes out of reset.
    bootrom: { base: 0x3400_0000, size: 0x0001_0000, alias: 0x0000_0000, alias_size: 0x0000_8000 },

    // The flash chip, which is the whole of this machine's storage: firmware at
    // the bottom and the guest above it. A real board would have a disk; this
    // one has a sixty-four megabyte NOR chip with nothing but a boot loader, a
    // kernel, an initramfs and a device tree in it, and the boot ROM copies
    // them into memory and jumps. The addresses are the ones the guest's own
    // device tree is built with, so the ROM and the tree cannot disagree.
    flash: { size: 16 * 1024 * 1024 },
};

/** Clocks, in Hz. The board has one 24 MHz oscillator and divides it. */
export const clocks = {
    xtal24mhz: 24_000_000,
    pclk: 24_000_000,
    timclk: 1_000_000,   // 24 MHz / 24, which is what the device tree says
    mclk: 24_000_000,
    clcdclk: 24_000_000,

    // The processor's clock, and the one number here that is a decision rather
    // than a datasheet value.
    //
    // Everything else on the board is counted from this: a slice of N
    // instructions is N cycles, and each device is advanced by that many of its
    // own cycles through the ratio of its clock to this one. The machine is
    // therefore self-consistent -- a timer programmed for ten milliseconds
    // fires after ten milliseconds of *guest* time -- but guest time is not
    // wall time, because the host runs thirty-five thousand instructions a
    // second and no ARM926 ever ran that slowly.
    //
    // The rate matters for exactly one thing: how many instructions pass
    // between two ticks of the kernel's timer. At 1 MHz the guest's clock and
    // the timer's are the same number, so a 10 ms tick is ten thousand
    // instructions; at 200 MHz it would be two million, and the kernel's
    // timeouts and delays would be measured in hundreds of thousands of
    // instructions each. 4 MHz puts a 10 ms tick every forty thousand
    // instructions, which is often enough for the kernel to keep time and rare
    // enough not to spend the machine in its own interrupt handler.
    cpu: 4_000_000,
};

/** The panel the display adapter is driving.
 *
 *  The value below is what the board's `SYS_CLCD` register reports, and it
 *  is the whole of how the guest learns what is plugged into it: the panel
 *  driver reads bits 8 to 12 and matches them against a table of panels
 *  compiled into the kernel. A value no driver knows means no display at
 *  all, so this is a contract with the guest and not decoration.
 *
 *  The kernel knows four panels for this board and this is the Sanyo
 *  TM38QV67A02A: 320 by 240 at ten megahertz, forty columns and fifteen
 *  rows of an eight by sixteen console. It is chosen over the 640 by 480
 *  Sharp because 320 by 240 is exactly two thirds of a 480 by 360 Stage --
 *  every panel pixel lands on the screen and none is dropped, and the
 *  monitor has a quarter of the pixels to walk. A board file that named
 *  the Sharp would be a board with a sharper screen and a monitor that had
 *  to throw a quarter of its columns away to fit.
 */
export const display = {
    panel: 'sanyo-tm38qv67a02a',
    magic: 0x00 << 8,   // SYS_CLCD_ID_SANYO_3_8
    width: 320,
    height: 240,
};
/** Where each thing the boot ROM loads lives in the flash chip, and where in
 *  memory it is put.
 *
 *  The kernel's destination is not free to choose. `arch/arm/kernel/head.S`
 *  works out the physical start of RAM from where it was loaded:
 *
 *      adr_l r8, _text
 *      sub   r8, r8, #TEXT_OFFSET      @ PHYS_OFFSET
 *
 *  and this kernel is linked at `0xc0008000`, so `TEXT_OFFSET` is `0x8000` and
 *  the image has to sit at `0x00008000` for the kernel to conclude that RAM
 *  starts at zero -- which is where this board's SDRAM does start, and what its
 *  device tree's memory node says. Put it anywhere else and the offset is wrong
 *  by the difference, which is not even two megabytes, so every physical
 *  address the kernel computes is wrong and it stops before it prints anything.
 *  A raw `Image` is not a `zImage`: there is no decompressor to relocate it. */
export const flashLayout = {
    rom: { offset: 0x000000 },
    kernel: { offset: 0x010000, dest: 0x00008000 },
    initrd: { offset: 0x800000, dest: 0x00800000 },
    dtb: { offset: 0xa00000, dest: 0x00c00000 },
};

export const board = {
    name: 'versatile-pb',
    compatible: ['arm,versatile-pb', 'arm,versatile-ab'],
    PERIPH_BASE, PERIPH_SIZE, PAGE_SIZE, devices, memory, clocks, display, flashLayout,
};

export default board;
