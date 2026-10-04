// Generate the machine's tables and its image, and build one `.sb3` per image.
//
//     node tools/build.mjs             all three images
//     node tools/build.mjs baremetal   one of them
//
// Three generated files come out of it:
//
//   src/rv32/tables.rav   the ALU's three 64 KiB logic tables, the 256 byte to
//                         character table, and nothing that depends on the
//                         guest. Generated once and committed.
//   src/rv32/image.rav    the guest's image and the device tree that describes
//                         the machine to it. One image at a time, so it is not
//                         committed: `raven build` compiles whatever it holds.
//   dist/<name>.sb3       the project.
//
// Two of the images and the device tree come from `ref/mini-rv32ima-rs`, the
// Rust port of mini-rv32ima that this example is checked against:
//
//   baremetal.bin   442 bytes  prints a greeting from RV32 land and powers off
//   linux_image     2,945,224  Linux 6.1.14 with a Buildroot rootfs on it
//
// The third is the one the Scratch project itself carries, which exists nowhere
// else -- 4,263,001 bytes of Linux 6.1.14 with the original project's own rootfs,
// inside that `.sb3`'s `project.json`.
//
// All three are run with the device tree in the last 1728 bytes of RAM, which is
// where `examples/cli.rs` puts it and where the kernels' own decompressors leave
// room for it. The tree is the one that file carries, byte for byte, which is
// also the one the Scratch project ships.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repo = path.resolve(root, '..', '..', '..');
const reference = path.join(repo, 'ref', 'mini-rv32ima-rs');

const IMAGES = [
    { name: 'linux', from: 'linux_image', manifest: 'raven.toml', description: 'Linux 6.1.14 with a Buildroot rootfs' },
    { name: 'baremetal', from: 'baremetal.bin', manifest: 'raven-baremetal.toml', description: 'the bare metal greeting' },
    { name: 'scratch', from: null, manifest: 'raven-scratch.toml', description: "the original project's own Linux image" },
    // The one image whose bytes and device tree this repository owns rather than
    // borrows: the reference's kernel with a second initramfs on the end of it,
    // which the tree points the kernel at. `tools/mini-image.sh` builds both.
    { name: 'mini', file: 'images/mini_image', dtb: 'images/mini.dtb', manifest: 'raven-mini.toml',
        description: 'a small Linux: coremark, duktape, screenfetch and ed on the reference kernel' }
];

// ---------------------------------------------------------------------------
// Reading the reference
// ---------------------------------------------------------------------------

/// The device tree `examples/cli.rs` carries, as bytes.
///
/// The reference patches the memory size into it when the RAM is not the 64 MiB
/// its own array describes, and the value it looks for is not the value that is
/// there, so the patch never fires and the array is what runs. This takes the
/// array as it stands, which is the same 1536 bytes the Scratch project ships.
function deviceTree() {
    const source = fs.readFileSync(path.join(reference, 'examples', 'cli.rs'), 'utf8');
    const start = source.indexOf('static DEFAULT64MBDTB');
    if (start < 0) throw new Error('examples/cli.rs has no DEFAULT64MBDTB');
    const open = source.indexOf('[', start);
    const close = source.indexOf('];', open);
    const bytes = [...source.slice(open, close).matchAll(/0x([0-9a-fA-F]{2})/g)].map((m) => parseInt(m[1], 16));
    if (bytes.length !== 1536) throw new Error(`the device tree is ${bytes.length} bytes, not 1536`);
    const be32 = (o) => ((bytes[o] << 24) | (bytes[o + 1] << 16) | (bytes[o + 2] << 8) | bytes[o + 3]) >>> 0;
    if (be32(0) !== 0xd00dfeed) throw new Error('the device tree has no FDT magic');
    return bytes;
}

// ---------------------------------------------------------------------------
// Writing raven
// ---------------------------------------------------------------------------

const escape = (value) => (typeof value === 'string'
    ? `"${[...value].map((c) => {
        const code = c.codePointAt(0);
        return code === 34 || code === 92 ? '\\' + c
            : code >= 32 && code < 127 ? c
                : `\\u{${code.toString(16).toUpperCase().padStart(4, '0')}}`;
    }).join('')}"`
    : String(value));

/// One `pub var` per table. A list initializer is a literal the compiler bakes
/// into the project's own list, so nothing is pushed at run time.
function table(name, type, items, perLine) {
    const lines = [];
    for (let i = 0; i < items.length; i += perLine) {
        lines.push(items.slice(i, i + perLine).map(escape).join(', '));
    }
    return `pub var ${name}: list<${type}> = [\n${lines.join(',\n')}\n];`;
}

function write(relative, text) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, text);
    return { file, size: text.length };
}

// ---------------------------------------------------------------------------
// The two files
// ---------------------------------------------------------------------------

/// Everything about the machine that does not depend on the guest.
function tables() {
    // The guest hands the console single bytes, so the terminal needs byte to
    // text and text to byte in both directions: the forward table is the list,
    // the backward one is `index_of`. Item 1 is code 0.
    const byteChars = [];
    for (let i = 0; i < 256; i++) byteChars.push(String.fromCharCode(i));

    // Scratch has no bitwise operator and raven does not add one, so a 32 bit
    // AND is four lookups into a table indexed by the two bytes of a 16 bit
    // pair. mini-rv32ima builds these at startup; they are the same 65536
    // values, written as a literal so that nothing has to build them.
    const or = [], and = [], xor = [];
    for (let i = 0; i < 256; i++) {
        for (let j = 0; j < 256; j++) {
            or.push(i | j);
            and.push(i & j);
            xor.push(i ^ j);
        }
    }

    return `// The machine's read-only tables, generated by tools/build.mjs. Do not edit.
//
// The three logic tables are the ALU: Scratch has no bitwise operator, so a 32
// bit AND, OR or XOR is four lookups into a table indexed by the two bytes of a
// 16 bit pair, and the tables cost nothing to start because a list initializer
// is a literal.
//
// The terminal draws through penfont, which has no idea what a byte is, so
// \`byte_chars\` is the other half of the console: item i is the character the
// guest's byte i-1 means, and \`index_of\` is the way back.

${table('byte_chars', 'str', byteChars, 16)}

${table('or_lut', 'num', or, 64)}

${table('and_lut', 'num', and, 64)}

${table('xor_lut', 'num', xor, 64)}
`;
}

function image(name, bytes, dtb, source) {
    return `// The guest's image and its device tree, generated by tools/build.mjs.
// Do not edit: \`node tools/build.mjs\` writes this file for one image at a
// time, and the image it holds is the image the project boots.
//
// From ${source} (${bytes.length} bytes) and the tree in the reference's
// \`examples/cli.rs\` (${dtb.length} bytes). The tree is placed in the last 1728
// bytes of RAM, which is where the reference puts it and where the image's own
// stub leaves room for it: the bytes at 0x8000_0000 unpack the kernel to
// 0x8100_0000, so anything between the image and there is written over.

${table('rom', 'num', bytes, 48)}

${table('dtb', 'num', dtb, 48)}
`;
}

/// The image the Scratch project in `ref/` carries, which is nowhere else: it is
/// a list inside that project's own `project.json`.
///
/// `project.json` is 162 MB and most of it is the project's 64 MiB of *RAM*,
/// written out one byte to an item. Reading the whole thing as JSON costs a
/// four gigabyte heap and buys nothing, so the image is cut out of the text: the
/// list's name is found, the array after it is walked to its closing bracket,
/// and the items are read. The device tree next to it is not needed -- it is
/// byte for byte the one `examples/cli.rs` carries, which is what `deviceTree`
/// returns.
function scratchImage() {
    const sb3 = path.join(repo, 'ref', 'Linux 6.1.14-rv32ima On Scratch.sb3');
    if (!fs.existsSync(sb3)) throw new Error(`no ${sb3}`);
    const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'rv32-'));
    try {
        // The system `tar` reads the ZIP container, which keeps this tool free
        // of a dependency and of a ZIP reader of its own.
        execFileSync('tar', ['-xf', sb3, '-C', temporary]);
        const text = fs.readFileSync(path.join(temporary, 'project.json'), 'utf8');
        const key = '"RISCV.ROM"';
        const at = text.indexOf(key);
        if (at < 0) throw new Error('the project has no RISCV.ROM');
        let depth = 0, end = text.indexOf('[', at + key.length);
        if (end < 0) throw new Error('RISCV.ROM is not a list');
        const from = end;
        for (; end < text.length; end++) {
            const c = text[end];
            if (c === '[') depth++;
            else if (c === ']' && --depth === 0) break;
        }
        const items = text.slice(from + 1, end).split(',');
        const bytes = new Array(items.length);
        for (let i = 0; i < items.length; i++) bytes[i] = Number(items[i].trim().slice(1, -1));
        if (bytes.length < 1024) throw new Error(`RISCV.ROM is only ${bytes.length} items`);
        return bytes;
    } finally {
        fs.rmSync(temporary, { recursive: true, force: true });
    }
}

// ---------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------

const wanted = process.argv.slice(2);
const chosen = wanted.length > 0 ? IMAGES.filter((i) => wanted.includes(i.name)) : IMAGES;
if (chosen.length === 0) {
    console.error(`usage: node tools/build.mjs [${IMAGES.map((i) => i.name).join('|')}]`);
    process.exit(2);
}

const dtb = deviceTree();
const generated = write('src/rv32/tables.rav', tables());
console.log(`${path.relative(process.cwd(), generated.file)}  ${(generated.size / 1048576).toFixed(1)} MB`);

for (const image_ of chosen) {
    const manifest = path.join(root, image_.manifest);
    const named = /name\s*=\s*"([^"]+)"/.exec(fs.readFileSync(manifest, 'utf8'));
    // Three of the images are the reference's, read out of its checkout; the
    // fourth is this repository's own, in `images/`.
    const bytes = image_.file
        ? [...fs.readFileSync(path.join(root, image_.file))]
        : image_.from
            ? [...fs.readFileSync(path.join(reference, image_.from))]
            : scratchImage();
    const tree = image_.dtb ? [...fs.readFileSync(path.join(root, image_.dtb))] : dtb;
    const source = image_.file
        ? `this repository's ${image_.file}`
        : image_.from ? `the reference's ${image_.from}` : "the Scratch project's RISCV.ROM";
    const written = write('src/rv32/image.rav', image(image_.name, bytes, tree, source));
    console.log(`${path.relative(process.cwd(), written.file)}  ${image_.name}: ${bytes.length} bytes, ` +
        `${(written.size / 1048576).toFixed(1)} MB of source`);
    execFileSync('cargo', ['run', '--release', '-q', '-p', 'raven', '--', 'build', '-m', manifest],
        { cwd: repo, stdio: 'inherit' });
    console.log(`  dist/${named[1]}.sb3  ${image_.description}`);
}
