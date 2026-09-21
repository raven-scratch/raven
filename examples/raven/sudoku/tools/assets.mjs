// Generates the costumes the board draws with and the font the HUD writes in.
//
//   node examples/raven/sudoku/tools/assets.mjs
//
// Everything here is a shape rather than text: Scratch deletes `<text>` elements
// from an SVG, so a digit and a letter have to be drawn.
//
// The board's digits are seven segments of a 26x26 box, which is exact, tiny and
// reads the same in every renderer. The HUD's font is a 3x5 pixel grid blown up
// by four, one costume per character, and each costume is the exact ink of its
// glyph — 12x20 for every one of them — so a line of text is one stamp per
// character at a fixed pitch, with no measuring.
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
  return `<svg xmlns="http://www.w3.org/2000/svg" width="26" height="26" viewBox="0 0 26 26">${
    `<g stroke="${colour}" stroke-width="3.5" stroke-linecap="round" fill="none">${lines}</g>`
  }</svg>`;
}

/** The outline of the cell the player is on. */
const cursor = `<svg xmlns="http://www.w3.org/2000/svg" width="26" height="26" viewBox="0 0 26 26">${
  `<rect x="1.5" y="1.5" width="23" height="23" rx="3" fill="none" stroke="#1565c0" stroke-width="3"/>`
}</svg>`;

/** The tint under every other cell holding the digit the cursor is on. */
const match = `<svg xmlns="http://www.w3.org/2000/svg" width="26" height="26" viewBox="0 0 26 26">${
  `<rect x="1" y="1" width="24" height="24" rx="3" fill="#4a90d9" fill-opacity="0.22"/>`
}</svg>`;

/** What a sprite parks on between redraws: one transparent pixel. */
const dot = `<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1" viewBox="0 0 1 1">${
  `<rect width="1" height="1" fill="#000000" fill-opacity="0"/>`
}</svg>`;

// ---------------------------------------------------------------------------
// The HUD font
// ---------------------------------------------------------------------------

const PIXEL = 4;
const GLYPH_W = 3 * PIXEL;
const GLYPH_H = 5 * PIXEL;

// 3x5 glyphs, one string of fifteen `0`s and `1`s per character, row by row.
const GLYPHS = {
  " ": "000000000000000",
  A: "111101111101101",
  B: "110101110101110",
  C: "111100100100111",
  D: "110101101101110",
  E: "111100111100111",
  F: "111100111100100",
  G: "111100101101111",
  H: "101101111101101",
  I: "111010010010111",
  J: "001001001101111",
  K: "101101110101101",
  L: "100100100100111",
  M: "101111111101101",
  N: "110101101101101",
  O: "111101101101111",
  P: "111101111100100",
  Q: "111101101111001",
  R: "111101110101101",
  S: "111100111001111",
  T: "111010010010010",
  U: "101101101101111",
  V: "101101101101010",
  W: "101101111111101",
  X: "101101010101101",
  Y: "101101010010010",
  Z: "111001010100111",
  0: "111101101101111",
  1: "010110010010111",
  2: "111001111100111",
  3: "111001111001111",
  4: "101101111001001",
  5: "111100111001111",
  6: "111100111101111",
  7: "111001001001001",
  8: "111101111101111",
  9: "111101111001111",
  "-": "000000111000000",
  ":": "000010000010000",
  "/": "001001010100100",
  ">": "001010100010001",
};

/** One character, as 4x4 blocks of a 3x5 grid. */
function glyph(character) {
  const bits = GLYPHS[character];
  const rects = [];
  for (let r = 0; r < 5; r += 1) {
    for (let c = 0; c < 3; c += 1) {
      if (bits[r * 3 + c] === "1") {
        rects.push(
          `<rect x="${c * PIXEL}" y="${r * PIXEL}" width="${PIXEL}" height="${PIXEL}"/>`,
        );
      }
    }
  }
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${GLYPH_W}" height="${GLYPH_H}" ` +
    `viewBox="0 0 ${GLYPH_W} ${GLYPH_H}"><g fill="#2b2b2b">${rects.join("")}</g></svg>`
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
const backdrop = `<svg xmlns="http://www.w3.org/2000/svg" width="480" height="360" viewBox="0 0 480 360">${
  `<rect width="480" height="360" fill="#f6f4ef"/>`
}</svg>`;

mkdirSync(assets, { recursive: true });
writeFileSync(join(assets, "backdrop.svg"), backdrop);
writeFileSync(join(assets, "dot.svg"), dot);
writeFileSync(join(assets, "cursor.svg"), cursor);
writeFileSync(join(assets, "match.svg"), match);
for (const value of Object.keys(DIGITS)) {
  writeFileSync(join(assets, `given${value}.svg`), digit(value, GIVEN));
  writeFileSync(join(assets, `user${value}.svg`), digit(value, USER));
}
for (const character of ALPHABET) {
  writeFileSync(join(assets, `${glyphName(character)}.svg`), glyph(character));
}

console.log(
  `wrote ${4 + Object.keys(DIGITS).length * 2 + ALPHABET.length} costumes to ${assets}`,
);
void GLYPH_W;
