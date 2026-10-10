// Compare block counts across every example, optimiser on and off.
//
//     node tools/bench-optimize-blocks.mjs
//
// What the raven-asm optimiser is worth, measured on every example this
// repository ships rather than on one of them. It rebuilds each example twice
// with the real `raven` binary -- once with the optimiser and once with
// `--no-optimize` -- and counts the blocks in the archives, so what it reports
// is what a user gets from the two command lines.
//
// A block count is not a proof that the layer is safe; that is
// `crates/raven-re/tests/roundtrip.rs`, which holds the optimised build against
// the fingerprint of everything Scratch can observe. This is the other half: it
// says whether the layer is worth having at all.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

/// The repository root, from this file's own location rather than a literal, so
/// the tool works from a checkout anywhere.
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function projectJson(bytes) {
    let eocd = bytes.length - 22;
    while (eocd >= 0 && bytes.readUInt32LE(eocd) !== 0x06054b50) eocd--;
    const count = bytes.readUInt16LE(eocd + 10);
    let at = bytes.readUInt32LE(eocd + 16);
    for (let i = 0; i < count; i++) {
        const nameLen = bytes.readUInt16LE(at + 28);
        const extraLen = bytes.readUInt16LE(at + 30);
        const commentLen = bytes.readUInt16LE(at + 32);
        const name = bytes.toString('latin1', at + 46, at + 46 + nameLen);
        if (name === 'project.json') {
            const method = bytes.readUInt16LE(at + 10);
            const local = bytes.readUInt32LE(at + 42);
            const lName = bytes.readUInt16LE(local + 26);
            const lExtra = bytes.readUInt16LE(local + 28);
            const size = bytes.readUInt32LE(at + 20);
            const raw = bytes.subarray(local + 30 + lName + lExtra,
                local + 30 + lName + lExtra + size);
            return JSON.parse(method === 0 ? raw.toString('utf8')
                : zlib.inflateRawSync(raw).toString('utf8'));
        }
        at += 46 + nameLen + extraLen + commentLen;
    }
    throw new Error('no project.json');
}

const stats = (bytes) => {
    const j = projectJson(bytes);
    let blocks = 0, calls = 0, argUses = 0;
    for (const t of j.targets) {
        blocks += Object.keys(t.blocks).length;
        for (const [id, b] of Object.entries(t.blocks)) {
            if (b.opcode === 'procedures_call') calls++;
            if (b.opcode.startsWith('argument_reporter')) {
                const parent = b.parent && t.blocks[b.parent];
                if (!parent || parent.opcode !== 'procedures_prototype') argUses++;
            }
        }
    }
    return { blocks, calls, argUses };
};

const EXAMPLES = ['desktop', 'chess', 'sudoku', 'penfont', 'case'];
const ARTIFACT = {
    desktop: 'desktop-arm-virt-linux.sb3',
    chess: 'chess.sb3',
    sudoku: 'sudoku.sb3',
    penfont: 'penfont.sb3',
    case: 'case.sb3'
};

console.log('');
console.log(`  ${'example'.padEnd(9)} ${'plain'.padStart(8)} ${'optimised'.padStart(10)} ` +
    `${'delta'.padStart(8)}  ${'calls'.padStart(12)}  ${'param uses'.padStart(12)}`);
const totals = { plain: 0, on: 0 };
for (const name of EXAMPLES) {
    const manifest = path.join(repo, 'examples/raven', name, 'raven.toml');
    if (!fs.existsSync(manifest)) continue;
    const artifact = path.join(repo, 'examples/raven', name, 'dist', ARTIFACT[name]);
    const run = (extra) => {
        execFileSync('cargo', ['run', '-q', '-p', 'raven', '--', 'build', '-m', manifest, ...extra],
            { cwd: repo, stdio: 'ignore' });
        return stats(fs.readFileSync(artifact));
    };
    const off = run(['--no-optimize']);
    const on = run([]);
    totals.plain += off.blocks;
    totals.on += on.blocks;
    console.log(`  ${name.padEnd(9)} ${String(off.blocks).padStart(8)} ${String(on.blocks).padStart(10)} ` +
        `${String(on.blocks - off.blocks).padStart(8)}  ` +
        `${String(`${on.calls} / ${off.calls}`).padStart(12)}  ` +
        `${String(`${on.argUses} / ${off.argUses}`).padStart(12)}`);
}
console.log('');
console.log(`  total     ${String(totals.plain).padStart(8)} ${String(totals.on).padStart(10)} ` +
    `${String(totals.on - totals.plain).padStart(8)}  ` +
    `(${((totals.on - totals.plain) / totals.plain * 100).toFixed(2)}%)`);
