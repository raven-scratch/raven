// Trace the machine's processor, one instruction at a time.
//
//     node tools/cpu-trace.mjs <start-address> <instructions> [--mmu]
//
// The kernel image is also in the flash chip, at `FLASH_BASE + kernel.offset`,
// and the bus answers a fetch there whether or not the boot ROM has copied
// anything -- so the processor can be pointed straight at the first instruction
// of the kernel and stepped, with no two-and-a-half-minute copy in the way.
//
// This is the instrument for "the kernel does not start": it prints the program
// counter, the status register and the registers after every instruction, so
// the trace can be held against the reference machine's (`tools/ref.sh`) and
// the first instruction they disagree on is the first instruction this
// processor gets wrong.
//
// `machine_slice` is one instruction and the sequencer advances one slice per
// step, so this runs at about two instructions a second. A trace of sixty is
// half a minute; a trace of ten thousand is not what this is for.
import fs from 'node:fs';
import path from 'node:path';
import Module from 'node:module';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '..');
const repo = path.resolve(root, '..', '..', '..');
const STUBS = { '@scratch/scratch-svg-renderer': () => ({ sanitizeSvg: { sanitizeByteStream: (d) => d }, loadSvgString: () => Promise.resolve(), serializeSvgToString: () => '' }) };
const ol = Module._load;
Module._load = function (r, p, m) { if (Object.prototype.hasOwnProperty.call(STUBS, r)) return STUBS[r](); return ol.call(this, r, p, m); };
globalThis.document = { hidden: true };
const require_ = createRequire(import.meta.url);
const VM = require_(path.join(repo, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm', 'src', 'virtual-machine.js'));
const vm = new VM();
console.warn = () => {};
console.error = () => {};
const d = fs.readFileSync(path.join(root, 'dist', 'desktop-arm-virt-linux.sb3'));
await vm.loadProject(d.buffer.slice(d.byteOffset, d.byteOffset + d.byteLength));
const rt = vm.runtime;
rt.currentStepTime = 1000 / 30;

const stage = rt.getTargetForStage();
const machine = rt.targets.find((t) => t.getName() === 'Machine');
const cell = (target, name) => Object.values(target.variables).find((v) => v.name === name);
// A module's `@scratch_global` cell belongs to the project and lands on the
// stage; a sprite's own belongs to the sprite. Look in both, and say which one
// each name was found in -- poking the wrong copy is a silent no-op.
const find = (name) => cell(machine, name) ?? cell(stage, name);
const where = (name) => (cell(machine, name) ? 'machine' : cell(stage, name) ? 'stage' : 'MISSING');
const get = (name) => { const v = find(name); return v ? v.value : undefined; };
const set = (name, value) => {
    const v = find(name);
    if (!v) throw new Error(`nothing has a cell called ${name}`);
    v.value = value;
};
if (process.argv.includes('--where')) {
    for (const n of ['cpu_pc', 'cpu_cpsr', 'cpu_regs', 'machine_slice', 'bus_remap',
        'cpu_mmu_on', 'cpu_halted', 'cpu_instructions', 'cpu_banked']) {
        console.log(`  ${n.padEnd(18)} ${where(n)}`);
    }
}

const start = Number(process.argv[2] ?? 0x34010000);
const count = Number(process.argv[3] ?? 60);
const useMmu = process.argv.includes('--mmu');

vm.greenFlag();
// Let power-on finish, then take the machine's frame loop out of the way so
// the poke below cannot land in the middle of a slice.
set('machine_slice', 1);
for (let i = 0; i < 3; i++) rt._step();

set('cpu_pc', start >>> 0);
set('cpu_cpsr', 0x13);          // SVC mode, both interrupts masked
set('cpu_mmu_on', useMmu ? 1 : 0);
set('cpu_halted', 0);
set('bus_remap', 0);
set('cpu_branched', 0);
const regs = find('cpu_regs');
if (regs) regs.value = new Array(17).fill(0);
set('cpu_instructions', 0);

// A stack at a known place, when one was asked for. Every word of it is filled
// with a recognisable value first, so a frame that lands where it should is
// told apart from one that lands a word out by what is left of the pattern.
const spArg = process.argv.indexOf('--sp');
const sp = spArg >= 0 ? Number(process.argv[spArg + 1]) >>> 0 : 0;
const watchArg = process.argv.indexOf('--watch');
if (watchArg >= 0) set('machine_watch_pc', Number(process.argv[watchArg + 1]) >>> 0);

// Registers to start with, so a function can be *called* rather than watched:
// `--regs 0x600000,64,0x600100,0` is `vscnprintf(buf, 64, fmt, args)`, and what
// lands in the buffer afterwards says what the formatter did.
const regsArg = process.argv.indexOf('--regs');
if (regsArg >= 0) {
    const values = process.argv[regsArg + 1].split(',').map((v) => Number(v) >>> 0);
    const r = (find('cpu_regs').value || []).slice();
    for (let i = 0; i < values.length; i++) r[i] = values[i];
    find('cpu_regs').value = r;
}

// Bytes to place in memory first. `--poke 0x600100:68656c6c6f00` puts "hello"
// at 0x600100, and a format string is the one thing a formatter cannot be
// tested without.
const pokeArg = process.argv.indexOf('--poke');
if (pokeArg >= 0) {
    const ram = find('ram').value;
    for (const spec of process.argv[pokeArg + 1].split(',')) {
        const [addrText, hexText] = spec.split(':');
        const base = Number(addrText) >>> 0;
        for (let i = 0; i < hexText.length; i += 2) {
            const at = base + i / 2;
            const byte = parseInt(hexText.slice(i, i + 2), 16);
            const shift = (at & 3) * 8;
            ram[at >>> 2] = ((Number(ram[at >>> 2]) >>> 0) & ~(0xff << shift)) | (byte << shift);
        }
    }
}
if (sp) {
    const ram = find('ram');
    for (let i = 0; i < 64; i++) ram.value[(sp >>> 2) + i] = 0x5a5a0000 + i;
    if (regs) {
        const r = regs.value.slice();
        r[13] = sp;
        regs.value = r;
    }
    // The bank is what `cpu_get_reg(13)` and `cpu_set_reg(13)` actually use.
    const bankOff = Number(get('cpu_bank_off')) || 0;
    const bankList = find('cpu_bank');
    if (bankList) bankList.value[bankOff] = sp;
}

const hex = (x) => (Number(x) >>> 0).toString(16).padStart(8, '0');
// `cpu_regs` is a one-based Scratch list whose first item is r0, so its item
// `n + 1` is register `n` and the JavaScript array index of r0 is zero.
const readRegs = () => {
    const raw = get('cpu_regs');
    const out = [];
    for (let i = 0; i < 16; i++) out.push(hex(raw[i] ?? 0));
    return out;
};

// Named cells of the machine, printed at the end. The decode leaves what it
// read in cells rather than in registers, and a subtraction whose flags are
// wrong is a question about the two numbers it was given, not about the flags.
if (process.argv.includes('--vars')) {
    for (const n of ['cpu_opcode', 'cpu_setflags', 'cpu_rn', 'cpu_rd', 'cpu_op1', 'cpu_op2',
        'cpu_result', 'cpu_cpsr', 'cpu_branched', 'cpu_address']) {
        console.log(`  ${n.padEnd(14)} ${hex(get(n))}`);
    }
}

for (let i = 0; i < count; i++) {
    const before = Number(get('cpu_pc')) >>> 0;
    const cpsr = Number(get('cpu_cpsr')) >>> 0;
    const insBefore = Number(get('cpu_instructions'));
    const r = readRegs();
    rt._step();
    const after = Number(get('cpu_pc')) >>> 0;
    const delta = Number(get('cpu_instructions')) - insBefore;
    console.log(`step ${String(i).padStart(4)}  ${hex(before)}  cpsr ${hex(cpsr)} ${hex(get('cpu_cpsr'))} ` +
        `-> ${hex(after)}  (+${delta} instruction${delta === 1 ? '' : 's'})  r9=${r[9]} r5=${r[5]}`);
    if (i < 4) console.log(`            r0=${r[0]} r1=${r[1]} r2=${r[2]} r3=${r[3]} r4=${r[4]} r6=${r[6]} r7=${r[7]}`);
    if (i < 4) console.log(`            r8=${r[8]} r10=${r[10]} r11=${r[11]} r12=${r[12]} sp=${r[13]} lr=${r[14]}`);
    if (sp) {
        const ram = find('ram').value;
        const banks = find('cpu_bank').value;
        const now = Number(banks[Number(get('cpu_bank_off')) || 0]) >>> 0;
        const words = [];
        for (let k = -4; k < 12; k++) {
            const w = Number(ram[((now + k * 4) >>> 2)]) >>> 0;
            words.push(`${k >= 0 ? '+' : ''}${k * 4}:${w.toString(16)}`);
        }
        console.log(`            sp ${now.toString(16)}  ${words.join(' ')}`);
    }
}

// What a called function left behind.
const dumpArg = process.argv.indexOf('--dump');
if (dumpArg >= 0) {
    const [addrText, lenText] = process.argv[dumpArg + 1].split(',');
    const base = Number(addrText) >>> 0;
    const len = Number(lenText) || 32;
    const ram = find('ram').value;
    let text = '';
    for (let i = 0; i < len; i++) {
        const at = base + i;
        const b = (Number(ram[at >>> 2]) >>> ((at & 3) * 8)) & 0xff;
        text += (b >= 32 && b < 127) ? String.fromCharCode(b) : (b === 0 ? '.' : '?');
    }
    console.log(`dump ${hex(base)}  ${JSON.stringify(text)}`);
    for (let i = 0; i < len; i += 16) {
        const words = [];
        for (let k = i; k < Math.min(i + 16, len); k += 4) {
            words.push(hex(Number(ram[((base + k) >>> 2)]) >>> 0));
        }
        console.log(`  +${String(i).padStart(3)} ${words.join(' ')}`);
    }
}

