// What the HUD stamps, page by page, read out of a real Scratch VM.
//
//   node examples/raven/chess/tools/hud.mjs
//   node examples/raven/chess/tools/hud.mjs --svg <dir>   # also write the pages
//   node examples/raven/chess/tools/hud.mjs --slow        # and a bot's turn
//
// `--slow` starts a bot's turn and watches the panel's clock while it runs,
// which is the one thing on the panel that has to move on its own and the one
// thing the board cannot say from its own loop: a search runs inside a single
// frame of that loop. It costs about fifteen seconds.
//
// The built project is loaded into a real VM with a stand-in in place of the
// WebGL renderer. The stand-in answers the two questions the VM asks a renderer
// about geometry -- how big a skin is, and where a drawable is allowed to move
// to -- with the costume's own size, and records every `pen stamp` as the
// costume, position and size the sprite had when it happened.
//
// That recording is enough to check the thing a screenshot shows. `set size to`
// is clamped by the VM to a floor read from the *current* costume, so a run of
// text whose size is set before its glyph costume is on comes back larger than
// it was measured for, and its characters are stamped on top of one another.
// Two checks catch that and anything like it, and neither is a rendering:
//
//   * a run of characters is stamped at a pitch that its own advances and its
//     own stamped size agree with;
//   * two runs of text do not overlap.
//
// `--svg` writes each page's stamps as one SVG, which is the HUD's half of the
// stage drawn the way the renderer would have drawn it -- the board is the
// other sprite's, and is not in it. That is what to look at when the checks
// pass and the page is still wrong.

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join, resolve as resolvePath } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const Module = require("module");

const VM_ROOT = process.env.SCRATCH_VM_ROOT;
if (!VM_ROOT) {
  console.error("set SCRATCH_VM_ROOT to scratch-editor/packages/scratch-vm");
  process.exit(1);
}

// Scratch does not draw the SVG a costume was built from: it draws what is left
// of it after its own sanitizer, and the sanitizer drops elements the svg
// profile does not allow -- filter primitives, `use`, `foreignObject` among
// them -- which can leave a shape with nothing to draw it. Reading the costumes
// through the real sanitizer is what makes a page this writes the page Scratch
// draws, so it is used whenever the checkout that has the VM in it is there.
let sanitizeByteStream = (data) => data;
try {
  const svgRenderer = require(
    resolvePath(process.cwd(), VM_ROOT, "..", "scratch-svg-renderer", "src", "sanitize-svg.js"),
  );
  // A costume this harness made out of a PNG is not a costume the sanitizer has
  // an opinion about: it holds a data URI, which is exactly what the sanitizer
  // strips out of an SVG.
  sanitizeByteStream = (data) => {
    const text = Buffer.from(data).toString("utf8");
    if (text.startsWith("<svg") && text.includes("data:image/png;base64,")) return data;
    return svgRenderer.sanitizeByteStream(data);
  };
  console.log("costumes are read through scratch-svg-renderer's sanitizer");
} catch {
  console.log("costumes are read as they are: no scratch-svg-renderer beside the VM");
}

const originalLoad = Module._load;
Module._load = function (request, parent, isMain) {
  if (request === "@scratch/scratch-svg-renderer") {
    return {
      sanitizeSvg: { sanitizeByteStream },
      loadSvgString: () => Promise.resolve(),
      serializeSvgToString: () => "",
    };
  }
  return originalLoad.call(this, request, parent, isMain);
};

const VirtualMachine = require(
  resolvePath(process.cwd(), VM_ROOT, "src", "virtual-machine.js"),
);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const args = process.argv.slice(2);
const svgDir = args.includes("--svg") ? args[args.indexOf("--svg") + 1] : null;
// `--slow` also runs a bot's turn, which takes a few seconds and is what the
// clock is checked during; the rest of the run is over in about ten.
const slow = args.includes("--slow");

// ---------------------------------------------------------------------------
// The renderer, stood in for
// ---------------------------------------------------------------------------

/** The staged geometry the recording is read against. */
let nextId = 1;

function svgSize(svgString) {
  const attrs = /<svg\b[^>]*>/i.exec(svgString);
  const tag = attrs ? attrs[0] : "";
  let w = Number((/\bwidth="([\d.eE+-]+)"/.exec(tag) || [])[1]);
  let h = Number((/\bheight="([\d.eE+-]+)"/.exec(tag) || [])[1]);
  if (!Number.isFinite(w) || !Number.isFinite(h)) {
    const box = (/\bviewBox="([^"]+)"/.exec(tag) || [])[1];
    const v = box ? box.trim().split(/[\s,]+/).map(Number) : null;
    if (v && v.length === 4) {
      w = v[2];
      h = v[3];
    }
  }
  return [Number.isFinite(w) ? w : 0, Number.isFinite(h) ? h : 0];
}

/** Everything a drawable's screen geometry is read from. */
const skins = new Map();
const drawables = new Map();
const targetOfDrawable = new Map();
const stamps = [];
let recording = true;

const renderer = {
  setLayerGroupOrdering() {},
  createSVGSkin(svg, rotationCenter) {
    const [w, h] = svgSize(svg);
    const rc = rotationCenter && Number.isFinite(rotationCenter[0])
      ? [rotationCenter[0], rotationCenter[1]]
      : [w / 2, h / 2];
    const id = nextId++;
    // A costume this harness wrapped a PNG in: the image is kept as it is, so
    // that a page written out puts the PNG in the layer itself. An SVG that is
    // loaded as an image is not allowed to pull in another one, and a page that
    // nested the two would draw a hole.
    const png = /data:image\/png;base64,([A-Za-z0-9+/=]+)/.exec(svg);
    skins.set(id, {w, h, rc, svg, png: png ? png[1] : null});
    return id;
  },
  createBitmapSkin(canvas) {
    const id = nextId++;
    skins.set(id, {w: canvas.width, h: canvas.height, rc: [canvas.width / 2, canvas.height / 2], svg: null});
    return id;
  },
  createTextSkin() {
    const id = nextId++;
    skins.set(id, {w: 0, h: 0, rc: [0, 0], svg: null});
    return id;
  },
  createPenSkin() {
    return nextId++;
  },
  destroySkin(id) {
    skins.delete(id);
  },
  updateSVGSkin(id, svg) {
    const [w, h] = svgSize(svg);
    const skin = skins.get(id);
    if (skin) Object.assign(skin, {w, h, svg});
  },
  updateBitmapSkin() {},
  updateTextSkin() {},
  getSkinSize(id) {
    const skin = skins.get(id);
    return skin ? [skin.w, skin.h] : [0, 0];
  },
  getSkinRotationCenter(id) {
    const skin = skins.get(id);
    return skin ? skin.rc.slice() : [0, 0];
  },
  getNativeSize() {
    return [480, 360];
  },
  createDrawable() {
    const id = nextId++;
    drawables.set(id, {skinId: -1, position: [0, 0], scale: 1, visible: true});
    return id;
  },
  destroyDrawable(id) {
    drawables.delete(id);
  },
  updateDrawableSkinId(id, skinId) {
    const d = drawables.get(id);
    if (d) d.skinId = skinId;
  },
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
  // What the VM fences a move against: the same rule the real renderer applies,
  // so a position that the real renderer would have clamped is clamped here too.
  getFencedPositionOfDrawable(id, position) {
    const d = drawables.get(id);
    const skin = d && skins.get(d.skinId);
    if (!d || !skin) return [position[0], position[1]];
    const s = d.scale;
    const w = skin.w * s;
    const h = skin.h * s;
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
  getCurrentSkinSize(id) {
    const d = drawables.get(id);
    const skin = d && skins.get(d.skinId);
    return skin ? [skin.w, skin.h] : [0, 0];
  },
  getBounds(id) {
    const d = drawables.get(id);
    const skin = d && skins.get(d.skinId);
    if (!d || !skin) return {left: 0, right: 0, top: 0, bottom: 0};
    const w = (skin.w * d.scale) / 2;
    const h = (skin.h * d.scale) / 2;
    return {
      left: d.position[0] - w,
      right: d.position[0] + w,
      top: d.position[1] + h,
      bottom: d.position[1] - h,
    };
  },
  getBoundsForBubble(id) {
    return renderer.getBounds(id);
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
    if (recording) stamps.push({clear: true});
  },
  penStamp(_penSkinId, drawableId) {
    if (!recording) return;
    const target = targetOfDrawable.get(drawableId);
    const d = drawables.get(drawableId);
    const skin = d && skins.get(d.skinId);
    if (!target || !skin) return;
    stamps.push({
      target: target.getName(),
      costume: target.getCurrentCostume().name,
      x: target.x,
      y: target.y,
      size: target.size,
      skin,
    });
  },
  penLine() {},
  penPoint() {},
  draw() {},
};

// ---------------------------------------------------------------------------
// The font, as the project's own generated files describe it
// ---------------------------------------------------------------------------

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/** `gindex`'s arms: the character a costume stem is, and its index. */
function readFont() {
  const rav = readFileSync(join(root, "src", "sprites", "glyphs.rav"), "utf8");
  const constant = (name) => {
    const m = new RegExp(`pub const ${name}: num = ([\\d.eE+-]+);`).exec(rav);
    if (!m) throw new Error(`glyphs.rav has no ${name}`);
    return Number(m[1]);
  };
  const list = /pub var gadv: list<num> = \[([\s\S]*?)\];/.exec(rav);
  if (!list) throw new Error("glyphs.rav has no gadv");
  const advances = list[1]
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length)
    .map(Number);
  const index = new Map();
  for (const arm of rav.split("\n")) {
    const m = /^\s*(".*")\s*=>\s*\{\s*return\s+(\d+);\s*\},?\s*$/.exec(arm);
    if (m) index.set(JSON.parse(m[1]), Number(m[2]));
  }
  const json = JSON.parse(readFileSync(join(root, "assets", "glyphs.json"), "utf8"));
  // The stem of every character, both weights, the way the tool named them.
  const info = new Map();
  for (const map of [json.cap, json.bold]) {
    for (const [ch, stem] of Object.entries(map)) {
      const at = index.get(ch.toUpperCase());
      if (at === undefined) continue;
      // A lowercase letter is found at its capital's index and drawn 26 on.
      const i = ch >= "a" && ch <= "z" && !index.has(ch) ? at + 26 : at;
      info.set(stem, {ch, advance: advances[i - 1]});
    }
  }
  return {
    info,
    cap: constant("GCAP"),
    scale: constant("GSCALE"),
    box: constant("GBOX"),
    origin: constant("GORIGIN"),
  };
}

const font = readFont();

// ---------------------------------------------------------------------------
// Drive the pages
// ---------------------------------------------------------------------------

const bytes = readFileSync(join(root, "dist", "chess.sb3"));
const vm = new VirtualMachine();
vm.attachRenderer(renderer);
// The assets come out of the project's own zip; a storage that only has to
// hand back what the deserializer put in is all the VM needs of one, and it is
// what makes a costume's `decodeText` and its skin size real.
const AssetType = {
  ImageVector: {runtimeFormat: "svg", contentType: "image/svg+xml"},
  ImageBitmap: {runtimeFormat: "png", contentType: "image/png"},
  Sound: {runtimeFormat: "wav", contentType: "audio/x-wav"},
  Project: {runtimeFormat: "json", contentType: "application/json"},
};
const DataFormat = {SVG: "svg", PNG: "png", JPG: "jpg", WAV: "wav", JSON: "json"};
let assets = 0;

/**
 * A PNG costume, as an SVG that holds it.
 *
 * The pieces are PNGs, and Scratch draws a bitmap by rasterising it through a
 * canvas, which a Node harness cannot do. The number the HUD's layout is measured
 * against is the costume's *size*, and that is what the file's own header says,
 * so the PNG goes in wrapped: the VM takes its vector path, the skin is the
 * PNG's size in stage units — the same footprint Scratch gives it — and a page
 * this writes shows the piece where Scratch would have put it.
 */
const wrapPng = (data) => {
  const bytes = Buffer.from(data);
  if (bytes.length < 24 || bytes.readUInt32BE(12) !== 0x49484452) return null;
  const w = bytes.readUInt32BE(16);
  const h = bytes.readUInt32BE(20);
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">` +
    `<image width="${w}" height="${h}" href="data:image/png;base64,${bytes.toString("base64")}"/></svg>`
  );
};

const storage = {
  AssetType,
  DataFormat,
  scratchFetch: {
    RequestMetadata: {RunId: "run"},
    setMetadata() {},
    createQueue() {},
    scratchFetch() {},
  },
  createAsset(assetType, dataFormat, data, assetId) {
    const id = assetId || `asset${++assets}`;
    const asset = {
      assetType,
      dataFormat,
      assetId: id,
      data,
      decodeText: () => Buffer.from(data).toString("utf8"),
      encodeDataURI: () => "",
    };
    const wrapped = dataFormat === DataFormat.PNG ? wrapPng(data) : null;
    if (wrapped) {
      asset.assetType = AssetType.ImageVector;
      asset.dataFormat = DataFormat.SVG;
      asset.decodeText = () => wrapped;
    }
    return asset;
  },
};
vm.attachStorage(storage);
console.log(`loading ${(bytes.length / 1048576).toFixed(1)} MiB of project`);
const began = Date.now();
await vm.loadProject(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
vm.start();
console.log(`loaded in ${((Date.now() - began) / 1000).toFixed(1)} s`);

for (const target of vm.runtime.targets) {
  if (target.drawableID !== undefined) targetOfDrawable.set(target.drawableID, target);
}

function mouse(x, y, isDown) {
  vm.postIOData("mouse", {
    x: 240 + x,
    y: 180 - y,
    isDown,
    canvasWidth: 480,
    canvasHeight: 360,
  });
}

async function click(x, y) {
  mouse(x, y, true);
  await sleep(300);
  mouse(x, y, false);
  await sleep(700);
}

/** Press on one square, walk the pointer to another, and let go. */
async function drag(from, to, after) {
  mouse(from.x, from.y, true);
  await sleep(250);
  for (let step = 1; step <= 4; step += 1) {
    mouse(from.x + ((to.x - from.x) * step) / 4, from.y + ((to.y - from.y) * step) / 4, true);
    await sleep(120);
  }
  mouse(to.x, to.y, false);
  if (after) await after();
  await sleep(900);
}

/** The FLIP button, pressed and let go as quickly as the loop can see it. */
async function quickFlip() {
  mouse(num("FLIP_X"), num("P_BTN"), true);
  await sleep(60);
  mouse(num("FLIP_X"), num("P_BTN"), false);
}

// A square is 36 units, the board's bottom left corner is (-222, -144) and the
// centre of file a's first rank is (-204, -126): the numbers `layout.rav` holds
// and `board.rav` tests a click against. With the board turned round, both the
// file and the rank are mirrored.
const square = (name, flipped = false) => {
  let file = "abcdefgh".indexOf(name[0]);
  let rank = Number(name[1]) - 1;
  if (flipped) {
    file = 7 - file;
    rank = 7 - rank;
  }
  return {x: -204 + file * 36, y: -126 + rank * 36};
};

/**
 * What one stamp covers: the ink it is, not the rectangle of its costume.
 *
 * A character's box is its own ink and not its costume: the costume is a box for
 * the whole font, taller and wider than any one character, and a line of text
 * fits inside the patch it was erased with while the costume box of a character
 * in it does not. Everything else is drawn as its costume, at its own size.
 */
function box(m) {
  const s = m.size / 100;
  const glyph = font.info.get(m.costume);
  if (!glyph) {
    return {
      left: m.x - (m.skin.w * s) / 2,
      right: m.x + (m.skin.w * s) / 2,
      bottom: m.y - (m.skin.h * s) / 2,
      top: m.y + (m.skin.h * s) / 2,
    };
  }
  const origin = m.x - (font.box / 2 - font.origin) * s;
  return {
    left: origin,
    right: origin + glyph.advance * font.scale * s,
    bottom: m.y - (font.cap * s) / 2,
    top: m.y + (font.cap * s) / 2,
  };
}

/** The last stamp over a point: what the page shows there. */
function inkAt(marks, x, y) {
  let last = null;
  for (const m of marks) {
    const b = box(m);
    if (x >= b.left && x <= b.right && y >= b.bottom && y <= b.top) last = m;
  }
  return last;
}

/**
 * Everything stamped since the last clear, in the order it was stamped.
 *
 * Every target, not just the HUD: a page is a page of ink, and the board's ink
 * is under it. A stamp of the board that comes after the HUD's own is a stamp on
 * top of the page, which is what drawing a piece over the end-of-game card is,
 * and the order is the only place that can be seen.
 */
function page() {
  const last = stamps.map((s) => s.clear).lastIndexOf(true);
  const marks = stamps.slice(last + 1);
  stamps.length = 0;
  // The layer itself: `patch` is the panel's own colour cut to a line, and it
  // is stamped over a live row to erase it before the row is written again --
  // the clock once a second, the evaluation label as the bar moves. Whatever
  // the patch covers is not on the layer any more, so a row that was rewritten
  // is not two rows on top of each other.
  const live = [];
  for (const m of marks) {
    if (m.costume === "patch") {
      const under = box(m);
      const keeps = live.filter((o) => {
        const b = box(o);
        return !(b.left >= under.left && b.right <= under.right &&
          b.bottom >= under.bottom && b.top <= under.top);
      });
      live.length = 0;
      live.push(...keeps);
    }
    live.push(m);
  }
  return live;
}

let failures = 0;
function fail(what, detail) {
  failures += 1;
  if (failures > 24) return;
  console.log(`FAIL ${what}`);
  if (detail) console.log(`     ${detail}`);
  if (failures === 24) console.log("     ... no more failures are printed");
}

const pages = [];

/** A number out of `src/layout.rav`: where the press has to land. */
const layout = readFileSync(join(root, "src", "layout.rav"), "utf8");
const num = (name) => {
  const m = new RegExp(`pub const ${name}: num = ([\\d.eE+-]+);`).exec(layout);
  if (!m) throw new Error(`layout.rav has no ${name}`);
  return Number(m[1]);
};

// A `pub var` is a cell of the stage's `_gvm` list, not a Scratch variable with
// a name a project can look up, so the page the game is on and the state it is
// in are found by what a press changes: opening the settings puts a 2 in one
// cell, and starting a game puts a 1 in that same cell and in the cell the
// state of the game lives in.
const stage = vm.runtime.targets.find((t) => t.getName() === "Stage");
const gvm = stage.lookupVariableByNameAndType("_gvm", "list");
const seen = () => gvm.value.map((v) => v);
const changed = (before, after) => {
  const at = [];
  for (let i = 0; i < Math.max(before.length, after.length); i += 1) {
    if (String(before[i]) !== String(after[i])) at.push(i);
  }
  return at;
};

vm.greenFlag();
await sleep(1200);
pages.push({name: "menu", marks: page()});

// The settings page, through the button that opens it.
const beforeSettings = seen();
await click(60, -70);
const pageCell = changed(beforeSettings, seen()).filter((i) => Number(gvm.value[i]) === 2);
pages.push({name: "settings", marks: page()});
if (pageCell.length !== 1) {
  fail("the settings page did not take the stage to page 2", `${pageCell.length} cells moved`);
}

// Back, then the black side set to you as well, so PLAY is a game with no bot
// to unpack and the panel is the only thing left to draw.
await click(0, -162);
await sleep(500);
await click(-140, 6);
pages.push({name: "menu-black-you", marks: page()});

const beforePlay = seen();
await click(-60, -70);
await sleep(2500);
const modeCell = changed(beforePlay, seen()).filter(
  (i) => Number(gvm.value[i]) === 1 && !pageCell.includes(i),
);
pages.push({name: "panel", marks: page()});
if (modeCell.length !== 1) {
  fail("starting a game did not leave one cell holding the state of the game",
    `${modeCell.length} cells moved`);
}

// The way back to the menu, pressed while a game is on: the panel's third
// button, which is the one thing the board could not do before.
await click(num("MENU_X"), num("P_BTN"));
await sleep(500);
{
  const back = page();
  if (!back.some((m) => m.costume === "menu-seat")) {
    fail("MENU does not put the menu back while a game is on", `${back.length} stamps`);
  }
  pages.push({name: "menu-again", marks: back});
}

// Then a game again, played to its end with the mouse rather than left on the
// page by writing the state of the game by hand: the scholar's mate, so that the
// last thing to happen before the card is a move like any other, with a piece
// travelling to the square it is played on. What the card has to survive is that
// travel and the frames of it that were still to come.
await click(num("M_PLAY_X"), num("M_PLAY"));
await sleep(1500);
page();

// The same move played twice, once on the board as it starts and once on the
// board turned round. FLIP is the one button that changes where every square is,
// and the piece that travels while it is on has two geometries to be right
// about: where the board draws it, and which squares its flight repaints as it
// crosses them. A square it left that still shows it, or a piece repainted on a
// square it never crossed, is the one geometry being used for the other.
await drag(square("e2"), square("e4"));
await sleep(1500);
pages.push({name: "move", marks: page()});
await click(num("NEW_X"), num("P_BTN"));
await sleep(1500);
page();
await click(num("FLIP_X"), num("P_BTN"));
await sleep(500);
page();
await drag(square("e2", true), square("e4", true));
await sleep(1500);
pages.push({name: "flipped", marks: page()});

// And a piece still in the air when the board is turned round. The flight is
// the board's own space, so a turn is the whole board rotating under it and the
// piece arrives where the square it was sent to now is: a piece whose goal was
// read off the stage before the turn arrives where that square used to be.
await click(num("NEW_X"), num("P_BTN"));
await sleep(1500);
page();
await drag(square("e2"), square("e4"), async () => {
  // Late enough for the release to have been read, early enough for the piece to
  // still be travelling: the flight it turns is the one the probe is about.
  await sleep(150);
  await quickFlip();
});
await sleep(2000);
pages.push({name: "flip-flight", marks: page()});

// And a new game, so the mate below starts from the position that one did.
await click(num("NEW_X"), num("P_BTN"));
await sleep(1500);
page();
for (const [from, to] of [
  ["e2", "e4"], ["e7", "e5"], ["f1", "c4"], ["b8", "c6"],
  ["d1", "h5"], ["g8", "f6"], ["h5", "f7"],
]) {
  await drag(square(from), square(to));
}
await sleep(2000);
{
  const over = page();
  if (!over.some((m) => m.costume === "veil")) {
    fail("the end of a game did not draw its veil", `${over.length} stamps`);
  }
  pages.push({name: "over", marks: over});
}

// And back to a game with moves left in it, so that the search below starts where
// the first game started rather than on a board with nothing left to play.
await click(num("MENU_X"), num("P_BTN"));
await sleep(500);
await click(num("M_PLAY_X"), num("M_PLAY"));
await sleep(1500);
page();

// ---------------------------------------------------------------------------
// What the pages should say
// ---------------------------------------------------------------------------

const round = (v, n = 2) => Number(v.toFixed(n));

/**
 * The runs of text in one page: a stamp of a glyph costume whose pitch against
 * the stamp before it is that glyph's own advance. A stamp that is not the
 * continuation of a run starts a new one, so a run laid out for a size it was
 * not drawn at falls apart into single characters, and the overlap check below
 * is what says so.
 */
function runsOf(marks) {
  const runs = [];
  let open = null;
  for (const m of marks) {
    const glyph = font.info.get(m.costume);
    if (!glyph) {
      open = null;
      continue;
    }
    // The sprite's size is the stage units one costume unit is drawn at, so it
    // is also the factor from the font's own lengths to the stage's. What a
    // caller asked for as the size of the text is the capital's height, which
    // is GCAP of those costume units.
    const scale = m.size / 100;
    const previous = open && font.info.get(open.last.costume);
    const expected = previous ? previous.advance * font.scale * (open.last.size / 100) : 0;
    const continues =
      open &&
      Math.abs(m.y - open.y) < 0.002 &&
      Math.abs(m.size - open.size) < 0.002 &&
      m.x > open.last.x &&
      Math.abs((m.x - open.last.x) / expected - 1) <= 0.25;
    if (continues) {
      open.stamps.push(m);
    } else {
      open = {y: m.y, size: m.size, nominal: (m.size / 100) * font.cap, stamps: [m]};
      runs.push(open);
    }
    open.last = m;
    // The pen starts at the glyph's origin and the stamp is centred on the
    // glyph's box, so the origin is this far back from where it was stamped.
    open.origin = open.stamps[0].x - (font.box / 2 - font.origin) * scale;
    open.wide = open.stamps.reduce((sum, s) => sum + font.info.get(s.costume).advance, 0) *
      font.scale * scale;
  }
  return runs.map((r) => ({
    ...r,
    left: r.origin,
    right: r.origin + r.wide,
    bottom: r.y - r.nominal / 2,
    top: r.y + r.nominal / 2,
    text: r.stamps.map((s) => font.info.get(s.costume).ch).join(""),
  }));
}

function overlap(a, b) {
  return Math.min(a.right, b.right) - Math.max(a.left, b.left) > 0.5 &&
    Math.min(a.top, b.top) - Math.max(a.bottom, b.bottom) > 0.5;
}

/** The HUD's own stamps: what a line of text or a button is written with. */
const hud = (marks) => marks.filter((m) => m.target === "Hud");

/**
 * The board's stamps: the ink that a square of the board is made of. The panel
 * and the page are the other sprite's, and they are laid over the board rather
 * than part of it -- the frame is a costume the size of the stage with the panel
 * drawn in a corner of it, so a question about a square of the board is a
 * question about the board's own stamps.
 */
const board = (marks) => marks.filter((m) => m.target === "Board");

for (const p of pages) {
  const runs = runsOf(hud(p.marks));
  const others = new Map();
  for (const m of hud(p.marks)) {
    if (font.info.has(m.costume)) continue;
    others.set(m.costume, (others.get(m.costume) || 0) + 1);
  }
  console.log(
    `\n== ${p.name}: ${p.marks.length} stamps (${hud(p.marks).length} of them the hud), ` +
    `${runs.length} runs of text`,
  );
  console.log(
    `   not text: ${[...others].map(([name, n]) => `${name} x${n}`).join(", ")}`,
  );
  for (const r of runs) {
    console.log(
      `   y ${String(round(r.y)).padStart(7)}  size ${String(round(r.nominal)).padStart(5)}` +
      `  x ${String(round(r.left)).padStart(7)}..${String(round(r.right)).padStart(7)}` +
      `  ${JSON.stringify(r.text)}`,
    );
  }
  // (A) every character of a run sits at the pitch its own advance gives it.
  for (const r of runs) {
    for (let i = 1; i < r.stamps.length; i += 1) {
      const a = r.stamps[i - 1];
      const b = r.stamps[i];
      const want = font.info.get(a.costume).advance * font.scale * (a.size / 100);
      if (Math.abs(b.x - a.x - want) > 0.05) {
        fail(
          `${p.name}: ${JSON.stringify(r.text)} is not at its own pitch`,
          `${a.costume} to ${b.costume}: ${round(b.x - a.x)} apart, ${round(want)} at size ${a.size}`,
        );
      }
    }
  }
  // (B) no two runs of text are stamped over each other. A run is narrowed by a
  // unit a side, so that two lines whose boxes merely touch -- which is what a
  // side bearing is -- are not called an overlap.
  for (let i = 0; i < runs.length; i += 1) {
    for (let j = i + 1; j < runs.length; j += 1) {
      const a = {...runs[i], left: runs[i].left + 1, right: runs[i].right - 1};
      const b = {...runs[j], left: runs[j].left + 1, right: runs[j].right - 1};
      if (overlap(a, b)) {
        fail(
          `${p.name}: two runs of text overlap`,
          `${JSON.stringify(runs[i].text)} at y ${round(runs[i].y)} and ` +
          `${JSON.stringify(runs[j].text)} at y ${round(runs[j].y)}`,
        );
      }
    }
  }
}

// (C) the menu's two buttons are buttons. This is the one that says a button
// cannot be stamped as whatever glyph the last label ended on.
{
  const menu = pages.find((p) => p.name === "menu");
  const buttons = [[num("M_PLAY_X"), num("M_PLAY")], [num("M_SET_X"), num("M_PLAY")]];
  for (const [x, y] of buttons) {
    const at = hud(menu.marks).filter((m) => Math.abs(m.x - x) < 0.01 && Math.abs(m.y - y) < 0.01);
    if (!at.some((m) => m.costume === "btn-wide")) {
      fail(
        "the menu's buttons are not button costumes",
        `the button at ${x}, ${y} was stamped as ${at.map((m) => m.costume).join(", ") || "nothing"}`,
      );
    }
  }
}

// (D) a seat that was pressed is the seat the row then shows.
{
  const after = pages.find((p) => p.name === "menu-black-you");
  const seats = hud(after.marks).filter((m) => m.costume === "cell-on");
  if (seats.length !== 2) {
    fail("pressing a seat does not light exactly one cell per row", `${seats.length} lit`);
  } else if (!seats.some((s) => Math.abs(s.y - num("M_CARD_B")) < 0.01)) {
    fail("pressing a seat on the black row did not light a cell on it");
  }
}

// (E) nothing is stamped over the card that ends a game. The card is the last
// thing the HUD draws on the page, so the board's ink is under it and every
// board stamp has to come before the veil: a board stamp after it is a piece
// painted across the result, which is what a move still travelling its spring
// did while the search that made it was already over.
{
  const over = pages.find((p) => p.name === "over");
  if (over) {
    const at = over.marks.map((m) => m.costume).lastIndexOf("veil");
    const late = over.marks.slice(at + 1).filter((m) => m.target !== "Hud");
    if (late.length) {
      fail(
        "the board is stamped over the card that ends a game",
        `${late.length} stamp(s) after the veil, the first ${late[0].costume} ` +
        `at ${round(late[0].x)}, ${round(late[0].y)}`,
      );
    }
  }
}

// (F) a move of e2 to e4 leaves e2 empty and e4 with the pawn on it, on the board
// as it starts, on the board turned round, and on the board turned round while
// the piece is in the air. What a page shows at a point is the last stamp over
// it, so a square the flight left is a square the flight repainted: while the
// board is turned round, a repaint that read the position it was given as a
// square rather than as a place on the stage paints the square it would have been
// without turning the board round, which leaves the piece on its old square. And
// a goal read off the stage before the turn is wherever that square used to be,
// which is not the square the piece was sent to.
const PIECE = /^[pd]\d[wb][PNBRQK]$/;
for (const [name, flipped] of [["move", false], ["flipped", true], ["flip-flight", true]]) {
  const p = pages.find((q) => q.name === name);
  if (!p) continue;
  const marks = board(p.marks);
  for (const [what, where] of [["left", "e2"], ["reached", "e4"]]) {
    const at = square(where, flipped);
    const ink = inkAt(marks, at.x, at.y);
    const piece = Boolean(ink && PIECE.test(ink.costume));
    if (what === "left" ? piece : !piece) {
      fail(
        `${name}: the square the piece ${what} ${piece ? "shows it" : "shows no piece"}`,
        `${where} is ${ink ? ink.costume : "nothing"} at ${at.x}, ${at.y}`,
      );
    }
  }
}

// The page a turn in the middle of a flight leaves is only about that if the
// piece was in the air when the board turned: the frames of it that the turn's
// own redraw did not erase are between the two squares, and there are none at all
// if it had already landed.
{
  const p = pages.find((q) => q.name === "flip-flight");
  const from = square("e2", true);
  const to = square("e4", true);
  const air = board(p.marks).filter(
    (m) => PIECE.test(m.costume) &&
      Math.abs(m.x - from.x) < 1 &&
      m.y > to.y + 1 && m.y < from.y - 1,
  );
  if (!air.length) {
    fail("flip-flight: the board turned after the piece had landed, so nothing was checked",
      `${board(p.marks).filter((m) => PIECE.test(m.costume)).length} piece stamp(s)`);
  }
}

// (G) the clock moves while a bot is working, and the panel's buttons answer a
// press while it is. The board runs a search inside one frame of its own loop,
// so the loop is not running while it does: a clock the loop advanced stands
// still for the whole turn, and a press the loop sampled is missed if it comes
// and goes inside the window. The clock is watched through the panel's clock
// line, which the HUD writes; a button is pressed in the middle of the search,
// which is what a game between two bots is almost entirely made of.
if (slow && modeCell.length === 1) {
  gvm.value[modeCell[0]] = 1;
  vm.postIOData("mouse", {x: 240, y: 180, isDown: false, canvasWidth: 480, canvasHeight: 360});
  await sleep(300);
  stamps.length = 0;
  vm.runtime.startHats("event_whenkeypressed", {KEY_OPTION: "y"});
  const seconds = [];
  const until = Date.now() + 60000;
  while (Date.now() < until && seconds.length < 4) {
    await sleep(400);
    for (const r of runsOf(stamps.filter((s) => s.target === "Hud"))) {
      if (Math.abs(r.y - num("P_CLOCK")) > 0.5) continue;
      const m = /(\d+):(\d\d)$/.exec(r.text.trim());
      if (!m) continue;
      const at = Number(m[1]) * 60 + Number(m[2]);
      if (!seconds.includes(at)) seconds.push(at);
    }
  }
  if (seconds.length < 4) {
    fail("the clock does not move while a bot is working", `it showed ${seconds.length} second(s)`);
  } else if (seconds.some((s, i) => i > 0 && s <= seconds[i - 1])) {
    fail("the clock does not go forwards while a bot is working", `${seconds.join(", ")}`);
  } else {
    console.log(`\n== a bot's turn: the clock showed ${seconds.map((s) => `0:0${s}`).join(", ")}`);
  }
  // The search is still running: this press is the whole of the bug report.
  stamps.length = 0;
  await click(num("MENU_X"), num("P_BTN"));
  await sleep(1200);
  const menu = page();
  if (!menu.some((m) => m.costume === "menu-seat")) {
    fail("a panel button pressed while a bot is thinking does nothing", `${menu.length} stamps`);
  } else {
    console.log("== MENU pressed during a search: the menu is back");
  }
  vm.stopAll();
}

// ---------------------------------------------------------------------------
// The pages, drawn
// ---------------------------------------------------------------------------

if (svgDir) {
  mkdirSync(svgDir, {recursive: true});
  for (const p of pages) {
    const body = p.marks
      .filter((m) => m.skin.svg || m.skin.png)
      .map((m) => {
        const rc = m.skin.rc;
        const s = m.size / 100;
        const href = m.skin.png
          ? `data:image/png;base64,${m.skin.png}`
          : `data:image/svg+xml;base64,${Buffer.from(m.skin.svg).toString("base64")}`;
        // Scratch draws a costume with its rotation centre where the sprite
        // is, at the sprite's size, in a stage whose y grows up: the page is
        // the stage turned back the right way round and 240, 180 over.
        return (
          `<g transform="translate(${240 + m.x} ${180 - m.y}) scale(${s}) ` +
          `translate(${-rc[0]} ${-rc[1]})">` +
          `<image width="${m.skin.w}" height="${m.skin.h}" href="${href}"/></g>`
        );
      })
      .join("\n");
    const svg =
      `<svg xmlns="http://www.w3.org/2000/svg" width="480" height="360" viewBox="0 0 480 360">` +
      `<rect width="480" height="360" fill="#1b1d20"/>\n${body}\n</svg>\n`;
    const path = join(svgDir, `hud-${p.name}.svg`);
    writeFileSync(path, svg);
    console.log(`wrote ${path}`);
  }
}

console.log(failures === 0 ? "\nthe HUD is laid out as it measured" : `\n${failures} check(s) failed`);
process.exit(failures === 0 ? 0 : 1);
