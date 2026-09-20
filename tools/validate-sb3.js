#!/usr/bin/env node
/**
 * Load a generated `.sb3` into a real Scratch VM and run it.
 *
 * raven-asm's integration tests check the shape of `project.json`, but the only way
 * to know a project actually *works* is to hand it to the runtime. This script
 * does that: it deserializes the archive with the Scratch VM, verifies that
 * every opcode in the file is one the runtime knows about, runs the green-flag
 * scripts for a while, and serializes the result back.
 *
 * It needs a checkout of the Scratch VM and its dependencies:
 *
 *   git clone https://github.com/raven-scratch/scratch-editor ../scratch-editor
 *   cd ../scratch-editor/packages/scratch-vm && npm install --ignore-scripts
 *
 * Then, from the repository root:
 *
 *   raven-asm new scratch && cd scratch
 *   raven-asm build --debug
 *   node ../../tools/validate-sb3.js dist/scratch.sb3 --steps 1000
 *
 * `--steps` is milliseconds of simulated play time. The exit code is non-zero
 * if the VM reported an error or met an unknown opcode.
 *
 * Environment:
 *   SCRATCH_VM_ROOT  path to `packages/scratch-vm` (defaults to the sibling
 *                    checkout described above).
 */

const fs = require('fs');
const path = require('path');
const Module = require('module');

// The checkout is a workspace monorepo and some sibling packages have no built
// `dist/`. Only the SVG sanitiser is needed here, and it does not affect block
// structure or execution, so stub it out.
const STUBS = {
    '@scratch/scratch-svg-renderer': () => ({
        sanitizeSvg: { sanitizeByteStream: data => data },
        loadSvgString: () => Promise.resolve(),
        serializeSvgToString: () => ''
    })
};
const originalLoad = Module._load;
Module._load = function (request, parent, isMain) {
    if (Object.prototype.hasOwnProperty.call(STUBS, request)) {
        return STUBS[request]();
    }
    return originalLoad.call(this, request, parent, isMain);
};

const VM_ROOT = process.env.SCRATCH_VM_ROOT
    ? path.resolve(process.env.SCRATCH_VM_ROOT)
    : path.resolve(__dirname, '../../scratch-editor/packages/scratch-vm');

let VirtualMachine;
try {
    VirtualMachine = require(path.join(VM_ROOT, 'src/virtual-machine.js'));
} catch (err) {
    console.error(`Could not load the Scratch VM from ${VM_ROOT}`);
    console.error('See the header of this file for how to get a checkout.');
    console.error(String(err.message || err));
    process.exit(2);
}

/**
 * Opcodes the runtime resolves without a registered primitive: hats, control
 * flow, procedure plumbing and the shadow blocks Scratch stores inside inputs.
 */
const SHADOW_OPCODES = new Set([
    'math_number', 'math_positive_number', 'math_whole_number', 'math_integer',
    'math_angle', 'colour_picker', 'text', 'event_broadcast_menu',
    'data_variable', 'data_listcontents',
    'looks_costume', 'looks_backdrops', 'sound_sounds_menu', 'motion_goto_menu',
    'motion_glideto_menu', 'motion_pointtowards_menu', 'sensing_touchingobjectmenu',
    'sensing_distancetomenu', 'sensing_keyoptions', 'sensing_of_object_menu',
    'control_create_clone_of_menu'
]);

function isKnownOpcode (runtime, opcode) {
    if (Object.prototype.hasOwnProperty.call(runtime._primitives, opcode)) return true;
    if (Object.prototype.hasOwnProperty.call(runtime._hats, opcode)) return true;
    if (opcode.startsWith('procedures_') || opcode.startsWith('argument_reporter_')) return true;
    if (SHADOW_OPCODES.has(opcode)) return true;
    // Extension menus are named `<extension>_menu_<MENU>`.
    if (opcode.includes('_menu_')) return true;
    return false;
}

async function main () {
    const sb3 = process.argv[2];
    if (!sb3) {
        console.error('usage: node tools/validate-sb3.js <file.sb3> [--steps MS]');
        process.exit(2);
    }
    const stepsArg = process.argv.indexOf('--steps');
    const playMs = stepsArg >= 0 ? Number(process.argv[stepsArg + 1]) : 1000;

    const errors = [];
    const vm = new VirtualMachine();

    const originalError = console.error;
    const originalWarn = console.warn;
    console.error = (...args) => {
        errors.push(args.map(String).join(' '));
        originalError(...args);
    };
    console.warn = () => {};

    const data = fs.readFileSync(sb3);
    const buffer = data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength);
    await vm.loadProject(buffer);

    const runtime = vm.runtime;
    const targets = runtime.targets.filter(t => t.isOriginal);
    const unknownOpcodes = new Set();
    const allOpcodes = new Set();
    for (const target of targets) {
        for (const block of Object.values(target.blocks._blocks)) {
            if (!block || !block.opcode) continue;
            allOpcodes.add(block.opcode);
            if (!isKnownOpcode(runtime, block.opcode)) unknownOpcodes.add(block.opcode);
        }
    }

    const before = targets.map(t => ({
        name: t.getName(),
        variables: Object.values(t.variables).map(v => `${v.name}=${JSON.stringify(v.value)}`)
    }));

    vm.greenFlag();
    vm.start();
    await new Promise(resolve => setTimeout(resolve, playMs));
    vm.stopAll();

    const after = targets.map(t => ({
        name: t.getName(),
        x: Math.round(t.x * 100) / 100,
        y: Math.round(t.y * 100) / 100,
        direction: Math.round(t.direction * 100) / 100,
        variables: Object.values(t.variables).map(v => `${v.name}=${JSON.stringify(v.value)}`)
    }));

    const roundTripped = JSON.parse(vm.toJSON());

    originalWarn(`file            ${sb3}`);
    originalWarn(`targets         ${targets.map(t => t.getName()).join(', ')}`);
    originalWarn(`opcodes         ${allOpcodes.size} distinct`);
    originalWarn(`blocks          ${targets.reduce((n, t) => n + Object.keys(t.blocks._blocks).length, 0)}`);
    originalWarn(`monitors        ${roundTripped.monitors.length}`);
    originalWarn(`played          ${playMs} ms`);
    for (const state of after) {
        originalWarn(`  ${state.name.padEnd(12)} x=${state.x} y=${state.y} direction=${state.direction}`);
        for (const variable of state.variables) originalWarn(`    ${variable}`);
    }
    void before;

    const failures = [];
    if (unknownOpcodes.size > 0) failures.push(`unknown opcodes: ${[...unknownOpcodes].join(', ')}`);
    if (errors.length > 0) failures.push(`${errors.length} runtime error(s)`);

    if (failures.length > 0) {
        originalWarn(`FAIL  ${failures.join('; ')}`);
        process.exit(1);
    }
    originalWarn('PASS');
    process.exit(0);
}

main().catch(err => {
    console.error('HARNESS FAILURE:', err && err.stack ? err.stack : err);
    process.exit(1);
});
