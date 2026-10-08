// How many instructions the machine retires in a given wall-clock time. The
// number that decides whether a kernel boot finishes.
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
const get = (n) => { const v = Object.values(stage.variables).find((x) => x.name === n); return v ? v.value : undefined; };
const machine = rt.targets.find((t) => t.getName() === 'Machine');
const set = (n, v) => { const x = Object.values(machine.variables).find((y) => y.name === n); if (x) x.value = v; };
set('machine_slice', 8192);
set('machine_budget', 2000000);
vm.greenFlag();
const budget = Number(process.argv[2] || 60) * 1000;
const t0 = Date.now();
let steps = 0;
while (Date.now() - t0 < budget) { rt._step(); steps++; }
const secs = (Date.now() - t0) / 1000;
const done = Number(get('cpu_instructions'));
const text = Buffer.from((get('console_trace') || []).map((b) => Number(b) & 0xff)).toString('latin1');
console.log(`instructions ${done}`);
console.log(`wall         ${secs.toFixed(1)} s`);
console.log(`rate         ${Math.round(done / secs)} instructions/s`);
console.log(`pc           ${'0x' + (Number(get('cpu_pc')) >>> 0).toString(16)}`);
console.log(`remap        ${get('bus_remap')}  mmu ${get('cpu_mmu_on')}`);
console.log('vic          ', get('vic_enable'), 'timer ctrl', JSON.stringify(get('sp804_ctrl')));
console.log('cpsr         ', (Number(get('cpu_cpsr')) >>> 0).toString(16));
console.log('regs         ', (get('cpu_regs') || []).slice(0, 13)
    .map((x) => (Number(x) >>> 0).toString(16)).join(' '));
console.log(`console      ${JSON.stringify(text.slice(0, 60))} ... ${JSON.stringify(text.slice(-80))}`);
