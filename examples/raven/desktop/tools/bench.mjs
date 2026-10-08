// Time the machine on a known number of instructions, so that "how long would
// a kernel take" has an answer instead of a guess.
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
const get = (n) => { const v = Object.values(stage.variables).find((x) => x.name === n); return v ? v.value : []; };
const machine = rt.targets.find((t) => t.getName() === 'Machine');
const set = (n, value) => { const v = Object.values(machine.variables).find((x) => x.name === n); if (v) v.value = value; };

const slice = Number(process.argv[2] || 65536);
const frames = Number(process.argv[3] || 16);
set('machine_slice', slice);
set('machine_budget', frames);

vm.greenFlag();
const t0 = Date.now();
let steps = 0;
const text = () => Buffer.from(get('console_trace').map((b) => Number(b) & 0xff)).toString('latin1');
while (Date.now() - t0 < 600000 && !text().includes('STOP')) { rt._step(); steps++; }
const secs = (Date.now() - t0) / 1000;
const done = get('cpu_instructions');
console.log(`slice ${slice} x ${frames} frames`);
console.log(`instructions ${done}`);
console.log(`wall         ${secs.toFixed(1)} s`);
console.log(`rate         ${Math.round(done / secs)} instructions/s`);
console.log(`steps        ${steps} runtime steps`);
console.log(JSON.stringify(text()));
