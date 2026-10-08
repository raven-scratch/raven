// A flattened device tree, written from the board file.
//
// The guest is handed a DTB, and the tree it is handed has to agree with the
// machine it is being handed by: the kernel probes the board through the tree,
// so an address that disagrees with where a device answers is a device the
// kernel never finds. `boards/mini-rv32.mjs` is the machine, so the tree is
// generated from it rather than written twice, exactly as the ARM board's tree
// is generated from `boards/versatile-pb.mjs` by `tools/wsl/20-guest.sh`.
//
// The ARM path shells out to `dtc`; this one does not, because the build has
// to run on the host that builds the project. The emitter is the FDT wire
// format directly -- there is no libfdt here and no need for one -- and it is
// checked against the tree the reference ships: `tools/dtb.mjs` regenerates
// `examples/raven/rv32ima/images/mini.dtb` byte for byte from the same numbers,
// which is what makes "the two agree" a measurement rather than a hope.
//
// A tree is a JS object. Keys that start with `#` or contain `,` or `-` are
// properties like any other; `children` is an array of `{ name, props,
// children }`, and a property value is one of:
//
//     { str: 'text' }              a string, NUL terminated
//     { strs: ['a', 'b'] }         NUL separated strings, one trailing NUL
//     { u32: [1, 2] }              big-endian 32 bit cells
//     { bytes: [..] }              raw
//
// Everything is big-endian on the wire, which is the one place in this project
// that is true.

const FDT_MAGIC = 0xd00dfeed;
const FDT_BEGIN_NODE = 1;
const FDT_END_NODE = 2;
const FDT_PROP = 3;
const FDT_NOP = 4;
const FDT_END = 9;

/// The value of a property, as bytes.
function valueBytes(value) {
    if (value === undefined || value === null) return Buffer.alloc(0);
    if (value.str !== undefined) {
        // A `\0` inside the string is how the tree spells a list of strings
        // that dtc prints as one quoted value; it stays a NUL on the wire.
        return Buffer.from(value.str + '\0', 'latin1');
    }
    if (value.strs !== undefined) {
        return Buffer.from(value.strs.join('\0') + '\0', 'latin1');
    }
    if (value.u32 !== undefined) {
        const out = Buffer.alloc(value.u32.length * 4);
        value.u32.forEach((cell, i) => out.writeUInt32BE(cell >>> 0, i * 4));
        return out;
    }
    if (value.bytes !== undefined) return Buffer.from(value.bytes);
    throw new Error(`a property value must be {str}, {strs}, {u32} or {bytes}`);
}

/// The structure block, and the strings block it refers to.
function structure(tree) {
    const chunks = [];
    const strings = [];
    const stringOffsets = new Map();

    const nameOffset = (name) => {
        if (!stringOffsets.has(name)) {
            stringOffsets.set(name, strings.reduce((n, s) => n + s.length + 1, 0));
            strings.push(name);
        }
        return stringOffsets.get(name);
    };

    const node = (entry) => {
        const head = Buffer.from(entry.name + '\0', 'latin1');
        const begin = Buffer.alloc(4 + head.length + ((4 - head.length % 4) % 4));
        begin.writeUInt32BE(FDT_BEGIN_NODE, 0);
        head.copy(begin, 4);
        chunks.push(begin);

        for (const [key, value] of Object.entries(entry.props ?? {})) {
            const bytes = valueBytes(value);
            const prop = Buffer.alloc(12 + bytes.length + ((4 - bytes.length % 4) % 4));
            prop.writeUInt32BE(FDT_PROP, 0);
            prop.writeUInt32BE(bytes.length, 4);
            prop.writeUInt32BE(nameOffset(key), 8);
            bytes.copy(prop, 12);
            chunks.push(prop);
        }
        for (const child of entry.children ?? []) node(child);

        const end = Buffer.alloc(4);
        end.writeUInt32BE(FDT_END_NODE, 0);
        chunks.push(end);
    };

    node(tree);

    const end = Buffer.alloc(4);
    end.writeUInt32BE(FDT_END, 0);
    chunks.push(end);

    return {
        struct: Buffer.concat(chunks),
        strings: Buffer.from(strings.map((s) => s + '\0').join(''), 'latin1')
    };
}

/// The whole flattened tree, as the bytes the guest is handed.
export function flatten(tree) {
    const { struct, strings } = structure(tree);
    const header = Buffer.alloc(40);
    const reserve = Buffer.alloc(16);            // one empty entry, then a terminator
    const offReserve = header.length;
    const offStruct = offReserve + reserve.length;
    const offStrings = offStruct + struct.length;
    // `dtc` writes the blob at its exact length rather than padding it, and
    // the tree the kernel is handed is compared byte for byte against the
    // reference's, so this does the same.
    const total = offStrings + strings.length;

    header.writeUInt32BE(FDT_MAGIC, 0);
    header.writeUInt32BE(total, 4);
    header.writeUInt32BE(offStruct, 8);
    header.writeUInt32BE(offStrings, 12);
    header.writeUInt32BE(offReserve, 16);
    header.writeUInt32BE(17, 20);                // version
    header.writeUInt32BE(16, 24);                // last compatible version
    header.writeUInt32BE(0, 28);                 // boot cpuid
    header.writeUInt32BE(strings.length, 32);
    header.writeUInt32BE(struct.length, 36);

    const blob = Buffer.alloc(total);
    header.copy(blob, 0);
    reserve.copy(blob, offReserve);
    struct.copy(blob, offStruct);
    strings.copy(blob, offStrings);
    return blob;
}

// ---------------------------------------------------------------------------
// The source form
// ---------------------------------------------------------------------------

const hex = (n) => `0x${(n >>> 0).toString(16).padStart(8, '0')}`;

function renderValue(value) {
    if (value.str !== undefined) return `"${value.str}"`;
    if (value.strs !== undefined) return `"${value.strs.join('\\0')}"`;
    if (value.u32 !== undefined) return `<${value.u32.map(hex).join(' ')}>`;
    return `<${[...value.bytes].map(hex).join(' ')}>`;
}

/// The tree as a `.dts`, so a reader can see what the guest is handed without
/// a decompiler. `node tools/build.mjs --board ...` writes it beside the DTB.
export function renderDts(tree, tabs = 0) {
    const pad = '\t'.repeat(tabs);
    const lines = [];
    if (tabs === 0) lines.push('/dts-v1/', '');
    lines.push(`${pad}${tabs === 0 ? '/' : tree.name} {`);
    for (const [key, value] of Object.entries(tree.props ?? {})) {
        lines.push(`${pad}\t${key} = ${renderValue(value)};`);
    }
    for (const child of tree.children ?? []) {
        lines.push('');
        lines.push(renderDts(child, tabs + 1));
    }
    lines.push(`${pad}};`);
    return lines.join('\n');
}

// ---------------------------------------------------------------------------
// The board's tree
// ---------------------------------------------------------------------------

/// The tree for a RISC-V board, built out of the board file's own numbers.
///
/// Every address, size and frequency below is read from the board rather than
/// written down again: the uart's `reg` is the uart device's `base` and `size`,
/// the clint's is the clint's, the memory node's size is the RAM the board
/// declares less the top the device tree itself occupies, and the timebase is
/// the processor's clock. A board that moves a device moves the tree with it.
///
/// `display` is the card's panel for this build, or nothing for a board whose
/// card has no direct colour mode. When there is one, a `simple-framebuffer`
/// node goes under `/chosen`, which is where the kernel's `simplefb` looks for
/// it -- `drivers/of/platform.c` creates the platform device for a
/// `simple-framebuffer` that is a child of `/chosen` and for nowhere else --
/// and it carries the card's own numbers, so the driver the kernel binds and
/// the memory the monitor scans out cannot disagree.
export function boardTree(board, guest, display = null) {
    const { memory, devices, clocks } = board;
    const device = (name) => devices.find((d) => d.name === name);
    const uart = device('uart0');
    const clint = device('clint');
    const syscon = device('syscon');
    const card = device('efb');
    const ram = memory.ram;
    const uartClock = board.uartClockFrequency ?? 0x1000000;
    const timebase = board.timebaseFrequency ?? clocks.cpu;

    // The card's framebuffer, as the kernel's `simplefb` is told about it. The
    // pixels are where the card's own PIXELS_OFF puts them and the stride is
    // the card's pitch, so the bytes this driver writes are the bytes the
    // monitor reads.
    const framebuffer = display && display.dt ? (() => {
        const base = card.base + display.pixelsOff;
        return {
            name: `framebuffer@${(base >>> 0).toString(16)}`,
            props: {
                compatible: { str: 'simple-framebuffer' },
                reg: { u32: [0, base, 0, display.pitch * display.height] },
                width: { u32: [display.width] },
                height: { u32: [display.height] },
                stride: { u32: [display.pitch] },
                format: { str: display.dt }
            },
            children: []
        };
    })() : null;

    // A node under `/chosen` is on a bus, and a bus has to say how wide its
    // addresses and sizes are and that it maps them through unchanged. Without
    // these three properties `/chosen` is not a translatable bus, so
    // `of_address_to_resource` refuses the framebuffer's `reg`, the platform
    // device the kernel creates for it has no memory resource at all, and
    // `simplefb` fails to probe with `No memory resource`. The kernel's own
    // device trees that put a `simple-framebuffer` under `/chosen` carry
    // exactly these three, and this is why.
    //
    // They are here only when there is a framebuffer node, so that a tree
    // without one is still the reference's byte for byte.
    const chosen = {
        ...(framebuffer ? {
            '#address-cells': { u32: [2] },
            '#size-cells': { u32: [2] },
            ranges: { bytes: [] }
        } : {}),
        // Where the initramfs is, which is the board's guest image layout and
        // not a number the tree is allowed to invent: the kernel is given an
        // address, and the boot fails if nothing is there.
        ...(guest && guest.initrdStart !== undefined ? {
            'linux,initrd-start': { u32: [guest.initrdStart] },
            'linux,initrd-end': { u32: [guest.initrdEnd] }
        } : {}),
        bootargs: { str: guest && guest.bootargs ? guest.bootargs : '' }
    };

    return {
        name: '',
        props: {
            '#address-cells': { u32: [2] },
            '#size-cells': { u32: [2] },
            // The tree's identity is its own field and not the board's
            // `compatible`: the board is named `riscv-mini-rv32` in its own file
            // and its *tree* is the reference's byte for byte, which says
            // `riscv-minimal-nommu`. A board that is a different machine says so
            // here.
            compatible: { str: board.dtCompatible ?? 'riscv-minimal-nommu' },
            model: { str: board.dtModel ?? 'riscv-minimal-nommu,qemu' }
        },
        children: [
            {
                name: 'chosen',
                props: chosen,
                children: framebuffer ? [framebuffer] : []
            },
            {
                name: `memory@${(ram.base >>> 0).toString(16)}`,
                props: {
                    device_type: { str: 'memory' },
                    reg: { u32: [0, ram.base, 0, ram.size - memory.reservedTop] }
                },
                children: []
            },
            {
                name: 'cpus',
                props: {
                    '#address-cells': { u32: [1] },
                    '#size-cells': { u32: [0] },
                    'timebase-frequency': { u32: [timebase] }
                },
                children: [{
                    name: 'cpu@0',
                    props: {
                        phandle: { u32: [1] },
                        device_type: { str: 'cpu' },
                        reg: { u32: [0] },
                        status: { str: 'okay' },
                        compatible: { str: 'riscv' },
                        // What the processor *is*, which a board states rather
                        // than this file assuming: the board's tree is the
                        // reference's byte for byte, so these defaults are its
                        // own values.
                        'riscv,isa': { str: board.isa ?? 'rv32ima' },
                        'mmu-type': { str: board.mmu ?? 'riscv,none' }
                    },
                    children: [{
                        name: 'interrupt-controller',
                        props: {
                            '#interrupt-cells': { u32: [1] },
                            'interrupt-controller': { bytes: [] },
                            compatible: { str: 'riscv,cpu-intc' },
                            phandle: { u32: [2] }
                        },
                        children: []
                    }]
                }, {
                    name: 'cpu-map',
                    props: {},
                    children: [{
                        name: 'cluster0',
                        props: {},
                        children: [{ name: 'core0', props: { cpu: { u32: [1] } }, children: [] }]
                    }]
                }]
            },
            {
                name: 'soc',
                props: {
                    '#address-cells': { u32: [2] },
                    '#size-cells': { u32: [2] },
                    compatible: { str: 'simple-bus' },
                    ranges: { bytes: [] }
                },
                children: [
                    {
                        name: `uart@${(uart.base >>> 0).toString(16)}`,
                        props: {
                            'clock-frequency': { u32: [uartClock] },
                            reg: { u32: [0, uart.base, 0, 0x100] },
                            compatible: { str: 'ns16850' }
                        },
                        children: []
                    },
                    {
                        name: 'poweroff',
                        props: {
                            value: { u32: [board.poweroffValue ?? 0x5555] },
                            offset: { u32: [0] },
                            regmap: { u32: [4] },
                            compatible: { str: 'syscon-poweroff' }
                        },
                        children: []
                    },
                    {
                        name: 'reboot',
                        props: {
                            value: { u32: [board.rebootValue ?? 0x7777] },
                            offset: { u32: [0] },
                            regmap: { u32: [4] },
                            compatible: { str: 'syscon-reboot' }
                        },
                        children: []
                    },
                    {
                        name: `syscon@${(syscon.base >>> 0).toString(16)}`,
                        props: {
                            phandle: { u32: [4] },
                            reg: { u32: [0, syscon.base, 0, syscon.size] },
                            compatible: { str: 'syscon' }
                        },
                        children: []
                    },
                    {
                        name: `clint@${(clint.base >>> 0).toString(16)}`,
                        props: {
                            'interrupts-extended': { u32: [2, 3, 2, 7] },
                            reg: { u32: [0, clint.base, 0, clint.size] },
                            compatible: { strs: ['sifive,clint0', 'riscv,clint0'] }
                        },
                        children: []
                    }
                ]
            }
        ]
    };
}
