// Generates the four sounds the game makes.
//
//   node examples/raven/chess/tools/sounds.mjs
//
// Each one is a short WAV written here rather than recorded, so the project
// carries no sample it cannot regenerate. They are deliberately small: a knock
// for a move, a harder knock for a capture, a two note rise for a check, and a
// falling figure for the end of a game.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const assets = join(dirname(fileURLToPath(import.meta.url)), "..", "assets");
const RATE = 22050;

/** One channel of 16 bit samples, as a WAV. */
function wav(samples) {
  const header = Buffer.alloc(44);
  header.write("RIFF", 0);
  header.writeUInt32LE(36 + samples.length * 2, 4);
  header.write("WAVE", 8);
  header.write("fmt ", 12);
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20);
  header.writeUInt16LE(1, 22);
  header.writeUInt32LE(RATE, 24);
  header.writeUInt32LE(RATE * 2, 28);
  header.writeUInt16LE(2, 32);
  header.writeUInt16LE(16, 34);
  header.write("data", 36);
  header.writeUInt32LE(samples.length * 2, 40);
  const body = Buffer.alloc(samples.length * 2);
  samples.forEach((value, i) => body.writeInt16LE(Math.max(-32767, Math.min(32767, value | 0)), i * 2));
  return Buffer.concat([header, body]);
}

/** A note: a sine at `hz` over `secs`, faded in and out so it cannot click. */
function tone(hz, secs, level = 0.35, shape = 0) {
  const count = Math.round(secs * RATE);
  const out = new Array(count);
  for (let i = 0; i < count; i += 1) {
    const t = i / RATE;
    const fade = Math.min(1, i / 120, (count - i) / 240);
    // A square through `shape` adds the odd harmonics that make a knock read as
    // a knock rather than a beep.
    let v = Math.sin(2 * Math.PI * hz * t);
    if (shape > 0) v = Math.sign(v) * (0.6 + 0.4 * Math.abs(v));
    out[i] = v * level * fade * 32767;
  }
  return out;
}

const silence = (secs) => new Array(Math.round(secs * RATE)).fill(0);

function cat(...parts) {
  return [].concat(...parts);
}

mkdirSync(assets, { recursive: true });

writeFileSync(join(assets, "move.wav"), wav(cat(tone(220, 0.06, 0.4, 1), silence(0.02))));
writeFileSync(join(assets, "capture.wav"), wav(cat(tone(160, 0.05, 0.45, 1), tone(110, 0.09, 0.4, 1), silence(0.02))));
writeFileSync(join(assets, "check.wav"), wav(cat(tone(660, 0.09), tone(990, 0.14), silence(0.02))));
writeFileSync(join(assets, "start.wav"), wav(cat(tone(523, 0.08), tone(659, 0.08), tone(784, 0.12), silence(0.02))));
writeFileSync(join(assets, "end.wav"), wav(cat(tone(523, 0.12), tone(392, 0.12), tone(330, 0.20), silence(0.02))));

console.log(`wrote 5 sounds to ${assets}`);
