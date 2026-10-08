// Make a picture out of the scanout, and write it as a PNG.
//
// The question this project answers is what the Stage holds, and a reader who
// cannot leave the machine running for hours cannot see the answer any other
// way: `dist/stage.png` is the framebuffer the LCD controller is scanning, out
// of SDRAM, as a file.
//
// Two pieces live here and neither needs a dependency.
//
// The first decodes the pixels the controller walks, applying the *same*
// channel routing the controller itself applies (`clcd_rgb565` in
// `src/dev/pl110.rav`, which is why the PLD mode in `SYS_CLCD` and the BGR bit
// in `LCD_CNTL` are read here too), so the file is the picture the Stage shows
// and not what the same memory would look like under a different mode.
//
// The second is a PNG writer. An eight-bit truecolour image with no
// interlacing is a signature, an IHDR, one deflated IDAT and an IEND, each
// chunk carrying its own CRC32, and Node's own zlib does the compression.

import fs from 'node:fs';
import zlib from 'node:zlib';

// ---------------------------------------------------------------------------
// The scanout
// ---------------------------------------------------------------------------

/// The pixel widths the controller can be in, indexed by the control
/// register's three-bit field. This is `clcd_bits` from `src/dev/pl110.rav`.
function bitsOf(control) {
    const field = Math.floor(control / 2) % 8;
    if (field === 0) return 1;
    if (field === 1) return 2;
    if (field === 2) return 4;
    if (field === 3) return 8;
    if (field === 5) return 24;
    return 16;
}

/// One sixteen-bit pixel as `[r, g, b]`, 0..255, through the board's PLD.
///
/// This is `clcd_rgb565` from `src/dev/pl110.rav` written again, because the
/// picture has to agree with the Stage and the Stage is what that procedure
/// drew. Mode two is `SYS_CLCD_MODE_565_R_LSB` -- red in the low five bits, the
/// arrangement the PLD turns over -- and mode three is
/// `SYS_CLCD_MODE_565_B_LSB`, the ordinary one. The two have to be changed
/// together; if they ever disagree, the file is a picture of a machine that
/// does not exist.
function rgb565(word, mode, bgr) {
    let red = (word >> 11) & 31, green = (word >> 5) & 63, blue = word & 31;
    if (mode === 2) { red = word & 31; blue = (word >> 11) & 31; }
    if (mode === 1) { red = (word >> 10) & 31; green = (word >> 5) & 31; blue = word & 31; }
    if (bgr) { const swap = red; red = blue; blue = swap; }
    return [red * 8, green * 4, blue * 8];
}

/// The picture the controller is scanning right now.
///
/// `read` is a name-to-value function over the machine's variables and `ram` is
/// the memory list. Returns `{ width, height, rgb }` with `rgb` a `Buffer` of
/// `width * height * 3` bytes, top row first. Throws rather than guessing when
/// the controller is in a mode it does not decode: sixteen-bit is the only
/// width this machine's kernel programs, and a picture of a mode the guest
/// never selected would be a picture of something that did not happen.
export function scanoutRgb(read, ram) {
    const ubas = Number(read('clcd_ubas')) >>> 0;
    const tim0 = Number(read('clcd_tim0')) >>> 0;
    const tim1 = Number(read('clcd_tim1')) >>> 0;
    const cntl = Number(read('clcd_cntl')) >>> 0;
    const width = ((Math.floor(tim0 / 4) % 64) + 1) * 16;
    const height = (tim1 % 1024) + 1;
    const bits = bitsOf(cntl);
    if (bits !== 16) {
        throw new Error(`the controller is in ${bits}-bit mode; only sixteen is decoded`);
    }
    const sysregs = read('sys_regs') || [];
    const mode = Number(sysregs[20] || 0) % 4;
    const bgr = Math.floor(cntl / 256) % 2 > 0;
    const stride = width * 2;
    const rgb = Buffer.alloc(width * height * 3);
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const address = ubas + y * stride + x * 2;
            const word = (Number(ram[address >>> 2]) >>> ((address & 2) * 8)) & 0xffff;
            const [r, g, b] = rgb565(word, mode, bgr);
            const at = (y * width + x) * 3;
            rgb[at] = r;
            rgb[at + 1] = g;
            rgb[at + 2] = b;
        }
    }
    return { width, height, rgb, ubas, mode, bgr, bits };
}

/// Nearest neighbour. Every source pixel becomes a block of `factor` squared
/// and no colour is invented between them, which is what a picture of *pixels*
/// wants: a bilinear blow-up of an eight-by-sixteen font is a blur.
export function scaleNearest(width, height, rgb, factor) {
    if (factor === 1) return { width, height, rgb };
    const out = Buffer.alloc(width * height * 3 * factor * factor);
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const at = (y * width + x) * 3;
            for (let dy = 0; dy < factor; dy++) {
                const row = (y * factor + dy) * width * factor;
                for (let dx = 0; dx < factor; dx++) {
                    const to = (row + x * factor + dx) * 3;
                    out[to] = rgb[at];
                    out[to + 1] = rgb[at + 1];
                    out[to + 2] = rgb[at + 2];
                }
            }
        }
    }
    return { width: width * factor, height: height * factor, rgb: out };
}

// ---------------------------------------------------------------------------
// The file
// ---------------------------------------------------------------------------

const SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

const CRC_TABLE = (() => {
    const table = new Int32Array(256);
    for (let n = 0; n < 256; n++) {
        let c = n;
        for (let k = 0; k < 8; k++) c = (c & 1) ? (0xedb88320 ^ (c >>> 1)) : (c >>> 1);
        table[n] = c;
    }
    return table;
})();

function crc32(buffer) {
    let c = 0xffffffff;
    for (let i = 0; i < buffer.length; i++) c = CRC_TABLE[(c ^ buffer[i]) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
    const out = Buffer.alloc(data.length + 12);
    out.writeUInt32BE(data.length, 0);
    out.write(type, 4, 'latin1');
    data.copy(out, 8);
    out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
    return out;
}

export function encodePng(width, height, rgb) {
    const ihdr = Buffer.alloc(13);
    ihdr.writeUInt32BE(width, 0);
    ihdr.writeUInt32BE(height, 4);
    ihdr[8] = 8;    // bits per channel
    ihdr[9] = 2;    // colour type 2: truecolour
    ihdr[10] = 0;   // the only compression method the format defines
    ihdr[11] = 0;   // adaptive filtering
    ihdr[12] = 0;   // no interlace
    // Every scanline carries a filter byte and filter zero is "none", which is
    // what a picture with long flat runs compresses to nothing without anyway.
    const stride = width * 3 + 1;
    const raw = Buffer.alloc(height * stride);
    for (let y = 0; y < height; y++) {
        raw[y * stride] = 0;
        rgb.copy(raw, y * stride + 1, y * width * 3, (y + 1) * width * 3);
    }
    return Buffer.concat([
        SIGNATURE,
        chunk('IHDR', ihdr),
        chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
        chunk('IEND', Buffer.alloc(0))
    ]);
}

/// Write the scanout to `file`, blown up by `factor`. Returns a one-line
/// description for whoever asked, or throws if the controller is in a mode
/// this cannot decode -- a caller that is printing a report would rather say
/// why there is no picture than write one that is a guess.
export function writeScanoutPng(file, read, ram, factor = 3) {
    const shot = scanoutRgb(read, ram);
    const big = scaleNearest(shot.width, shot.height, shot.rgb, factor);
    fs.mkdirSync(file.replace(/[\\/][^\\/]*$/, ''), { recursive: true });
    fs.writeFileSync(file, encodePng(big.width, big.height, big.rgb));
    return `${file}  ${big.width}x${big.height} from ${shot.width}x${shot.height} ` +
        `at 0x${shot.ubas.toString(16)}, PLD mode ${shot.mode}${shot.bgr ? ', BGR' : ''}`;
}
