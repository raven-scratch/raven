// Generates the costumes the board and the menu draw with.
//
//   node examples/raven/sudoku/tools/assets.mjs
//
// Everything here is a shape rather than text: Scratch deletes `<text>`
// elements from an SVG, so a digit has to be drawn. A digit is seven segments
// of a 28x28 box, which is exact, tiny, and reads the same in every renderer —
// no font is involved, so nothing depends on what the machine has installed.
//
// A digit is written twice, in the two faces the board needs: `given` for a
// value the puzzle started with, `user` for one the player entered. The colour
// is the only difference, and it is baked in here rather than set at run time
// because a costume's colours are part of the costume.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const assets = join(root, "assets");

const GIVEN = "#2b2b2b";
const USER = "#1565c0";

// The seven segments of the box, as endpoints. `a` is the top bar and they run
// clockwise, with `g` across the middle.
const SEGMENTS = {
  a: [6, 4, 22, 4],
  b: [22, 4, 22, 14],
  c: [22, 14, 22, 24],
  d: [6, 24, 22, 24],
  e: [6, 14, 6, 24],
  f: [6, 4, 6, 14],
  g: [6, 14, 22, 14],
};

// Which segments each digit lights.
const DIGITS = {
  0: "abcdef",
  1: "bc",
  2: "abged",
  3: "abgcd",
  4: "fgbc",
  5: "afgcd",
  6: "afgecd",
  7: "abc",
  8: "abcdefg",
  9: "abcdfg",
};

/** One digit, as seven segments of a 28x28 costume. */
function digit(value, colour) {
  const lines = [...DIGITS[value]]
    .map((segment) => {
      const [x1, y1, x2, y2] = SEGMENTS[segment];
      return `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`;
    })
    .join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 28 28">${
    `<g stroke="${colour}" stroke-width="4" stroke-linecap="round" fill="none">${lines}</g>`
  }</svg>`;
}

/** The backdrop: a quiet field, so the pen layer is the only contrast. */
const backdrop = `<svg xmlns="http://www.w3.org/2000/svg" width="480" height="360" viewBox="0 0 480 360">${
  `<rect width="480" height="360" fill="#f6f4ef"/>`
}</svg>`;

/** The selection highlight, stamped under the digits of the current cell. */
const cursor = `<svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 28 28">${
  `<rect x="1" y="1" width="26" height="26" rx="3" fill="#4a90d9" fill-opacity="0.28" ` +
  `stroke="#4a90d9" stroke-width="2"/>`
}</svg>`;

/** What the sprite parks on between redraws: one transparent pixel. */
const dot = `<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1" viewBox="0 0 1 1">${
  `<rect width="1" height="1" fill="#000000" fill-opacity="0"/>`
}</svg>`;

mkdirSync(assets, { recursive: true });
writeFileSync(join(assets, "backdrop.svg"), backdrop);
writeFileSync(join(assets, "cursor.svg"), cursor);
writeFileSync(join(assets, "dot.svg"), dot);
for (const value of Object.keys(DIGITS)) {
  writeFileSync(join(assets, `given${value}.svg`), digit(value, GIVEN));
  writeFileSync(join(assets, `user${value}.svg`), digit(value, USER));
}

const written = 3 + Object.keys(DIGITS).length * 2;
console.log(`wrote ${written} costumes to ${assets}`);
