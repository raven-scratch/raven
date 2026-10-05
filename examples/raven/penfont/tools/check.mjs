#!/usr/bin/env node
/**
 * Run the built project in a real Scratch VM and check the stage it draws.
 *
 *   python examples/raven/penfont/tools/font2vm.py --stage examples/raven/penfont/dist/page
 *   SCRATCH_VM_ROOT=ref/scratch-editor/packages/scratch-vm \
 *     node examples/raven/penfont/tools/check.mjs [--text "…"] [--png out.png]
 *
 * There is no way to check a pen renderer by reading `project.json`: the whole
 * question is whether the block sequence the compiler emitted puts the lines
 * where the font says they go, and whether Scratch let the sprite go there.
 *
 * So this hands the archive to the VM, stands in a renderer that records
 * `penLine` instead of drawing it -- with the renderer's own rule for a sprite
 * that is asked to move past the edge of the stage, which is the rule that turns
 * a page into a smear -- drives the project the way a reader does (a Space key,
 * then the answer), and then does two things with what came out.
 *
 * It compares the recorded lines one by one with the same lines taken
 * independently out of `src/sprites/font.rav` and the layout rules in
 * `src/sprites/text.rav`, which is the diagnosis. And it rasterises every stroke
 * the pen actually made and holds the stage against `dist/page.gray`, the page
 * `font2vm.py --stage` drew from those same tables, which is the verdict.
 *
 * It also fails if the sprite stamped, which is the one thing the engine is not
 * allowed to do.
 *
 * `--png` writes the stage it drew, which is how to look at it.
 */

import { createRequire } from "node:module";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const require = createRequire(import.meta.url);
const Module = require("module");
const resolvePath = (...parts) => parts.join("/");

// The VM reaches for the SVG sanitiser, which is a sibling package that may not
// have a built `dist`. It cannot change the block structure this checks.
const originalLoad = Module._load;
Module._load = function (request, parent, isMain) {
  if (request === "@scratch/scratch-svg-renderer") {
    return {
      sanitizeSvg: { sanitizeByteStream: (data) => data },
      loadSvgString: () => Promise.resolve(),
      serializeSvgToString: () => "",
    };
  }
  return originalLoad.call(this, request, parent, isMain);
};

const VM_ROOT = process.env.SCRATCH_VM_ROOT;
if (!VM_ROOT) {
  console.error("set SCRATCH_VM_ROOT to ref/scratch-editor/packages/scratch-vm");
  process.exit(2);
}
const VirtualMachine = require(resolvePath(process.cwd(), VM_ROOT, "src", "virtual-machine.js"));

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// ---------------------------------------------------------------------------
// The project's own numbers
// ---------------------------------------------------------------------------

const rav = readFileSync(join(root, "src", "penfont", "font.rav"), "utf8");
// The library and this project's page between them own the numbers: the engine
// draws with the box, the leading and the sheet, the page chooses where its
// block starts and how big it is.
const engineRav = readFileSync(join(root, "src", "penfont", "engine.rav"), "utf8");
const textRav = readFileSync(join(root, "src", "sprites", "text.rav"), "utf8");

function constant(name) {
  const m = new RegExp(`pub const ${name}: num = ([\\d.eE+-]+);`).exec(rav);
  if (!m) throw new Error(`font.rav has no ${name}`);
  return Number(m[1]);
}

function numbers(name) {
  const m = new RegExp(`pub var ${name}: list<num> = \\[([\\s\\S]*?)\\];`).exec(rav);
  if (!m) throw new Error(`font.rav has no ${name}`);
  return m[1].split(",").map((s) => s.trim()).filter(Boolean).map(Number);
}

/** A raven string literal, unescaped. `\\u{…}` is raven's, not JSON's. */
function ravenString(literal) {
  const body = literal.slice(1, -1);
  let out = "";
  for (let i = 0; i < body.length; i += 1) {
    if (body[i] !== "\\") {
      out += body[i];
      continue;
    }
    const next = body[i + 1];
    if (next === "u") {
      const end = body.indexOf("}", i);
      out += String.fromCodePoint(parseInt(body.slice(i + 3, end), 16));
      i = end;
    } else {
      out += next === "n" ? "\n" : next === "t" ? "\t" : next;
      i += 1;
    }
  }
  return out;
}

function strings(name) {
  const m = new RegExp(`pub var ${name}: list<str> = \\[([\\s\\S]*?)\\];`).exec(rav);
  if (!m) throw new Error(`font.rav has no ${name}`);
  // A key can be a comma, so the items are read as literals rather than split.
  return [...m[1].matchAll(/"(?:\\.|[^"\\])*"/g)].map((x) => ravenString(x[0]));
}

function fromText(pattern, what) {
  const m = pattern.exec(textRav);
  if (!m) throw new Error(`text.rav has no ${what}`);
  return m[1];
}

function fromEngine(pattern, what) {
  const m = pattern.exec(engineRav);
  if (!m) throw new Error(`engine.rav has no ${what}`);
  return m[1];
}

const font = {
  rows: constant("FONT_ROWS"),
  chars: strings("font_chars"),
  at: numbers("font_at"),
  runs: numbers("font_runs"),
  adv: numbers("font_adv"),
  run: numbers("font_run"),
};

const layout = {
  left: Number(fromText(/const LEFT: num = ([\d.eE+-]+);/, "LEFT")),
  top: Number(fromText(/const TOP: num = ([\d.eE+-]+);/, "TOP")),
  cap: Number(fromText(/const CAP: num = ([\d.eE+-]+);/, "CAP")),
  lead: Number(fromEngine(/const PF_LEAD: num = ([\d.eE+-]+);/, "PF_LEAD")),
  size: Number(fromText(/var size: num = ([\d.eE+-]+);/, "size")),
  ink: fromText(/var ink: str = "([^"]*)";/, "ink"),
  limit: Number(fromText(/var limit: num = ([\d.eE+-]+);/, "limit")),
  edgeX: Number(fromEngine(/const PF_EDGE_X: num = ([\d.eE+-]+);/, "PF_EDGE_X")),
  edgeY: Number(fromEngine(/const PF_EDGE_Y: num = ([\d.eE+-]+);/, "PF_EDGE_Y")),
  // How far the engine looks for a ligature, so the walk below takes the same
  // longest key it does.
  ligMax: Number(fromEngine(/const PF_LIG_MAX: num = ([\d.eE+-]+);/, "PF_LIG_MAX")),
  // The sheet's geometry, so the grid this checks is the grid the project draws.
  cols: Number(fromEngine(/const PF_COLS: num = ([\d.eE+-]+);/, "PF_COLS")),
  rows: Number(fromEngine(/const PF_ROWS: num = ([\d.eE+-]+);/, "PF_ROWS")),
  cell: Number(fromEngine(/const PF_CELL: num = ([\d.eE+-]+);/, "PF_CELL")),
  step: Number(fromEngine(/const PF_STEP: num = ([\d.eE+-]+);/, "PF_STEP")),
  sheetLeft: Number(fromEngine(/const PF_SHEET_LEFT: num = ([\d.eE+-]+);/, "PF_SHEET_LEFT")),
  sheetTop: Number(fromEngine(/const PF_SHEET_TOP: num = ([\d.eE+-]+);/, "PF_SHEET_TOP")),
  accent: fromText(/var accent: str = "([^"]*)";/, "accent"),
  // The line the project opens on, which is the page `font2vm.py --stage`
  // renders and therefore the page the two pictures are compared over.
  text: ravenString(fromText(/var text: str = ("(?:\\.|[^"\\])*");/, "text")),
};

// ---------------------------------------------------------------------------
// What the tables say the text becomes
// ---------------------------------------------------------------------------

/** The same escape rules `draw_text` reads, written out again. */
function walk(text) {
  const out = [];
  let cap = false;
  let i = 0;
  while (i < text.length) {
    let c = text[i];
    let step = 1;
    let show = true;
    // A character outside the basic plane is two UTF-16 code units and this
    // reads one at a time, exactly as the engine does: put the pair back
    // together, because the key for such a glyph is the whole pair.
    if (c > "\uD7FF" && c < "\uE000") {
      c += text[i + 1] || "";
      step = 2;
    }
    if (c === "\n" || c === "\r") {
      out.push({ newline: true });
      cap = false;
      show = false;
    }
    if (c === "\\") {
      const d = text[i + 1] || "";
      if (d === "c" || d === "C") {
        cap = true;
        show = false;
        step = 2;
      } else if (d === "n" || d === "N") {
        out.push({ newline: true });
        cap = false;
        show = false;
        step = 2;
      } else if (d === "\\") {
        step = 2;
      }
    }
    if (show) {
      const wasCap = cap;
      cap = false;
      let key = wasCap ? "\\c" + c : c;
      // The engine takes the longest ligature key that starts here, so this
      // does too: the candidates grow a character at a time and the last hit
      // is the one kept, which is how `===` beats `==` and then `=`.
      if (!wasCap && step === 1 && c !== "\\" && layout.ligMax > 1) {
        let run = "";
        for (let k = 0; k < layout.ligMax && i + k < text.length; k += 1) {
          const d = text[i + k];
          if (d === "\\" || d === "\n" || d === "\r") break;
          run += d;
          if (k > 0 && indexOfKey(run) > 0) {
            key = run;
            step = k + 1;
          }
        }
      }
      out.push({ key });
    }
    i += step;
  }
  return out;
}

/** Scratch finds a string in a list without regard to case; so does this. */
function indexOfKey(key) {
  const want = key.toLowerCase();
  for (let i = 0; i < font.chars.length; i += 1) {
    if (font.chars[i].toLowerCase() === want) return i + 1;
  }
  return 0;
}

/** Every line the engine should draw, in the order it should draw them. */
function expected(text) {
  const scale = layout.size / font.rows;
  const pen = Math.max(1, Math.ceil(scale));
  const half = pen / 2;
  const lead = layout.size * layout.lead;
  let px = layout.left;
  let py = layout.top - layout.size * layout.cap;
  const lines = [];
  for (const item of walk(text)) {
    if (item.newline) {
      px = layout.left;
      py -= lead;
      continue;
    }
    const g = indexOfKey(item.key);
    const adv = g > 0 ? font.adv[g - 1] * scale : layout.size / 2;
    if (layout.limit > 0 && px > layout.left && px + adv > layout.left + layout.limit) {
      px = layout.left;
      py -= lead;
    }
    if (g > 0) {
      const first = font.at[g - 1];
      const count = font.runs[g - 1];
      for (let k = 0; k < count; k += 1) {
        const o = first - 1 + 3 * k;
        const row = font.run[o];
        const a = font.run[o + 1];
        const b = font.run[o + 2];
        const y = py + (row + 0.5) * scale;
        let from = px + a * scale + half;
        let to = px + b * scale - half;
        if (to <= from) {
          from = px + ((a + b) * scale) / 2;
          to = from + 0.05;
        }
        // The engine cuts a run at the box the pen may move inside.
        from = Math.max(from, -layout.edgeX);
        to = Math.min(to, layout.edgeX);
        if (to > from && y > -layout.edgeY && y < layout.edgeY) lines.push({ from, to, y });
      }
    }
    px += adv;
  }
  return { lines, pen, scale, ink: layout.ink };
}

// ---------------------------------------------------------------------------
// The renderer, stood in for
// ---------------------------------------------------------------------------

let nextId = 1;
const skins = new Map();
const drawables = new Map();
let ink = [];
let strokes = [];
let stamps = 0;
let penSize = 1;
let penColour = null;

const renderer = {
  setLayerGroupOrdering() {},
  createSVGSkin(svg) {
    const id = nextId++;
    // `width` and `height` off the costume, which is what the fence is read
    // from. The blank costume is one unit square.
    const m = /<svg\b[^>]*\bwidth="([\d.]+)"[^>]*\bheight="([\d.]+)"/.exec(svg);
    skins.set(id, { w: m ? Number(m[1]) : 1, h: m ? Number(m[2]) : 1, rc: [0, 0] });
    return id;
  },
  createBitmapSkin() {
    const id = nextId++;
    skins.set(id, { w: 1, h: 1, rc: [0.5, 0.5] });
    return id;
  },
  createTextSkin() {
    return nextId++;
  },
  createPenSkin() {
    return nextId++;
  },
  destroySkin(id) {
    skins.delete(id);
  },
  updateSVGSkin() {},
  updateBitmapSkin() {},
  updateTextSkin() {},
  getSkinSize(id) {
    const s = skins.get(id);
    return s ? [s.w, s.h] : [0, 0];
  },
  getSkinRotationCenter(id) {
    const s = skins.get(id);
    return s ? s.rc.slice() : [0, 0];
  },
  getNativeSize() {
    return [480, 360];
  },
  createDrawable() {
    const id = nextId++;
    drawables.set(id, { position: [0, 0], scale: 1 });
    return id;
  },
  destroyDrawable(id) {
    drawables.delete(id);
  },
  updateDrawableSkinId() {},
  updateDrawablePosition(id, position) {
    const d = drawables.get(id);
    if (d) d.position = [position[0], position[1]];
  },
  updateDrawableDirectionScale(id, _direction, scale) {
    const d = drawables.get(id);
    if (d) d.scale = scale;
  },
  updateDrawableVisible() {},
  updateDrawableEffect() {},
  setDrawableOrder() {},
  getDrawableOrder() {
    return 0;
  },
  // The first thing that decides whether a pen project draws or smears: a
  // sprite asked to move past the edge of the stage is moved back, not clipped,
  // so a run from x = 300 is drawn at x = 240 and any glyph reaching past the
  // edge piles onto that column. This is the renderer's own rule, and a stand-in
  // that returns the position unchanged would hide the whole failure.
  getFencedPositionOfDrawable(id, position) {
    const d = drawables.get(id);
    const skin = d && skins.get(d.skinId);
    if (!d || !skin) return [position[0], position[1]];
    const w = skin.w * d.scale;
    const h = skin.h * d.scale;
    const inset = Math.floor(Math.min(w, h) / 2);
    const bx = 240 - Math.min(15, inset);
    const by = 180 - Math.min(15, inset);
    let x = position[0];
    let y = position[1];
    const left = d.position[0] - w / 2;
    const right = d.position[0] + w / 2;
    const top = d.position[1] + h / 2;
    const bottom = d.position[1] - h / 2;
    const dx = x - d.position[0];
    const dy = y - d.position[1];
    if (right + dx < -bx) x = Math.ceil(d.position[0] - (bx + right));
    else if (left + dx > bx) x = Math.floor(d.position[0] + (bx - left));
    if (top + dy < -by) y = Math.ceil(d.position[1] - (by + top));
    else if (bottom + dy > by) y = Math.floor(d.position[1] + (by - bottom));
    return [x, y];
  },
  getCurrentSkinSize() {
    return [1, 1];
  },
  getBounds() {
    return { left: 0, right: 0, top: 0, bottom: 0 };
  },
  getBoundsForBubble() {
    return this.getBounds();
  },
  pick() {
    return -1;
  },
  drawableTouching() {
    return false;
  },
  drawableTouchingScratchPoint() {
    return false;
  },
  drawableTouchingScratchRect() {
    return false;
  },
  isTouchingColor() {
    return false;
  },
  isTouchingDrawables() {
    return false;
  },
  penClear() {
    ink = [];
    strokes = [];
  },
  penStamp() {
    stamps += 1;
  },
  penLine(_skin, attrs, x0, y0, x1, y1) {
    penSize = attrs.diameter;
    // A copy: the pen extension hands out the same `color4f` array every time
    // and writes the next colour into it, so a recorded reference would read
    // back as whatever colour was set last.
    penColour = attrs.color4f.slice();
    ink.push({ from: x0, to: x1, y: y0 });
    strokes.push({ from: x0, to: x1, y: y0, pen: attrs.diameter, colour: attrs.color4f.slice() });
    if (y0 !== y1) ink.push({ diagonal: true, y0, y1 });
  },
  penPoint(_skin, attrs, x, y) {
    penSize = attrs.diameter;
    penColour = attrs.color4f.slice();
    strokes.push({ from: x, to: x, y, pen: attrs.diameter, colour: attrs.color4f.slice() });
  },
  draw() {},
};

// ---------------------------------------------------------------------------
// The stage, drawn from what the pen did
// ---------------------------------------------------------------------------

const STAGE_W = 480;
const STAGE_H = 360;

/**
 * Every pixel the pen inked, by the one rule the pen has: a stroke is a capsule
 * of the pen's diameter between two points, with round ends, so a pixel is inked
 * when its centre is within half the pen of the segment. `font2vm.py` rasterises
 * the same runs the same way, which is what makes the two pictures comparable.
 */
function rasterize(strokes) {
  const mask = new Uint8Array(STAGE_W * STAGE_H);
  for (const s of strokes) {
    const r = s.pen / 2;
    const x0 = Math.min(s.from, s.to);
    const x1 = Math.max(s.from, s.to);
    const ix0 = Math.max(0, Math.floor(x0 + STAGE_W / 2 - r - 1));
    const ix1 = Math.min(STAGE_W - 1, Math.ceil(x1 + STAGE_W / 2 + r + 1));
    const iy0 = Math.max(0, Math.floor(STAGE_H / 2 - s.y - r - 1));
    const iy1 = Math.min(STAGE_H - 1, Math.ceil(STAGE_H / 2 - s.y + r + 1));
    for (let iy = iy0; iy <= iy1; iy += 1) {
      const dy = STAGE_H / 2 - (iy + 0.5) - s.y;
      for (let ix = ix0; ix <= ix1; ix += 1) {
        const p = ix + 0.5 - STAGE_W / 2;
        const dx = p - Math.min(Math.max(p, x0), x1);
        if (dx * dx + dy * dy <= r * r) mask[iy * STAGE_W + ix] = 255;
      }
    }
  }
  return mask;
}

const CRC = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return (buf) => {
    let c = -1;
    for (const b of buf) c = table[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ -1) >>> 0;
  };
})();

/** An 8-bit greyscale PNG, which is all the stage mask needs. */
function png(mask, width, height) {
  const raw = Buffer.alloc((width + 1) * height);
  for (let y = 0; y < height; y += 1) {
    raw[y * (width + 1)] = 0; // filter: none
    Buffer.from(mask.buffer, mask.byteOffset + y * width, width).copy(raw, y * (width + 1) + 1);
  }
  const chunk = (type, data) => {
    const head = Buffer.alloc(8);
    head.writeUInt32BE(data.length, 0);
    head.write(type, 4, "ascii");
    const tail = Buffer.alloc(4);
    tail.writeUInt32BE(CRC(Buffer.concat([head.subarray(4), data])), 0);
    return Buffer.concat([head, data, tail]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 0; // greyscale
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function inkBox(mask) {
  let l = STAGE_W;
  let r = -1;
  let t = STAGE_H;
  let b = -1;
  for (let i = 0; i < mask.length; i += 1) {
    if (!mask[i]) continue;
    const x = i % STAGE_W;
    const y = (i / STAGE_W) | 0;
    if (x < l) l = x;
    if (x > r) r = x;
    if (y < t) t = y;
    if (y > b) b = y;
  }
  return r < 0 ? null : { l, r, t, b };
}

function overlap(a, b) {
  let both = 0;
  let either = 0;
  for (let i = 0; i < a.length; i += 1) {
    const p = a[i] > 127;
    const q = b[i] > 127;
    if (p && q) both += 1;
    if (p || q) either += 1;
  }
  return either ? both / either : 1;
}

// ---------------------------------------------------------------------------
// The sheet, which is a grid and can be checked as one
// ---------------------------------------------------------------------------

/** The stage box of the sheet cell the `n`th of a page is drawn in. */
function cellBox(k) {
  const col = k % layout.cols;
  const row = Math.floor(k / layout.cols);
  const x = layout.sheetLeft + 6 + col * layout.cell;
  const y = layout.sheetTop - row * layout.step;
  // A cell's square, kept clear of the next one: the glyph is drawn from its
  // origin up and right, and the pitch is wider than the square in both axies.
  return { x0: x, x1: x + layout.cell - 4, y0: y - 8, y1: y + 24 };
}

function inkIn(mask, box) {
  let n = 0;
  const ix0 = Math.max(0, Math.round(box.x0 + STAGE_W / 2));
  const ix1 = Math.min(STAGE_W - 1, Math.round(box.x1 + STAGE_W / 2));
  const iy0 = Math.max(0, Math.round(STAGE_H / 2 - box.y1));
  const iy1 = Math.min(STAGE_H - 1, Math.round(STAGE_H / 2 - box.y0));
  for (let iy = iy0; iy <= iy1; iy += 1) {
    for (let ix = ix0; ix <= ix1; ix += 1) if (mask[iy * STAGE_W + ix]) n += 1;
  }
  return n;
}

/** The 1-based index of a key in the table, or 0, without regard to case. */
function glyphIndex(key) {
  const want = key.toLowerCase();
  for (let i = 0; i < font.chars.length; i += 1) {
    if (font.chars[i].toLowerCase() === want) return i + 1;
  }
  return 0;
}

/** A `#rrggbb` literal, as three fractions, which is what the pen carries. */
const rgb = (s) => [1, 3, 5].map((i) => parseInt(s.slice(i, i + 2), 16) / 255);
const shown = (c) => `rgb(${c.slice(0, 3).map((v) => Math.round(v * 255)).join(",")})`;

const sheetPage = (g) => Math.floor((g - 1) / (layout.cols * layout.rows)) + 1;

// ---------------------------------------------------------------------------
// Drive it
// ---------------------------------------------------------------------------

const argOf = (name, fallback) => {
  const i = process.argv.indexOf(name);
  return i >= 0 ? process.argv[i + 1] : fallback;
};

// The project's own line, so the stage this drives can be held against the page
// `font2vm.py --stage` renders of the same line. `--text` tries another one, and
// then only the tables are the reference: the page on disk is a different page.
const TEXT = argOf("--text", layout.text);
const ownPage = argOf("--text", null) === null;

// The character the find is asked for. It has to be well past the first sheet
// page, or "the sheet turned to its page" would be true without anything having
// turned.
const FIND = argOf("--find", "中");

const vm = new VirtualMachine();
vm.attachRenderer(renderer);
const buffer = readFileSync(join(root, "dist", "penfont.sb3"));
await vm.loadProject(buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength));

// What the question gets answered with, which changes for the find.
let answer = TEXT;
vm.runtime.on("QUESTION", (q) => {
  if (q === null) return;
  setTimeout(() => vm.runtime.emit("ANSWER", answer), 0);
});

const tap = async (key, ms) => {
  vm.postIOData("keyboard", { key, isDown: true });
  vm.postIOData("keyboard", { key, isDown: false });
  await sleep(ms);
};

// A phase is one redraw: the pen layer is cleared at the top of it, so what is
// recorded when it settles is that page and nothing else.
const snapshot = () => ({
  ink: ink.slice(),
  strokes: strokes.slice(),
  penSize,
  penColour,
});

vm.greenFlag();
vm.start();
await sleep(500);
// Space asks for a line; the handler above answers it with the text under test.
await tap(" ", 4000);
const page = snapshot();

// B turns to the sheet: every glyph the font has, twelve by eight to a page.
await tap("b", 4000);
const sheet = snapshot();

// And Space on the sheet is a find, which turns to the page the character is on.
answer = FIND;
await tap(" ", 4000);
const found = snapshot();
vm.stopAll();

const want = expected(TEXT);
const failures = [];

if (stamps !== 0) failures.push(`the sprite stamped ${stamps} time(s); the engine draws with the pen only`);
if (page.ink.some((l) => l.diagonal)) failures.push("a pen move between two scan rows drew a line");
if (page.ink.length !== want.lines.length) {
  failures.push(`drew ${page.ink.length} lines, the tables say ${want.lines.length}`);
}
if (Math.abs(page.penSize - want.pen) > 1e-9) failures.push(`pen size ${page.penSize}, expected ${want.pen}`);

const close = (a, b) => Math.abs(a - b) < 1e-6;
let mismatched = 0;
for (let i = 0; i < Math.min(page.ink.length, want.lines.length); i += 1) {
  const got = page.ink[i];
  const exp = want.lines[i];
  if (!close(got.from, exp.from) || !close(got.to, exp.to) || !close(got.y, exp.y)) {
    if (mismatched < 3) {
      failures.push(
        `line ${i}: drew (${got.from}, ${got.y})..(${got.to}, ${got.y}), ` +
          `the tables say (${exp.from}, ${exp.y})..(${exp.to}, ${exp.y})`,
      );
    }
    mismatched += 1;
  }
}
if (mismatched > 3) failures.push(`... and ${mismatched - 3} more lines out of place`);

// The ink has to be the colour asked for, and it has to be somewhere on the page.
if (!page.penColour) failures.push("the pen never drew, so it never had a colour");
else if (rgb(layout.ink).some((v, i) => Math.abs(v - page.penColour[i]) > 0.01)) {
  failures.push(`the pen is ${shown(page.penColour)}, asked for ${layout.ink}`);
}
if (page.penColour && page.penColour[3] <= 0) failures.push("the pen is fully transparent");
if (page.ink.length === 0) failures.push("nothing was drawn at all");

// The stage itself. The line-by-line comparison above is the diagnosis; this is
// the verdict, because it is the picture a reader sees: every stroke the VM
// actually made, at the position the fence actually allowed, rasterised the way
// the pen inked it, against the page `font2vm.py` drew from the same tables.
const stage = rasterize(page.strokes);
const box = inkBox(stage);
const pngPath = argOf("--png", join(root, "dist", "stage.png"));
writeFileSync(pngPath, png(stage, STAGE_W, STAGE_H));

const reference = argOf("--reference", join(root, "dist", "page.gray"));
let pageIou = null;
if (!ownPage) {
  console.log(`note        --text asked for another page; ` +
    `${reference} is not it, so only the tables are the reference`);
} else if (existsSync(reference)) {
  pageIou = overlap(stage, readFileSync(reference));
  // Not a tolerance for the pen against FreeType: both of these are the same
  // pen, so anything but a match means the sprite was moved somewhere the page
  // did not ask for.
  if (pageIou < 0.98) {
    failures.push(`the stage is only ${(pageIou * 100).toFixed(1)}% the page font2vm.py drew`);
  }
} else {
  console.log(`note        no ${reference}; run font2vm.py --stage dist/page first`);
}

// ---------------------------------------------------------------------------
// The sheet
// ---------------------------------------------------------------------------

const cells = layout.cols * layout.rows;
const total = font.chars.length;
const sheetMask = rasterize(sheet.strokes);
const sheetBox = inkBox(sheetMask);
writeFileSync(join(root, "dist", "sheet.png"), png(sheetMask, STAGE_W, STAGE_H));

if (!sheetBox) {
  failures.push("the sheet drew nothing");
} else {
  // Every cell drawn, and drawn inside the box the pen may be moved in. A cell
  // whose glyph has no runs -- a space -- is the only one allowed to be empty.
  let empty = [];
  const blank = new Set();
  for (let k = 0; k < cells; k += 1) {
    const g = k + 1;
    if (g > total || font.runs[g - 1] === 0) {
      blank.add(k);
      continue;
    }
    if (inkIn(sheetMask, cellBox(k)) === 0) empty.push(k);
  }
  if (empty.length) {
    failures.push(`${empty.length} of ${cells - blank.size} sheet cells are empty (first ${empty[0]})`);
  }
  const outside = sheetBox.l < 0 || sheetBox.r > STAGE_W - 1 || sheetBox.t < 0 || sheetBox.b > STAGE_H - 1;
  const sx = sheetBox.l - 240;
  const sy = 180 - sheetBox.b;
  if (!outside && (sx < -layout.edgeX || sheetBox.r - 240 > layout.edgeX ||
    sy < -layout.edgeY || 180 - sheetBox.t > layout.edgeY)) {
    failures.push(`the sheet leaves the box: x ${sx}..${sheetBox.r - 240}, y ${sy}..${180 - sheetBox.t}`);
  }
}

// The find: the sheet turns to the page the character is on and rings it, and
// nothing else on the page is drawn in the accent ink.
const target = glyphIndex(FIND);
const accent = rgb(layout.accent);
const foundMask = rasterize(found.strokes);
const accentMask = rasterize(found.strokes.filter(
  (s) => Math.abs(s.colour[0] - accent[0]) < 0.01 && Math.abs(s.colour[1] - accent[1]) < 0.01));
writeFileSync(join(root, "dist", "sheet-found.png"), png(foundMask, STAGE_W, STAGE_H));

if (target === 0) failures.push(`the table has no ${JSON.stringify(FIND)} to find`);
else {
  const wantPage = sheetPage(target);
  const cell = (target - 1) % cells;
  const cx = layout.sheetLeft + 6 + (cell % layout.cols) * layout.cell;
  const cy = layout.sheetTop - Math.floor(cell / layout.cols) * layout.step;

  // The ring the project draws round a found glyph, which is the accent ink
  // that is in the grid and not in the line under it that says what was found.
  const gridAccent = [];
  for (let i = 0; i < accentMask.length; i += 1) {
    if (!accentMask[i]) continue;
    const x = (i % STAGE_W) - STAGE_W / 2 + 0.5;
    const y = STAGE_H / 2 - ((i / STAGE_W) | 0) - 0.5;
    if (y > -130) gridAccent.push([x, y]);
  }
  if (!gridAccent.length) {
    failures.push(`the find marked nothing in the grid for ${JSON.stringify(FIND)}`);
  } else {
    const bx = gridAccent.map((p) => p[0]);
    const by = gridAccent.map((p) => p[1]);
    const want = [cx - 4, cx + layout.cell - 10, cy - 5, cy + 27];
    const got = [Math.min(...bx), Math.max(...bx), Math.min(...by), Math.max(...by)];
    if (want.some((v, i) => Math.abs(v - got[i]) > 1.5)) {
      failures.push(`the find ringed ${got.map((v) => v.toFixed(1))}, ` +
        `${JSON.stringify(FIND)} is at ${want.map((v) => v.toFixed(1))}`);
    }
  }

  // And the page has to have turned to get there. A ring in the right place
  // would pass the check above on its own, on the wrong page.
  if (wantPage === 1) {
    failures.push(`--find ${JSON.stringify(FIND)} is on the first page, so it proves nothing; pick another`);
  } else if (overlap(sheetMask, foundMask) > 0.95) {
    failures.push(`the sheet looks unchanged after the find, so it did not turn to page ${wantPage}`);
  }
}

console.log(`text        ${JSON.stringify(TEXT)}`);
console.log(`glyphs      ${total} in the table, ${font.run.length} runs, ${sheetPage(total)} sheet pages`);
console.log(`drawn       ${page.ink.length} lines at pen size ${page.penSize}, ${stamps} stamps`);
console.log(`expected    ${want.lines.length} lines, scale ${want.scale}`);
console.log(`stage       ${box ? `x ${box.l - 240}..${box.r - 240}, y ${180 - box.b}..${180 - box.t}` : "empty"}, ` +
  `page overlap ${pageIou === null ? "n/a" : pageIou.toFixed(3)}`);
console.log(`sheet       ${cells} cells, ${sheet.strokes.length} strokes, ` +
  `${sheetBox ? `x ${sheetBox.l - 240}..${sheetBox.r - 240}, y ${180 - sheetBox.b}..${180 - sheetBox.t}` : "empty"}`);
console.log(`find        ${JSON.stringify(FIND)} at ${target}, page ${sheetPage(target)}`);
console.log(`wrote       ${pngPath}, dist/sheet.png, dist/sheet-found.png`);
if (failures.length) {
  for (const f of failures) console.error(`FAIL  ${f}`);
  process.exit(1);
}
console.log("PASS");
process.exit(0);
