#!/usr/bin/env node
/**
 * Check the audio the examples ship.
 *
 * `tools/synth.mjs` writes a WAV from a list of notes, and the examples' audio is
 * generated rather than recorded — so the tunes live in a generator as data and
 * the files are build output. Nothing about that is visible in the `.sb3`: a WAV
 * that is the wrong length, at the wrong rate, or with its channels declared
 * wrong still loads, and a synth with a bug in its waveforms still makes a noise
 * that "sounds like something". This is what asks the files what they contain.
 *
 *   node tools/check-audio.mjs
 *
 * For every `examples/**\/assets/*.wav` it checks the file is 16-bit mono PCM at
 * a rate the project was written for and holds a fraction of a second or more of
 * audio; for the two tunes it then asks whether each note is where the tune says
 * it is, by measuring how much of the note's own frequency is in the samples at
 * the moment that note should be playing — against the semitone either side of
 * it, which is the smallest error a listener hears as a wrong note.
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { frequency, read, tone } from "./synth.mjs";
import { TUNE as TETRIS } from "../examples/raven/tetris/tools/music.mjs";
import { AMBIENT, EFFECTS } from "../examples/raven/sudoku/tools/sounds.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];
const check = (ok, message) => {
  if (!ok) failures.push(message);
  return ok;
};

/** Every `.wav` under the examples, as a path relative to the repository. */
function wavs(directory) {
  const found = [];
  for (const entry of readdirSync(directory)) {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) found.push(...wavs(path));
    else if (entry.endsWith(".wav")) found.push(path);
  }
  return found;
}

const files = wavs(join(root, "examples"));
console.log(`${files.length} sound files`);
const peakOf = new Map();
for (const path of files) {
  const name = path.slice(root.length + 1).replace(/\\/g, "/");
  const parsed = read(readFileSync(path));
  check(
    parsed.bits === 16 && parsed.channels === 1,
    `${name}: expected 16-bit mono, found ${parsed.bits}-bit ${parsed.channels}-channel`,
  );
  check(
    parsed.rate === 11025 || parsed.rate === 22050,
    `${name}: sample rate ${parsed.rate} is not one the synth writes`,
  );
  // Ten milliseconds: enough to catch a file that was truncated to nothing, small
  // enough to allow a click, which is what a short effect is.
  check(parsed.frames > parsed.rate / 100, `${name}: only ${parsed.frames} frames`);
  let peak = 0;
  for (const sample of parsed.samples) peak = Math.max(peak, Math.abs(sample));
  check(peak > 0.01, `${name}: peak ${peak.toFixed(3)} is nearly silent`);
  check(peak <= 1, `${name}: peak ${peak.toFixed(3)} clips`);
  peakOf.set(name, peak);
}

// The music has to sit under the effects, not beside them. An effect that a
// player cannot pick out over the loop is an effect they do not notice, and the
// volume of a generated file is a number in the generator, so it is worth a
// check rather than an ear.
const BACKGROUND = [
  "examples/raven/tetris/assets/theme.wav",
  "examples/raven/sudoku/assets/ambient.wav",
];
for (const name of BACKGROUND) {
  const peak = peakOf.get(name);
  const loudest = Math.max(
    ...[...peakOf]
      .filter(([other]) => !BACKGROUND.includes(other))
      .map(([, value]) => value),
  );
  check(
    peak < loudest,
    `${name}: peak ${peak.toFixed(3)} is not under the effects at ${loudest.toFixed(3)}`,
  );
  console.log(`  ${name}: peak ${peak.toFixed(3)} against effects at ${loudest.toFixed(3)}`);
}

/**
 * Every note of a tune, in the samples where that note should be playing.
 *
 * A window a quarter of the way into the note, half a note long, so it cannot
 * catch a neighbouring one. The note's own frequency has to beat the semitone
 * either side of it: half a semitone is the point at which a listener says the
 * note is wrong, and a whole tone would not notice a synth that was tuning every
 * note a step out.
 */
function inTune(label, path, tune, notes) {
  const parsed = read(readFileSync(path));
  const beat = 60 / tune.tempo;
  let at = 0;
  let wrong = 0;
  let tested = 0;
  for (const [midi, beats] of notes) {
    if (midi > 0) {
      const start = Math.round((at + beats * 0.25) * beat * parsed.rate);
      const length = Math.round(beats * 0.5 * beat * parsed.rate);
      const expected = frequency(midi);
      const around = [
        expected / 2 ** (1 / 12),
        expected,
        expected * 2 ** (1 / 12),
      ].map((hz) => tone(parsed.samples, parsed.rate, hz, start, length));
      const best = around.indexOf(Math.max(...around));
      tested += 1;
      if (best !== 1) wrong += 1;
    }
    at += beats;
  }
  check(
    wrong === 0,
    `${label}: ${wrong} of ${tested} notes are not at the pitch the tune gives them`,
  );
  console.log(`  ${label}: ${tested - wrong}/${tested} notes on pitch`);
}

inTune(
  "tetris theme",
  join(root, "examples", "raven", "tetris", "assets", "theme.wav"),
  TETRIS,
  TETRIS.voices[0].notes,
);
inTune(
  "sudoku loop",
  join(root, "examples", "raven", "sudoku", "assets", "ambient.wav"),
  AMBIENT,
  AMBIENT.voices[0].notes,
);

// Every effect the generator knows about has to be on disk, or a `Sound::` name
// in the raven source points at nothing.
const sudoku = join(root, "examples", "raven", "sudoku", "assets");
for (const name of Object.keys(EFFECTS)) {
  check(
    readdirSync(sudoku).includes(name),
    `sudoku: ${name} is generated but not written`,
  );
}

if (failures.length > 0) {
  console.log(`\nFAIL ${failures.length}`);
  for (const failure of failures) console.log(`  - ${failure}`);
  process.exit(1);
}
console.log('\nPASS the shipped audio is what the generators say it is');
