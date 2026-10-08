// Turn the board file into the address decoder, and the guest's images into
// raven source, and build the `.sb3`.
//
//     node tools/build.mjs                      the ARM Versatile-PB board
//     node tools/build.mjs --board boards/mini-rv32.mjs --guest linux
//
// Three things come out of it:
//
//   src/cpu/luts.rav    the ALU's three 64 KiB logic tables: what a bitwise
//                       operator is when the language has none.
//   src/board/decode.rav   generated from `boards/*.mjs`: the device ids, the
//                       page-to-device table, each device's base, and the two
//                       interrupt controllers' line tables. This is the whole
//                       of the motherboard's knowledge about what is plugged
//                       into it.
//   src/rom/bootrom.rav the boot ROM's words, and the guest's kernel, device
//                       tree and initramfs, all generated from `images/`.
//   dist/desktop-<machine>-<guest>.sb3   the project.
//
// The artifact's name says which machine and which guest it is, and there is
// exactly one per build this example can make -- see `artifactName` below.
//
// The images are not in the repository. `tools/wsl/10-guest.sh` builds them
// from source on Linux; this tool reads what it leaves in `images/` and refuses
// to build a project that has none, because a machine with no guest in it is
// not a machine.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { boardTree, flatten, renderDts } from './dtb.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repo = path.resolve(root, '..', '..', '..');

// ---------------------------------------------------------------------------
// raven source, written
// ---------------------------------------------------------------------------

const hex = (value) => `0x${value.toString(16).toUpperCase().padStart(8, '0')}`;

function write(relative, text) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, text);
    return { file, size: Buffer.byteLength(text) };
}

/// One `pub var` per table. A list initializer is a literal the compiler bakes
/// into the project's own list, so nothing is pushed at run time.
function table(name, items, perLine, type = 'num') {
    const lines = [];
    for (let i = 0; i < items.length; i += perLine) {
        lines.push(items.slice(i, i + perLine).join(', '));
    }
    return `pub var ${name}: list<${type}> = [\n${lines.join(',\n')}\n];`;
}

// ---------------------------------------------------------------------------
// What each build is called
// ---------------------------------------------------------------------------
//
// One artifact per machine this example can build, and its name says which
// machine and which guest: `desktop-<machine>-<guest>.sb3`. The machine's word
// comes from the board file, so a board that is renamed renames its artifact
// and nothing else has to be edited twice.
//
// The three are
//
//   desktop-arm-virt-linux.sb3   the ARM Versatile-PB, running Linux
//   desktop-rv32-linux.sb3       the RISC-V board, running Linux
//   desktop-rv32-doom.sb3        the RISC-V board, running bare metal Doom
//
// `--board boards/mini-rv32.mjs --guest <name>` picks the RISC-V guest out of
// the board's own `guests`, so a board with another guest in it makes another
// artifact of this shape without anything here being edited.
const MACHINE_DESC = {
    'versatile-pb': 'arm-virt',
    'mini-rv32': 'rv32'
};

/// `desktop-<machine>-<guest>.sb3`, which is what `tools/check.mjs`,
/// `tools/check-rv32.mjs` and the README all name.
function artifactName(board, guestName) {
    const machine = MACHINE_DESC[board.name];
    if (!machine) {
        throw new Error(`no artifact descriptor for board "${board.name}"; ` +
            `add one to MACHINE_DESC, which is what names dist/`);
    }
    return `desktop-${machine}-${guestName}.sb3`;
}

/// The name `raven build` will give the archive, out of the manifest it built
/// from. `raven` names the file after `[project] name`, so the manifest is the
/// second half of the artifact's name and the two cannot be allowed to drift:
/// a rename that reached one and not the other leaves the check looking for a
/// project nothing writes. It is one line of TOML, so it is read and compared
/// rather than trusted.
function manifestProjectName(manifest) {
    const text = fs.readFileSync(path.join(root, manifest), 'utf8');
    const at = text.indexOf('[project]');
    const match = at < 0 ? null : /^\s*name\s*=\s*"([^"]+)"/m.exec(text.slice(at));
    if (!match) throw new Error(`${manifest} has no [project] name`);
    return match[1];
}

/// The artifact's name, having checked the manifest agrees.
function checkedArtifact(board, guestName, manifest) {
    const artifact = artifactName(board, guestName);
    const project = artifact.replace(/\.sb3$/, '');
    const named = manifestProjectName(manifest);
    if (named !== project) {
        throw new Error(`${manifest} calls the project "${named}", but the board and the ` +
            `guest make the artifact "${project}"; the manifest and the build would ` +
            `disagree about dist/${artifact}`);
    }
    return artifact;
}

// ---------------------------------------------------------------------------
// The address decoder
// ---------------------------------------------------------------------------

/// Device ids are the index in the board's list plus one, so an unmapped page
/// -- which reads back as zero -- needs no special case anywhere.
export function deviceIds(board) {
    const ids = { NONE: 0 };
    board.devices.forEach((device, index) => { ids[device.name.toUpperCase()] = index + 1; });
    return ids;
}

function decoder(board) {
    const ids = deviceIds(board);
    const { PERIPH_BASE, PAGE_SIZE, memory, clocks, display, flashLayout } = board;

    const pages = Math.ceil(
        Math.max(...board.devices.map(d => d.base + d.size)) - PERIPH_BASE) / PAGE_SIZE;
    const pageDev = new Array(pages).fill(0);
    const devBase = [0];
    const devInst = [0];
    const vicLine = [-1];
    const sicLine = [-1];

    const idConsts = [];
    for (const [name, id] of Object.entries(ids)) {
        if (name !== 'NONE') idConsts.push(`pub const DEV_${name}: num = ${id};`);
    }
    idConsts.unshift('pub const DEV_NONE: num = 0;');

    for (const device of board.devices) {
        const id = ids[device.name.toUpperCase()];
        const first = (device.base - PERIPH_BASE) / PAGE_SIZE;
        for (let page = first; page < first + device.size / PAGE_SIZE; page++) {
            if (pageDev[page] !== 0) throw new Error(`${device.name} overlaps another device`);
            pageDev[page] = id;
        }
        devBase[id] = device.base;
        // Most devices are one of several of the same part -- four UARTs, two
        // timers, four GPIO blocks -- and the instance number is how a driver
        // module tells them apart. It comes from the board file, so a board
        // with a different number of them needs no code change.
        devInst[id] = device.inst ?? 0;
        // A device with two lines has two numbers here and the CPU-visible one
        // is the first; the second is what a second controller takes, and the
        // board's `controller` says which is which.
        const lines = device.irq === null || device.irq === undefined ? []
            : Array.isArray(device.irq) ? device.irq : [device.irq];
        const onVic = device.controller === undefined || device.controller === 'vic';
        vicLine[id] = onVic && lines.length > 0 ? lines[0] : -1;
        sicLine[id] = (device.controller === 'sic' && lines.length > 0) ? lines[0] : -1;
    }

    const sdram = memory.sdram;
    return `// The address decoder, generated by tools/build.mjs from the board file.
// Do not edit: \`node tools/build.mjs\` writes this, and what it says is what
// the motherboard is built with.
//
// A device's id is its index in the board's device list plus one. Zero is
// "nothing answers here", which is also what an unbacked page reads back as,
// so the bus needs no special case for a hole.

${idConsts.join('\n')}

/// The peripheral window everything in the device list lives inside. A page
/// outside it is not looked up here at all, which is what makes the decoder
/// one list read rather than a walk.
pub const PERIPH_BASE: num = ${PERIPH_BASE};
pub const PERIPH_SIZE: num = ${pageDev.length * PAGE_SIZE};
pub const PAGE_SIZE: num = ${PAGE_SIZE};
pub const PERIPH_PAGES: num = ${pageDev.length};

/// The highest device id the decoder can hand out, so that the board can
/// walk its own device list without a constant written down twice.
pub const DEV_HIGHEST: num = ${board.devices.length};

/// SDRAM, one list item to the 32-bit word.
pub const RAM_BASE: num = ${sdram.base};
pub const RAM_SIZE: num = ${sdram.size};
pub const RAM_WORDS: num = ${sdram.size / 4};

/// The boot ROM, and the window it is aliased into while the board's remap bit
/// is clear. An ARM926 comes out of reset at address zero, so something has to
/// answer there before there is any SDRAM to answer with.
pub const BOOTROM_BASE: num = ${memory.bootrom.base};
pub const BOOTROM_SIZE: num = ${memory.bootrom.size};
pub const BOOTROM_ALIAS_SIZE: num = ${memory.bootrom.alias_size};

/// The flash chip, and where the boot ROM finds each thing it loads. Offsets
/// are from the chip's base; destinations are where they are copied to. The
/// guest's device tree is built with the same destinations, so the ROM and the
/// tree cannot disagree about where the initramfs is.
pub const FLASH_WORDS: num = ${memory.flash.size / 4};
pub const FLASH_SIZE: num = ${memory.flash.size};
pub const FLASH_KERNEL_OFFSET: num = ${flashLayout.kernel.offset};
pub const FLASH_KERNEL_DEST: num = ${flashLayout.kernel.dest};
pub const FLASH_INITRD_OFFSET: num = ${flashLayout.initrd.offset};
pub const FLASH_INITRD_DEST: num = ${flashLayout.initrd.dest};
pub const FLASH_DTB_OFFSET: num = ${flashLayout.dtb.offset};
pub const FLASH_DTB_DEST: num = ${flashLayout.dtb.dest};

/// The clocks the board runs its parts at, in Hz. CLOCK_CPU is the rate that a
/// slice of instructions is counted at, so every other clock is reached from it.
pub const CLOCK_CPU: num = ${clocks.cpu};
pub const CLOCK_PCLK: num = ${clocks.pclk};
pub const CLOCK_TIMCLK: num = ${clocks.timclk};
pub const CLOCK_CLCDCLK: num = ${clocks.clcdclk};

/// What the board reports in SYS_CLCD, which is how the guest's own display
/// driver decides which panel is plugged into it.
pub const SYS_CLCD_VALUE: num = ${display.magic};

${table('page_dev', pageDev, 32)}
${table('dev_base', devBase, 8)}
${table('dev_inst', devInst, 16)}
${table('vic_line', vicLine, 16)}
${table('sic_line', sicLine, 16)}
`;
}

// ---------------------------------------------------------------------------
// The ALU's tables
// ---------------------------------------------------------------------------

/// The three logic tables. Scratch has no bitwise operator and raven does not
/// add one, so a 32-bit AND, OR or XOR is four lookups into a table indexed by
/// the two bytes of a 16-bit pair. They are real Scratch lists rather than
/// arena cells because the arena is a literal too and there is no reason to put
/// two hundred thousand items into the same list as every temporary the CPU
/// keeps.
function luts() {
    const and = [], or = [], xor = [];
    for (let i = 0; i < 256; i++) {
        for (let j = 0; j < 256; j++) {
            and.push(i & j);
            or.push(i | j);
            xor.push(i ^ j);
        }
    }
    const condTable = (name, items) =>
        '@scratch_global\npub var ' + name + ': list<num> = [\n' +
        items.join(', ') + '\n];';
    const table = (name, items) => {
        const lines = [];
        for (let i = 0; i < items.length; i += 64) lines.push(items.slice(i, i + 64).join(', '));
        return `@scratch_global\npub var ${name}: list<num> = [\n${lines.join(',\n')}\n];`;
    };
    // The condition codes, one row per combination of the four flags. The four
    // bits of the status register that matter are N, Z, C and V, so the whole
    // question "should this instruction run" is a table indexed by those four
    // bits and the instruction's own four -- two hundred and fifty-six answers
    // where the first draft had twenty comparisons.
    const cond = [];
    for (let bits = 0; bits < 16; bits++) {
        const n = (bits >> 3) & 1, z = (bits >> 2) & 1, c = (bits >> 1) & 1, v = bits & 1;
        const answers = [
            z, !z, c, !c, n, !n, v, !v,
            c && !z, !c || z, n === v, n !== v,
            !z && n === v, z || n !== v, true, false
        ];
        for (const answer of answers) cond.push(answer ? 1 : 0);
    }

    // How many registers a multiple transfer names. Sixteen bits of register
    // list, so the count is a table; the decoder's first draft walked all
    // sixteen bits twice, which for a machine whose boot ROM copies a kernel
    // with eight thousand multiple transfers is most of the load.
    const pop = [];
    for (let i = 0; i < 65536; i++) {
        let bits = i, count = 0;
        while (bits) { count += bits & 1; bits >>= 1; }
        pop.push(count);
    }

    return `// The ALU's logic tables, generated by tools/build.mjs. Do not edit.
//
// Item \`a * 256 + b + 1\` is \`a OP b\` for two bytes, so a 32-bit operation is
// four of them, one per byte of the operands. regenerating these is
// \`node tools/build.mjs\`; nothing else in the project knows they exist except
// \`cpu::alu\`, which is where they are used.

${table('alu_and_lut', and)}

${table('alu_or_lut', or)}

${table('alu_xor_lut', xor)}

${condTable('alu_cond_lut', cond)}

${table('alu_pop_lut', pop)}
`;
}


// ---------------------------------------------------------------------------
// The Stage the monitor draws onto
// ---------------------------------------------------------------------------

/// The Stage is what the project is *put* in, so it is a build argument and
/// not something the monitor measures. Scratch's own is 480 by 360; a
/// TurboWarp Stage is resized by hand to whatever --stage was given. The
/// monitor reads the LCD controller's timings and maps the panel onto this
/// box, which is why changing it is changing the Stage and nothing else.
function screen(width, height) {
    const header = [
        "// The Stage this project was built for, written by tools/build.mjs.",
        "// Do not edit: --stage WxH decides it, and without one it is the 480 by",
        "// 360 that Scratch itself has.",
        ""
    ].join("\n");
    return header + "\n" + [
        "pub const SCREEN_W: num = " + width + ";",
        "pub const SCREEN_H: num = " + height + ";",
        "pub const SCREEN_LEFT: num = " + (-width / 2) + ";",
        "pub const SCREEN_TOP: num = " + (height / 2) + ";"
    ].join("\n") + "\n";
}

// ---------------------------------------------------------------------------
// SDRAM
// ---------------------------------------------------------------------------

/// The memory, as one Scratch list literal.
///
/// `src/mem/ram.rav` says why it has to be a literal: Scratch will not let a
/// list pass 200,000 items, by adding or by inserting, so the only list that
/// can be the size of a machine's memory is one that arrives holding it. The
/// size is the board file's, so a machine with more memory is a board with a
/// bigger `sdram`.
function sdram(board) {
    const words = board.memory.sdram.size / 4;
    const perLine = 256;
    const lines = [];
    for (let i = 0; i < words; i += perLine) {
        lines.push(new Array(Math.min(perLine, words - i)).fill('0').join(', '));
    }
    return `// The machine's memory, generated by tools/build.mjs. Do not edit.
//
// ${words} words of SDRAM, one Scratch list item each, all zero. This file is
// why \`src/mem/ram.rav\` is short: the list is already the size of the machine
// when the project loads, so every access to it is a replace and nothing ever
// has to grow.

@scratch_global
pub var ram: list<num> = [
${lines.join(',\n')}
];
`;
}
// ---------------------------------------------------------------------------
// The boot ROM and the guest's images
// ---------------------------------------------------------------------------

/// A hand-assembled stopgap, used only until `images/bootrom.bin` exists.
///
/// It is nine ARM words that write "Hi\n" to UART0 and spin, and it is the
/// project's first end-to-end test: if the CPU and the bus are right, those
/// three bytes reach the console. The real boot ROM is assembled from
/// `tools/rom/bootrom.S` by the guest build and lands in `images/bootrom.bin`;
/// this exists so that the machine can be brought up before the firmware is.
const SCAFFOLD_ROM = [
    0xE59F1018,  // ldr r1, [pc, #24]      -- the UART's data register
    0xE3A00048,  // mov r0, #'H'
    0xE5810000,  // str r0, [r1]
    0xE3A00069,  // mov r0, #'i'
    0xE5810000,  // str r0, [r1]
    0xE3A0000A,  // mov r0, #'\n'
    0xE5810000,  // str r0, [r1]
    0xEAFFFFFE,  // b .                    -- before the literal, not after it
    0x101F1000,  // (the literal, which is data and must not be executed)
];

function leWords(bytes) {
    const words = [];
    for (let i = 0; i + 3 < bytes.length; i += 4) {
        words.push((bytes[i] | (bytes[i + 1] << 8) | (bytes[i + 2] << 16) |
            (bytes[i + 3] << 24)) >>> 0);
    }
    return words;
}

function flashImage() {
    const size = board.memory.flash.size;
    const image = Buffer.alloc(size);
    const placed = [];
    const placedOffsets = [];
    const placedRanges = [];
    const place = (what, file, offset) => {
        const full = path.join(root, file);
        if (!fs.existsSync(full)) return;
        const bytes = fs.readFileSync(full);
        if (offset + bytes.length > size) throw new Error(`${what} does not fit in the flash`);
        // Laid over one another, the second piece silently wins and the first
        // is a guest that boots from a corrupt image -- which is what a kernel
        // that outgrew the space left for it looks like. The sizes are read
        // here, so this is the only place that can know, and it refuses rather
        // than writing a chip whose contents depend on the order they were put
        // in.
        for (const range of placedRanges) {
            if (offset < range.end && range.offset < offset + bytes.length) {
                throw new Error(
                    `${what} at ${hex(offset)}..${hex(offset + bytes.length - 1)} overlaps ` +
                    `${range.what}, which occupies ${hex(range.offset)}..${hex(range.end - 1)}`);
            }
        }
        placedRanges.push({ what, offset, end: offset + bytes.length });
        bytes.copy(image, offset);
        placedOffsets.push(offset + bytes.length - 1);
        placed.push(`${what} ${bytes.length} bytes at ${hex(offset)}`);
    };
    place('bootrom', 'images/bootrom.bin', 0);
    place('kernel', 'images/kernel.img', board.flashLayout.kernel.offset);
    place('initrd', 'images/initramfs.cpio.gz', board.flashLayout.initrd.offset);
    place('dtb', 'images/versatile-pb.dtb', board.flashLayout.dtb.offset);
    // Only what is used is written out, rounded up to a page. The chip is
    // sixteen megabytes and holds a few; the rest reads back as the zero an
    // unbacked address reads back as, which is also what an erased word reads
    // as to a machine that never asks for it.
    let used = 0;
    for (const entry of placedOffsets) used = Math.max(used, entry);
    used = Math.min(size, Math.ceil((used + 1) / 65536) * 65536);
    // The same bytes, as a file, for the reference machine. QEMU's Versatile PB
    // has the same NOR chip at the same address, so handing it this image lets
    // it run *this* board's firmware and *this* board's layout -- which is what
    // makes it a reference for the machine rather than for the guest alone.
    fs.writeFileSync(path.join(root, 'images', 'flash.bin'), image.subarray(0, used));
    const words = leWords(image.subarray(0, used));
    return `// The flash chip, generated by tools/build.mjs. Do not edit.
//
// The whole of this machine's storage: the firmware at the bottom and whatever
// the boot ROM loads above it. A real board's NOR chip holds whatever was
// written to it; this one holds exactly the bytes the guest build produced, so
// the machine is deterministic -- the same build is the same chip.
//
// ${placed.length ? placed.join('\n// ') : 'Nothing was placed: run the guest build.'}

pub const FLASH_IMAGE_WORDS: num = ${words.length};

${tableModel('flash', words)}
`;
}

/// The flash is stored the same way the memory is -- one Scratch list item to
/// the 32-bit word -- but it is a list that is never written, so it is a real
/// Scratch list of its own rather than a share of the arena.
function tableModel(name, words) {
    const lines = [];
    for (let i = 0; i < words.length; i += 64) lines.push(words.slice(i, i + 64).join(', '));
    return `@scratch_global\npub var ${name}: list<num> = [\n${lines.join(',\n')}\n];`;
}

// ---------------------------------------------------------------------------
// The RISC-V board
// ---------------------------------------------------------------------------
//
// A board with `arch: 'riscv'` is the same idea with a processor that brings a
// different memory map with it: no flash chip and no boot ROM, because a
// RISC-V Image is loaded flat at the bottom of RAM and the hart starts there;
// sparse byte-addressed RAM instead of a word list; and an MMIO window wide
// enough to hold the whole of mini-rv32ima's. Everything below is generated
// from the board file for exactly the same reason the ARM tables are, so a
// board that moves a device or changes the size of RAM regenerates them.

function chunk(items, size) {
    const out = [];
    for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size));
    return out;
}

/// A JavaScript string as raven source. Raven's escape for a code point is
/// `\u{...}` and a control character has no shorter form, so `JSON.stringify`
/// is not usable here: it writes `\u0000`, which raven refuses.
function ravenString(value) {
    return '"' + [...value].map((c) => {
        const code = c.codePointAt(0);
        if (code === 34 || code === 92) return '\\' + c;
        if (code >= 32 && code < 127) return c;
        return `\\u{${code.toString(16).toUpperCase().padStart(4, '0')}}`;
    }).join('') + '"';
}

/// The ALU's three logic tables and the console's byte table. The first three
/// are the same sixty-four kilobyte tables the ARM board uses -- Scratch has no
/// bitwise operator and raven does not add one -- and `rv_byte_chars` is the
/// other half of a serial console: the terminal draws characters and the UART
/// carries bytes, so something has to turn one into the other.
function rvTables() {
    const byteChars = [];
    for (let i = 0; i < 256; i++) byteChars.push(ravenString(String.fromCharCode(i)));
    const and = [], or = [], xor = [];
    for (let i = 0; i < 256; i++) {
        for (let j = 0; j < 256; j++) {
            and.push(i & j);
            or.push(i | j);
            xor.push(i ^ j);
        }
    }
    const per64 = (name, items) => `@scratch_global\npub var ${name}: list<num> = [\n` +
        chunk(items, 64).map((row) => row.join(', ')).join(',\n') + '\n];';
    return `// The RISC-V machine's read-only tables, generated by tools/build.mjs from
// the board file. Do not edit: \`node tools/build.mjs --board
// boards/mini-rv32.mjs\` writes this.
//
// Item \`a * 256 + b + 1\` of a logic table is \`a OP b\` for two bytes, so a
// 32 bit operation is four lookups, one per byte of the operands. That is the
// whole of how a language with no bitwise operator does one.
//
// \`rv_byte_chars\` is the console's other end: item \`code + 1\` is the character
// the guest's byte means, and \`index_of\` is the way back. Scratch compares two
// strings without their case, which is why the terminal marks a capital with a
// backslash rather than trusting the search.

${per64('rv_and_lut', and)}

${per64('rv_or_lut', or)}

${per64('rv_xor_lut', xor)}

@scratch_global
pub var rv_byte_chars: list<str> = [
${chunk(byteChars, 16).map((row) => row.join(', ')).join(',\n')}
];
`;
}

/// The address decoder, the memory map and the card's own contract, generated
/// from the board file.
///
/// `display` is the panel this build's guest finds on the card: the board
/// file's, or the guest's own entry if it has one. It is resolved before this
/// is called because two things read it -- the decoder below and the device
/// tree -- and they have to be the same card.
///
/// `input` is the guest's `input`, which is which keyboard its keys go to:
/// `console` for the 8250, `card` for the graphics card's own registers.
function rvDecoder(board, display, input, slice) {
    const ids = deviceIds(board);
    const { PERIPH_BASE, PERIPH_SIZE, PAGE_SIZE, memory, clocks } = board;
    const fmt = board.formats[display.format];
    const indexed = display.format === 'index8' ? 1 : 0;

    const pages = PERIPH_SIZE / PAGE_SIZE;
    const pageDev = new Array(pages).fill(0);
    const devBase = [0];
    for (const device of board.devices) {
        const id = ids[device.name.toUpperCase()];
        const first = (device.base - PERIPH_BASE) / PAGE_SIZE;
        for (let page = first; page < first + device.size / PAGE_SIZE; page++) {
            if (pageDev[page] !== 0) throw new Error(`${device.name} overlaps another device`);
            pageDev[page] = id;
        }
        devBase[id] = device.base;
    }

    const idConsts = ['pub const DEV_NONE: num = 0;'];
    for (const [name, id] of Object.entries(ids)) {
        if (name !== 'NONE') idConsts.push(`pub const DEV_${name}: num = ${id};`);
    }
    const cardId = Buffer.from(display.id, 'latin1').readUInt32LE(0);

    return `// The RISC-V board's address decoder, memory map and card contract,
// generated by tools/build.mjs from the board file. Do not edit: the board
// file is the machine and this is what the machine is compiled to.
//
// A device's id is its index in the board's device list plus one. Zero is
// "nothing answers here", which is also what an unassigned page reads back as,
// so the bus needs no special case for a hole.

${idConsts.join('\n')}

/// The MMIO window. Every device the board declares is inside it, and an
/// address outside it is not MMIO at all -- which is where mini-rv32ima's
/// "a store outside the window faults" comes from.
pub const RV_PERIPH_BASE: num = ${PERIPH_BASE};
pub const RV_PERIPH_SIZE: num = ${PERIPH_SIZE};
pub const RV_PAGE_SIZE: num = ${PAGE_SIZE};
pub const RV_PERIPH_PAGES: num = ${pages};

/// How many devices the board has, so the bus can walk its own table.
pub const RV_DEV_HIGHEST: num = ${board.devices.length};

/// RAM: its base, its size, and where it ends.
pub const RV_RAM_BASE: num = ${memory.ram.base};
pub const RV_RAM_SIZE: num = ${memory.ram.size};
pub const RV_RAM_END: num = ${memory.ram.base + memory.ram.size};
/// Where the device tree sits: in the last ${memory.dtbBytes} bytes of RAM, which is
/// where the reference puts it and where the image's own stub leaves room.
pub const RV_DTB_BYTES: num = ${memory.dtbBytes};
pub const RV_DTB_ADDR: num = ${memory.ram.base + memory.ram.size - memory.dtbBytes};

/// The clocks, in Hz. Every device is advanced by its share of the processor's
/// cycles, so guest time is consistent with itself even though it is not wall
/// time: at ${clocks.cpu / 1000000} MHz a microsecond of the guest's is one instruction of it.
pub const RV_CLOCK_CPU: num = ${clocks.cpu};
pub const RV_CLOCK_PCLK: num = ${clocks.pclk};

/// The graphics card, as the guest is told it is. These are the values the
/// card's identification registers answer with, and the monitor trusts them
/// rather than the guest's buffer.
pub const RV_EFB_ID: num = ${cardId};
pub const RV_EFB_VERSION: num = ${display.version};
pub const RV_EFB_WIDTH: num = ${display.width};
pub const RV_EFB_HEIGHT: num = ${display.height};
pub const RV_EFB_FORMAT: num = ${fmt.code};
pub const RV_EFB_PITCH: num = ${display.pitch};
pub const RV_EFB_PAL_ENTRIES: num = ${display.paletteEntries};
pub const RV_EFB_PIXELS_OFF: num = ${display.pixelsOff};
pub const RV_EFB_PAL_OFF: num = ${display.palOff};

/// Which of the card's two pictures this build wired to it.
///
/// An indexed card is a palette and a byte a pixel, and it is what the bare
/// metal guest's driver asks for: it commits a frame and the board latches it,
/// so the monitor draws a picture the guest has finished writing. A direct
/// colour card has no palette at all and holds the colour of every pixel, so
/// there is nothing to latch and nothing to commit -- the monitor reads the
/// memory the guest is writing, which is what a real card's scanout is.
pub const RV_EFB_INDEXED: num = ${indexed};
pub const RV_EFB_BPP: num = ${fmt.bpp};

/// Which of the machine's two keyboards this build's guest reads, because the
/// two want different bytes for the same key: an up arrow is the three bytes
/// \`ESC [ A\` to the 8250 the kernel's \`ttyS0\` is, and the one byte 128 to the
/// graphics card's own \`KBD_STATUS\`/\`KBD_DATA\` pair. The card's own contract
/// is \`ref/emdoom-bare/bare/README.md\` and its translation is \`fb_key_to_doom\`
/// in that directory's \`i_video_fb.c\`; \`console\` is what every Linux guest on
/// this board reads, and it is the default.
pub const RV_INPUT_CARD: num = ${input === 'card' ? 1 : 0};

/// How much of a Scratch frame this guest may have before the display gets it.
///
/// A slice stops at \\\`RV_SLICE\\\` instructions or \\\`RV_SLICE_US\\\` microseconds,
/// whichever comes first, and the two numbers are the *guest's* rather than the
/// machine's because what a guest costs the frame is what its own monitor pass
/// costs. A shell's console panel is a few thousand pen strokes a pass; a
/// bare metal game's is tens of thousands, and a budget that gives both the
/// same slice gives the game a frame it cannot draw in. The board file says
/// which guest this is and this is what it said.
pub const RV_SLICE: num = ${slice.instructions};
pub const RV_SLICE_US: num = ${slice.microseconds};

/// A direct colour framebuffer is one Scratch list item to the 32 bit word, so
/// two 16 bit pixels share an item: the low half is the even column and the
/// high half the odd one. That is half the list work of a byte an item, and it
/// is what keeps the list inside the two hundred thousand items Scratch will
/// grow one to -- 480 by 360 is 172800 pixels and 86400 words.
pub const RV_EFB_WORDS_PER_ROW: num = ${display.pitch / 4};
pub const RV_EFB_WORDS: num = ${Math.ceil(display.width * display.height / 2)};

${table('rv_page_dev', pageDev, 32)}
${table('rv_dev_base', devBase, 8)}
`;
}

/// The Stage this project is built for. Every picture this board has is drawn
/// on it by the monitor, from the graphics card's own scanout, so the size is
/// a build argument here the same way it is one for the ARM board: the monitor
/// maps the card's panel onto this box, and a Stage the size of the panel is a
/// scanout at one pixel to one pen unit.
function rvScreen(width, height) {
    return `// The Stage this project was built for, written by tools/build.mjs.
// Do not edit: \`--stage WxH\` decides it, and without one it is the 480 by
// 360 that Scratch itself has -- which is exactly the Linux guest's card, so
// that build scans out at one pixel to one pen unit.
//
// \`SCREEN_LEFT\` is the left edge of the drawing area in Scratch's own
// coordinates, which is where the monitor starts a row.

pub const SCREEN_W: num = ${width};
pub const SCREEN_H: num = ${height};
pub const SCREEN_LEFT: num = ${-width / 2};
`;
}

/// The guest and the tree it is booted with, as raven lists.
function rvImage(guest, bytes, dtb) {
    return `// The guest and the device tree, generated by tools/build.mjs from the board
// file. Do not edit.
//
// ${guest.name}: ${bytes.length} bytes, loaded flat at ${hex(guest.load)}, and
// ${dtb.length} bytes of device tree, which the hart lays into the last ${dtb.length}
// bytes of RAM and hands the kernel in \`a1\`.
//
// This is only what the guest boots from. A write past the end of it grows
// nothing, because the machine's RAM is its own list and this one is read-only.

pub const RV_ROM_BYTES: num = ${bytes.length};
pub const RV_LOAD_ADDR: num = ${guest.load};
pub const RV_ENTRY: num = ${guest.load};

${table('rv_rom', bytes, 48)}

${table('rv_dtb', dtb, 48)}
`;
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

const argv = process.argv.slice(2);
const stageArg = argv.indexOf('--stage');
if (stageArg >= 0 && !/^\d+x\d+$/i.test(argv[stageArg + 1] ?? '')) {
    console.error('usage: --stage WxH, the Stage this project is built for');
    process.exit(2);
}
const [stageW, stageH] = stageArg >= 0
    ? argv[stageArg + 1].toLowerCase().split('x').map(Number)
    : [480, 360];
const boardArg = argv.indexOf('--board');
const boardPath = boardArg >= 0 ? argv[boardArg + 1] : 'boards/versatile-pb.mjs';

const board = (await import(pathToFileURL(path.join(root, boardPath)).href)).default;

// A board file says which kind of machine it is, and that is the only place
// the builds part company. With no `--board` the ARM machine is built exactly
// as it always was; the RISC-V board is the branch below, and the hart is
// `src/rvcpu/hart.rav`, chosen by the manifest the guest names.
if (board.arch === 'riscv') {
    const guestArg = argv.indexOf('--guest');
    const guestName = guestArg >= 0 ? argv[guestArg + 1] : 'linux';
    const guest = board.guests[guestName];
    if (!guest) {
        console.error(`no guest "${guestName}"; the board has ` +
            `${Object.keys(board.guests).join(', ')}`);
        process.exit(2);
    }

    const bytes = [...fs.readFileSync(path.join(root, guest.image))];
    const initrd = guest.initrdOffset === undefined ? {} : {
        initrdStart: guest.load + guest.initrdOffset,
        initrdEnd: guest.load + bytes.length
    };

    // The card's panel for this build. The board's own is the card's power-on
    // mode -- the bare metal guest's -- and a guest that wants the other one
    // says so in its own entry, because which panel is on the card is a board
    // decision and not something a guest discovers at run time.
    const display = { ...board.display, ...(guest.display ?? {}) };
    const format = board.formats[display.format];
    if (!format) {
        throw new Error(`${guestName}: the card has no "${display.format}" mode; ` +
            `it has ${Object.keys(board.formats).join(', ')}`);
    }
    if (format.bpp > 1 && display.pitch % 4 !== 0) {
        throw new Error(`${guestName}: a direct colour card wants a pitch divisible by ` +
            `four, because two pixels share a 32 bit word; ${display.pitch} is not`);
    }
    const card = board.devices.find((d) => d.kind === 'efb');
    if (display.pixelsOff + display.pitch * display.height > card.size) {
        throw new Error(`${guestName}: ${display.width}x${display.height} of ` +
            `${display.format} does not fit the card's ${hex(card.size)} window`);
    }

    // Which keyboard the guest reads. The board's default is the console, so a
    // guest that says nothing gets the 8250; a guest whose keyboard is the
    // card says `card`, and a name that is neither is a typo the build catches
    // rather than a guest that silently cannot be typed at.
    const input = guest.input ?? 'console';
    if (input !== 'console' && input !== 'card') {
        throw new Error(`${guestName}: input "${input}" is neither "console" nor "card"`);
    }

    // And how much of a Scratch frame the guest may have before the monitor
    // gets it. The board's default is the console's, because a console is what
    // this board boots by default; a guest whose panel costs more says so in
    // its own entry, and both numbers are compiled into the machine.
    const slice = guest.slice ?? board.slice;
    if (!(slice.instructions > 0) || !(slice.microseconds > 0)) {
        throw new Error(`${guestName}: slice {instructions, microseconds} must both be positive`);
    }

    const tree = boardTree(board, { ...guest, ...initrd }, { ...display, dt: format.dt });
    const dtb = flatten(tree);

    // The tree is copied into the last `memory.dtbBytes` bytes of RAM and the
    // kernel is handed its address, so a tree that does not fit is a tree the
    // kernel reads off the end of memory: the blob's `totalsize` says one thing
    // and the bytes say another, and the boot stops before it has a console to
    // complain on. It is one number and it is checkable here, so it is checked
    // here rather than at boot.
    if (dtb.length > board.memory.dtbBytes) {
        throw new Error(`${guestName}: the device tree is ${dtb.length} bytes and the ` +
            `board reserves ${board.memory.dtbBytes} for it; raise memory.dtbBytes`);
    }

    // The tree is generated from the same numbers the decoder is, and the two
    // are checked against the reference's own tree for this machine: a board
    // that has drifted from the tree a working guest was handed is a guest
    // probing for a device that answers somewhere else.
    const dts = write(`boards/${board.name}-${guestName}.dts`, renderDts(tree) + '\n');
    fs.mkdirSync(path.join(root, 'images'), { recursive: true });
    const dtbFile = path.join(root, 'images', `${board.name}-${guestName}.dtb`);
    fs.writeFileSync(dtbFile, dtb);
    console.log(`${path.relative(process.cwd(), dts.file)}  ${dtb.length} bytes of device tree`);

    // The comparison is against the guest the reference built its tree for --
    // `referenceGuest`, which is the bootargs and the initramfs addresses that
    // were in the blob -- and with no framebuffer node, because a
    // `simple-framebuffer` is this board's addition and the reference's tree
    // has none. Everything else in the tree is then the reference's byte for
    // byte: every device address, every size and every frequency. A guest
    // without a `referenceGuest` is compared as itself.
    if (guest.referenceDtb) {
        const referenceFile = path.join(root, guest.referenceDtb);
        if (fs.existsSync(referenceFile)) {
            const reference = fs.readFileSync(referenceFile);
            const asReference = flatten(boardTree(board,
                { ...guest, ...(guest.referenceGuest ?? {}), ...initrd }, null));
            if (Buffer.compare(reference, asReference) !== 0) {
                throw new Error(`the generated device tree is not the reference's: ` +
                    `${asReference.length} bytes against ${reference.length} at ${guest.referenceDtb}`);
            }
            console.log(`  checked against ${guest.referenceDtb}: identical, byte for byte, ` +
                `with the bootargs and the framebuffer node this build adds taken back out`);
        }
    }

    // And the node this build does add is the card's, so the driver the kernel
    // binds and the memory the monitor scans out are the same memory.
    if (format.dt) {
        const node = tree.children.find((c) => c.name === 'chosen')
            .children.find((c) => c.props.compatible.str === 'simple-framebuffer');
        if (!node) throw new Error(`${guestName}: the card offers ${format.dt} and the tree has no simple-framebuffer node`);
        const [base, size] = [node.props.reg.u32[1], node.props.reg.u32[3]];
        console.log(`  ${node.name}: ${node.props.width.u32[0]}x${node.props.height.u32[0]} ` +
            `${node.props.format.str} at ${hex(base)}+${hex(size)}, ` +
            `the card's own PIXELS_OFF and PITCH`);
    }

    const dec = write('src/rvboard/decode.rav', rvDecoder(board, display, input, slice));
    console.log(`${path.relative(process.cwd(), dec.file)}  ${board.devices.length} devices, ` +
        `${(dec.size / 1024).toFixed(1)} KB, keys on the ${input}, ` +
        `a slice of ${slice.instructions} instructions or ${slice.microseconds} us`);
    for (const [name, id] of Object.entries(deviceIds(board))) {
        if (name === 'NONE') continue;
        const device = board.devices[id - 1];
        console.log(`  DEV_${name.padEnd(8)} ${String(id).padStart(2)}  ` +
            `${hex(device.base)}  ${device.name}  ${device.kind}`);
    }

    const tab = write('src/rvcpu/tables.rav', rvTables());
    console.log(`${path.relative(process.cwd(), tab.file)}  ${(tab.size / 1048576).toFixed(1)} MB of source`);

    const scr = write('src/rvscreen.rav', rvScreen(stageW, stageH));
    console.log(`${path.relative(process.cwd(), scr.file)}  ${stageW}x${stageH} Stage`);

    const img = write('src/rvboard/image.rav', rvImage(guest, bytes, [...dtb]));
    console.log(`${path.relative(process.cwd(), img.file)}  ${guestName}: ${bytes.length} bytes, ` +
        `${(img.size / 1048576).toFixed(1)} MB of source`);

    const artifact = checkedArtifact(board, guestName, guest.manifest ?? 'raven-rv32.toml');
    execFileSync('cargo', ['run', '--release', '-q', '-p', 'raven', '--', 'build', '-m',
        path.join(root, guest.manifest ?? 'raven-rv32.toml')], { cwd: repo, stdio: 'inherit' });
    console.log(`  dist/${artifact}`);
    process.exit(0);
}

const ids = deviceIds(board);

const written = write('src/board/decode.rav', decoder(board));
console.log(`${path.relative(process.cwd(), written.file)}  ${board.devices.length} devices, ` +
    `${(written.size / 1024).toFixed(1)} KB`);

for (const [name, id] of Object.entries(ids)) {
    if (name !== 'NONE') console.log(`  DEV_${name.padEnd(12)} ${String(id).padStart(2)}  ` +
        `${hex(board.devices[id - 1].base)}  ${board.devices[id - 1].name}`);
}

const scr = write('src/screen.rav', screen(stageW, stageH));
console.log(`${path.relative(process.cwd(), scr.file)}  ${stageW}x${stageH} Stage`);

const luts_ = write('src/cpu/luts.rav', luts());
console.log(`${path.relative(process.cwd(), luts_.file)}  ${(luts_.size / 1048576).toFixed(1)} MB of source`);

const mem = write('src/mem/sdram.rav', sdram(board));
console.log(`${path.relative(process.cwd(), mem.file)}  ` +
    `${(mem.size / 1048576).toFixed(1)} MB of source`);

const rom = write('src/dev/flash_image.rav', flashImage());
console.log(`${path.relative(process.cwd(), rom.file)}  ${(rom.size / 1048576).toFixed(1)} MB of source`);

const artifact = checkedArtifact(board, 'linux', 'raven.toml');
execFileSync('cargo', ['run', '--release', '-q', '-p', 'raven', '--', 'build', '-m',
    path.join(root, 'raven.toml')], { cwd: repo, stdio: 'inherit' });
console.log(`  dist/${artifact}`);
