// Generates the costumes the board draws with and the font the HUD writes in.
//
//   node examples/raven/sudoku/tools/assets.mjs
//
// Everything here is a shape rather than text: Scratch deletes `<text>` elements
// from an SVG, so a digit and a letter have to be drawn.
//
// Both alphabets are drawn as strokes. The board's digits are seven segments of a
// 26x26 box, and the HUD's characters are SVG paths — lines and quadratic curves
// — stroked with round caps, so a letter is a letter and not a grid of squares.
// Every glyph sits in the same box, 6 wide by 10 tall with a unit of air around
// it for the stroke to fill, so a costume is 16x24 and a character advances 19;
// a line of text is one stamp per character at that pitch, with no measuring.
//
// The font costumes are written in the order `Hud.rav` declares its `alphabet`
// list, because the HUD finds a character's costume by counting from the first
// one. The two orders have to match; changing one means changing the other.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const assets = join(root, "assets");

const GIVEN = "#2b2b2b";
const USER = "#1565c0";
const FLASH = "#1565c0";

// ---------------------------------------------------------------------------
// The board
// ---------------------------------------------------------------------------

// The seven segments of a 26x26 cell, as endpoints. `a` is the top bar and they
// run clockwise, with `g` across the middle.
const SEGMENTS = {
  a: [7, 5, 19, 5],
  b: [19, 5, 19, 13],
  c: [19, 13, 19, 21],
  d: [7, 21, 19, 21],
  e: [7, 13, 7, 21],
  f: [7, 5, 7, 13],
  g: [7, 13, 19, 13],
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

/** One digit, as seven segments of a 26x26 costume. */
function digit(value, colour) {
  const lines = [...DIGITS[value]]
    .map((segment) => {
      const [x1, y1, x2, y2] = SEGMENTS[segment];
      return `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"/>`;
    })
    .join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="26" height="26" viewBox="0 0 26 26">${
    `<g stroke="${colour}" stroke-width="3.5" stroke-linecap="round" fill="none">${lines}</g>`
  }</svg>`;
}

/** The outline of the cell the player is on. */
const cursor = `<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="26" height="26" viewBox="0 0 26 26">${
  `<rect x="1.5" y="1.5" width="23" height="23" rx="3" fill="none" stroke="#1565c0" stroke-width="3"/>`
}</svg>`;

/** The tint under every other cell holding the digit the cursor is on. */
const match = `<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="26" height="26" viewBox="0 0 26 26">${
  `<rect x="1" y="1" width="24" height="24" rx="3" fill="#4a90d9" fill-opacity="0.22"/>`
}</svg>`;

/** What a sprite parks on between redraws: one transparent pixel. */
const dot = `<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="1" height="1" viewBox="0 0 1 1">${
  `<rect width="1" height="1" fill="#000000" fill-opacity="0"/>`
}</svg>`;

/// The cell's own size, which is what the flash tile covers.
const flash = `<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="26" height="26" viewBox="0 0 26 26">${
  `<rect x="0.5" y="0.5" width="25" height="25" rx="2" fill="${FLASH}"/>`
}</svg>`;

// ---------------------------------------------------------------------------
// The HUD font
// ---------------------------------------------------------------------------

// Every glyph is drawn inside a 6 by 10 box, and the costume is that box with a
// unit of air all round for the stroke to fill: an 8 by 12 SVG, one character
// per costume, advancing 19 at the sprite's 200%.
//
// The costume's `width`, `height` and `viewBox` are the same box, and that is not
// decoration. Scratch draws a costume at its `width` and `height` and does not
// scale a `viewBox` that disagrees with them, so a costume that asks for one size
// and declares another renders at its own units in the top-left corner of the
// larger box — a glyph at half size in the corner, which is exactly what a
// mismatched costume looks like on the stage. Every costume this file writes is
// 1:1 with its viewBox at the origin, and `check.mjs` fails the build if one is
// not.
const GLYPH_W = 8;
const GLYPH_H = 12;
const STROKE = 1.5;

// The glyph box is 0..6 by 0..10 and the costume has a unit of air round it, so
// every coordinate moves by one. Offsetting both axes by the same amount is what
// lets this add one to every number in the path without reading its commands.
function pad(path) {
  return path.replace(/-?\d+(?:\.\d+)?/g, (number) =>
    String(Number((Number(number) + 1).toFixed(3))),
  );
}

const PATHS = {
  " ": "",
  A: "M0 10 L3 0 L6 10 M1.2 6.6 H4.8",
  B: "M0 10 V0 H3.4 Q5.8 0 5.8 2.5 Q5.8 5 3.4 5 H0 M3.4 5 Q5.8 5 5.8 7.5 Q5.8 10 3.4 10 H0",
  C: "M5.8 2.2 Q5.8 0 3.4 0 Q0 0 0 5 Q0 10 3.4 10 Q5.8 10 5.8 7.8",
  D: "M0 10 V0 H3.2 Q5.8 0 5.8 5 Q5.8 10 3.2 10 H0",
  E: "M5.6 0 H0 V10 H5.6 M0 5 H4.4",
  F: "M5.6 0 H0 V10 M0 5 H4.4",
  G: "M5.8 2.2 Q5.8 0 3.4 0 Q0 0 0 5 Q0 10 3.4 10 Q5.8 10 5.8 7.6 V6.2 H3.4",
  H: "M0 0 V10 M5.6 0 V10 M0 5 H5.6",
  I: "M1.2 0 H4.4 M2.8 0 V10 M1.2 10 H4.4",
  J: "M5.6 0 V7.4 Q5.6 10 3.2 10 Q0 10 0 7.4",
  K: "M0 0 V10 M5.6 0 L0.4 5.4 M2.4 3.6 L5.6 10",
  L: "M0 0 V10 H5.6",
  M: "M0 10 V0 L3 6.2 L6 0 V10",
  N: "M0 10 V0 L5.6 10 V0",
  O: "M3.4 0 Q0 0 0 5 Q0 10 3.4 10 Q5.8 10 5.8 5 Q5.8 0 3.4 0 Z",
  P: "M0 10 V0 H3.4 Q5.8 0 5.8 2.7 Q5.8 5.4 3.4 5.4 H0",
  Q: "M3.4 0 Q0 0 0 5 Q0 10 3.4 10 Q5.8 10 5.8 5 Q5.8 0 3.4 0 Z M3.6 7.4 L6.2 10.6",
  R: "M0 10 V0 H3.4 Q5.8 0 5.8 2.6 Q5.8 5.2 3.4 5.2 H0 M3.4 5.2 L5.8 10",
  S: "M5.8 2.1 Q5.8 0 3.2 0 Q0 0 0 2.6 Q0 5 3.2 5 Q5.8 5 5.8 7.4 Q5.8 10 3.2 10 Q0 10 0 7.9",
  T: "M0 0 H5.6 M2.8 0 V10",
  U: "M0 0 V7 Q0 10 2.8 10 Q5.6 10 5.6 7 V0",
  V: "M0 0 L2.8 10 L5.6 0",
  W: "M0 0 L1.4 10 L2.8 3.6 L4.2 10 L5.6 0",
  X: "M0 0 L5.6 10 M5.6 0 L0 10",
  Y: "M0 0 L2.8 5.6 L5.6 0 M2.8 5.6 V10",
  Z: "M0 0 H5.6 L0 10 H5.6",
  0: "M2.5 0 Q0 0 0 5 Q0 10 2.5 10 Q5 10 5 5 Q5 0 2.5 0 Z",
  1: "M0.6 2.2 L2.4 0 V10 M0 10 H5",
  2: "M0 2 Q0 0 2.5 0 Q5 0 5 2.4 Q5 4.6 0 10 H5",
  3: "M0 1.6 Q0 0 2.5 0 Q5 0 5 2.5 Q5 5 2.5 5 Q5 5 5 7.5 Q5 10 2.5 10 Q0 10 0 8.4",
  4: "M4 10 V0 L0 6.6 H5",
  5: "M5 0 H0 V4.4 Q1 3.9 2.5 3.9 Q5 3.9 5 7 Q5 10 2.5 10 Q0 10 0 8.5",
  6: "M5 1.6 Q5 0 2.5 0 Q0 0 0 5 Q0 10 2.5 10 Q5 10 5 7.2 Q5 4.6 2.5 4.6 Q0 4.6 0 7.2",
  7: "M0 0 H5 L1.6 10",
  8: "M2.5 0 Q0 0 0 2.4 Q0 4.8 2.5 5 Q5 5 5 7.5 Q5 10 2.5 10 Q0 10 0 7.5 Q0 5 2.5 5 Q5 4.8 5 2.4 Q5 0 2.5 0 Z",
  9: "M0 8.4 Q0 10 2.5 10 Q5 10 5 5 Q5 0 2.5 0 Q0 0 0 2.8 Q0 5.4 2.5 5.4 Q5 5.4 5 2.8",
  "-": "M1 5 H5",
  ":": "M2.5 3 L2.5 3.02 M2.5 7 L2.5 7.02",
  "/": "M5 0 L1 10",
  ">": "M1 0 L5 5 L1 10",
};

/** One character, as a stroked path in the shared 6x10 box. */
function glyph(character) {
  const path = PATHS[character];
  // The whole box is inked, at an opacity no one can see, before the letter is
  // drawn on it. A costume's rotation centre is the middle of its box, and a
  // letter that is narrower than its box would otherwise be centred on its own
  // ink and land a shade off the pitch. The same wash is what gives the space
  // something to be — a glyph with no ink at all has no box to measure.
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="${GLYPH_W}" ` +
    `height="${GLYPH_H}" viewBox="0 0 ${GLYPH_W} ${GLYPH_H}">` +
    `<rect width="${GLYPH_W}" height="${GLYPH_H}" fill="#2b2b2b" fill-opacity="0.004"/>` +
    `<path d="${pad(path)}" fill="none" stroke="#2b2b2b" ` +
    `stroke-width="${STROKE}" stroke-linecap="round" stroke-linejoin="round"/></svg>`
  );
}

/** The costume name for a character: `f` plus the character, or its name. */
function glyphName(character) {
  if (character === " ") return "fspace";
  if (character === "-") return "fdash";
  if (character === ":") return "fcolon";
  if (character === "/") return "fslash";
  if (character === ">") return "fgt";
  return `f${character}`;
}

// The order `Hud.rav` lists its alphabet in.
const ALPHABET = [
  " ",
  ..."ABCDEFGHIJKLMNOPQRSTUVWXYZ",
  ..."0123456789",
  "-",
  ":",
  "/",
  ">",
];

/** The backdrop: a quiet field, so the pen layer is the only contrast. */
const backdrop = `<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="480" height="360" viewBox="0 0 480 360">${
  `<rect width="480" height="360" fill="#f6f4ef"/>`
}</svg>`;

mkdirSync(assets, { recursive: true });
writeFileSync(join(assets, "backdrop.svg"), backdrop);
writeFileSync(join(assets, "dot.svg"), dot);
writeFileSync(join(assets, "cursor.svg"), cursor);
writeFileSync(join(assets, "match.svg"), match);
writeFileSync(join(assets, "flash.svg"), flash);
for (const value of Object.keys(DIGITS)) {
  writeFileSync(join(assets, `given${value}.svg`), digit(value, GIVEN));
  writeFileSync(join(assets, `user${value}.svg`), digit(value, USER));
}
for (const character of ALPHABET) {
  writeFileSync(join(assets, `${glyphName(character)}.svg`), glyph(character));
}

console.log(
  `wrote ${5 + Object.keys(DIGITS).length * 2 + ALPHABET.length} costumes to ${assets}`,
);
