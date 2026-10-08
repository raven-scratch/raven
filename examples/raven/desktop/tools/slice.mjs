// Where the wall clock goes: for each step, how many instructions retired and
// how long the step took. A step that retires far fewer than `machine_slice`
// instructions is a slice being cut short, and the thing cutting it is the
// sequencer rather than the machine.
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
// The same choice `check.mjs` makes, and for the same reason: TurboWarp's VM
// compiles the blocks and runs the machine about three times as fast, which is
// the difference between a slice being a coffee and being a wait. A debugging
// run does not care which VM answered, only that it is the same project.
const VM_ROOT = [
    process.env.SCRATCH_VM_ROOT && path.resolve(process.env.SCRATCH_VM_ROOT),
    path.join(repo, 'ref', 'turbowarp-vm'),
    path.join(repo, 'ref', 'scratch-vm', 'node_modules', 'scratch-vm')
].filter(Boolean).find((dir) => fs.existsSync(path.join(dir, 'src', 'virtual-machine.js')));
const VM = require_(path.join(VM_ROOT, 'src', 'virtual-machine.js'));
const vm = new VM();
console.warn = () => {};
console.error = () => {};
const d = fs.readFileSync(path.join(root, 'dist', 'desktop-arm-virt-linux.sb3'));
await vm.loadProject(d.buffer.slice(d.byteOffset, d.byteOffset + d.byteLength));
const rt = vm.runtime;
rt.currentStepTime = 1000 / 30;
if (process.argv.includes('--turbo')) rt.turboMode = true;
const stage = rt.getTargetForStage();
const machine = rt.targets.find((t) => t.getName() === 'Machine');
const get = (n) => { const v = Object.values(stage.variables).find((x) => x.name === n); return v ? v.value : undefined; };
const setMachine = (n, v) => {
    const x = Object.values(machine.variables).find((y) => y.name === n);
    if (x) x.value = v;
};
const hex = (x) => (Number(x) >>> 0).toString(16);
if (process.argv.includes('--stop')) setMachine('machine_stop_on_fault', 1);
if (process.argv.includes('--trace')) setMachine('machine_trace_on', 1);
if (process.env.WATCH_PC) setMachine('machine_watch_pc', Number(process.env.WATCH_PC) >>> 0);
vm.greenFlag();
// The seconds are the first argument, but only when they are one: a run that
// leads with a flag put `Number('--limit')` here, which is NaN, which made the
// loop condition false and the run report nothing at all.
const secondsArg = Number(process.argv[2]);
const budget = (Number.isFinite(secondsArg) ? secondsArg : 40) * 1000;
// A wall-clock budget is a budget on *this* machine, not on the emulated one,
// and the guest is not indifferent to how many instructions fit in a slice: the
// timer is real, so an interrupt lands at a different instruction each run and
// the boot takes a different path through the same code. Two runs of the same
// image therefore disagree, and a disagreement between two samples is not
// evidence of anything until the machine repeats itself. `--steps N` runs
// exactly N runtime steps and stops -- no clock, no sampling, one answer.
const stepArg = process.argv.indexOf('--steps');
const fixedSteps = stepArg >= 0 ? Number(process.argv[stepArg + 1]) : 0;
// `--limit N` asks the *machine* to stop after N guest instructions, which is
// the only fixed unit on offer: a runtime step is budgeted by wall clock, and a
// step is therefore a variable amount of machine. The cell is off at zero, so
// nothing about an ordinary run changes. This path is not finished -- it stops
// the machine but the report reads zero -- and the flag is left in place
// unwired rather than half-working.
const limitArg = process.argv.indexOf('--limit');
const limit = limitArg >= 0 ? Number(process.argv[limitArg + 1]) : 0;
if (limit > 0) setMachine('machine_limit', limit);
const t0 = Date.now();
let steps = 0;
let prevIns = 0;
let prevAt = t0;
let prevText = '';
let prevFar = -1;
let prevFsr = -1;
if (limit > 0) setMachine('machine_limit', limit);
const reached = false;
while (!reached && (fixedSteps > 0 ? steps < fixedSteps : Date.now() - t0 < budget)) {
    rt._step();
    steps++;
    // A fault records where it happened; log every change, so the *first*
    // abort after the MMU comes on is visible and not just the loop it caused.
    const farNow = Number(get('cp15_far')) >>> 0;
    const fsrNow = Number(get('cp15_fsr')) >>> 0;
    if (farNow !== prevFar || fsrNow !== prevFsr) {
        console.log(`  fault change at step ${steps}: fsr=${fsrNow.toString(16)} far=${farNow.toString(16)} ` +
            `pc=${(Number(get('cpu_pc')) >>> 0).toString(16)} cpsr=${(Number(get('cpu_cpsr')) >>> 0).toString(16)}`);
        prevFar = farNow;
        prevFsr = fsrNow;
    }
    const trace = get('console_trace') || [];
    const text = Buffer.from(trace.map((b) => Number(b) & 0xff)).toString('latin1');
    if (text !== prevText) {
        // One line per change, with the newlines escaped, so a grep for the
        // guest's output finds it rather than a block of it.
        const added = text.slice(prevText.length);
        console.log(`console +${JSON.stringify(added)} at ${((Date.now() - t0) / 1000).toFixed(1)} s ` +
            `(${Number(get('cpu_instructions'))} instructions)`);
        prevText = text;
    }
    if (steps % 10 === 0) {
        const now = Date.now();
        const ins = Number(get('cpu_instructions'));
        console.log(`step ${steps}  +${((now - prevAt) / 1000).toFixed(3)} s  +${ins - prevIns} instructions  ` +
            `(${Math.round((ins - prevIns) / ((now - prevAt) / 1000))} /s)  pc=${hex(get('cpu_pc'))} ` +
            `cpsr=${hex(get('cpu_cpsr'))} ir=${hex(get('cpu_ir'))}`);
        console.log(`      fault=${get('mem_fault')} fsr=${hex(get('cp15_fsr'))} far=${hex(get('cp15_far'))} ` +
            `ctl=${hex(get('cp15_control'))} ttb=${hex(get('cp15_ttb'))} dacr=${hex(get('cp15_dacr'))} ` +
            `perm=${get('mem_perm')}`);
        console.log(`      halted=${get('cpu_halted')} irqpend=${get('cpu_irq_pending')} irqdirty=${get('bus_irq_dirty')} ` +
            `r0=${hex((get('cpu_regs') || [])[0])} lr=${hex((get('cpu_regs') || [])[14])}`);
        // The level one table the kernel built, for the entries that matter:
        // the kernel's own section, the device tree, and the vector page.
        const ram = get('ram') || [];
        const l1 = (va) => hex(ram[(0x4000 >> 2) + (va >>> 20)]);
        console.log(`      L1@4000  va0=${l1(0)} va8000=${l1(0x8000)} vaC00000=${l1(0xc00000)} ` +
            `vaFEF00000=${l1(0xfef00000)} vaFF000000=${l1(0xff000000)} vaFFFF0000=${l1(0xffff0000)}`);
        prevIns = ins;
        prevAt = now;
    }
}
const ins = Number(get('cpu_instructions'));
const secs = (Date.now() - t0) / 1000;
console.log(`steps        ${steps} in ${secs.toFixed(1)} s (${(steps / secs).toFixed(2)} steps/s)`);
console.log(`instructions ${ins} at ${Math.round(ins / secs)} /s`);
console.log(`per slice    ${((ins) / steps).toFixed(0)} instructions per step, slice is 8192`);
console.log(`mmu          ${get('cpu_mmu_on')}  remap ${get('bus_remap')}  pc 0x${(Number(get('cpu_pc')) >>> 0).toString(16)}`);
console.log(`cpsr         ${(Number(get('cpu_cpsr')) >>> 0).toString(16)}`);

// Every mode's banked stack pointer and link register, and the status register
// its exception saved. `cpu_bank` is a one-based Scratch list and `cpu_bank_off`
// is already the zero-based offset into it, so item `cpu_bank_off + 1` is that
// bank's r13 -- which is JavaScript index `cpu_bank_off`.
const bankOff = Number(get('cpu_bank_off'));
const bank = get('cpu_bank') || [];
const mode = Number(get('cpu_cpsr')) & 0x1f;
const MODES = [['usr', 16], ['fiq', 17], ['irq', 18], ['svc', 19], ['abt', 23], ['und', 27]];
console.log(`mode         ${mode}  bank_off ${bankOff}`);
for (const [name, num] of MODES) {
    const off = Number(get('cpu_mode_bank')?.[num] ?? NaN) * 3;
    if (!Number.isFinite(off)) continue;
    console.log(`  bank ${name}  r13=${hex(bank[off])} r14=${hex(bank[off + 1])} spsr=${hex(bank[off + 2])}` +
        `${off === bankOff ? '   <- current' : ''}`);
}
if (bankOff > 0) {
    const lr = Number(bank[bankOff + 1]) >>> 0;
    const regs = get('cpu_regs') || [];
    console.log(`  registers r0..r15`);
    for (let i = 0; i < 16; i += 4) {
        console.log(`    ${[0, 1, 2, 3].map((k) => `r${i + k}=${hex(regs[i + k])}`).join(' ')}`);
    }
    // A data abort saves the instruction's address plus eight and a prefetch
    // abort plus four, so the instruction word says which of the two this was.
    console.log(`abort lr     ${hex(lr)}   candidate instructions ${hex(lr - 8)} and ${hex(lr - 4)}`);
    console.log(`instruction  ${hex(get('cpu_ir'))}`);
    console.log(`fault        fsr=${hex(get('cp15_fsr'))} far=${hex(get('cp15_far'))} ` +
        `(bit 10 of the status set means a write)`);
    // Where the instruction should have come from: the kernel is linked at
    // 0xc0008000 and loaded at 0x8000, so the kernel's virtual addresses are its
    // physical ones plus PAGE_OFFSET. Both the level one entry and the physical
    // word are printed, because a wrong one of either looks the same from here.
    const ram = get('ram') || [];
    const va = lr - 8;
    const word = (pa) => ram[pa >>> 2];
    console.log(`  va ${hex(va)}  L1 entry ${hex(ram[(0x4000 >> 2) + (va >>> 20)])}`);
    console.log(`  physical ${hex(va & 0x000fffff)} holds ${hex(word(va & 0x000fffff))}, ` +
        `physical ${hex(va)} holds ${hex(word(va))}`);
    // The kernel maps itself with sections from entry 0xc00, so a run of them
    // says whether the bases are consecutive. The section flags are the low
    // twenty bits; the base is what is left.
    const parts = [];
    for (let i = 0xc00; i <= 0xc0a; i++) {
        const e = Number(ram[(0x4000 >> 2) + i]) >>> 0;
        parts.push(`c${(i & 0xfff).toString(16)}:${hex(e & 0xfff00000)}`);
    }
    console.log(`  kernel sections ${parts.join(' ')}`);
    // And the physical section the entry names, to see what is really there.
    const base = (Number(ram[(0x4000 >> 2) + (va >>> 20)]) >>> 0) & 0xfff00000;
    console.log(`  mapped physical ${hex(base + (va & 0x000fffff))} holds ` +
        `${hex(word(base + (va & 0x000fffff)))}`);
}

// What the kernel has said, out of its own log buffer.
//
// A kernel that panics before it has registered a console prints into its log
// buffer and nowhere else, so the reason is in RAM and not on the wire. The
// buffer is `__log_buf` -- but `log_buf` is a *pointer* to it and boot may have
// moved it, so the pointer is followed and the symbol is only the fallback.
// Both addresses come from the kernel's `System.map`.
const LOG_BUF_PTR = Number(process.env.LOG_BUF_PTR || 0xc06d9fd8);
const LOG_BUF = Number(process.env.LOG_BUF || 0xc0702a7c);
const ram = get('ram') || [];
const word = (va) => Number(ram[(va - 0xc0000000) >>> 2]) >>> 0;
const pointed = word(LOG_BUF_PTR);
const buffer = (pointed >= 0xc0000000 && pointed < 0xc1000000) ? pointed : LOG_BUF;
console.log(`kernel log   log_buf ${hex(LOG_BUF_PTR)} -> ${hex(pointed)} ` +
    `(${buffer === pointed ? 'followed' : 'using __log_buf'})`);
console.log(`uart0        ${get('console_writes')} bytes written to the data register, ` +
    `${get('console_refused')} refused by the transmitter, ` +
    `${(get('console_trace') || []).length} on the wire`);
const bytes = [];
for (let pa = buffer - 0xc0000000; bytes.length < 16384; pa += 4) {
    const w = word(0xc0000000 + pa);
    bytes.push(w & 0xff, (w >>> 8) & 0xff, (w >>> 16) & 0xff, (w >>> 24) & 0xff);
}
let run = '';
const runs = [];
for (const b of bytes) {
    if (b >= 32 && b < 127) { run += String.fromCharCode(b); }
    else { if (run.length >= 8) runs.push(run); run = ''; }
}
if (run.length >= 8) runs.push(run);
console.log(`             ${runs.length} printable runs`);
for (const line of runs.slice(-14)) console.log(`  | ${line}`);

// The call chain, out of the supervisor stack.
//
// There is no unwinder here, but a kernel stack is a list of return addresses
// with saved registers between them, so the words that point into the kernel's
// own text *are* the call chain in order. They print as addresses; the names
// come from the kernel's `System.map`.
const SVC_BANK = 19;
const svcOff = Number(get('cpu_mode_bank')?.[SVC_BANK] ?? NaN) * 3;
const sp = Number(bank[svcOff]) >>> 0;
if (Number.isFinite(svcOff) && sp >= 0xc0000000 && sp < 0xc1000000) {
    const chain = [];
    for (let i = 0; i < 512; i += 4) {
        const w = word(sp + i);
        if (w >= 0xc0008000 && w < 0xc0700000) chain.push(w);
    }
    console.log(`stack        sp ${hex(sp)}  ${chain.length} return addresses`);
    console.log(`  ${chain.map(hex).join(' ')}`);
}
// Where a string is in the machine's memory, if it is anywhere.
//
// A kernel that has printed has its words *somewhere* -- in its log buffer, in
// the buffer a message was formatted into, or in a console driver's own ring --
// and "nothing appeared on the wire" and "nothing was ever written" look
// identical from the outside. Searching for the text tells them apart.
for (const needle of String(process.env.FIND_TEXT || '').split('|').filter(Boolean)) {
    const ram = get('ram') || [];
    const target = Buffer.from(needle, 'latin1');
    const hits = [];
    // Every byte offset, not just the ones a word starts at: a kernel's strings
    // are packed end to end in `.rodata` and a search that only looked at four
    // byte boundaries would report that a string the machine is *running* is
    // not in its memory at all.
    const byteAt = (p) => (Number(ram[p >>> 2]) >>> ((p & 3) * 8)) & 0xff;
    for (let p = 0; p < (ram.length << 2) - target.length && hits.length < 8; p++) {
        if (byteAt(p) !== target[0]) continue;
        let ok = true;
        for (let k = 1; k < target.length; k++) {
            if (byteAt(p + k) !== target[k]) { ok = false; break; }
        }
        if (ok) hits.push(hex(0xc0000000 + p));
    }
    console.log(`find ${JSON.stringify(needle)}  ${hits.length ? hits.join(' ') : 'nowhere in memory'}`);
}

// Individual words at arbitrary virtual addresses, comma separated. A canary
// and the slot it was saved in are two words, and seeing both at once is how a
// failed comparison is told apart from a corrupted frame.
for (const spec of String(process.env.WORD_AT || '').split(',')) {
    const va = Number(spec);
    if (!va) continue;
    console.log(`word ${hex(va)} = ${hex(word(va))}`);
}

// Printable text at arbitrary physical addresses, given as a comma-separated
// list of the *virtual* addresses the kernel's `System.map` names. A formatted
// message that never reached a console is still in the buffer it was formatted
// into, and `panic` formats into a static one before it prints.
for (const spec of String(process.env.TEXT_AT || '').split(',')) {
    const va = Number(spec);
    if (!va) continue;
    let text = '';
    for (let i = 0; i < 2048; i++) {
        const w = word((va + (i & ~3)) >>> 0);
        const b = (w >>> ((i & 3) * 8)) & 0xff;
        text += (b >= 32 && b < 127) ? String.fromCharCode(b) : (b === 0 ? '\u0000' : '.');
    }
    const runs = text.split(/[\u0000]+/).filter((s) => s.replace(/[^ -~]/g, '').length >= 6);
    console.log(`text at ${hex(va)}  ${runs.length} runs`);
    for (const line of runs.slice(0, 10)) console.log(`  > ${line}`);
}

// The processor's recent history, if the machine was asked to keep one. A ring
// of program counters, oldest first, run-length encoded so a loop is a line
// rather than a page.
if (process.argv.includes('--trace')) {
    const cellOf = (n) => Object.values(machine.variables).find((v) => v.name === n) ?? find(n);
    const ring = cellOf('machine_trace_pc')?.value ?? [];
    const spr = cellOf('machine_trace_sp')?.value ?? [];
    const next = Number(cellOf('machine_trace_next')?.value) || 0;
    const ordered = [];
    for (let i = 0; i < ring.length; i++) ordered.push([ring[(next + i) % ring.length], spr[(next + i) % ring.length]]);
    console.log(`pc ring      ${ring.length} entries, oldest first, pc x count then sp when it changed`);
    let line = '';
    let lastSp = null;
    let i = 0;
    while (i < ordered.length) {
        let j = i;
        while (j < ordered.length && ordered[j][0] === ordered[i][0]) j++;
        line += (line ? '  ' : '') + `${hex(ordered[i][0])}x${j - i}`;
        if (line.length > 92) { console.log(`  ${line}`); line = ''; }
        i = j;
    }
    if (line) console.log(`  ${line}`);
    // The stack pointer at each step where it changed, with the instruction
    // that changed it -- which is the whole point of keeping the pair.
    console.log('  sp changes');
    for (const [pc, sp] of ordered) {
        if (sp !== lastSp) {
            console.log(`    ${hex(pc)}  sp=${hex(sp)}`);
            lastSp = sp;
        }
    }
}



