// Writes `assets/theme.wav`: the Tetris theme.
//
//   node examples/raven/tetris/tools/music.mjs
//
// The melody is Korobeiniki, the Russian folk song the game has been playing
// since 1984 and which has been in the public domain for a century. It is written
// here as note numbers with a length in beats, which is the whole reason the file
// is generated rather than stored: a tune is text, and the WAV is a build output.
//
// Two voices, because a chiptune has two: a band-limited square for the tune and
// a triangle for the bass, one root a bar. Sixteen bars of A minor at 150 beats a
// minute is a little under thirteen seconds, which is the loop.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { TUNE_RATE, render, wav } from "../../../../tools/synth.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// Note numbers, middle C is 60, and 0 is a rest. The tune is in A minor.
const E5 = 76;
const F5 = 77;
const G5 = 79;
const A5 = 81;
const B4 = 71;
const C5 = 72;
const D5 = 74;
const A4 = 69;

// The A section, then the B section. Every line is one bar of four beats, and the
// two together are the sixteen bars the loop plays.
const MELODY = [
  [E5, 1], [B4, 0.5], [C5, 0.5], [D5, 1], [C5, 0.5], [B4, 0.5],
  [A4, 1], [A4, 0.5], [C5, 0.5], [E5, 1], [D5, 0.5], [C5, 0.5],
  [B4, 1.5], [C5, 0.5], [D5, 1], [E5, 1],
  [C5, 1], [A4, 1], [A4, 1], [0, 1],

  [D5, 1.5], [F5, 0.5], [A5, 1], [G5, 0.5], [F5, 0.5],
  [E5, 1.5], [C5, 0.5], [E5, 1], [D5, 0.5], [C5, 0.5],
  [B4, 1], [B4, 0.5], [C5, 0.5], [D5, 1], [E5, 1],
  [C5, 1], [A4, 1], [A4, 1], [0, 1],
];

// One root a bar: A, A, E, A, D, A, E, A.
const A2 = 45;
const E2 = 40;
const D3 = 50;
const BASS = [[A2, 4], [A2, 4], [E2, 4], [A2, 4], [D3, 4], [A2, 4], [E2, 4], [A2, 4]];

/// The tune, as data, so `tools/check-audio.mjs` can ask the WAV whether it is
/// the tune this file says it is.
export const TUNE = {
  tempo: 150,
  rate: TUNE_RATE,
  voices: [
    { wave: "square", level: 0.55, notes: MELODY },
    { wave: "triangle", level: 0.7, notes: BASS },
  ],
};

/// Where the sound it writes belongs, for the same reason.
export const FILE = join(root, "assets", "theme.wav");

export function build() {
  return wav(render(TUNE), TUNE.rate);
}

// Write only when this file is what was run: importing it is how the audio check
// reads the tune, and an import must not rewrite the asset.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const bytes = build();
  mkdirSync(dirname(FILE), { recursive: true });
  writeFileSync(FILE, bytes);
  const beats = MELODY.reduce((total, [, length]) => total + length, 0);
  console.log(
    `wrote ${FILE}: ${beats} beats at ${TUNE.tempo}, ` +
      `${(beats * 60) / TUNE.tempo}s, ${(bytes.length / 1024).toFixed(0)} KiB`,
  );
}
