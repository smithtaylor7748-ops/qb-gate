// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 浏览器助手的工具栏图标：用面板自己的图标（src-tauri/icons），保持同一个样子。
// 32 / 128 直接复制现成的那一份；16 / 48 没有现成尺寸，从 512 那一份按面积平均缩小。
// 只用 Node 自带模块（zlib 解压 / 压缩 PNG），不引图像库。重新生成：
//
//   node scripts/browser-extension-icons.mjs
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { deflateSync, inflateSync } from "node:zlib";

const root = resolve(import.meta.dirname, "..");
const source = resolve(root, "src-tauri/icons");
const target = resolve(root, "browser-extension/icons");

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

/** 只认 8 位 RGBA、不隔行（面板图标就是这种），别的格式直接报错。 */
function decode(file) {
  const png = readFileSync(file);
  let offset = 8;
  let width = 0;
  let height = 0;
  const data = [];
  while (offset < png.length) {
    const length = png.readUInt32BE(offset);
    const type = png.toString("latin1", offset + 4, offset + 8);
    const body = png.subarray(offset + 8, offset + 8 + length);
    if (type === "IHDR") {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      if (body[8] !== 8 || body[9] !== 6 || body[12] !== 0)
        throw new Error(`${file}：只支持 8 位 RGBA、不隔行的 PNG`);
    } else if (type === "IDAT") data.push(body);
    else if (type === "IEND") break;
    offset += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(data));
  const stride = width * 4;
  const pixels = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let x = 0; x < stride; x++) {
      const a = x >= 4 ? pixels[y * stride + x - 4] : 0;
      const b = y > 0 ? pixels[(y - 1) * stride + x] : 0;
      const c = x >= 4 && y > 0 ? pixels[(y - 1) * stride + x - 4] : 0;
      let value = line[x];
      if (filter === 1) value += a;
      else if (filter === 2) value += b;
      else if (filter === 3) value += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a);
        const pb = Math.abs(p - b);
        const pc = Math.abs(p - c);
        value += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      pixels[y * stride + x] = value & 0xff;
    }
  }
  return { width, height, pixels };
}

/** 面积平均（按 alpha 加权，边缘不发黑）。 */
function resize({ width, height, pixels }, size) {
  const out = new Uint8Array(size * size * 4);
  const scale = width / size;
  for (let oy = 0; oy < size; oy++) {
    for (let ox = 0; ox < size; ox++) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      let area = 0;
      const x0 = ox * scale;
      const y0 = oy * scale;
      for (let y = Math.floor(y0); y < Math.ceil(y0 + scale); y++) {
        const wy = Math.min(y + 1, y0 + scale) - Math.max(y, y0);
        for (let x = Math.floor(x0); x < Math.ceil(x0 + scale); x++) {
          const wx = Math.min(x + 1, x0 + scale) - Math.max(x, x0);
          const w = wx * wy;
          const i = (y * width + x) * 4;
          const alpha = pixels[i + 3] / 255;
          r += pixels[i] * alpha * w;
          g += pixels[i + 1] * alpha * w;
          b += pixels[i + 2] * alpha * w;
          a += alpha * w;
          area += w;
        }
      }
      const i = (oy * size + ox) * 4;
      out[i] = a ? Math.round(r / a) : 0;
      out[i + 1] = a ? Math.round(g / a) : 0;
      out[i + 2] = a ? Math.round(b / a) : 0;
      out[i + 3] = Math.round((a / area) * 255);
    }
  }
  return out;
}

function encode(size, pixels) {
  const stride = size * 4;
  const raw = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y++)
    raw.set(
      pixels.subarray(y * stride, (y + 1) * stride),
      y * (stride + 1) + 1,
    );
  const chunk = (type, body) => {
    const head = Buffer.alloc(8);
    head.writeUInt32BE(body.length, 0);
    head.write(type, 4, "latin1");
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), body])), 0);
    return Buffer.concat([head, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr.set([8, 6, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

mkdirSync(target, { recursive: true });
copyFileSync(resolve(source, "32x32.png"), resolve(target, "icon32.png"));
copyFileSync(resolve(source, "128x128.png"), resolve(target, "icon128.png"));
const large = decode(resolve(source, "icon.png"));
for (const size of [16, 48])
  writeFileSync(
    resolve(target, `icon${size}.png`),
    encode(size, resize(large, size)),
  );
console.log(`图标已生成：${target}`);
