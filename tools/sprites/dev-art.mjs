#!/usr/bin/env node
// Draws the dev-art pixel sprites: placeholder PNGs in assets/sprites/, one per
// tag the game looks for (docs/sprites.md lists them, with sizes and anchors).
// Run `node tools/sprites/dev-art.mjs` from anywhere; it overwrites every file
// it makes and leaves any other file alone. `--sheet out.png [prefix]` writes
// the sprites (or those whose tag starts with `prefix`) blown up on one sheet
// instead, for looking them over. No dependencies: it writes the PNGs itself
// (zlib is Node's).
//
// Everything is drawn in sRGB, as an artist would. Pure magenta in any shade
// (red equal to blue, no green) is the team color: the game paints a unit's,
// city's or building's side color there, scaled by the magenta's brightness.
// Shapes are filled at pixel centers and most get a one-pixel dark outline
// (`outlined`), which keeps them readable on any ground.

import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateSync } from 'node:zlib';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const OUT = join(ROOT, 'assets', 'sprites');

// ---------------------------------------------------------------- PNG output

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(bytes) {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const head = Buffer.alloc(8);
  head.writeUInt32BE(data.length, 0);
  head.write(type, 4, 'ascii');
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])), 0);
  return Buffer.concat([head, data, crc]);
}

function encodePng(image) {
  const { w, h, data } = image;
  const header = Buffer.alloc(13);
  header.writeUInt32BE(w, 0);
  header.writeUInt32BE(h, 4);
  header[8] = 8; // bit depth
  header[9] = 6; // RGBA
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0; // no filter
    Buffer.from(data.buffer, y * w * 4, w * 4).copy(raw, y * (w * 4 + 1) + 1);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ---------------------------------------------------------------- drawing

/** '#rrggbb' as [r, g, b, 255]. */
function rgb(hex) {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 255];
}

function mix(a, b, t) {
  return a.map((v, i) => Math.round(v + (b[i] - v) * t));
}

const same = (a, b) => a.every((v, i) => v === b[i]);

class Image {
  constructor(w, h) {
    this.w = w;
    this.h = h;
    this.data = new Uint8Array(w * h * 4);
  }
  inside(x, y) {
    return x >= 0 && y >= 0 && x < this.w && y < this.h;
  }
  get(x, y) {
    if (!this.inside(x, y)) return [0, 0, 0, 0];
    const i = (y * this.w + x) * 4;
    return Array.from(this.data.subarray(i, i + 4));
  }
  opaque(x, y) {
    return this.get(x, y)[3] > 0;
  }
  set(x, y, color) {
    if (!color || !this.inside(x, y)) return;
    this.data.set(color, (y * this.w + x) * 4);
  }
  /** Rows of characters at (x0, y0); each character is a palette key, and
   * '.' or a key missing from the palette leaves the pixel as it was. */
  stamp(x0, y0, rows, palette) {
    rows.forEach((row, dy) => {
      [...row].forEach((ch, dx) => this.set(x0 + dx, y0 + dy, palette[ch]));
    });
  }
  /** Sets every pixel whose center `inside` accepts. */
  fill(inside, color) {
    for (let y = 0; y < this.h; y++) {
      for (let x = 0; x < this.w; x++) if (inside(x + 0.5, y + 0.5)) this.set(x, y, color);
    }
  }
  /** Clears every pixel `keep` refuses. */
  mask(keep) {
    for (let y = 0; y < this.h; y++) {
      for (let x = 0; x < this.w; x++) if (!keep(x, y)) this.set(x, y, [0, 0, 0, 0]);
    }
  }
  /** `other`'s opaque pixels over this one's, its corner at (x0, y0). */
  over(other, x0 = 0, y0 = 0) {
    for (let y = 0; y < other.h; y++) {
      for (let x = 0; x < other.w; x++) {
        if (other.opaque(x, y)) this.set(x0 + x, y0 + y, other.get(x, y));
      }
    }
    return this;
  }
}

// Shapes, as tests of a point (a pixel's center).
const ellipse = (cx, cy, rx, ry) => (x, y) => ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2 <= 1;
const rect = (x0, y0, x1, y1) => (x, y) => x >= x0 && x < x1 && y >= y0 && y < y1;
function segment(x0, y0, x1, y1, width) {
  return (x, y) => {
    const [dx, dy] = [x1 - x0, y1 - y0];
    const t = Math.max(0, Math.min(1, ((x - x0) * dx + (y - y0) * dy) / (dx * dx + dy * dy || 1)));
    return Math.hypot(x - x0 - t * dx, y - y0 - t * dy) <= width / 2;
  };
}
function polygon(points) {
  return (x, y) => {
    let inside = false;
    for (let i = 0, j = points.length - 1; i < points.length; j = i++) {
      const [xi, yi] = points[i];
      const [xj, yj] = points[j];
      if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
    }
    return inside;
  };
}

/** A layer of `shapes` (each [test, color], painted in order), with a
 * one-pixel `edge` round the whole of it. */
function outlined(w, h, shapes, edge = INK) {
  const layer = new Image(w, h);
  for (const [inside, color] of shapes) layer.fill(inside, color);
  const out = new Image(w, h);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      if (layer.opaque(x, y)) continue;
      const near = [[-1, 0], [1, 0], [0, -1], [0, 1]].some(([dx, dy]) => layer.opaque(x + dx, y + dy));
      if (near) out.set(x, y, edge);
    }
  }
  return out.over(layer);
}

/** A fixed hash of a pixel, from 0 to 1: the same picture every run. */
function noise(x, y, salt) {
  let h = Math.imul(x + 1, 374761393) ^ Math.imul(y + 1, 668265263) ^ Math.imul(salt + 7, 2246822519);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

/** Smooth value noise, about `cell` pixels across. */
function smooth(x, y, cell, salt) {
  const fx = x / cell;
  const fy = y / cell;
  const [ix, iy] = [Math.floor(fx), Math.floor(fy)];
  const [tx, ty] = [fx - ix, fy - iy].map((t) => t * t * (3 - 2 * t));
  const at = (dx, dy) => noise(ix + dx, iy + dy, salt);
  const top = at(0, 0) + (at(1, 0) - at(0, 0)) * tx;
  const bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * tx;
  return top + (bottom - top) * ty;
}

// ---------------------------------------------------------------- colors

const INK = rgb('#16161a');
const TEAM = rgb('#ff00ff');
const TEAM_SHADE = rgb('#b400b4');
const TEAM_DARK = rgb('#800080');
const PALE = rgb('#e4e0d4');
const WHITE = rgb('#f2f2ee');
const STEEL = rgb('#c0c8d2');
const STEEL_DARK = rgb('#7c8696');
const WOOD = rgb('#9a6432');
const WOOD_DARK = rgb('#5e3a1a');
const GOLD = rgb('#e0b040');
const GOLD_DARK = rgb('#a87820');
const STONE = rgb('#b8b0a0');
const STONE_DARK = rgb('#8a8272');

// ---------------------------------------------------------------- terrain

const TILE_W = 64;
const TILE_H = 56;

/** Whether a pixel of a tile is inside its flat-top hexagon. */
function inHex(x, y) {
  const dx = Math.abs(x + 0.5 - TILE_W / 2);
  const dy = Math.abs(y + 0.5 - TILE_H / 2);
  return dy <= TILE_H / 2 && dx <= TILE_W / 2 - (dy * (TILE_W / 4)) / (TILE_H / 2);
}

const GROUNDS = {
  grassland: { base: '#5c8a3c', light: '#6f9e47', dark: '#4b7431', mark: '#88b858' },
  plains: { base: '#8c8640', light: '#a09a4c', dark: '#736e35', mark: '#bdb262' },
  desert: { base: '#c2a160', light: '#d2b474', dark: '#a8884c', mark: '#94733a' },
  tundra: { base: '#6e7568', light: '#7f877a', dark: '#5c6257', mark: '#9aa38f' },
  snow: { base: '#d4dce6', light: '#eef2f7', dark: '#b6c2d0', mark: '#a6b4c6' },
  marsh: { base: '#4a6650', light: '#587a5c', dark: '#3c5644', mark: '#7e9c50' },
  mountains: { base: '#57514e', light: '#6a6461', dark: '#433e3c', mark: '#8a8480' },
  coast: { base: '#2e5f92', light: '#3a6ea2', dark: '#27537f', mark: '#6aa0d4' },
  ocean: { base: '#173a6e', light: '#1d4580', dark: '#13315e', mark: '#3c6aa8' },
  lake: { base: '#2c6b8e', light: '#377b9e', dark: '#255d7c', mark: '#78b2d0' },
};

/** Ground of `name`: mottled base in three shades, its own marks, hills if
 * asked, and a bevel round the hexagon. */
function ground(name, hills) {
  const g = GROUNDS[name];
  const [base, light, dark, mark] = [g.base, g.light, g.dark, g.mark].map(rgb);
  const img = new Image(TILE_W, TILE_H);
  const salt = name.length * 31;
  for (let y = 0; y < TILE_H; y++) {
    for (let x = 0; x < TILE_W; x++) {
      const n = smooth(x, y, 6, salt) * 0.7 + noise(x, y, salt) * 0.3;
      img.set(x, y, n < 0.3 ? dark : n > 0.68 ? light : base);
    }
  }
  marks(img, name, { light, dark, mark });
  if (hills) drawHills(img, base, light, dark);
  rim(img, light, dark);
  img.mask(inHex);
  return img;
}

/** Places for `count` marks `w` by `h` on a tile, kept off its rim. */
function spots(count, salt, w = 8, h = 6) {
  const found = [];
  for (let i = 0; found.length < count && i < count * 8; i++) {
    const x = 4 + Math.floor(noise(i, 0, salt) * (TILE_W - 8 - w));
    const y = 4 + Math.floor(noise(i, 1, salt) * (TILE_H - 8 - h));
    if (inHex(x, y) && inHex(x + w, y + h) && inHex(x, y + h) && inHex(x + w, y)) found.push([x, y]);
  }
  return found;
}

function marks(img, name, { light, dark, mark }) {
  switch (name) {
    case 'grassland':
    case 'tundra': {
      const tuft = ['#...#', '.#.#.', '.#.#.', '..#..'];
      for (const [x, y] of spots(name === 'grassland' ? 14 : 9, 3)) img.stamp(x, y, tuft, { '#': mark });
      if (name === 'grassland') {
        const flower = ['.f.', 'fyf', '.f.'];
        for (const [x, y] of spots(4, 17)) img.stamp(x, y, flower, { f: rgb('#e8e0a0'), y: rgb('#d8a030') });
      } else {
        for (const [x, y] of spots(4, 23)) img.stamp(x, y, ['.oo.', 'oOOo', '.oo.'], { o: dark, O: rgb('#8e9488') });
      }
      break;
    }
    case 'plains':
      for (const [x, y] of spots(14, 5)) img.stamp(x, y, ['#..', '#.#', '.##', '.#.'], { '#': mark });
      break;
    case 'desert':
      for (const [x, y] of [[8, 12], [32, 20], [12, 36], [38, 40], [24, 6]]) {
        img.stamp(x, y, ['....######....', '..##llllll##..', '##..........##'], { '#': mark, l: light });
      }
      break;
    case 'snow':
      for (const [x, y] of [[10, 14], [34, 10], [18, 34], [40, 36]]) {
        img.stamp(x, y, ['...####...', '.##llll##.', '#........#'], { '#': mark, l: light });
      }
      break;
    case 'marsh': {
      const pool = rgb('#35586c');
      const shine = rgb('#5a86a0');
      for (const [x, y] of [[8, 14], [36, 10], [20, 32], [42, 34]]) {
        img.stamp(x, y, ['..######..', '.##ss####.', '##########', '.########.', '..######..'], { '#': pool, s: shine });
      }
      for (const [x, y] of [[22, 12], [48, 22], [10, 30], [32, 42], [26, 22]]) {
        img.stamp(x, y, ['r.r.r', 'r.r.r', '#.#.#', '#.#.#', '.###.'], { '#': mark, r: rgb('#6a4a2a') });
      }
      break;
    }
    case 'mountains': {
      const rock = rgb('#7c7572');
      const shade = rgb('#48423f');
      const snow = rgb('#eef2f6');
      const snowShade = rgb('#b8c4d4');
      const outline = rgb('#28241f');
      const peak = (cx, top, bottom, slope, snowLine) => {
        for (let y = top; y < bottom; y++) {
          const half = (y - top) * slope;
          for (let x = 0; x < TILE_W; x++) {
            const dx = x + 0.5 - cx;
            if (Math.abs(dx) > half + 0.5) continue;
            const edge = Math.abs(dx) > half - 0.6 || y === bottom - 1;
            const ridge = Math.abs(dx - (y - top) * 0.15) < 0.6 && y > snowLine;
            let color = y < snowLine ? (dx < 0 ? snow : snowShade) : dx < 0 ? rock : shade;
            if (ridge) color = shade;
            img.set(x, y, edge ? outline : color);
          }
        }
      };
      peak(46, 14, 48, 0.75, 22);
      peak(28, 4, 50, 0.72, 16);
      break;
    }
    case 'coast':
    case 'ocean':
    case 'lake': {
      const count = name === 'ocean' ? 6 : 9;
      for (const [x, y] of spots(count, 11)) img.stamp(x, y, ['..###....', '.#lll##..', '#.....###'], { '#': mark, l: light });
      if (name === 'coast') for (const [x, y] of spots(5, 29, 2, 2)) img.stamp(x, y, ['ww'], { w: rgb('#a8d0f0') });
      break;
    }
    default:
      break;
  }
}

/** Rounded hills along the bottom, lit from the upper left. */
function drawHills(img, base, light, dark) {
  const outline = mix(dark, [0, 0, 0, 255], 0.45);
  const hill = mix(base, dark, 0.3);
  const lit = mix(base, light, 0.9);
  const shade = mix(dark, [0, 0, 0, 255], 0.15);
  for (const [cx, cy, rx, ry] of [[20, 50, 16, 14], [44, 51, 14, 12], [32, 54, 10, 7]]) {
    for (let y = cy - ry - 1; y <= cy; y++) {
      for (let x = cx - rx - 1; x <= cx + rx + 1; x++) {
        const dx = (x + 0.5 - cx) / rx;
        const dy = (y + 0.5 - cy) / ry;
        if (dx * dx + dy * dy > 1) continue;
        const ix = (x + 0.5 - cx) / (rx - 1);
        const iy = (y + 0.5 - cy) / (ry - 1);
        let color = hill;
        if (dx < -0.1 && dy < -0.35) color = lit;
        else if (dx > 0.45) color = shade;
        img.set(x, y, ix * ix + iy * iy > 1 ? outline : color);
      }
    }
  }
}

/** A one-pixel bevel round the hexagon. */
function rim(img, light, dark) {
  for (let y = 0; y < TILE_H; y++) {
    for (let x = 0; x < TILE_W; x++) {
      if (!inHex(x, y)) continue;
      const edge = [[-1, 0], [1, 0], [0, -1], [0, 1]].some(([dx, dy]) => !inHex(x + dx, y + dy));
      if (!edge) continue;
      const upperLeft = x + 0.5 - TILE_W / 2 + (y + 0.5 - TILE_H / 2) < 0;
      img.set(x, y, upperLeft ? light : dark);
    }
  }
}

/** A pine, 13 wide and 20 tall, lit from the left. */
function pine() {
  const shapes = [];
  for (const [top, half, bottom] of [[0, 3, 7], [4, 5, 12], [9, 6.5, 17]]) {
    shapes.push([polygon([[6.5, top], [6.5 - half, bottom], [6.5 + half, bottom]]), rgb('#2c5a24')]);
    shapes.push([polygon([[6.5, top], [6.5 - half, bottom], [5.5, bottom]]), rgb('#3f7a32')]);
  }
  shapes.push([rect(5.5, 17, 7.5, 20), rgb('#4a2e18')]);
  return outlined(13, 21, shapes, rgb('#14280f'));
}

/** A jungle canopy, 15 wide and 13 tall. */
function canopy(salt) {
  const shapes = [
    [ellipse(4.5, 6, 4, 3.5), rgb('#23603a')],
    [ellipse(10, 5, 4.5, 4), rgb('#23603a')],
    [ellipse(7.5, 3.5, 3.5, 3), rgb('#3c8a4c')],
    [rect(6.5, 9, 8.5, 12), rgb('#3e2a16')],
  ];
  const img = outlined(16, 14, shapes, rgb('#0c2614'));
  for (let i = 0; i < 4; i++) {
    const x = 2 + Math.floor(noise(i, 3, salt) * 11);
    const y = 2 + Math.floor(noise(i, 4, salt) * 6);
    if (img.get(x, y)[1] > 60) img.set(x, y, rgb('#8ab03c'));
  }
  return img;
}

/** A feature drawn over any ground: trees across the top and sides, where
 * hills (along the bottom) and units (in the middle) don't sit. */
function feature(name) {
  const img = new Image(TILE_W, TILE_H);
  const places = [[16, 2], [32, 0], [44, 4], [6, 16], [50, 18], [24, 10], [38, 12]];
  places.forEach(([x, y], i) => img.over(name === 'forest' ? pine() : canopy(i), x, y));
  img.mask(inHex);
  return img;
}

// ---------------------------------------------------------------- tokens

const UNIT = 28;

/** A military unit's token: a disc in the team color with a dark rim,
 * shaded toward the lower right. */
function disc() {
  const img = new Image(UNIT, UNIT);
  const c = UNIT / 2;
  const inDisc = (x, y) => Math.hypot(x - c, y - c) <= c - 1.2;
  img.fill((x, y) => Math.hypot(x - c, y - c) <= c, INK);
  img.fill(inDisc, TEAM);
  img.fill((x, y) => inDisc(x, y) && Math.hypot(x - c + 2.5, y - c + 2.5) > c - 2.2, TEAM_SHADE);
  return img;
}

/** A civilian's token: a pale pointy-top hexagon in a team-colored rim. */
function hexToken() {
  const img = new Image(UNIT, UNIT);
  const c = UNIT / 2;
  const hex = (r) => (x, y) => {
    const dx = Math.abs(x - c);
    const dy = Math.abs(y - c);
    return dx <= r * 0.866 && dy <= r - dx * 0.577;
  };
  img.fill(hex(c), INK);
  img.fill(hex(c - 1.3), TEAM);
  img.fill(hex(c - 4), PALE);
  return img;
}

// Pictograms on the tokens: shapes on a 28x28 grid, outlined in ink.
const PICTOGRAMS = {
  melee: [
    [polygon([[12.5, 18], [12.5, 7], [14, 3.5], [15.5, 7], [15.5, 18]]), STEEL],
    [rect(13.5, 7, 14.5, 17), STEEL_DARK],
    [rect(8, 17, 20, 19.5), GOLD],
    [rect(12.5, 19.5, 15.5, 23), WOOD],
    [ellipse(14, 24, 2, 1.6), GOLD],
  ],
  ranged: [
    ...Array.from({ length: 12 }, (_, i) => {
      const a = (-70 + (140 * i) / 12) * (Math.PI / 180);
      const b = (-70 + (140 * (i + 1)) / 12) * (Math.PI / 180);
      return [segment(9 + 9 * Math.cos(a), 14 + 9 * Math.sin(a), 9 + 9 * Math.cos(b), 14 + 9 * Math.sin(b), 2.2), WOOD];
    }),
    [segment(12.1, 5.5, 12.1, 22.5, 1), WHITE],
    [segment(5, 14, 21, 14, 1.4), WOOD_DARK],
    [polygon([[20.5, 11], [25, 14], [20.5, 17]]), STEEL],
    [polygon([[4, 11.5], [8, 14], [4, 16.5]]), WHITE],
  ],
  cavalry: [
    [polygon([[8, 23], [9, 14], [12, 8], [14, 4], [16, 7.5], [22, 12], [23, 16], [20, 17.5], [15, 15], [16, 23]]), rgb('#9a5a2a')],
    [segment(12, 8, 9, 13, 2.2), rgb('#3a2010')],
    [segment(9, 13, 8.5, 20, 2.2), rgb('#3a2010')],
    [ellipse(16.5, 11, 1, 1), INK],
    [ellipse(21.5, 15.5, 0.8, 0.8), INK],
  ],
  siege: [
    [polygon([[10, 19], [14, 10], [17, 19]]), WOOD_DARK],
    [segment(12, 18, 21, 7, 2.2), WOOD],
    [ellipse(21.5, 6, 2.6, 2.2), WOOD_DARK],
    [ellipse(21.5, 5, 1.5, 1.2), STONE],
    [rect(5, 18, 23, 20.5), WOOD],
    [ellipse(9, 21.5, 2.8, 2.8), WOOD_DARK],
    [ellipse(19, 21.5, 2.8, 2.8), WOOD_DARK],
    [ellipse(9, 21.5, 0.8, 0.8), GOLD],
    [ellipse(19, 21.5, 0.8, 0.8), GOLD],
  ],
  scout: [
    [segment(5.5, 22.5, 10, 18, 3), GOLD_DARK],
    [segment(10, 18, 17, 11, 4), GOLD],
    [segment(17, 11, 21.5, 6.5, 5), rgb('#f0c860')],
    [ellipse(22.2, 5.8, 1.8, 1.8), rgb('#9fd0ff')],
  ],
  armored: [
    [polygon([[6.5, 5], [21.5, 5], [21.5, 13], [18, 19.5], [14, 23.5], [10, 19.5], [6.5, 13]]), STEEL],
    [polygon([[6.5, 5], [14, 5], [14, 23.5], [10, 19.5], [6.5, 13]]), rgb('#d8dee6')],
    [rect(12.5, 7, 15.5, 20), GOLD],
    [rect(8.5, 10, 19.5, 12.5), GOLD],
  ],
  pikeman: [
    [segment(5.5, 23.5, 19, 9.5, 1.8), WOOD],
    [polygon([[17, 10.5], [18.5, 6], [23.5, 3], [21.5, 8.5]]), STEEL],
    [segment(16.5, 9, 19.5, 12, 1.4), GOLD],
  ],
  patrol_galley: [
    [segment(14, 16, 14, 4.5, 1.6), WOOD_DARK],
    [polygon([[15, 5.5], [21.5, 9], [15, 13.5]]), WHITE],
    [polygon([[3.5, 15.5], [24.5, 15.5], [21, 20.5], [7, 20.5]]), WOOD],
    [rect(5, 15.5, 23, 17), WOOD_DARK],
    ...[8, 12, 16, 20].map((x) => [segment(x, 20, x - 3, 24.5, 1), WOOD_DARK]),
  ],
  landing_craft: [
    [ellipse(9, 12, 2, 2), STEEL],
    [ellipse(14, 12, 2, 2), STEEL],
    [ellipse(19, 12, 2, 2), STEEL],
    [polygon([[3.5, 14], [24.5, 14], [24.5, 20.5], [6, 21.5], [3.5, 18.5]]), WOOD],
    [rect(4, 14, 24.5, 15.5), WOOD_DARK],
    [polygon([[24.5, 14], [26, 11], [26, 20.5], [24.5, 20.5]]), WOOD_DARK],
  ],
  bombard_ship: [
    [segment(12, 11.5, 23.5, 7.5, 2.8), rgb('#3c3c44')],
    [rect(9, 11.5, 19, 16), WOOD_DARK],
    [ellipse(12, 12.5, 1.8, 1.8), GOLD],
    [polygon([[3.5, 16], [24.5, 16], [21, 21.5], [7, 21.5]]), WOOD],
    [rect(5, 16, 23, 17.5), WOOD_DARK],
  ],
  settler: [
    [ellipse(9, 24, 5, 1.8), WOOD_DARK],
    [segment(9, 4, 9, 23.5, 1.8), WOOD],
    [polygon([[10, 4.5], [22, 7], [18.5, 10], [22, 13.5], [10, 13.5]]), TEAM],
    [polygon([[10, 10], [20, 10.6], [22, 13.5], [10, 13.5]]), TEAM_SHADE],
  ],
  worker: [
    [segment(8.5, 5, 16, 16.5, 1.8), WOOD],
    [rect(6.5, 3, 11.5, 5), WOOD_DARK],
    [polygon([[14, 15], [19, 13], [22.5, 19], [19.5, 23.5], [14.5, 21]]), STEEL],
    [polygon([[14, 15], [16.5, 14], [17, 22.5], [14.5, 21]]), rgb('#e0e6ee')],
  ],
  wolf_pack: [
    [polygon([[6.5, 10], [8, 3], [12.5, 10]]), rgb('#7a7a82')],
    [polygon([[21.5, 10], [20, 3], [15.5, 10]]), rgb('#7a7a82')],
    [polygon([[6.5, 9], [11, 11], [17, 11], [21.5, 9], [21.5, 15.5], [17.5, 21], [14, 23.5], [10.5, 21], [6.5, 15.5]]), rgb('#8e8e96')],
    [polygon([[8.5, 5.5], [9, 9], [11, 9.5]]), rgb('#5a5a62')],
    [polygon([[19.5, 5.5], [19, 9], [17, 9.5]]), rgb('#5a5a62')],
    [ellipse(14, 19, 3.5, 3), rgb('#cacace')],
    [ellipse(14, 17.6, 1.4, 1), INK],
    [rect(10, 13.5, 12, 15), rgb('#f0c040')],
    [rect(16, 13.5, 18, 15), rgb('#f0c040')],
  ],
  bear: [
    [ellipse(7.5, 8, 3, 3), rgb('#5a361c')],
    [ellipse(20.5, 8, 3, 3), rgb('#5a361c')],
    [ellipse(7.5, 8, 1.4, 1.4), rgb('#3a2010')],
    [ellipse(20.5, 8, 1.4, 1.4), rgb('#3a2010')],
    [ellipse(14, 15, 8.5, 7.5), rgb('#6a4224')],
    [ellipse(14, 18.5, 3.8, 2.8), rgb('#b08a60')],
    [ellipse(14, 17, 1.6, 1.1), INK],
    [rect(10, 12.5, 11.5, 14), INK],
    [rect(16.5, 12.5, 18, 14), INK],
  ],
};

const CIVILIANS = new Set(['settler', 'worker']);

function unit(name) {
  const token = CIVILIANS.has(name) ? hexToken() : disc();
  return token.over(outlined(UNIT, UNIT, PICTOGRAMS[name]));
}

// ---------------------------------------------------------------- buildings

const BUILDING = 28;
const CITY = 32;

// Letters, 5x7, as the vector badges mark each building.
const LETTERS = {
  B: ['####.', '#...#', '#...#', '####.', '#...#', '#...#', '####.'],
  M: ['#...#', '##.##', '#.#.#', '#.#.#', '#...#', '#...#', '#...#'],
  W: ['#...#', '#...#', '#...#', '#.#.#', '#.#.#', '##.##', '#...#'],
  C: ['.####', '#....', '#....', '#....', '#....', '#....', '.####'],
  F: ['#####', '#....', '#....', '####.', '#....', '#....', '#....'],
  S: ['.####', '#....', '#....', '.###.', '....#', '....#', '####.'],
  V: ['#...#', '#...#', '#...#', '#...#', '#...#', '.#.#.', '..#..'],
  H: ['#...#', '#...#', '#...#', '#####', '#...#', '#...#', '#...#'],
  N: ['#...#', '##..#', '##..#', '#.#.#', '#..##', '#..##', '#...#'],
  K: ['#...#', '#..#.', '#.#..', '##...', '#.#..', '#..#.', '#...#'],
  T: ['#####', '..#..', '..#..', '..#..', '..#..', '..#..', '..#..'],
  R: ['####.', '#...#', '#...#', '####.', '#.#..', '#..#.', '#...#'],
  P: ['####.', '#...#', '#...#', '####.', '#....', '#....', '#....'],
  D: ['####.', '#...#', '#...#', '#...#', '#...#', '#...#', '####.'],
};

// Each building: its letter and wall color, as the vector badges have them.
const BUILDINGS = {
  mill: ['M', '#a2d48f'],
  workshop: ['W', '#a6bfe9'],
  canoe_house: ['C', '#96d4e9'],
  forge: ['F', '#e9b081'],
  stable: ['S', '#d6b98d'],
  watchpost: ['V', '#e5e39e'],
  field_hospital: ['H', '#e9999e'],
  cannery: ['N', '#b2dcb4'],
  work_camp: ['K', '#d6c49f'],
  smelter: ['T', '#e9b092'],
  railhead: ['R', '#b4d8e7'],
  harbor: ['P', '#9fd6f4'],
  coastal_battery: ['D', '#eec290'],
};

/** A house: a tiled roof (`roof`, `roofShade`) and a chimney over plank
 * walls of `walls`, its letter on a sign. */
function house(letter, walls, roof, roofShade) {
  const wallShade = mix(walls, [0, 0, 0, 255], 0.22);
  const shapes = [
    [rect(19, 3, 22, 9), STONE_DARK],
    [polygon([[1.5, 13], [14, 2], [26.5, 13]]), roof],
    [polygon([[14, 2], [26.5, 13], [14, 13]]), roofShade],
    [rect(4, 13, 24, 26), walls],
    [rect(19, 13, 24, 26), wallShade],
  ];
  const img = outlined(BUILDING, BUILDING, shapes);
  // Rows of roof tiles, and planks.
  for (const y of [6, 9, 12]) {
    for (let x = 0; x < BUILDING; x++) {
      const c = img.get(x, y);
      if (same(c, roof) || same(c, roofShade)) img.set(x, y, mix(c, [0, 0, 0, 255], 0.25));
    }
  }
  for (const x of [6, 21]) {
    for (let y = 14; y < 25; y++) {
      const c = img.get(x, y);
      if (same(c, walls) || same(c, wallShade)) img.set(x, y, mix(c, [0, 0, 0, 255], 0.12));
    }
  }
  img.fill(rect(8, 15, 19, 25), rgb('#f4f0e2'));
  img.fill((x, y) => rect(8, 15, 19, 25)(x, y) && !rect(9, 16, 18, 24)(x, y), INK);
  img.stamp(11, 16, LETTERS[letter], { '#': INK });
  return img;
}

function building(name) {
  const [letter, walls] = BUILDINGS[name];
  return house(letter, rgb(walls), TEAM, TEAM_SHADE);
}

/** The Barracks: walls in the team color under a dark roof, as on the
 * vector map. */
function barracks() {
  return house('B', TEAM, TEAM_DARK, rgb('#600060'));
}

/** A city: a crenellated stone tower in the team color, its middle left
 * plain for the population the game writes there. */
function city() {
  const merlon = (x0, x1) => rect(x0, 4, x1, 12);
  const shapes = [
    [rect(3, 12, 29, 31), TEAM],
    [merlon(3, 9), TEAM],
    [merlon(13, 19), TEAM],
    [merlon(23, 29), TEAM],
    [rect(24, 4, 29, 31), TEAM_SHADE],
    [rect(13, 26, 19, 31), INK],
    [ellipse(16, 26.5, 3, 2.5), INK],
  ];
  const img = outlined(CITY, CITY, shapes);
  // Courses of stone, clear of the middle where the number goes.
  const plain = rect(7, 13, 25, 26);
  for (let y = 15; y < 30; y += 4) {
    for (let x = 4; x < 28; x++) {
      if (!plain(x + 0.5, y + 0.5) && same(img.get(x, y), TEAM)) img.set(x, y, TEAM_SHADE);
    }
  }
  for (const x of [5, 15, 25]) img.fill(rect(x, 6, x + 2, 10), TEAM_DARK);
  return img;
}

// ---------------------------------------------------------------- icons

const ICON = 16;
const icons = {
  horse_head: [
    [polygon([[3.5, 15], [4, 8.5], [6, 5], [7, 1.5], [8.5, 4], [12.5, 7], [13.5, 10], [11, 11], [8, 9.5], [9, 15]]), rgb('#9a5a2a')],
    [segment(6, 5, 4, 12, 1.6), rgb('#3a2010')],
    [rect(9, 6, 10, 7), INK],
  ],
  ingot: [
    [polygon([[1.5, 9], [4, 5], [14.5, 5], [12, 9]]), rgb('#dce2ea')],
    [polygon([[1.5, 9], [12, 9], [12, 13], [1.5, 13]]), STEEL],
    [polygon([[12, 9], [14.5, 5], [14.5, 9.5], [12, 13]]), STEEL_DARK],
  ],
  wheat: [
    [segment(8, 7, 8, 15, 1.2), rgb('#8a9a3a')],
    [segment(8, 12, 5, 10, 1), rgb('#8a9a3a')],
    ...[3, 6, 9].flatMap((y) => [
      [ellipse(6.6, y, 1.4, 1.9), rgb('#f0c040')],
      [ellipse(9.4, y, 1.4, 1.9), rgb('#d8a030')],
    ]),
    [ellipse(8, 1.8, 1.1, 1.4), rgb('#f0c040')],
  ],
  ore_cart: [
    [ellipse(5, 5.5, 2.4, 2), rgb('#6a6460')],
    [ellipse(9, 4.5, 2.6, 2.2), rgb('#5a5450')],
    [ellipse(12, 6, 2.2, 1.8), rgb('#7a7470')],
    [polygon([[1.5, 6.5], [14.5, 6.5], [13, 12], [3, 12]]), rgb('#7a4a24')],
    [rect(2, 6.5, 14, 8), rgb('#5a3418')],
    [ellipse(5, 13, 2, 2), INK],
    [ellipse(11, 13, 2, 2), INK],
  ],
  fence: [
    [rect(2.5, 2, 5, 15), rgb('#c08a4a')],
    [rect(11, 2, 13.5, 15), rgb('#c08a4a')],
    [rect(1, 5, 15, 7), rgb('#a8743a')],
    [rect(1, 10, 15, 12), rgb('#a8743a')],
  ],
  logs: [
    [ellipse(4.5, 11, 3.5, 3.5), rgb('#d09a5a')],
    [ellipse(11.5, 11, 3.5, 3.5), rgb('#d09a5a')],
    [ellipse(8, 5, 3.5, 3.5), rgb('#d09a5a')],
    [ellipse(4.5, 11, 1.4, 1.4), rgb('#8a5a2a')],
    [ellipse(11.5, 11, 1.4, 1.4), rgb('#8a5a2a')],
    [ellipse(8, 5, 1.4, 1.4), rgb('#8a5a2a')],
  ],
  ruins: [
    [polygon([[2.5, 13], [2.5, 4], [4, 2], [6.5, 3.5], [6.5, 13]]), STONE],
    [polygon([[9.5, 13], [9.5, 7], [11, 6], [13.5, 8], [13.5, 13]]), STONE],
    [rect(3.5, 4, 4.5, 13), STONE_DARK],
    [rect(10.5, 8, 11.5, 13), STONE_DARK],
    [rect(1, 13, 15, 15.5), STONE_DARK],
  ],
  fruit_tree: [
    [rect(7, 10, 9, 15), rgb('#6a4220')],
    [ellipse(8, 6.5, 6.5, 5.5), rgb('#3c8a34')],
    [ellipse(6.5, 5, 3, 2.5), rgb('#58a848')],
    ...[[5, 7], [10, 4], [11, 8], [7, 9.5]].map(([x, y]) => [rect(x, y, x + 1.5, y + 1.5), rgb('#e0402a')]),
  ],
  stone_blocks: [
    [rect(4.5, 3, 11.5, 8), rgb('#d8d0c0')],
    [rect(1, 8, 8, 13.5), rgb('#c8c0b0')],
    [rect(8, 8, 15, 13.5), rgb('#b0a898')],
  ],
  paw: [
    [ellipse(8, 11, 4.5, 3.6), rgb('#5a3a22')],
    [ellipse(3, 6.5, 1.6, 2), rgb('#5a3a22')],
    [ellipse(6.3, 3.5, 1.6, 2), rgb('#5a3a22')],
    [ellipse(9.7, 3.5, 1.6, 2), rgb('#5a3a22')],
    [ellipse(13, 6.5, 1.6, 2), rgb('#5a3a22')],
  ],
  food: [
    ...[4.5, 8, 11.5].map((x) => [segment(x, 7, 8, 15, 1), rgb('#8a9a3a')]),
    ...[4.5, 8, 11.5].flatMap((x) => [
      [ellipse(x - 1, 3.5, 1, 1.6), rgb('#f0c040')],
      [ellipse(x + 1, 3.5, 1, 1.6), rgb('#d8a030')],
      [ellipse(x, 1.5, 0.9, 1.2), rgb('#f0c040')],
    ]),
    [rect(5, 10, 11, 11), rgb('#a87820')],
  ],
  wood: [
    [segment(3.5, 12.5, 12, 4, 5.5), rgb('#9a6432')],
    [segment(3.5, 12.5, 12, 4, 1.5), rgb('#b07a42')],
    [ellipse(12.3, 3.7, 2.6, 2.6), rgb('#d09a5a')],
    [ellipse(12.3, 3.7, 1, 1), rgb('#8a5a2a')],
  ],
  metal: [
    [polygon([[1.5, 9], [4, 5], [14.5, 5], [12, 9]]), rgb('#e4e8f0')],
    [polygon([[1.5, 9], [12, 9], [12, 12.5], [1.5, 12.5]]), rgb('#b4bcc8')],
    [polygon([[12, 9], [14.5, 5], [14.5, 9], [12, 12.5]]), rgb('#7c8696')],
  ],
  clock: [
    [ellipse(8, 8, 6.5, 6.5), rgb('#e8e4d0')],
    [segment(8, 8, 8, 3.5, 1.3), INK],
    [segment(8, 8, 11, 9.5, 1.3), INK],
  ],
};

function icon(name) {
  return outlined(ICON, ICON, icons[name]);
}

// ---------------------------------------------------------------- UI orders

// Order and ability buttons' pictures, 16x16, light on the dark buttons.
const LIGHT = rgb('#e8e8e4');
const RED = rgb('#e04a36');
const orders = {
  move: [[polygon([[1.5, 6.5], [9, 6.5], [9, 2.5], [14.5, 8], [9, 13.5], [9, 9.5], [1.5, 9.5]]), LIGHT]],
  attack: [
    [segment(3, 13, 12.5, 3.5, 2.4), STEEL],
    [polygon([[11.5, 2.5], [14.5, 1.5], [13.5, 4.5]]), STEEL],
    [segment(2, 10, 6, 14, 1.8), GOLD],
    [segment(1.5, 14.5, 3.5, 12.5, 2), WOOD],
  ],
  swap: [
    [polygon([[1.5, 4], [9, 4], [9, 1.5], [13, 5], [9, 8.5], [9, 6], [1.5, 6]]), LIGHT],
    [polygon([[14.5, 10], [7, 10], [7, 7.5], [3, 11], [7, 14.5], [7, 12], [14.5, 12]]), LIGHT],
  ],
  shield_wall: [2.5, 8, 13.5].map((x) => [polygon([[x - 2.5, 4], [x + 2.5, 4], [x + 2.5, 9], [x, 13], [x - 2.5, 9]]), STEEL]),
  volley: [3, 7, 11].flatMap((x) => [
    [segment(x - 1.5, 14.5, x + 2.5, 4, 1.2), WOOD],
    [polygon([[x + 1, 4.5], [x + 3.5, 1], [x + 3.8, 5.5]]), STEEL],
  ]),
  charge: [
    [polygon([[6, 15], [6.5, 9], [8.5, 5.5], [9.5, 2], [11, 4.5], [14.5, 7.5], [15, 10.5], [12.5, 11], [10.5, 10], [11, 15]]), rgb('#b07038')],
    [segment(1, 6, 5, 6, 1.2), LIGHT],
    [segment(0.5, 9.5, 5, 9.5, 1.2), LIGHT],
    [segment(1.5, 13, 5, 13, 1.2), LIGHT],
  ],
  deploy: [
    [segment(6, 11, 13, 3, 1.8), WOOD],
    [ellipse(13.5, 2.8, 2, 1.6), WOOD_DARK],
    [rect(1.5, 11, 14.5, 13), WOOD],
    [ellipse(4.5, 13.5, 2, 2), WOOD_DARK],
    [ellipse(11.5, 13.5, 2, 2), WOOD_DARK],
  ],
  lookout: [
    [segment(2, 14, 6, 10, 2.4), GOLD_DARK],
    [segment(6, 10, 10, 6, 3.2), GOLD],
    [segment(10, 6, 13.5, 2.5, 4), rgb('#f0c860')],
    [ellipse(14, 2.2, 1.4, 1.4), rgb('#9fd0ff')],
  ],
  hold: [[rect(3.5, 2.5, 6.5, 13.5), LIGHT], [rect(9.5, 2.5, 12.5, 13.5), LIGHT]],
  guard: [
    [polygon([[2.5, 2], [13.5, 2], [13.5, 8], [8, 14.5], [2.5, 8]]), STEEL],
    [polygon([[2.5, 2], [8, 2], [8, 14.5], [2.5, 8]]), rgb('#dce2ea')],
  ],
  alert: [
    [(x, y) => Math.abs(Math.hypot(x - 8, y - 8) - 5) < 1, RED],
    [segment(8, 0.5, 8, 4.5, 1.4), RED],
    [segment(8, 11.5, 8, 15.5, 1.4), RED],
    [segment(0.5, 8, 4.5, 8, 1.4), RED],
    [segment(11.5, 8, 15.5, 8, 1.4), RED],
  ],
  clear: [
    [rect(1.5, 3, 9.5, 5), LIGHT],
    [rect(1.5, 7, 9.5, 9), LIGHT],
    [rect(1.5, 11, 9.5, 13), LIGHT],
    [segment(10.5, 5.5, 15, 12.5, 1.6), RED],
    [segment(15, 5.5, 10.5, 12.5, 1.6), RED],
  ],
  disband: [
    [rect(3.5, 5, 12.5, 15), STEEL_DARK],
    [rect(2, 3, 14, 5), STEEL],
    [rect(6, 1.5, 10, 3), STEEL],
    [rect(5.5, 7, 6.5, 13), INK],
    [rect(9.5, 7, 10.5, 13), INK],
  ],
  settle: [
    [segment(4, 1.5, 4, 14.5, 1.6), WOOD],
    [polygon([[5, 2], [14, 4], [11.5, 6.5], [14, 9], [5, 9]]), LIGHT],
    [ellipse(4, 14.5, 3.5, 1.2), WOOD_DARK],
  ],
};

function order(name) {
  return outlined(ICON, ICON, orders[name]);
}

// ---------------------------------------------------------------- clouds

// The clouds' tones, from shadow to lit tops: muted, so they stay behind the
// map, like the vector clouds.
const CLOUD = ['#40424a', '#4e5059', '#5e616b', '#727580', '#888b95'].map(rgb);
const BAYER = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/** A cumulus puff `w` by `h`: a heap of round bumps on a flat base, lit
 * from the upper left in four bands that meet in an ordered dither. */
function puff(w, h, salt) {
  const img = new Image(w, h);
  const bumps = [];
  const count = 3 + Math.floor(w / 28);
  for (let i = 0; i < count; i++) {
    const t = (i + 0.5) / count;
    const r = h * (0.3 + noise(i, 2, salt) * 0.14) * (1 - Math.abs(t - 0.5) * 0.6);
    bumps.push([w * (0.14 + t * 0.72), h * 0.6 - r * 0.35, r]);
  }
  const inside = (x, y) =>
    (y < h * 0.9 && bumps.some(([cx, cy, r]) => Math.hypot(x - cx, (y - cy) * 1.1) <= r)) ||
    ellipse(w / 2, h * 0.72, w * 0.46, h * 0.2)(x, y);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const [px, py] = [x + 0.5, y + 0.5];
      if (!inside(px, py)) continue;
      const lit = Math.min(...bumps.map(([cx, cy, r]) => Math.hypot(px - cx + r * 0.3, py - cy + r * 0.5) / (r * 1.4)));
      const depth = Math.max(0, Math.min(1, lit * 0.7 + (py / h) * 0.5));
      const band = Math.max(1, Math.min(4, Math.floor((1 - depth) * 4 + BAYER[y % 4][x % 4] / 16)));
      img.set(x, y, CLOUD[band]);
    }
  }
  // A dark rim under it.
  for (let x = 0; x < w; x++) {
    for (let y = h - 1; y >= 0; y--) {
      if (img.opaque(x, y)) {
        img.set(x, y, CLOUD[0]);
        break;
      }
    }
  }
  return img;
}

/** Cloud cover over one unexplored hex: a dark ground with puffs heaped on
 * it, a different heap each tile. */
function cloudTile(index) {
  const img = new Image(TILE_W, TILE_H);
  img.fill((x, y) => inHex(x - 0.5, y - 0.5), CLOUD[0]);
  const places = [
    [[4, 6, 48, 32], [-6, 20, 80, 56]],
    [[18, 2, 48, 32], [-14, 16, 80, 56], [30, 28, 48, 32]],
    [[-6, 10, 48, 32], [22, 2, 48, 32], [10, 26, 48, 32]],
    [[-10, -4, 80, 56], [26, 26, 48, 32]],
  ][index];
  places.forEach(([x, y, w, h], i) => img.over(puff(w, h, index * 7 + i), x, y));
  for (let y = 0; y < TILE_H; y++) {
    for (let x = 0; x < TILE_W; x++) {
      if (inHex(x, y) && noise(x, y, 99 + index) < 0.04 && same(img.get(x, y), CLOUD[0])) img.set(x, y, CLOUD[1]);
    }
  }
  img.mask(inHex);
  return img;
}

// ---------------------------------------------------------------- sheet

/** Every sprite blown up 4 times on a grey ground, for looking them over
 * (`--sheet out.png [prefix]` writes it instead of the sprites). */
function sheet(images) {
  const scale = 4;
  const cell = 116 * scale;
  const columns = 8;
  const out = new Image(columns * cell, Math.max(1, Math.ceil(images.length / columns)) * cell);
  out.fill(() => true, rgb('#3a3c40'));
  images.forEach((img, i) => {
    const x0 = (i % columns) * cell + scale;
    const y0 = Math.floor(i / columns) * cell + scale;
    for (let y = 0; y < img.h * scale; y++) {
      for (let x = 0; x < img.w * scale; x++) {
        const c = img.get(Math.floor(x / scale), Math.floor(y / scale));
        if (c[3] > 0) out.set(x0 + x, y0 + y, c);
      }
    }
  });
  return out;
}

// ---------------------------------------------------------------- the set

const HILLY = ['grassland', 'plains', 'desert', 'tundra', 'snow', 'marsh'];

const sprites = {};
for (const name of Object.keys(GROUNDS)) sprites[`terrain/${name}`] = ground(name, false);
for (const name of HILLY) sprites[`terrain/${name}_hills`] = ground(name, true);
for (const name of ['forest', 'jungle']) sprites[`feature/${name}`] = feature(name);
for (const name of Object.keys(PICTOGRAMS)) sprites[`unit/${name}`] = unit(name);
sprites['building/city'] = city();
sprites['building/barracks'] = barracks();
for (const name of Object.keys(BUILDINGS)) sprites[`building/${name}`] = building(name);
for (const name of Object.keys(icons)) sprites[`icon/${name}`] = icon(name);
for (const name of Object.keys(orders)) sprites[`ui/${name}`] = order(name);
for (let i = 0; i < 4; i++) sprites[`cloud/tile_${i + 1}`] = cloudTile(i);
sprites['cloud/puff_small'] = puff(48, 32, 1);
sprites['cloud/puff_medium'] = puff(80, 56, 2);
sprites['cloud/puff_large'] = puff(112, 80, 3);

const sheetAt = process.argv.indexOf('--sheet');
if (sheetAt >= 0) {
  const prefix = process.argv[sheetAt + 2] || '';
  const chosen = Object.entries(sprites)
    .filter(([tag]) => tag.startsWith(prefix))
    .map(([, image]) => image);
  writeFileSync(resolve(process.argv[sheetAt + 1]), encodePng(sheet(chosen)));
  process.exit(0);
}

for (const [tag, image] of Object.entries(sprites)) {
  const path = join(OUT, `${tag}.png`);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, encodePng(image));
}
console.log(`wrote ${Object.keys(sprites).length} sprites to ${OUT}`);
