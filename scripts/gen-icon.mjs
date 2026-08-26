// Generates a simple placeholder 1024x1024 app icon: a dark veil of
// concentric layers parting toward a warm core - "peel back the layers to
// find what's hidden underneath". No dependencies; hand-rolls a PNG encoder
// (raw scanlines + zlib deflate + manual chunk/CRC framing) so this can run
// with nothing but Node itself.
//
// Usage: node scripts/gen-icon.mjs  (writes src-tauri/icons/source.png)
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SIZE = 1024;
const OUT = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "icons", "source.png");

const outer = [13, 11, 26];   // #0d0b1a - deep almost-black plum, outer edge
const mid = [61, 47, 114];    // #3d2f72 - mid violet
const inner = [233, 196, 106]; // #e9c46a - warm gold core, "the light behind the veil"

function lerp(a, b, t) {
  return a + (b - a) * t;
}

function colorAt(t) {
  // t: 0 (outer edge) -> 1 (center). Two-stage gradient: outer->mid->inner.
  if (t < 0.55) {
    const local = t / 0.55;
    return outer.map((c, i) => lerp(c, mid[i], local));
  }
  const local = (t - 0.55) / 0.45;
  return mid.map((c, i) => lerp(c, inner[i], local));
}

const cx = SIZE / 2;
const cy = SIZE / 2;
const maxR = SIZE * 0.47;
// A few thin darker "seam" rings to read as onion-like layers.
const seams = [0.32, 0.52, 0.7, 0.85].map((f) => f * maxR);

const raw = Buffer.alloc((SIZE * 4 + 1) * SIZE);
let pos = 0;
for (let y = 0; y < SIZE; y++) {
  raw[pos++] = 0; // no filter for this scanline
  for (let x = 0; x < SIZE; x++) {
    const dx = x - cx;
    const dy = y - cy;
    const r = Math.sqrt(dx * dx + dy * dy);

    let rgb;
    let alpha = 255;
    if (r > maxR + 2) {
      rgb = [0, 0, 0];
      alpha = 0; // fully transparent outside the disc
    } else {
      const t = 1 - Math.min(r / maxR, 1);
      rgb = colorAt(Math.pow(t, 0.85));
      // Soft-edge anti-aliasing right at the disc boundary.
      if (r > maxR - 2) alpha = Math.round(255 * (maxR - r + 2) / 4);
      for (const s of seams) {
        if (Math.abs(r - s) < 1.4) {
          const darken = 0.55;
          rgb = rgb.map((c) => c * darken);
        }
      }
    }

    raw[pos++] = Math.round(rgb[0]);
    raw[pos++] = Math.round(rgb[1]);
    raw[pos++] = Math.round(rgb[2]);
    raw[pos++] = alpha;
  }
}

function crc32(buf) {
  let c;
  const table = crc32.table || (crc32.table = (() => {
    const t = new Uint32Array(256);
    for (let n = 0; n < 256; n++) {
      c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      t[n] = c >>> 0;
    }
    return t;
  })());
  let crc = 0xffffffff;
  for (let i = 0; i < buf.length; i++) crc = table[(crc ^ buf[i]) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeAndData = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(typeAndData), 0);
  return Buffer.concat([len, typeAndData, crc]);
}

const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // color type: RGBA
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;

const idat = deflateSync(raw, { level: 9 });

const png = Buffer.concat([
  signature,
  chunk("IHDR", ihdr),
  chunk("IDAT", idat),
  chunk("IEND", Buffer.alloc(0)),
]);

mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, png);
console.log(`Wrote ${OUT} (${png.length} bytes)`);
