// Generates the glyphs the text example is made of.
//
//   node tools/font.mjs
//
// It writes three things, all from the art below:
//
//   assets/font/<code>.svg          one costume per printable ASCII character
//   src/lib/font.rav                the glyph table, its widths, and the stamp
//   src/sprites/typewriter.rav      the costume block, between two markers
//
// The art is 5 columns wide and 8 rows tall, written as rows of `#` and `.`:
// row 0 is the top of a capital, row 5 is the baseline, and rows 6 and 7 are
// the descenders. `+N:` pads N blank rows above the art, and blank rows at the
// bottom are implied. Columns are trimmed to the ink, so the font is
// proportional — `i` is one unit wide and `m` is five.

import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

/// Cell height in font units. The baseline is row 6, which the art below says
/// with blank and non-blank rows rather than with a number.
const CELL_H = 8;

/// The ink colour. A saturated one, because the sprite's colour effect is what
/// tints a glyph, and a shift of grey is still grey.
const INK = "#ff3b30";

const ART = {
  " ": null,
  "!": "#/#/#/./#",
  '"': "#.#/#.#",
  "#": ".#.#./#####/.#.#./#####/.#.#.",
  $: "..#../..###/.#.../.###./...#./###../..#..",
  "%": "#...#/#..#./..#../.#.../#..##/#...#",
  "&": ".##../#..#./.##../#.#.#/#..#./.##.#",
  "'": "#/#",
  "(": ".#/#./#./#./#./.#",
  ")": "#./.#/.#/.#/.#/#.",
  "*": "+1:#.#/.#./#.#",
  "+": "+1:..#../..#../#####/..#../..#..",
  ",": "+5:#./#./.#",
  "-": "+3:###",
  ".": "+5:#",
  "/": "..#/..#/.#./.#./#../#..",
  0: ".###./#...#/#..##/#.#.#/##..#/.###.",
  1: "..#../.##../..#../..#../..#../.###.",
  2: ".###./#...#/...#./..#../.#.../#####",
  3: "#####/...#./..##./....#/#...#/.###.",
  4: "...#./..##./.#.#./#..#./#####/...#.",
  5: "#####/#..../####./....#/#...#/.###.",
  6: "..##./.#.../####./#...#/#...#/.###.",
  7: "#####/....#/...#./..#../..#../..#..",
  8: ".###./#...#/.###./#...#/#...#/.###.",
  9: ".###./#...#/#...#/.####/...#./.##..",
  ":": "+2:#/././#",
  ";": "+2:#/././#/.#",
  "<": "+1:..#/.#./#../.#./..#",
  "=": "+2:#####/...../#####",
  ">": "+1:#../.#./..#/.#./#..",
  "?": ".###./#...#/...#./..#../...../..#..",
  "@": ".###./#...#/#.###/#.#.#/#..../.###.",
  A: ".###./#...#/#...#/#####/#...#/#...#",
  B: "####./#...#/####./#...#/#...#/####.",
  C: ".###./#...#/#..../#..../#...#/.###.",
  D: "####./#...#/#...#/#...#/#...#/####.",
  E: "#####/#..../####./#..../#..../#####",
  F: "#####/#..../####./#..../#..../#....",
  G: ".###./#...#/#..../#.###/#...#/.###.",
  H: "#...#/#...#/#####/#...#/#...#/#...#",
  I: "#####/..#../..#../..#../..#../#####",
  J: "..###/...#./...#./...#./#..#./.##..",
  K: "#...#/#..#./###../#..#./#..#./#...#",
  L: "#..../#..../#..../#..../#..../#####",
  M: "#...#/#####/#####/#...#/#...#/#...#",
  N: "#...#/##..#/#.#.#/#..##/#...#/#...#",
  O: ".###./#...#/#...#/#...#/#...#/.###.",
  P: "####./#...#/####./#..../#..../#....",
  Q: ".###./#...#/#...#/#...#/#..#./.##.#",
  R: "####./#...#/####./#..#./#..#./#...#",
  S: ".####/#..../.###./....#/....#/####.",
  T: "#####/..#../..#../..#../..#../..#..",
  U: "#...#/#...#/#...#/#...#/#...#/.###.",
  V: "#...#/#...#/#...#/#...#/.#.#./..#..",
  W: "#...#/#...#/#...#/#####/#####/#...#",
  X: "#...#/#...#/.#.#./.#.#./#...#/#...#",
  Y: "#...#/#...#/.#.#./..#../..#../..#..",
  Z: "#####/....#/...#./..#../.#.../#####",
  "[": "##/#./#./#./#./##",
  "\\": "#../#../.#./.#./..#/..#",
  "]": "##/.#/.#/.#/.#/##",
  "^": "+1:.#./#.#",
  _: "+7:#####",
  "`": "#./.#",
  a: "+2:.##./#..#/#..#/.###",
  b: "#.../#.../####/#..#/#..#/####",
  c: "+2:.##./#.../#.../.###",
  d: "...#/...#/####/#..#/#..#/####",
  e: "+2:.##./#..#/####/.###",
  f: ".##/#../###/#../#../#..",
  g: "+2:.###/#..#/.###/...#/.##.",
  h: "#.../#.../####/#..#/#..#/#..#",
  i: "#/./#/#/#/#",
  j: ".#/../.#/.#/.#/.#/#.",
  k: "#.../#.../#..#/#.#./##../#..#",
  l: "#/#/#/#/#/#",
  m: "+2:##.##/#.#.#/#.#.#/#.#.#",
  n: "+2:##./#.#/#.#/#.#",
  o: "+2:.##./#..#/#..#/.##.",
  p: "+2:##./#.#/#.#/##./#../#..",
  q: "+2:.###/#..#/#..#/.###/...#/...#",
  r: "+2:#.#/##./#../#..",
  s: "+2:.##/#../.#./##.",
  t: ".#/.#/###/.#/.#/.#",
  u: "+2:#.#/#.#/#.#/.##",
  v: "+2:#.#/#.#/#.#/.#.",
  w: "+2:#...#/#...#/#.#.#/.#.#.",
  x: "+2:#.#/.#./.#./#.#",
  y: "+2:#.#/#.#/#.#/.##/.#./#..",
  z: "+2:##./.#./#../##.",
  "{": ".##/.#./##./.#./.##",
  "|": "#/#/#/#/#/#/#",
  "}": "##./.#./.##/.#./##.",
  "~": "+3:.##.#/#..##",
};

/// Which characters the match chain tests first. English letter frequency, so
/// the chain a common character walks is short.
const FREQUENT =
  " etaoinshrdlucmfwypvbgkjqxz" +
  "ABCDEFGHIJKLMNOPQRSTUVWXYZ" +
  "0123456789" +
  ".,'\"-:;!?/()+=<>[]{}#$%&*@\\^_`|~";

/// Every printable character, frequent ones first and `?` last, because the
/// last entry is what the match reaches for anything the font does not have.
function order() {
  const chars = [...FREQUENT].filter((char) => char !== "?");
  const all = [];
  for (let code = 32; code <= 126; code += 1) {
    all.push(String.fromCharCode(code));
  }
  for (const char of all) {
    if (!chars.includes(char) && char !== "?") {
      chars.push(char);
    }
  }
  chars.push("?");
  const missing = all.filter((char) => !chars.includes(char));
  if (missing.length > 0) {
    throw new Error(`the order is missing ${JSON.stringify(missing)}`);
  }
  return chars;
}

/// A glyph's rows, padded to the cell. `+N:` adds N blank rows on top.
function rows(art) {
  if (art === null) return [];
  const [, pad = "0", body] = /^(?:\+(\d+):)?(.*)$/s.exec(art);
  const out = [];
  for (let i = 0; i < Number(pad); i += 1) out.push("");
  for (const row of body.split("/")) {
    if (row !== "") out.push(row);
  }
  while (out.length < CELL_H) out.push("");
  return out;
}

/// A glyph's ink: its width in units, and one horizontal run per row of `#`.
function ink(char) {
  const grid = rows(ART[char]);
  const width = Math.max(1, ...grid.map((row) => row.length));
  const runs = [];
  for (let y = 0; y < grid.length; y += 1) {
    let x = 0;
    while (x < grid[y].length) {
      if (grid[y][x] !== "#") {
        x += 1;
        continue;
      }
      let end = x;
      while (end + 1 < grid[y].length && grid[y][end + 1] === "#") end += 1;
      runs.push({ x, y, w: end - x + 1 });
      x = end + 1;
    }
  }
  // A blank glyph — the space — has no ink to trim to, so it keeps its width.
  if (runs.length === 0) return { width: 3, runs: [] };
  const first = Math.min(...runs.map((run) => run.x));
  const last = Math.max(...runs.map((run) => run.x + run.w));
  return {
    width: last - first,
    runs: runs.map((run) => ({ ...run, x: run.x - first })),
  };
}

function svg({ width, runs }) {
  const rects = runs
    .map((run) => `  <rect x="${run.x}" y="${run.y}" width="${run.w}" height="1"/>`)
    .join("\n");
  return `<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="${width}" height="${CELL_H}" viewBox="0 0 ${width} ${CELL_H}">
  <g fill="${INK}">
${rects}
  </g>
</svg>
`;
}

/// The costume name for a character: its code, so the name of every glyph is
/// the same shape and the variant is derivable.
function costume(char) {
  return `g${char.charCodeAt(0)}`;
}

function variant(char) {
  return `G${char.charCodeAt(0)}`;
}

/// Replace what is between two markers in a file, and leave the rest alone.
function splice(path, start, end, lines) {
  const source = readFileSync(path, "utf8");
  const from = source.indexOf(start);
  const to = source.indexOf(end);
  if (from < 0 || to < 0) throw new Error(`${path} has no ${start} … ${end} block`);
  const head = source.slice(0, from + start.length);
  const tail = source.slice(to);
  writeFileSync(path, `${head}\n${lines.join("\n")}\n    ${tail}`);
}

/// The art as text, so a change to a glyph can be looked at without a Scratch
/// project: `node tools/font.mjs "Hello, raven!"`.
function preview(text) {
  const lines = Array.from({ length: CELL_H }, () => "");
  for (const char of text) {
    const { width, runs } = ink(char);
    const grid = Array.from({ length: CELL_H }, () => Array(width).fill(" "));
    for (const run of runs) {
      for (let x = 0; x < run.w; x += 1) grid[run.y][run.x + x] = "#";
    }
    for (let y = 0; y < CELL_H; y += 1) lines[y] += grid[y].join("") + " ";
  }
  console.log(lines.join("\n"));
}

function main() {
  if (process.argv[2] !== undefined) {
    preview(process.argv[2]);
    return;
  }
  const chars = order();
  const glyphs = chars.map((char) => ({ char, ...ink(char) }));

  const assets = join(ROOT, "assets/font");
  rmSync(assets, { recursive: true, force: true });
  mkdirSync(assets, { recursive: true });
  let bytes = 0;
  for (const glyph of glyphs) {
    const text = svg(glyph);
    bytes += Buffer.byteLength(text);
    writeFileSync(join(assets, `${glyph.char.charCodeAt(0)}.svg`), text);
  }

  const arms = glyphs
    .map((glyph, index) => {
      const arm = `        ${index + 1} => { looks::switch_costume_to(Costume::${variant(glyph.char)}); },`;
      return index === glyphs.length - 1
        ? `        _ => { looks::switch_costume_to(Costume::${variant(glyph.char)}); }`
        : arm;
    })
    .join("\n");

  const quoted = glyphs.map((glyph) => JSON.stringify(glyph.char)).join(", ");
  const widths = glyphs.map((glyph) => glyph.width).join(", ");

  writeFileSync(
    join(ROOT, "src/lib/font.rav"),
    `// Generated by tools/font.mjs — do not edit.
//
// The face: the printable ASCII characters in match order, the ink width of
// each in font units, and the stamp that draws one. A costume is as wide as its
// ink and ${CELL_H} units tall, so its rotation centre is the middle of the cell
// and every glyph sits on the same baseline. Widths are ink, not advance: the
// engine adds the tracking between two glyphs, and the cell height means the
// engine knows where the baseline is without asking the costume.

/// The characters, in the order the match below tests them.
pub var glyph_chars: list<str> = [${quoted}];

/// The ink width of each, in font units.
pub var glyph_wide: list<num> = [${widths}];

/// One glyph, centred on (cx, cy). Index 0, or any character the font does not
/// have, draws \`?\`.
pub proc glyph(i: num, cx: num, cy: num) {
    motion::go_to_xy(cx, cy);
    match i {
${arms}
    }
    pen::stamp();
}
`,
  );

  splice(
    join(ROOT, "src/sprites/typewriter.rav"),
    "// font:costumes",
    "// font:end",
    glyphs.map(
      (glyph) => `    costume "${costume(glyph.char)}" = "assets/font/${glyph.char.charCodeAt(0)}.svg";`,
    ),
  );

  const widest = Math.max(...glyphs.map((glyph) => glyph.width));
  console.log(
    `font: ${glyphs.length} glyphs, ${widest} units at the widest, ` +
      `${(bytes / 1024).toFixed(1)} KiB of SVG`,
  );
}

main();
