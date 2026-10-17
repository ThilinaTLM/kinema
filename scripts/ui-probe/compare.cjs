#!/usr/bin/env node
'use strict';
const fs = require('node:fs');
const { decode, assertMatch, diff } = require('./pixels.cjs');
const args = process.argv.slice(2);
if (args.length < 2 || args.length > 3 || (args[2] && args[2] !== '--negative-control')) {
  console.error('Usage: node scripts/ui-probe/compare.cjs BASELINE.png ACTUAL.png [--negative-control]');
  process.exit(2);
}
try {
  const baseline = decode(fs.readFileSync(args[0]));
  const actual = decode(fs.readFileSync(args[1]));
  // Intentionally fail the real comparison, without changing either PNG on disk.
  if (args[2]) actual.pixels[0] ^= 255;
  console.log(JSON.stringify(diff(baseline, actual)));
  assertMatch(baseline, actual);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
