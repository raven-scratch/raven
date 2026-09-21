// Writes sudoku's audio: `assets/ambient.wav` and one file per effect.
//
//   node examples/raven/sudoku/tools/sounds.mjs
//
// The background is an original loop, not a quotation. There is no theme that
// belongs to Sudoku the way Korobeiniki belongs to Tetris — puzzle games generally
// license something quiet, and a licensed track is not something to copy into a
// repository. So this is a slow arpeggio over C, Am, F and G with a root under it:
// the shape every calm loop has, written here as notes so it can be read and
// changed rather than listened to and guessed at.
//
// The effects are short and each one says what happened: a rise when a cell takes
// its digit, a fall when one is refused, an arpeggio when a line or a box
// finishes, a fanfare for the board, a longer fall when the tries run out, and a
// tick for the menu.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { EFFECT_RATE, TUNE_RATE, render, wav } from "../../../../tools/synth.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const assets = join(root, "assets");

// --- the background ---------------------------------------------------------

// Four bars of 4/4 at 76 beats a minute: twelve and a half seconds, which is long
// enough not to nag and short enough to be audio rather than a download.
const ARPEGGIO = [
  // Cmaj7
  [72, 0.5], [76, 0.5], [79, 0.5], [83, 0.5], [79, 0.5], [76, 0.5], [72, 0.5], [76, 0.5],
  // Am7
  [69, 0.5], [72, 0.5], [76, 0.5], [81, 0.5], [76, 0.5], [72, 0.5], [69, 0.5], [72, 0.5],
  // Fmaj7
  [65, 0.5], [69, 0.5], [72, 0.5], [77, 0.5], [72, 0.5], [69, 0.5], [65, 0.5], [69, 0.5],
  // G
  [67, 0.5], [71, 0.5], [74, 0.5], [79, 0.5], [74, 0.5], [71, 0.5], [67, 0.5], [71, 0.5],
];

// One root a bar: C, A, F, G.
const ROOTS = [[48, 4], [45, 4], [41, 4], [43, 4]];

const tempo = 76;

/// The loop, as data, so `tools/check-audio.mjs` can ask the WAV whether it is
/// the tune this file says it is.
///
/// `level` is the whole loop's loudness, and it is a third of what the synthesizer
/// would otherwise peak at: the background plays under the effects, which are at
/// full and are the ones that have to be heard over it.
export const AMBIENT = {
  tempo,
  rate: TUNE_RATE,
  level: 0.3,
  voices: [
    { wave: "triangle", level: 0.5, notes: ARPEGGIO },
    { wave: "sine", level: 0.6, notes: ROOTS },
  ],
};

// --- the effects ------------------------------------------------------------

// Beats at 120 a minute, so a beat is half a second: these lengths are the sound,
// and they are all under half a second.
export const EFFECTS = {
  // A cell took its digit.
  "good.wav": { wave: "square", notes: [[84, 0.1], [91, 0.18]] },
  // A digit was refused.
  "bad.wav": { wave: "saw", notes: [[41, 0.14], [38, 0.26]] },
  // A line or a box finished.
  "unit.wav": { wave: "square", notes: [[79, 0.1], [83, 0.1], [88, 0.22]] },
  // The board is solved.
  "win.wav": { wave: "square", notes: [[72, 0.14], [76, 0.14], [79, 0.14], [84, 0.5]] },
  // The tries ran out.
  "lose.wav": { wave: "saw", notes: [[64, 0.2], [59, 0.2], [52, 0.55]] },
  // The menu's marker moved.
  "move.wav": { wave: "square", notes: [[88, 0.06]] },
};

/** The tempo the effects were written at. */
export const EFFECT_TEMPO = 120;

/// Every file this writes: its name and the bytes that belong in it.
export function sounds() {
  const out = [["ambient.wav", wav(render(AMBIENT), AMBIENT.rate)]];
  for (const [name, effect] of Object.entries(EFFECTS)) {
    const bytes = wav(
      render({
        tempo: EFFECT_TEMPO,
        rate: EFFECT_RATE,
        level: 0.8,
        voices: [{ wave: effect.wave, level: 0.5, notes: effect.notes }],
      }),
      EFFECT_RATE,
    );
    out.push([name, bytes]);
  }
  return out;
}

// Write only when this file is what was run: importing it is how the audio check
// reads the sounds, and an import must not rewrite the assets.
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  mkdirSync(assets, { recursive: true });
  let total = 0;
  for (const [name, bytes] of sounds()) {
    writeFileSync(join(assets, name), bytes);
    total += bytes.length;
  }
  const seconds = (ARPEGGIO.reduce((sum, [, beats]) => sum + beats, 0) * 60) / tempo;
  console.log(
    `wrote ${Object.keys(EFFECTS).length + 1} sounds to ${assets}: ` +
      `ambient ${seconds.toFixed(1)}s, ${(total / 1024).toFixed(0)} KiB in all`,
  );
}
