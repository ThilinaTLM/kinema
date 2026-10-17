'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const zlib = require('node:zlib');
const { decode, diff, assertMatch, encode, differenceImage, crop } = require('./pixels.cjs');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
function png(raw, colorType = 6) {
  // Decoder unit tests do not need CRCs; browser acceptance uses real Chromium PNGs.
  const chunk = (type, data) => {
    const result = Buffer.alloc(data.length + 12);
    result.writeUInt32BE(data.length); result.write(type, 4); data.copy(result, 8);
    return result;
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(2, 0); header.writeUInt32BE(raw.length / (1 + 2 * (colorType === 6 ? 4 : 3)), 4); header[8] = 8; header[9] = colorType;
  return Buffer.concat([Buffer.from('89504e470d0a1a0a', 'hex'), chunk('IHDR', header), chunk('IDAT', zlib.deflateSync(Buffer.from(raw))), chunk('IEND', Buffer.alloc(0))]);
}
test('RGBA screenshot decode, exact match and one-pixel failure', () => {
  const a = decode(png([0, 1,2,3,255, 4,5,6,255]));
  assert.deepEqual([...a.pixels], [1,2,3,255,4,5,6,255]);
  assert.equal(assertMatch(a, a).changedPixels, 0);
  assertMatch(a, decode(encode(a)));
  assert.deepEqual([...crop(a, {x:1,y:0,width:1,height:1}).pixels], [4,5,6,255]);
  assert.throws(() => crop(a, {x:2,y:0,width:1,height:1}), /Invalid pixel crop/);
  const b = { ...a, pixels: Buffer.from(a.pixels) }; b.pixels[0] ^= 255;
  assert.equal(diff(a, b).changedPixels, 1);
  assert.deepEqual([...decode(differenceImage(a, b)).pixels], [255,0,255,255,24,24,24,255]);
  assert.throws(() => assertMatch(a, b), /Pixel mismatch/);
  assert.throws(() => assertMatch(a, { ...a, width: 1 }), /width mismatch/);
});
test('malformed PNGs and decompression bounds fail closed', () => {
  const valid = png([0, 1,2,3,255, 4,5,6,255]);
  const mutated = (offset, value, bytes = 1) => {
    const result = Buffer.from(valid);
    if (bytes === 4) result.writeUInt32BE(value, offset); else result[offset] = value;
    return result;
  };
  for (const [offset, value, bytes] of [[16,0,4],[16,4097,4],[20,4000000,4],[8,12,4],[24,16],[25,0],[26,1],[27,1],[28,1]]) {
    assert.throws(() => decode(mutated(offset, value, bytes)));
  }
  const tooManyPixels = Buffer.from(valid);
  tooManyPixels.writeUInt32BE(2048, 16); tooManyPixels.writeUInt32BE(2048, 20);
  assert.throws(() => decode(tooManyPixels), /dimensions out of bounds/);
  assert.throws(() => decode(valid.subarray(0, valid.length - 1)), /chunk/);
  assert.throws(() => decode(mutated(33, 0xffffffff, 4)), /outside buffer/);
  assert.throws(() => decode(valid.subarray(0, valid.length - 12)), /Missing/);
  assert.throws(() => decode(Buffer.concat([valid, Buffer.from([0])])), /Trailing/);
  assert.throws(() => decode(Buffer.concat([valid.subarray(0,33), valid.subarray(8,33), valid.subarray(33)])), /Duplicate IHDR/);
  assert.throws(() => decode(Buffer.concat([valid.subarray(0,8), valid.subarray(33)])), /IHDR must be first/);
  const withRaw = raw => {
    const compressed = zlib.deflateSync(Buffer.from(raw));
    const chunk = Buffer.alloc(compressed.length + 12);
    chunk.writeUInt32BE(compressed.length); chunk.write('IDAT', 4); compressed.copy(chunk, 8);
    return Buffer.concat([valid.subarray(0,33), chunk, valid.subarray(-12)]);
  };
  assert.throws(() => decode(withRaw([0,1,2])), /inflated length/);
  assert.throws(() => decode(withRaw(Buffer.alloc(100_000))), /larger than|too large|length/i);
});
test('CLI rejects negative control without rewriting baseline', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kinema-ui-probe-pixel-test-'));
  try {
    const file = path.join(dir, 'baseline.png');
    const original = png([0, 1,2,3,255, 4,5,6,255]);
    fs.writeFileSync(file, original);
    const command = path.join(__dirname, 'compare.cjs');
    assert.equal(spawnSync(process.execPath, [command, file, file]).status, 0);
    const negative = spawnSync(process.execPath, [command, file, file, '--negative-control']);
    assert.equal(negative.status, 1);
    assert.match(negative.stderr.toString(), /Pixel mismatch/);
    assert.deepEqual(fs.readFileSync(file), original);
  } finally { fs.rmSync(dir, { recursive: true }); }
});
test('browser baseline cannot be overwritten by artifact capture, even on acceptance', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'kinema-ui-probe-baseline-test-'));
  try {
    const file = path.join(dir, 'first-frame.png');
    fs.writeFileSync(file, 'baseline sentinel');
    for (const extra of [[], ['--accept-baseline']]) {
      const result = spawnSync(process.execPath, [path.join(__dirname, 'verify.cjs'), '--artifacts', dir, '--baseline', file, ...extra]);
      assert.equal(result.status, 1);
      assert.match(result.stderr.toString(), /Baseline must be outside/);
      assert.equal(fs.readFileSync(file, 'utf8'), 'baseline sentinel');
      assert.deepEqual(fs.readdirSync(dir), ['first-frame.png']);
    }
  } finally { fs.rmSync(dir, { recursive: true }); }
});
test('RGB decode and PNG row filters', () => {
  assert.deepEqual([...decode(png([1, 10,20,30, 1,2,3], 2)).pixels], [10,20,30,255,11,22,33,255]);
  for (const filter of [0,2,3,4]) {
    // First row has no previous row; left pixel is zero so all predictors are zero.
    assert.deepEqual([...decode(png([filter, 0,0,0, 5,6,7], 2)).pixels], [0,0,0,255,5,6,7,255]);
  }
  for (const filter of [1,2,3,4]) {
    const previous = [10,20,30,40,50,60], current = [70,80,90,100,110,120];
    const filtered = current.map((value, i) => {
      const a = i >= 3 ? current[i-3] : 0, b = previous[i], c = i >= 3 ? previous[i-3] : 0;
      const distances = [Math.abs(b-c), Math.abs(a-c), Math.abs(a+b-2*c)];
      const paeth = [a,b,c][distances.indexOf(Math.min(...distances))];
      return (value - [0,a,b,Math.floor((a+b)/2),paeth][filter] + 256) % 256;
    });
    const result = decode(png([0, ...previous, filter, ...filtered], 2));
    assert.deepEqual([...result.pixels], [10,20,30,255,40,50,60,255,70,80,90,255,100,110,120,255]);
  }
  assert.throws(() => decode(Buffer.from('not png')));
});
