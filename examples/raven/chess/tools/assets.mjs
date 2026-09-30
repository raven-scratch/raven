// Generates every costume the chess stage draws with.
//
//   node examples/raven/chess/tools/assets.mjs
//
// The pieces are the lichess sets, copied into `tools/piecesets/`. Each piece is
// fitted into one square of the board and then *rasterised*: what the project
// carries is a 96 by 96 PNG per piece per set, not the SVG it was drawn from.
//
// The reason is that every consumer of an SVG costume is a different parser with
// its own idea of what an SVG is. Scratch hands the file to the browser as an
// image, after sanitizing it with a profile that quietly drops filter
// primitives, `use` and `foreignObject` — a shape drawn through the empty filter
// that leaves behind is drawn as nothing. The editor imports it with Paper.js,
// which crashes on a gradient that inherits from one defined later in the file
// (`Cannot read properties of undefined (reading 'getGradient')`) and refuses a
// gradient with a single stop. TurboWarp's renderer has its own path again. A
// PNG has one reader, and it is the same one everywhere.
//
// 96 pixels is four times the area a piece is drawn at on the board (a square is
// 36 units), so the picture is supersampled where it matters; at 512 it would be
// a 14 to 1 downscale, which aliases, and ten times the bytes for nothing.
//
// The browser that does the rasterising is playwright's chromium, the same one
// `tools/pieces.mjs` renders the board's proof sheets with.
//
// Every other costume is written 1:1 with its own viewBox, at the origin,
// because Scratch draws a costume at its `width` and `height` and does not
// rescale a viewBox that disagrees with them.
//
// The HUD's text is not here and is not a costume at all: it is drawn with the
// pen, from the glyph table `lib/penfont`'s generator writes into
// `src/penfont/`.

import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const assets = join(root, "assets");
const piecesets = join(root, "tools", "piecesets");

// ---------------------------------------------------------------------------
// The board
// ---------------------------------------------------------------------------

// One square of the board. The board is 8 squares of 36; a costume is 36 by 36,
// so a stamp at size 100 covers exactly one square, and that 36 leaves the twelve
// units of file letters and the twenty-four of rank digits the board's
// coordinates are drawn in. The piece costumes are the exception: they are 96
// pixel PNGs, so a piece fills a square at size 37.5 (`PIECE_FILL` in
// `src/layout.rav`).
const SQUARE = 36;
const LIGHT = "#f0d9b5";
const DARK = "#b58863";

/// The board's palettes, in the order the settings page offers them. The first
/// is the one the example is drawn for; the rest are the usual alternates.
const BOARDS = [
  ["#f0d9b5", "#b58863"],
  ["#eeeed2", "#769656"],
  ["#dee3e6", "#8ca2ad"],
  ["#e2e2e2", "#7d7d7d"],
];

const PIECES = ["wK", "wQ", "wR", "wB", "wN", "wP", "bK", "bQ", "bR", "bB", "bN", "bP"];

function svg(width, height, body, viewBox = `0 0 ${width} ${height}`) {
  // A body lifted out of a lichess file carries `xlink:href`, and the namespace
  // it uses is declared on the file it came from rather than on this one. An
  // SVG that uses a prefix it has not declared is not well-formed XML, and
  // Scratch draws a costume by handing the file to the browser as an image,
  // which refuses it and draws nothing: eleven of the thirty-nine piece sets
  // are invisible without this declaration.
  const xlink = body.includes("xlink:")
    ? ' xmlns:xlink="http://www.w3.org/1999/xlink"'
    : "";
  return (
    `<svg xmlns="http://www.w3.org/2000/svg"${xlink} version="1.1" width="${width}" ` +
    `height="${height}" viewBox="${viewBox}">${body}</svg>`
  );
}

const square = (fill) =>
  svg(SQUARE, SQUARE, `<rect width="${SQUARE}" height="${SQUARE}" fill="${fill}"/>`);

/** The tint under the two squares the last move touched, and under the one the player picked. */
const tint = (colour, opacity) =>
  svg(SQUARE, SQUARE, `<rect width="${SQUARE}" height="${SQUARE}" fill="${colour}" fill-opacity="${opacity}"/>`);

/** The square the king is on when it is in check. */
const check = svg(
  SQUARE,
  SQUARE,
  `<defs><radialGradient id="g"><stop offset="0" stop-color="#ff2b2b" stop-opacity="0.95"/>` +
    `<stop offset="1" stop-color="#ff2b2b" stop-opacity="0"/></radialGradient></defs>` +
    `<rect width="${SQUARE}" height="${SQUARE}" fill="url(#g)"/>`,
);

/** A quiet destination: a dot in the middle of the square. */
const dot = svg(
  SQUARE,
  SQUARE,
  `<circle cx="${SQUARE / 2}" cy="${SQUARE / 2}" r="7" fill="#2b2b2b" fill-opacity="0.24"/>`,
);

/** A destination that takes something: a ring round the square. */
const ring = svg(
  SQUARE,
  SQUARE,
  `<circle cx="${SQUARE / 2}" cy="${SQUARE / 2}" r="${SQUARE / 2 - 3}" fill="none" ` +
    `stroke="#2b2b2b" stroke-opacity="0.24" stroke-width="5"/>`,
);

/** The square the pointer is over. */
const hover = svg(
  SQUARE,
  SQUARE,
  `<rect width="${SQUARE}" height="${SQUARE}" fill="#ffffff" fill-opacity="0.14"/>`,
);

/** The four promotion choices, behind a rounded slab so they read as one thing. */
const promo = svg(
  SQUARE + 8,
  SQUARE * 4 + 8,
  `<rect x="0.5" y="0.5" width="${SQUARE + 7}" height="${SQUARE * 4 + 7}" rx="6" ` +
    `fill="#26292e" stroke="#3a4046" stroke-width="1"/>`,
);

/** What the sprite parks on between redraws: one transparent pixel. */
const blank = svg(1, 1, `<rect width="1" height="1" fill="#000000" fill-opacity="0"/>`);

/** The dark field behind the menu and the end of a game. */
const veil = svg(480, 360, `<rect width="480" height="360" fill="#1b1d20" fill-opacity="0.82"/>`);

/** The stage the pen draws on. */
const backdrop = svg(480, 360, `<rect width="480" height="360" fill="#1b1d20"/>`);

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

const PANEL = "#26292e";
const EDGE = "#3a4046";
const GOLD = "#c8a24a";
const PANEL_W = 150;

const panel = svg(
  PANEL_W,
  348,
  `<rect x="0" y="0" width="${PANEL_W}" height="348" rx="8" fill="${PANEL}" ` +
    `stroke="${EDGE}" stroke-width="1"/>`,
);

/** One player's card at an end of the panel. */
const seat = svg(
  142,
  30,
  `<rect x="0.5" y="0.5" width="141" height="29" rx="6" fill="#22252a" ` +
    `stroke="${EDGE}" stroke-width="1"/>`,
);

/** Whose turn it is. */
const turnDot = svg(12, 12, `<circle cx="6" cy="6" r="4.5" fill="${GOLD}"/>`);

/** The four pixels of frame the board sits in. */
const frame = svg(
  SQUARE * 8 + 8,
  SQUARE * 8 + 8,
  `<rect x="2" y="2" width="${SQUARE * 8 + 4}" height="${SQUARE * 8 + 4}" rx="4" ` +
    `fill="none" stroke="${EDGE}" stroke-width="4"/>`,
);

const button = (fill) =>
  svg(60, 22, `<rect x="0.5" y="0.5" width="59" height="21" rx="5" fill="${fill}" ` +
    `stroke="${EDGE}" stroke-width="1"/>`);

/** A hairline rule across the panel. */
const rule = svg(134, 2, `<rect width="134" height="2" fill="${EDGE}"/>`);

/** The panel's own colour, cut to a line: what a live row is erased with. */
const patch = svg(140, 16, `<rect width="140" height="16" fill="${PANEL}"/>`);

/** The play button on the menu. */
const wide = svg(
  200,
  34,
  `<rect x="1" y="1" width="198" height="32" rx="7" fill="#33373d" ` +
    `stroke="${GOLD}" stroke-width="2"/>`,
);

/** One seat row on the menu: which side it is, and its ten seats. */
const menuSeat = svg(
  440,
  38,
  `<rect x="0" y="0" width="440" height="38" rx="8" fill="#22252a" ` +
    `stroke="${EDGE}" stroke-width="1"/>`,
);

/** The evaluation bar: a track, and the cell the white share of it is made of. */
const track = svg(
  134,
  10,
  `<rect width="134" height="10" rx="4" fill="#1b1d20" stroke="${EDGE}" stroke-width="1"/>`,
);
const cell = svg(5, 10, `<rect width="5" height="10" fill="#c8d3dd"/>`);

mkdirSync(assets, { recursive: true });
// This file owns every `p<set><piece>` costume, and an earlier scheme named them
// `p<set><piece>.svg`: a piece left behind by one would answer for a costume whose
// own file failed to be written.
for (const stale of readdirSync(assets).filter((f) => /^p\d+(w|b)[A-Z]\.(svg|png)$/.test(f))) {
  rmSync(join(assets, stale));
}

const written = [];

function write(name, body) {
  writeFileSync(join(assets, `${name}.svg`), body);
  written.push(name);
}

write("sq-light", square(LIGHT));
write("sq-dark", square(DARK));
for (let i = 0; i < BOARDS.length; i += 1) {
  write(`bs${i}l`, square(BOARDS[i][0]));
  write(`bs${i}d`, square(BOARDS[i][1]));
}
write("cell", svg(40, 40, `<rect x="0.5" y="0.5" width="39" height="39" rx="7" fill="#22252a" stroke="${EDGE}" stroke-width="1"/>`));
write("cell-on", svg(40, 40, `<rect x="1" y="1" width="38" height="38" rx="7" fill="#2f3a45" stroke="${GOLD}" stroke-width="2"/>`));
write("hl-last", tint("#f7e26b", "0.45"));
write("hl-sel", tint("#f7e26b", "0.72"));
write("hl-check", check);
write("dot", dot);
write("ring", ring);
write("blank", blank);
write("veil", veil);
write("backdrop", backdrop);
write("panel", panel);
write("seat", seat);
write("turn-dot", turnDot);
write("patch", patch);
write("frame", frame);
write("btn", button("#33373d"));
write("rule", rule);
write("hl-hover", hover);
write("promo-box", promo);
write("bar-track", track);
write("bar-cell", cell);
write("menu-seat", menuSeat);
write("btn-wide", wide);

// ---------------------------------------------------------------------------
// The piece sets
// ---------------------------------------------------------------------------

// Every set lichess draws with, one costume per piece per set, fitted into one
// square of the board. A set's pieces do not have to agree about their canvas —
// nine of them do not — so each piece is fitted by its own viewBox and centred,
// which is what the browser does when lichess shows them at one size.
//
// The costumes are named `p<set><piece>`, and the only way to turn a theme and
// a piece into one is a matching arm each: a costume's name is a literal in the
// language. That match is generated into `src/sprites/pieces.rav`, and the
// costume lists of both sprites are generated into them between two markers,
// because there are 468 of them and they change with this file.


const PIECE_FILES = ["wP", "wN", "wB", "wR", "wQ", "wK", "bP", "bN", "bB", "bR", "bQ", "bK"];
const sets = JSON.parse(readFileSync(join(piecesets, "sets.json"), "utf8")).sets;

/**
 * One piece, fitted into a square of the board, as the SVG the rasteriser draws.
 *
 * A set's files do not agree about their canvas — nine of them do not — so the
 * fit is by the file's own `viewBox`, centred, which is what lichess does when
 * it shows them at one size. The fit is a `transform` rather than a viewBox
 * because the drawing is then 1:1 with the square, and the rasteriser scales it
 * up from there.
 */
function setPiece(set, name) {
  const source = readFileSync(join(piecesets, set, `${name}.svg`), "utf8");
  const body = source.slice(source.indexOf(">") + 1, source.lastIndexOf("</svg>"));
  let x = 0;
  let y = 0;
  let w = SQUARE;
  let h = SQUARE;
  const box = source.match(/viewBox="([^"]+)"/);
  const dimensions = source.match(/width="([\d.]+)"\s+height="([\d.]+)"/);
  if (box) {
    const v = box[1].trim().split(/[\s,]+/).map(Number);
    if (v.length === 4 && v[2] > 0 && v[3] > 0) [x, y, w, h] = v;
  } else if (dimensions) {
    w = Number(dimensions[1]);
    h = Number(dimensions[2]);
  }
  const scale = SQUARE / Math.max(w, h);
  const tx = -x * scale + (SQUARE - w * scale) / 2;
  const ty = -y * scale + (SQUARE - h * scale) / 2;
  return svg(
    SQUARE,
    SQUARE,
    `<g transform="translate(${tx.toFixed(4)} ${ty.toFixed(4)}) scale(${scale.toFixed(6)})">` +
      `${body}</g>`,
  );
}

/**
 * Every size a piece costume is baked at, and the letter its name starts with.
 *
 * These are the sizes the project stamps a piece at, worked out from the layout
 * and written down in `src/layout.rav`: a piece filling a square of the board
 * (36 stage units), the one the pointer has hold of, a shade larger (40), one
 * icon in the panel's row of what a side has taken (24), and one knight in a
 * cell of the settings grid (22). A costume is its own size in stage units at
 * size 100, so every stamp of a piece is at 100 percent and nothing is ever
 * scaled.
 */
const PIECE_SIZES = [
  ["p", 36],
  ["d", 40],
  ["c", 24],
  ["s", 22],
];

/** Every piece costume: its name, the SVG behind it, and the pixels it is drawn at. */
const pieceCostumes = [];
sets.forEach((set, i) => {
  PIECE_FILES.forEach((name) => {
    const svg = setPiece(set.name, name);
    for (const [letter, px] of PIECE_SIZES) {
      pieceCostumes.push({ name: `${letter}${i}${name}`, svg, px, letter });
    }
  });
});

await rasterizePieces();

// ---------------------------------------------------------------------------
// The rasteriser
// ---------------------------------------------------------------------------

/**
 * A browser, for turning the pieces into PNGs.
 *
 * Playwright is not a dependency of the repository: this is looked for where a
 * checkout of `scratch-editor` keeps it, and reported as an error rather than
 * half-done if it is not there, because a run without it would leave the costume
 * lists pointing at files it did not write.
 */
async function openBrowser() {
  const require = createRequire(import.meta.url);
  const candidates = [
    "playwright",
    join(root, "..", "..", "..", "ref", "scratch-editor", "node_modules", "playwright", "index.mjs"),
  ];
  let chromium = null;
  for (const candidate of candidates) {
    try {
      const module = await import(candidate.startsWith("/") || candidate.includes(":")
        ? `file://${candidate.replace(/\\/g, "/")}`
        : candidate);
      chromium = (module.chromium || (module.default && module.default.chromium)) ?? null;
    } catch {
      chromium = null;
    }
    if (chromium) break;
  }
  if (!chromium) {
    throw new Error(
      "no playwright: install it (`npm install -g playwright && npx playwright install chromium`),\n" +
      "or run this from a checkout that has `ref/scratch-editor` in it",
    );
  }
  const exe = chromium.executablePath();
  const attempts = [
    exe && existsSync(exe) ? {executablePath: exe} : {},
    {channel: "msedge"},
    {channel: "chrome"},
  ];
  for (const options of attempts) {
    try {
      return await chromium.launch(options);
    } catch {
      // The next one: a checkout of the browser playwright wants is not always
      // there, and an installed Edge or Chrome draws the same SVG.
    }
  }
  throw new Error(
    "playwright is here but no browser it can start is: `npx playwright install chromium`",
  );
}

/**
 * Every piece costume, drawn at its own size and written where the project reads
 * it.
 *
 * One page per size, so that the images of one size share a viewport, and each
 * piece an image of its own rather than an SVG inlined into the page: two
 * lichess files both call their gradient `a`, and a document that holds them at
 * once gives every `fill="url(#a)"` the first one, which paints the wrong
 * colour. An image is a document, so an image is what the costume is.
 */
async function rasterizePieces() {
  const browser = await openBrowser();
  for (const [, px] of PIECE_SIZES) {
    const of = pieceCostumes.filter((p) => p.px === px);
    const page = await browser.newPage({viewport: {width: px * 16, height: px * 8}});
    const grid = of
      .map((p) =>
        `<img id="d${p.name}" src="data:image/svg+xml;base64,${Buffer.from(p.svg).toString("base64")}">`)
      .join("");
    await page.setContent(
      `<!doctype html><meta charset="utf-8"><body style="margin:0;background:transparent">` +
      `<style>img{width:${px}px;height:${px}px;display:block}</style>${grid}</body>`,
    );
    await page.waitForTimeout(300);
    for (const p of of) {
      const png = await page.locator(`#d${p.name}`).screenshot({omitBackground: true});
      writeFileSync(join(assets, `${p.name}.png`), png);
      written.push(`${p.name}.png`);
    }
    await page.close();
  }
  await browser.close();
}

// `src/sprites/theme.rav`: the count, which only this file knows.
writeFileSync(
  join(root, "src", "sprites", "theme.rav"),
  "// How many piece sets this project carries. Generated by tools/assets.mjs,\n" +
    "// which is the only thing that knows: the sets are whatever\n" +
    "// tools/piecesets/sets.json lists, in that order, and set 0 is cburnett.\n" +
    "\n" +
    `pub const SET_N: num = ${sets.length};\n\n` +
    "/// The name of each set, in the order the themes number them.\n" +
    "pub var setname: list<str> = [" +
    sets.map((s) => `"${s.name}"`).join(",") +
    "];\n",
);

// `src/sprites/pieces.rav`: the match, one arm per set, piece and size.
//
// The arms are nested rather than flat. A single match with a thousand of them
// overflows the compiler's stack while it lowers the chain of tests, so a set is
// one procedure, its sizes are the four arms of a match inside it, and a size's
// twelve pieces are the arms of the match inside that.
{
  const lines = [
    "// Which costume a piece of a set is drawn with. Generated by tools/assets.mjs.",
    "//",
    "// A costume's name is a literal in raven, so the costume of a theme, a piece",
    `// and a size cannot be combined into one any other way than an arm each:`,
    `// ${sets.length} sets of twelve pieces of ${PIECE_SIZES.length} sizes.`,
    "// One procedure per set, chosen by a match on the set, because a single match",
    "// with every pair in it is deeper than the compiler can lower.",
    "//",
    "// `size` is the size the piece is going to be stamped at, and the costumes",
    "// are that size: a piece on the board, one the pointer is holding, an icon in",
    "// the panel's capture row and a knight in the settings grid.",
    ...PIECE_SIZES.map(([letter, px], v) => `//   ${v}  ${letter}  ${px} units`),
    "",
  ];
  for (let i = 0; i < sets.length; i += 1) {
    lines.push(`pub proc set${i}(p: num, size: num) warp {`);
    lines.push("    match size {");
    PIECE_SIZES.forEach(([letter], v) => {
      lines.push(`        ${v} => {`);
      lines.push("            match p {");
      PIECE_FILES.forEach((name, j) => {
        lines.push(`                ${j + 1} => { looks::switch_costume_to("${letter}${i}${name}"); },`);
      });
      lines.push('                _ => { looks::switch_costume_to("blank"); }');
      lines.push("            }");
      lines.push("        },");
    });
    lines.push('        _ => { looks::switch_costume_to("blank"); }');
    lines.push("    }");
    lines.push("}");
    lines.push("");
  }
  lines.push("/// The costume of piece `p` (1 pawn .. 12 king, white then black) in set");
  lines.push("/// `theme`, at the size it is about to be stamped at. Out of range pieces");
  lines.push("/// are the transparent pixel both sprites park on.");
  lines.push("pub proc take(theme: num, p: num, size: num) warp {");
  lines.push("    match theme {");
  for (let i = 0; i < sets.length; i += 1) {
    lines.push(`        ${i} => { set${i}(p, size); },`);
  }
  lines.push('        _ => { looks::switch_costume_to("blank"); }');
  lines.push("    }");
  lines.push("}");
  writeFileSync(join(root, "src", "sprites", "pieces.rav"), `${lines.join("\n")}\n`);
}

// The costume lists of the two sprites that stamp pieces, between their markers.
// Both carry all four sizes: `pieces.rav` names every one of them, and a name a
// module mentions has to exist in every target that uses the module.
const costumeBlock = () => [
  "    // BEGIN PIECE COSTUMES",
  "    // Everything between these two markers is generated by tools/assets.mjs:",
  "    // one PNG per piece per size it is stamped at, and every one of them is",
  "    // stamped at size 100. The board draws `p` and `d`, the panel `c` and",
  "    // `s`; all four are here because `pieces.rav` names them all and a module",
  "    // is compiled into every target that uses it. Do not edit it by hand.",
  ...PIECE_SIZES.map(([letter, px]) => `    // ${px} units: ${letter}<set><piece>`),
  ...pieceCostumes.map((p) => `    costume "${p.name}" = "assets/${p.name}.png";`),
];
for (const sprite of ["board.rav", "hud.rav"]) {
  const path = join(root, "src", "sprites", sprite);
  const text = readFileSync(path, "utf8");
  const open = "    // BEGIN PIECE COSTUMES";
  const close = "    // END PIECE COSTUMES";
  const i = text.indexOf(open);
  const j = text.indexOf(close);
  if (i < 0 || j < 0) throw new Error(`${sprite} has no piece costume markers`);
  writeFileSync(path, text.slice(0, i) + costumeBlock().join("\n") + "\n" + text.slice(j));
}

// ---------------------------------------------------------------------------
// The check
// ---------------------------------------------------------------------------

/** The first thing wrong with a document's tags, or "" when there is nothing. */
function unbalanced(text) {
  const markup = text
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<\?[\s\S]*?\?>/g, "")
    .replace(/<!\[CDATA\[[\s\S]*?\]\]>/g, "")
    .replace(/<style[\s\S]*?<\/style>/gi, "<style></style>");
  const stack = [];
  const tag = /<(\/?)([A-Za-z][\w.:-]*)\b[^>]*?(\/?)>/g;
  for (let m = tag.exec(markup); m; m = tag.exec(markup)) {
    if (m[1]) {
      if (stack.pop() !== m[2]) return `</${m[2]}> closes an element that is not open`;
    } else if (!m[3]) {
      stack.push(m[2]);
    }
  }
  return stack.length ? `<${stack[stack.length - 1]}> is never closed` : "";
}

// The SVGs are read back and tested for the two things that stop Scratch drawing
// a costume at all: it hands the file to the browser as an image, so the file has
// to be well-formed XML, with every prefix it uses declared on its root. The
// pieces are PNGs and are tested for their size instead, which is what the layout
// numbers in `src/layout.rav` are computed from.
const problems = [];
const pngSize = (data) => {
  if (data.length < 24 || data.readUInt32BE(12) !== 0x49484452) return null;
  return [data.readUInt32BE(16), data.readUInt32BE(20)];
};
const pxOf = new Map(pieceCostumes.map((p) => [`${p.name}.png`, p.px]));
for (const name of written) {
  if (name.endsWith(".png")) {
    const size = pngSize(readFileSync(join(assets, name)));
    const want = pxOf.get(name);
    if (!size || size[0] !== want || size[1] !== want) {
      problems.push(`${name}: is ${size ? size.join("x") : "not a PNG"}, expected ${want}x${want}`);
    }
    continue;
  }
  const text = readFileSync(join(assets, `${name}.svg`), "utf8");
  const root = /<svg\b[^>]*>/.exec(text);
  if (!root) {
    problems.push(`${name}: no root <svg> element`);
    continue;
  }
  const declared = new Set(
    [...root[0].matchAll(/xmlns(?::([\w.-]+))?=/g)].map((m) => m[1] || ""),
  );
  if (!declared.has("")) problems.push(`${name}: the root declares no xmlns`);
  const markup = text.replace(/<style[\s\S]*?<\/style>/gi, "");
  const used = new Set(
    [...markup.matchAll(/<([A-Za-z_][\w.-]*):|[\s"]([A-Za-z_][\w.-]*):[\w.-]+=/g)]
      .map((m) => m[1] || m[2])
      .filter((p) => p !== "xmlns" && p !== "xml"),
  );
  for (const prefix of used) {
    if (!declared.has(prefix)) {
      problems.push(`${name}: uses the prefix ${prefix}: without declaring it`);
    }
  }
  const bad = unbalanced(text);
  if (bad) problems.push(`${name}: ${bad}`);
}
if (problems.length) {
  console.error(`\n${problems.length} costume(s) are not what Scratch can draw:`);
  for (const p of problems.slice(0, 20)) console.error(`  ${p}`);
  if (problems.length > 20) console.error(`  ... and ${problems.length - 20} more`);
  process.exit(1);
}

// ---------------------------------------------------------------------------

const pngs = written.filter((n) => n.endsWith(".png")).length;
console.log(`wrote ${written.length} costumes to ${assets}: ${pngs} piece PNGs at ` +
  `${PIECE_SIZES.map(([letter, px]) => `${px}px ${letter}`).join(", ")}, ` +
  `${written.length - pngs} SVGs`);
console.log(`checked ${written.length} files: every piece its own size, every SVG well formed with its prefixes declared`);
