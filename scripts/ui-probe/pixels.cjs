'use strict';
const zlib = require('node:zlib');
const assert = require('node:assert/strict');

// Trusted generated screenshot subset, not a general PNG decoder. CRCs are not
// validated; structure, dimensions and decompression are bounded fail-closed.
function decode(buffer) {
  assert.equal(buffer.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  assert.ok(buffer.length <= 64 * 1024 * 1024, 'PNG input too large');
  let width, height, channels, ended = false;
  const parts = [];
  for (let at = 8; at < buffer.length;) {
    assert.ok(at + 12 <= buffer.length, 'Truncated PNG chunk');
    const size = buffer.readUInt32BE(at), type = buffer.toString('ascii', at + 4, at + 8);
    assert.ok(at + 12 + size <= buffer.length, 'PNG chunk outside buffer');
    if (at === 8) assert.equal(type, 'IHDR', 'IHDR must be first');
    const data = buffer.subarray(at + 8, at + 8 + size);
    if (type === 'IHDR') {
      assert.equal(at, 8, 'Duplicate IHDR'); assert.equal(size, 13, 'Invalid IHDR length');
      width = data.readUInt32BE(0); height = data.readUInt32BE(4);
      assert.ok(width > 0 && height > 0 && width <= 4096 && height <= 4096 && width * height <= 4_000_000, 'PNG dimensions out of bounds');
      assert.equal(data[8], 8, 'Expected 8-bit PNG');
      assert.equal(data[10], 0, 'Unsupported compression');
      assert.equal(data[11], 0, 'Unsupported filter method');
      assert.equal(data[12], 0, 'Unsupported interlace');
      assert.ok([2, 6].includes(data[9]), 'Expected RGB/RGBA PNG');
      channels = data[9] === 6 ? 4 : 3;
    }
    if (type === 'IDAT') parts.push(data);
    at += size + 12;
    if (type === 'IEND') {
      assert.equal(size, 0, 'Invalid IEND');
      assert.equal(at, buffer.length, 'Trailing PNG data');
      ended = true; break;
    }
  }
  assert.ok(channels && ended && parts.length, 'Missing PNG header, data or IEND');
  const stride = width * channels, expectedLength = height * (stride + 1);
  const raw = zlib.inflateSync(Buffer.concat(parts), { maxOutputLength: expectedLength });
  assert.equal(raw.length, expectedLength, 'Incorrect PNG inflated length');
  const pixels = Buffer.alloc(width * height * 4), rows = Buffer.alloc(height * stride);
  const paeth = (a, b, c) => {
    const p = a + b - c, x = Math.abs(p - a), y = Math.abs(p - b), z = Math.abs(p - c);
    return x <= y && x <= z ? a : y <= z ? b : c;
  };
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    assert.ok(filter <= 4);
    for (let x = 0; x < stride; x++) {
      const a = x >= channels ? rows[y * stride + x - channels] : 0;
      const b = y ? rows[(y - 1) * stride + x] : 0;
      const c = y && x >= channels ? rows[(y - 1) * stride + x - channels] : 0;
      rows[y * stride + x] = raw[y * (stride + 1) + x + 1] + [0, a, b, Math.floor((a + b) / 2), paeth(a,b,c)][filter];
    }
    for (let x = 0; x < width; x++) {
      const src = y * stride + x * channels, dst = (y * width + x) * 4;
      pixels[dst] = rows[src]; pixels[dst+1] = rows[src+1]; pixels[dst+2] = rows[src+2];
      pixels[dst+3] = channels === 4 ? rows[src+3] : 255;
    }
  }
  return { width, height, pixels };
}
function diff(a, b) {
  assert.equal(a.width, b.width, 'PNG width mismatch');
  assert.equal(a.height, b.height, 'PNG height mismatch');
  let changedPixels = 0, maxChannelDelta = 0;
  for (let i = 0; i < a.pixels.length; i += 4) {
    let changed = false;
    for (let c = 0; c < 4; c++) {
      const delta = Math.abs(a.pixels[i+c] - b.pixels[i+c]);
      maxChannelDelta = Math.max(maxChannelDelta, delta); changed ||= delta !== 0;
    }
    if (changed) changedPixels++;
  }
  return { changedPixels, maxChannelDelta, totalPixels: a.width * a.height };
}
function assertMatch(a, b) {
  const result = diff(a, b);
  assert.equal(result.changedPixels, 0, `Pixel mismatch: ${JSON.stringify(result)}`);
  return result;
}
function encode(image) {
  const crc32 = buffer => {
    let crc = 0xffffffff;
    for (const byte of buffer) {
      crc ^= byte;
      for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
    return (crc ^ 0xffffffff) >>> 0;
  };
  const chunk = (name, data) => {
    const type = Buffer.from(name), result = Buffer.alloc(data.length + 12);
    result.writeUInt32BE(data.length); type.copy(result, 4); data.copy(result, 8);
    result.writeUInt32BE(crc32(Buffer.concat([type, data])), data.length + 8);
    return result;
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(image.width); header.writeUInt32BE(image.height, 4);
  header[8] = 8; header[9] = 6;
  const stride = image.width * 4, raw = Buffer.alloc((stride + 1) * image.height);
  for (let y = 0; y < image.height; y++) image.pixels.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  return Buffer.concat([Buffer.from('89504e470d0a1a0a', 'hex'), chunk('IHDR', header), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}
function differenceImage(a, b) {
  diff(a, b); // Validate matching dimensions.
  const pixels = Buffer.alloc(a.pixels.length);
  for (let i = 0; i < pixels.length; i += 4) {
    const changed = !a.pixels.subarray(i, i+4).equals(b.pixels.subarray(i, i+4));
    pixels[i] = changed ? 255 : 24; pixels[i+1] = changed ? 0 : 24;
    pixels[i+2] = changed ? 255 : 24; pixels[i+3] = 255;
  }
  return encode({ width: a.width, height: a.height, pixels });
}
function crop(image, bounds) {
  const x = Math.floor(bounds.x), y = Math.floor(bounds.y);
  const width = Math.ceil(bounds.x + bounds.width) - x;
  const height = Math.ceil(bounds.y + bounds.height) - y;
  assert.ok([x,y,width,height].every(Number.isFinite) && x >= 0 && y >= 0 && width > 0 && height > 0 && x + width <= image.width && y + height <= image.height, 'Invalid pixel crop');
  const pixels = Buffer.alloc(width * height * 4);
  for (let row = 0; row < height; row++) {
    const start = ((y + row) * image.width + x) * 4;
    image.pixels.copy(pixels, row * width * 4, start, start + width * 4);
  }
  return { width, height, pixels };
}
module.exports = { decode, diff, assertMatch, encode, differenceImage, crop };
