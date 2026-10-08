// Check the generated condition table against the sixteen answers ARM defines.
//
// The table is a Scratch list in `src/cpu/luts.rav`, indexed by the flags'
// own four bits and the instruction's four, so an entry that is off by one
// position is a condition that is taken when it should not be -- and a taken
// `blt` on a positive value is not a subtle failure.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const source = fs.readFileSync(path.join(here, '..', 'src', 'cpu', 'luts.rav'), 'utf8');
const match = /pub var alu_cond_lut: list<num> = \[([\s\S]*?)\];/.exec(source);
if (!match) { console.log('alu_cond_lut not found'); process.exit(1); }
const got = match[1].split(',').map((s) => Number(s.trim())).filter((n) => !Number.isNaN(n));

// The names, in the order ARM numbers them, so a mismatch can be named.
const NAMES = ['eq', 'ne', 'cs', 'cc', 'mi', 'pl', 'vs', 'vc',
    'hi', 'ls', 'ge', 'lt', 'gt', 'le', 'al', 'nv'];

let wrong = 0;
for (let bits = 0; bits < 16; bits++) {
    const n = (bits >> 3) & 1, z = (bits >> 2) & 1, c = (bits >> 1) & 1, v = bits & 1;
    const answers = [
        z, !z, c, !c, n, !n, v, !v,
        c && !z, !c || z, n === v, n !== v,
        !z && n === v, z || n !== v, true, false
    ];
    for (let cond = 0; cond < 16; cond++) {
        const want = answers[cond] ? 1 : 0;
        const at = bits * 16 + cond;
        if (got[at] !== want) {
            if (wrong < 12) {
                console.log(`  nzcv=${n}${z}${c}${v} ${NAMES[cond]}  table[${at}]=${got[at]} want=${want}`);
            }
            wrong++;
        }
    }
}
console.log(`entries ${got.length}  wrong ${wrong}`);
// The one that matters here: nzcv = 0010 (C set), `lt`.
const at = 2 * 16 + 11;
console.log(`nzcv=0010 lt  table[${at}] = ${got[at]} (want 0)`);
