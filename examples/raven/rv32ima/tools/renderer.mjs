// A renderer that records what the pen did instead of drawing it, and the
// rasteriser that turns those records into a picture.
//
// The Scratch pen draws a round-capped line of a diameter, in a colour, and
// nothing else. `recordingRenderer` is the surface the VM calls; `rasterise`
// is the same pen, drawn into a pixel buffer, so that what a check sees is what
// a reader would see.

import zlib from 'node:zlib';
import fs from 'node:fs';

/// What the VM calls. Only the calls these projects make are here, and the
/// fence is the identity: the terminal's grid is inside the pen's box by
/// construction, which is the layout's job and not this file's.
export function recordingRenderer() {
    let nextId = 1;
    const drawables = new Map();
    const lines = [];
    let size = 1;
    return {
        lines,
        sizeOf: () => size,
        setLayerGroupOrdering() {},
        createSVGSkin() { return nextId++; },
        createBitmapSkin() { return nextId++; },
        createTextSkin() { return nextId++; },
        createPenSkin() { return nextId++; },
        destroySkin() {},
        updateSVGSkin() {},
        updateBitmapSkin() {},
        updateTextSkin() {},
        getSkinSize() { return [1, 1]; },
        getSkinRotationCenter() { return [0, 0]; },
        getCurrentSkinSize() { return [1, 1]; },
        getNativeSize() { return [480, 360]; },
        createDrawable() { const id = nextId++; drawables.set(id, { position: [0, 0] }); return id; },
        destroyDrawable(id) { drawables.delete(id); },
        updateDrawableSkinId() {},
        updateDrawablePosition(id, position) {
            const d = drawables.get(id);
            if (d) d.position = [position[0], position[1]];
        },
        updateDrawableDirectionScale() {},
        updateDrawableVisible() {},
        updateDrawableEffect() {},
        setDrawableOrder() {},
        getDrawableOrder() { return 0; },
        // The stage is a build argument, not something the project measures, so
        // nothing here has to know where the fence is.
        getFencedPositionOfDrawable(_id, position) { return [position[0], position[1]]; },
        getBounds() { return { left: -STAGE_W / 2, right: STAGE_W / 2, top: STAGE_H / 2, bottom: -STAGE_H / 2 }; },
        getBoundsForBubble() { return { left: 0, right: 0, top: 0, bottom: 0 }; },
        pick() { return -1; },
        drawableTouching() { return false; },
        drawableTouchingScratchPoint() { return false; },
        drawableTouchingScratchRect() { return false; },
        isTouchingColor() { return false; },
        isTouchingDrawables() { return false; },
        penClear() { lines.length = 0; },
        penStamp() {},
        penLine(_skin, attrs, x0, y0, x1, y1) {
            size = attrs.diameter;
            // A copy: the pen extension hands out the same `color4f` array every
            // time and writes the next colour into it, so a recorded reference
            // would read back as whatever colour was set last.
            lines.push({ x0, y0, x1, y1, pen: attrs.diameter, colour: attrs.color4f.slice() });
        },
        penPoint(_skin, attrs, x, y) {
            size = attrs.diameter;
            lines.push({ x0: x, y0: y, x1: x, y1: y, pen: attrs.diameter, colour: attrs.color4f.slice() });
        },
        draw() {}
    };
}

/// The stage, drawn from what the pen did: one buffer, `p_background` to start
/// with, and every line a run of round discs the width of the pen.
///
/// Stage coordinates are `STAGE_W` by `STAGE_H` with (0, 0) in the middle and y
/// up. The size is the one the project is *run* in and not always Scratch's: the
/// terminal measures its stage and lays its page out from the answer, so a
/// picture of a project that was resized -- TurboWarp's stage size -- has to be
/// taken at that size too, or it is a picture of a different project.
export function rasterise(lines, p_background = [0, 0, 0], W = 480, H = 360) {
    const pixels = new Uint8Array(W * H * 3);
    for (let i = 0; i < W * H; i++) {
        pixels[i * 3] = p_background[0];
        pixels[i * 3 + 1] = p_background[1];
        pixels[i * 3 + 2] = p_background[2];
    }
    const disc = (cx, cy, radius, rgb) => {
        const x0 = Math.max(0, Math.floor(cx - radius)), x1 = Math.min(W - 1, Math.ceil(cx + radius));
        const y0 = Math.max(0, Math.floor(cy - radius)), y1 = Math.min(H - 1, Math.ceil(cy + radius));
        for (let y = y0; y <= y1; y++) {
            for (let x = x0; x <= x1; x++) {
                const dx = x + 0.5 - cx, dy = y + 0.5 - cy;
                if (dx * dx + dy * dy > radius * radius) continue;
                const o = (y * W + x) * 3;
                pixels[o] = rgb[0];
                pixels[o + 1] = rgb[1];
                pixels[o + 2] = rgb[2];
            }
        }
    };
    for (const line of lines) {
        const rgb = line.colour.slice(0, 3).map((v) => Math.round(Math.max(0, Math.min(1, v)) * 255));
        const radius = Math.max(0.5, line.pen / 2);
        // Stage to pixels: x right from the middle, y up from the middle.
        const sx0 = line.x0 + W / 2, sy0 = H / 2 - line.y0;
        const sx1 = line.x1 + W / 2, sy1 = H / 2 - line.y1;
        const steps = Math.max(1, Math.ceil(Math.hypot(sx1 - sx0, sy1 - sy0) / Math.max(0.5, radius / 2)));
        for (let i = 0; i <= steps; i++) {
            const t = steps === 0 ? 0 : i / steps;
            disc(sx0 + (sx1 - sx0) * t, sy0 + (sy1 - sy0) * t, radius, rgb);
        }
    }
    return { width: W, height: H, pixels };
}

/// A PNG, because a picture is the only way to check a picture.
export function writePng(file, image) {
    const { width, height, pixels } = image;
    const raw = Buffer.alloc((width * 3 + 1) * height);
    for (let y = 0; y < height; y++) {
        raw[y * (width * 3 + 1)] = 0;
        Buffer.from(pixels.buffer, pixels.byteOffset + y * width * 3, width * 3)
            .copy(raw, y * (width * 3 + 1) + 1);
    }
    const chunk = (type, body) => {
        const out = Buffer.alloc(12 + body.length);
        out.writeUInt32BE(body.length, 0);
        out.write(type, 4, 'ascii');
        body.copy(out, 8);
        out.writeUInt32BE(zlib.crc32(Buffer.concat([Buffer.from(type, 'ascii'), body])), 8 + body.length);
        return out;
    };
    const ihdr = Buffer.alloc(13);
    ihdr.writeUInt32BE(width, 0);
    ihdr.writeUInt32BE(height, 4);
    ihdr[8] = 8;    // eight bits a channel
    ihdr[9] = 2;    // truecolour
    fs.writeFileSync(file, Buffer.concat([
        Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
        chunk('IHDR', ihdr),
        chunk('IDAT', zlib.deflateSync(raw)),
        chunk('IEND', Buffer.alloc(0))
    ]));
}
