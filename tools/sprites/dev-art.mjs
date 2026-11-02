#!/usr/bin/env node
// Draws the dev-art pixel sprites: placeholder PNGs in assets/sprites/, one per
// tag the game looks for (docs/sprites.md lists them, with sizes and anchors).
// Run `node tools/sprites/dev-art.mjs` from anywhere; it overwrites every file
// it makes and leaves any other file alone. No dependencies: it writes the
// PNGs itself (zlib is Node's).
//
// Everything is drawn in sRGB, as an artist would. Pure magenta in any shade
// (red equal to blue, no green) is the team color: the game paints a unit's,
// city's or building's side color there, scaled by the magenta's brightness.

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
  /** Clears every pixel `keep` refuses. */
  mask(keep) {
    for (let y = 0; y < this.h; y++) {
      for (let x = 0; x < this.w; x++) if (!keep(x, y)) this.set(x, y, [0, 0, 0, 0]);
    }
  }
}

/** A fixed hash of a pixel, from 0 to 1: the same picture every run. */
function noise(x, y, salt) {
  let h = Math.imul(x + 1, 374761393) ^ Math.imul(y + 1, 668265263) ^ Math.imul(salt + 7, 2246822519);
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

// ---------------------------------------------------------------- terrain

const TILE_W = 32;
const TILE_H = 28;

/** Whether a pixel of a 32x28 tile is inside its flat-top hexagon. */
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

/** Ground of `name`: speckled base, a light upper-left rim and a dark
 * lower-right one, then its own marks. */
function ground(name, hills) {
  const g = GROUNDS[name];
  const [base, light, dark, mark] = [g.base, g.light, g.dark, g.mark].map(rgb);
  const img = new Image(TILE_W, TILE_H);
  for (let y = 0; y < TILE_H; y++) {
    for (let x = 0; x < TILE_W; x++) {
      const n = noise(x, y, name.length);
      img.set(x, y, n < 0.12 ? dark : n > 0.88 ? light : base);
    }
  }
  marks(img, name, mark);
  if (hills) drawHills(img, base, light, dark);
  rim(img, light, dark);
  img.mask(inHex);
  return img;
}

function marks(img, name, mark) {
  const spots = (count, salt) =>
    Array.from({ length: count }, (_, i) => [
      3 + Math.floor(noise(i, 0, salt) * (TILE_W - 8)),
      3 + Math.floor(noise(i, 1, salt) * (TILE_H - 8)),
    ]).filter(([x, y]) => inHex(x, y) && inHex(x + 3, y + 2));
  switch (name) {
    case 'grassland':
    case 'tundra':
      // Grass tufts.
      for (const [x, y] of spots(name === 'grassland' ? 9 : 6, 3)) {
        img.stamp(x, y, ['#.#', '.#.'], { '#': mark });
      }
      break;
    case 'plains':
      // Dry stalks.
      for (const [x, y] of spots(8, 5)) img.stamp(x, y, ['#.', '#.', '.#'], { '#': mark });
      break;
    case 'desert':
      // Dune ridges.
      for (const [x, y] of [[6, 7], [17, 12], [8, 19], [20, 21]]) {
        img.stamp(x, y, ['..####..', '##....##'], { '#': mark });
      }
      break;
    case 'snow':
      for (const [x, y] of spots(7, 9)) img.stamp(x, y, ['##'], { '#': mark });
      break;
    case 'marsh': {
      // Pools and reeds.
      const pool = rgb('#35586c');
      for (const [x, y] of [[5, 8], [18, 6], [11, 17], [21, 18]]) {
        img.stamp(x, y, ['.####.', '######', '.####.'], { '#': pool });
      }
      for (const [x, y] of [[10, 6], [24, 11], [6, 15], [17, 20]]) {
        img.stamp(x, y, ['#.#', '#.#', '###'], { '#': mark });
      }
      break;
    }
    case 'mountains': {
      // One big peak, snow on top, lit from the left.
      const rock = rgb('#7a7370');
      const shade = rgb('#4a4441');
      const snow = rgb('#eef2f6');
      const outline = rgb('#2a2624');
      for (let y = 3; y < 25; y++) {
        const half = (y - 3) * 0.72;
        for (let x = 0; x < TILE_W; x++) {
          const dx = x + 0.5 - 16;
          if (Math.abs(dx) > half + 0.5) continue;
          const edge = Math.abs(dx) > half - 0.6 || y === 24;
          img.set(x, y, edge ? outline : y < 10 ? snow : dx < 0 ? rock : shade);
        }
      }
      // A smaller peak behind, to the right.
      for (let y = 9; y < 25; y++) {
        const half = (y - 9) * 0.8;
        for (let x = 20; x < TILE_W; x++) {
          const dx = x + 0.5 - 25;
          if (Math.abs(dx) > half + 0.5 || img.get(x, y)[0] === outline[0]) continue;
          if (x < 22 && y > 12) continue;
          const edge = Math.abs(dx) > half - 0.6 || y === 24;
          img.set(x, y, edge ? outline : y < 13 ? snow : shade);
        }
      }
      break;
    }
    case 'coast':
    case 'ocean':
    case 'lake': {
      const count = name === 'ocean' ? 4 : 6;
      for (const [x, y] of spots(count, 11)) img.stamp(x, y, ['.##..', '#..##'], { '#': mark });
      break;
    }
    default:
      break;
  }
}

/** Two rounded hills along the bottom, lit from the upper left. */
function drawHills(img, base, light, dark) {
  const outline = mix(dark, [0, 0, 0, 255], 0.45);
  const hill = mix(base, dark, 0.35);
  const lit = mix(base, light, 0.9);
  for (const [cx, cy, rx, ry] of [[10, 24, 8, 7], [22, 25, 7, 6]]) {
    for (let y = cy - ry - 1; y <= cy; y++) {
      for (let x = cx - rx - 1; x <= cx + rx + 1; x++) {
        const dx = (x + 0.5 - cx) / rx;
        const dy = (y + 0.5 - cy) / ry;
        const d = dx * dx + dy * dy;
        if (d > 1) continue;
        const ix = (x + 0.5 - cx) / (rx - 1);
        const iy = (y + 0.5 - cy) / (ry - 1);
        img.set(x, y, ix * ix + iy * iy > 1 ? outline : dx < -0.15 && dy < -0.3 ? lit : hill);
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

const PINE = [
  '...o...',
  '..ogo..',
  '..ogo..',
  '.oglgo.',
  '.ogggo.',
  'oglgggo',
  'ooggooo',
  '...t...',
];

const CANOPY = [
  '.oooo..',
  'ollggo.',
  'olgggvo',
  'ogggggo',
  '.ogvgo.',
  '..oto..',
];

/** A feature drawn over any ground: trees across the top and sides, where
 * hills (along the bottom) and units (in the middle) don't sit. */
function feature(name) {
  const img = new Image(TILE_W, TILE_H);
  if (name === 'forest') {
    const palette = { o: rgb('#173214'), g: rgb('#2c5a24'), l: rgb('#3f7a32'), t: rgb('#4a2e18') };
    for (const [x, y] of [[8, 1], [16, 0], [22, 2], [3, 8], [26, 9], [12, 6]]) {
      img.stamp(x, y, PINE, palette);
    }
  } else {
    const palette = {
      o: rgb('#0f2e18'),
      g: rgb('#23603a'),
      l: rgb('#3c8a4c'),
      v: rgb('#8ab03c'),
      t: rgb('#3e2a16'),
    };
    for (const [x, y] of [[8, 1], [16, 0], [22, 2], [3, 8], [25, 9], [13, 6]]) {
      img.stamp(x, y, CANOPY, palette);
    }
  }
  img.mask(inHex);
  return img;
}

// ---------------------------------------------------------------- tokens

const INK = rgb('#16161a');
const TEAM = rgb('#ff00ff');
const TEAM_SHADE = rgb('#b400b4');
const TEAM_DARK = rgb('#800080');
const PALE = rgb('#e4e0d4');
const WHITE = rgb('#f2f2ee');

const UNIT = 14;

/** A military unit's token: a disc in the team color with a dark rim. */
function disc() {
  const img = new Image(UNIT, UNIT);
  const c = UNIT / 2;
  for (let y = 0; y < UNIT; y++) {
    for (let x = 0; x < UNIT; x++) {
      const d = Math.hypot(x + 0.5 - c, y + 0.5 - c);
      if (d > c) continue;
      const shaded = x + y > UNIT + 2 && d > c - 3;
      img.set(x, y, d > c - 1 ? INK : shaded ? TEAM_SHADE : TEAM);
    }
  }
  return img;
}

/** A civilian's token: a pale pointy-top hexagon in a team-colored rim. */
function hexToken() {
  const img = new Image(UNIT, UNIT);
  const c = UNIT / 2;
  const inside = (x, y, r) => {
    const dx = Math.abs(x + 0.5 - c);
    const dy = Math.abs(y + 0.5 - c);
    return dx <= r * 0.866 && dy <= r - dx * 0.577;
  };
  for (let y = 0; y < UNIT; y++) {
    for (let x = 0; x < UNIT; x++) {
      if (!inside(x, y, c)) continue;
      img.set(x, y, !inside(x, y, c - 1) ? INK : inside(x, y, c - 3) ? PALE : TEAM);
    }
  }
  return img;
}

// Pictograms, 8x8, in ink ('#') with white ('w') highlights, as the vector
// tokens have them.
const PICTOGRAMS = {
  melee: ['...##...', '...#w...', '...#w...', '...#w...', '...#w...', '.######.', '...##...', '...##...'],
  ranged: ['.##.....', '.#.#....', '.#..#.#.', '########', '.#..#.#.', '.#.#....', '.##.....', '........'],
  cavalry: ['..#.#...', '..####..', '.##w###.', '.#######', '.##..###', '.##.....', '###.....', '###.....'],
  siege: ['......##', '.....##.', '....##..', '...##...', '.#####..', '########', '.##..##.', '.##..##.'],
  scout: ['......ww', '.....###', '....###.', '...###..', '..##....', '.##.....', '##......', '#.......'],
  armored: ['########', '###ww###', '#wwwwww#', '###ww###', '.##ww##.', '.######.', '..####..', '...##...'],
  pikeman: ['.....##.', '....#ww#', '.....##.', '....#...', '...#....', '..#.....', '.#......', '#.......'],
  patrol_galley: ['...#....', '...##...', '...###..', '########', '.######.', '..####..', '.#.#.#..', '#.#.#...'],
  landing_craft: ['........', '.######.', '.#wwww#.', '########', '########', '.######.', '..####..', '........'],
  bombard_ship: ['........', '.....##.', '.#####..', '.#####..', '########', '.######.', '..####..', '........'],
  settler: ['.#......', '.######.', '.#####..', '.######.', '.#......', '.#......', '.#......', '.###....'],
  worker: ['..####..', '...##...', '...##...', '...##...', '.######.', '.######.', '..####..', '...##...'],
  wolf_pack: ['#......#', '##....##', '########', '#w####w#', '########', '.######.', '..#ww#..', '...##...'],
  bear: ['##....##', '########', '########', '#w####w#', '########', '###ww###', '.######.', '..####..'],
};

const CIVILIANS = new Set(['settler', 'worker']);

function unit(name) {
  const img = CIVILIANS.has(name) ? hexToken() : disc();
  img.stamp(3, 3, PICTOGRAMS[name], { '#': INK, w: WHITE });
  return img;
}

// ---------------------------------------------------------------- buildings

const BUILDING = 14;
const CITY = 16;

// Letters, 5x5, as the vector badges mark each building.
const LETTERS = {
  B: ['####.', '#...#', '####.', '#...#', '####.'],
  M: ['#...#', '##.##', '#.#.#', '#...#', '#...#'],
  W: ['#...#', '#...#', '#.#.#', '##.##', '#...#'],
  C: ['.####', '#....', '#....', '#....', '.####'],
  F: ['#####', '#....', '####.', '#....', '#....'],
  S: ['.####', '#....', '.###.', '....#', '####.'],
  V: ['#...#', '#...#', '#...#', '.#.#.', '..#..'],
  H: ['#...#', '#...#', '#####', '#...#', '#...#'],
  N: ['#...#', '##..#', '#.#.#', '#..##', '#...#'],
  K: ['#...#', '#..#.', '###..', '#..#.', '#...#'],
  T: ['#####', '..#..', '..#..', '..#..', '..#..'],
  R: ['####.', '#...#', '####.', '#..#.', '#...#'],
  P: ['####.', '#...#', '####.', '#....', '#....'],
  D: ['####.', '#...#', '#...#', '#...#', '####.'],
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

/** A house: a roof in the team color over walls of `walls`, its letter on
 * them. */
function house(letter, walls, roof) {
  const img = new Image(BUILDING, BUILDING);
  for (let y = 0; y <= 6; y++) {
    const from = 6 - y;
    const to = 7 + y;
    for (let x = from; x <= to; x++) {
      const edge = x === from || x === to || y === 6;
      img.set(x, y, edge ? INK : roof);
    }
  }
  for (let y = 7; y < BUILDING; y++) {
    for (let x = 2; x <= 11; x++) {
      const edge = x === 2 || x === 11 || y === BUILDING - 1;
      img.set(x, y, edge ? INK : walls);
    }
  }
  img.stamp(5, 7, LETTERS[letter], { '#': INK });
  return img;
}

function building(name) {
  const [letter, walls] = BUILDINGS[name];
  return house(letter, rgb(walls), TEAM);
}

/** The Barracks: all in the team color, as on the vector map. */
function barracks() {
  return house('B', TEAM, TEAM_DARK);
}

/** A city: a crenellated tower in the team color, its middle left plain for
 * the population the game writes there. */
function city() {
  const img = new Image(CITY, CITY);
  const solid = (x, y) => {
    if (x < 1 || x > 14 || y > 15) return false;
    if (y >= 5) return true;
    if (y < 1) return false;
    return [1, 2, 3, 4, 6, 7, 8, 9, 11, 12, 13, 14].includes(x);
  };
  for (let y = 0; y < CITY; y++) {
    for (let x = 0; x < CITY; x++) {
      if (!solid(x, y)) continue;
      const edge = [[-1, 0], [1, 0], [0, -1], [0, 1]].some(([dx, dy]) => !solid(x + dx, y + dy));
      img.set(x, y, edge ? INK : x >= 12 ? TEAM_SHADE : TEAM);
    }
  }
  return img;
}

// ---------------------------------------------------------------- map icons

const ICON = 8;
const ICONS = {
  horse_head: [
    ['..k.k...', '.khkhk..', '.khhhhk.', 'khhhhhhk', 'khhkkhhk', 'khhk.kk.', 'khhk....', 'kkkk....'],
    { h: '#9a5a2a' },
  ],
  ingot: [
    ['........', '..kkkkk.', '.kllllk.', 'kllllk..', 'kkkkkkkk', 'kssssssk', 'kssssssk', 'kkkkkkkk'],
    { l: '#c4ccd6', s: '#7c8692' },
  ],
  wheat: [
    ['...kk...', '..kyyk..', '..kYyk..', '..kyYk..', '..kYyk..', '...kk...', '..g.g...', '...g....'],
    { y: '#f0c040', Y: '#c08a20', g: '#8a9a3a' },
  ],
  ore_cart: [
    ['..kkkk..', '.kooook.', 'kkkkkkkk', 'kbbbbbbk', 'kbbbbbbk', '.kbbbbk.', '.kk..kk.', '.kk..kk.'],
    { o: '#5a5450', b: '#7a4a24' },
  ],
  fence: [
    ['.k....k.', 'kwk..kwk', 'kwkkkkwk', 'kwwwwwwk', 'kwkkkkwk', 'kwwwwwwk', 'kwkkkkwk', 'kkk..kkk'],
    { w: '#c08a4a' },
  ],
  logs: [
    ['..kkkk..', '.kwrrwk.', '.kwrrwk.', 'kkkkkkkk', 'kwrwkwrw', 'kwrwkwrw', 'kkkkkkkk', '........'],
    { w: '#d09a5a', r: '#8a5a2a' },
  ],
  ruins: [
    ['.kk.....', 'kssk.kk.', 'kssk.ksk', 'kssk.ksk', 'kssk.ksk', 'kkkkkkkk', 'ksssssss', 'kkkkkkkk'],
    { s: '#b8b0a0' },
  ],
  fruit_tree: [
    ['..kkkk..', '.klrllk.', 'kllllrlk', 'klrllllk', '.kllrlk.', '..kttk..', '...tt...', '..kkkk..'],
    { l: '#3c8a34', r: '#e0402a', t: '#6a4220' },
  ],
  stone_blocks: [
    ['........', '..kkkk..', '..kssk..', 'kkkkkkkk', 'ksskkssk', 'ksskkssk', 'kkkkkkkk', '........'],
    { s: '#c8c0b0' },
  ],
  paw: [
    ['.k.k.k..', 'kpkpkpk.', '.k.k.k..', '..kkk...', '.kpppk..', 'kpppppk.', '.kpppk..', '..kkk...'],
    { p: '#5a3a22' },
  ],
};

function icon(name) {
  const [rows, colors] = ICONS[name];
  const palette = { k: INK };
  for (const [key, hex] of Object.entries(colors)) palette[key] = rgb(hex);
  const img = new Image(ICON, ICON);
  img.stamp(0, 0, rows, palette);
  return img;
}

/** Every sprite blown up 6 times on a grey ground, for looking them over
 * (`--sheet out.png` writes it instead of the sprites). */
function sheet(images) {
  const scale = 6;
  const cell = 34 * scale;
  const columns = 8;
  const out = new Image(columns * cell, Math.ceil(images.length / columns) * cell);
  for (let y = 0; y < out.h; y++) for (let x = 0; x < out.w; x++) out.set(x, y, rgb('#3a3c40'));
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
for (const name of Object.keys(ICONS)) sprites[`icon/${name}`] = icon(name);

const sheetAt = process.argv.indexOf('--sheet');
if (sheetAt >= 0) {
  writeFileSync(resolve(process.argv[sheetAt + 1]), encodePng(
      sheet(
        Object.entries(sprites)
          .filter(([tag]) => tag.startsWith(process.argv[sheetAt + 2] || ''))
          .map(([, image]) => image),
      ),
    ),);
  process.exit(0);
}

for (const [tag, image] of Object.entries(sprites)) {
  const path = join(OUT, `${tag}.png`);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, encodePng(image));
}
console.log(`wrote ${Object.keys(sprites).length} sprites to ${OUT}`);
