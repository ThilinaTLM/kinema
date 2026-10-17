#!/usr/bin/env node
'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const http = require('node:http');
const assert = require('node:assert/strict');
const { decode, diff, assertMatch, differenceImage, crop, encode } = require('./pixels.cjs');
function argumentsFor(argv) {
  const options = { build: 'target/ui-probe/web', artifacts: 'target/ui-probe/artifacts' };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--accept-baseline') options.accept = true;
    else if (['--build', '--artifacts', '--baseline'].includes(arg)) {
      assert.ok(argv[i+1] && !argv[i+1].startsWith('--'), `Missing value: ${arg}`);
      options[arg.slice(2)] = argv[++i];
    } else throw new Error(`Unknown argument: ${arg}`);
  }
  assert.ok(!options.accept || options.baseline, '--accept-baseline requires --baseline');
  return options;
}
function canonicalPath(file) {
  try { return require('node:fs').realpathSync(file); }
  catch (error) {
    if (error.code !== 'ENOENT') throw error;
    return path.join(canonicalPath(path.dirname(file)), path.basename(file));
  }
}
const options = argumentsFor(process.argv.slice(2));
const build = path.resolve(options.build), artifacts = path.resolve(options.artifacts);
if (options.baseline) {
  const relative = path.relative(canonicalPath(artifacts), canonicalPath(path.resolve(options.baseline)));
  assert.ok(relative !== '' && (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)), 'Baseline must be outside the artifact output directory, including with --accept-baseline');
}
const playwrightModule = process.env.KINEMA_PLAYWRIGHT_MODULE || 'playwright';
const { chromium } = require(playwrightModule);
const playwrightVersion = require(`${playwrightModule}/package.json`).version;
assert.equal(playwrightVersion, '1.62.1', 'Use matching Playwright 1.62.1 and its Chromium');
const report = { playwrightVersion, environment: 'Chromium headless, WebGL2 ANGLE SwiftShader, 800x600 DPR 1', console: [], pageErrors: [], blockedRequests: [], requests: [], snapshots: {} };
let browser, server;
let cleaning = false;
async function cleanup() {
  if (cleaning) return;
  cleaning = true;
  if (browser) await browser.close().catch(() => {});
  if (server) { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
}
for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => {
  cleanup().finally(() => process.exit(signal === 'SIGINT' ? 130 : 143));
});
async function main() {
  await fs.mkdir(artifacts, { recursive: true });
  const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.ttf': 'font/ttf', '.woff2': 'font/woff2', '.png': 'image/png' };
  // Only the generated bundle is exposed; no repository/user directories or proxies.
  server = http.createServer(async (request, response) => {
    try {
      const pathname = decodeURIComponent(new URL(request.url, 'http://127.0.0.1').pathname);
      const file = path.resolve(build, `.${pathname === '/' ? '/index.html' : pathname}`);
      if (!file.startsWith(build + path.sep)) { response.writeHead(403).end(); return; }
      const data = await fs.readFile(file);
      response.writeHead(200, { 'Content-Type': types[path.extname(file)] || 'application/octet-stream', 'Cache-Control': 'no-store' });
      response.end(data);
    } catch { response.writeHead(404).end('Not found'); }
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const origin = `http://127.0.0.1:${server.address().port}`;
  browser = await chromium.launch({ headless: true, args: ['--enable-unsafe-swiftshader', '--use-angle=swiftshader', '--disable-webgpu', '--disable-background-networking'] });
  report.browser = browser.version();
  const context = await browser.newContext({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 1, locale: 'en-US', timezoneId: 'UTC', reducedMotion: 'reduce', colorScheme: 'dark', serviceWorkers: 'block' });
  await context.route('**/*', route => {
    const url = route.request().url();
    if (url.startsWith(origin + '/')) return route.continue();
    report.blockedRequests.push(url); return route.abort('blockedbyclient');
  });
  const page = await context.newPage();
  page.on('console', message => report.console.push({ type: message.type(), text: message.text() }));
  page.on('pageerror', error => report.pageErrors.push(String(error)));
  page.on('response', response => report.requests.push({ url: response.url().replace(origin, ''), status: response.status() }));
  await page.goto(origin, { waitUntil: 'load', timeout: 30000 });
  await page.waitForFunction(() => window.probeLoadError || window.probeSnapshot?.().ready, null, { timeout: 60000 });
  assert.equal(await page.evaluate(() => window.probeLoadError), null, 'WASM bootstrap failed');
  const snapshot = () => page.evaluate(() => window.probeSnapshot());
  const counter = s => s.counter ?? s.state?.counter;
  const focused = s => s.focused_id ?? s.focusedId ?? s.focused;
  const elements = s => Array.isArray(s.elements) ? s.elements : Object.entries(s.elements ?? s.bounds ?? {}).map(([id, element]) => ({ id, disabled: id === 'probe.disabled' && s.disabled === true, ...element }));
  const first = await snapshot();
  report.snapshots.initial = first;
  assert.equal(counter(first), 0, 'Fixture must start with counter 0');
  assert.equal(first.status, 'fixture-only', 'Expected isolated fixture status');
  const enabled = elements(first).find(e => e.id === 'probe.increment' && !e.disabled);
  const disabled = elements(first).find(e => e.disabled);
  assert.ok(enabled, 'Snapshot must expose increment button bounds and ID');
  assert.ok(disabled, 'Snapshot must expose disabled action');
  const readout = elements(first).find(e => e.id === 'probe.counter');
  assert.ok(readout, 'Snapshot must expose actual counter readout bounds');
  const canvas = page.locator('canvas');
  await canvas.waitFor({ state: 'visible' });
  const box = await canvas.boundingBox();
  assert.ok(box && box.width === 800 && box.height === 600, 'Unexpected canvas bounds');
  report.graphics = await canvas.evaluate(element => {
    const gl = element.getContext('webgl2');
    if (!gl) return { backend: 'unknown' };
    const info = gl.getExtension('WEBGL_debug_renderer_info');
    return { backend: 'webgl2', renderer: info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER), vendor: info ? gl.getParameter(info.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR) };
  });
  assert.equal(report.graphics.backend, 'webgl2', 'Expected actual WebGL2 context');
  assert.match(report.graphics.renderer, /SwiftShader/i, 'Expected bounded software renderer');
  async function settledFrame() {
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  }
  async function capture(name) {
    await settledFrame();
    const png = await canvas.screenshot({ path: path.join(artifacts, `${name}.png`), animations: 'disabled', timeout: 30000 });
    return { png, image: decode(png) };
  }
  function center(element) {
    const b = element.bounds ?? element;
    const x = b.x ?? b.origin?.x, y = b.y ?? b.origin?.y;
    const width = b.width ?? b.size?.width, height = b.height ?? b.size?.height;
    assert.ok([x,y,width,height].every(Number.isFinite) && width > 0 && height > 0, `Invalid element bounds: ${JSON.stringify(element)}`);
    assert.ok(x >= 0 && y >= 0 && x + width <= box.width + 1 && y + height <= box.height + 1, 'Element outside canvas');
    return [box.x + x + width / 2, box.y + y + height / 2];
  }
  const initial = await capture('first-frame');
  assert.equal(initial.image.width, 800); assert.equal(initial.image.height, 600);
  const readoutBounds = readout.bounds ?? readout;
  // The readout ROI must not overlap either action: focus/hover pixels are not proof.
  for (const control of [enabled, disabled]) {
    const b = control.bounds ?? control, r = readoutBounds;
    assert.ok(r.x + r.width <= b.x || b.x + b.width <= r.x || r.y + r.height <= b.y || b.y + b.height <= r.y, 'Counter ROI overlaps action/focus region');
  }
  const initialReadout = crop(initial.image, readoutBounds);
  await fs.writeFile(path.join(artifacts, 'counter-initial.png'), encode(initialReadout));
  report.visualChecks = { glyphAndCounterRegionChecked: false, counterBounds: readoutBounds };
  const colors = new Set();
  for (let i = 0; i < initial.image.pixels.length; i += 4) colors.add(initial.image.pixels.readUInt32BE(i));
  assert.ok(colors.size > 16, `Canvas has insufficient painted detail (${colors.size} colors)`);
  report.paintedColors = colors.size;
  const repeat = await capture('first-frame-repeat');
  report.stability = assertMatch(initial.image, repeat.image);
  await page.mouse.click(...center(disabled));
  await settledFrame();
  report.snapshots.disabled = await snapshot();
  assert.equal(counter(report.snapshots.disabled), 0, 'Disabled action changed counter');
  await page.mouse.click(...center(enabled));
  await page.waitForFunction(() => { const s = window.probeSnapshot(); return (s.counter ?? s.state?.counter) === 1; });
  report.snapshots.clicked = await snapshot();
  const clicked = await capture('clicked');
  report.clickDiff = diff(initial.image, clicked.image);
  await fs.writeFile(path.join(artifacts, 'clicked-diff.png'), differenceImage(initial.image, clicked.image));
  assert.ok(report.clickDiff.changedPixels > 0, 'Pointer input did not change actual pixels');
  const clickedReadout = crop(clicked.image, readoutBounds);
  await fs.writeFile(path.join(artifacts, 'counter-clicked.png'), encode(clickedReadout));
  await fs.writeFile(path.join(artifacts, 'counter-clicked-diff.png'), differenceImage(initialReadout, clickedReadout));
  report.visualChecks.clickCounterDiff = diff(initialReadout, clickedReadout);
  assert.ok(report.visualChecks.clickCounterDiff.changedPixels > 0, 'Counter glyph ROI did not repaint after pointer activation');
  // Cycle real keyboard focus; never invoke a model/action export.
  let keyboardFocused = false;
  for (let i = 0; i < 8; i++) {
    await page.keyboard.press('Tab'); await settledFrame();
    if (focused(await snapshot()) === enabled.id) { keyboardFocused = true; break; }
  }
  assert.ok(keyboardFocused, 'Tab never focused increment action');
  report.snapshots.focused = await snapshot();
  await capture('keyboard-focused');
  await page.keyboard.press('Enter');
  await page.waitForFunction(() => { const s = window.probeSnapshot(); return (s.counter ?? s.state?.counter) === 2; });
  report.snapshots.keyboard = await snapshot();
  const keyboard = await capture('keyboard-activated');
  const keyboardReadout = crop(keyboard.image, readoutBounds);
  await fs.writeFile(path.join(artifacts, 'counter-keyboard.png'), encode(keyboardReadout));
  await fs.writeFile(path.join(artifacts, 'counter-keyboard-diff.png'), differenceImage(clickedReadout, keyboardReadout));
  report.visualChecks.keyboardCounterDiff = diff(clickedReadout, keyboardReadout);
  assert.ok(report.visualChecks.keyboardCounterDiff.changedPixels > 0, 'Counter glyph ROI did not repaint after keyboard activation');
  report.visualChecks.glyphAndCounterRegionChecked = true;
  // Same matcher used for real baselines; deliberately corrupt a pixel in memory.
  const corrupt = { ...initial.image, pixels: Buffer.from(initial.image.pixels) };
  corrupt.pixels[0] ^= 255;
  assert.throws(() => assertMatch(initial.image, corrupt), /Pixel mismatch/);
  report.negativeControl = { rejected: true, ...diff(initial.image, corrupt) };
  assert.equal(report.blockedRequests.length, 0, 'Public/unexpected network request attempted');
  assert.equal(report.pageErrors.length, 0, 'Browser page errors');
  assert.ok(report.requests.every(r => r.status >= 200 && r.status < 300), 'Failed asset request');
  const consoleErrors = report.console.filter(m => m.type === 'error');
  assert.equal(consoleErrors.length, 0, `Console errors: ${JSON.stringify(consoleErrors)}`);
  if (options.baseline) {
    const baselinePath = path.resolve(options.baseline);
    if (options.accept) {
      await fs.mkdir(path.dirname(baselinePath), { recursive: true });
      await fs.writeFile(baselinePath, initial.png);
      report.baseline = { acceptedExplicitly: true, path: baselinePath };
    } else {
      const baselineImage = decode(await fs.readFile(baselinePath));
      report.baseline = { path: baselinePath, ...diff(baselineImage, initial.image) };
      await fs.writeFile(path.join(artifacts, 'baseline-diff.png'), differenceImage(baselineImage, initial.image));
      assertMatch(baselineImage, initial.image);
    }
  } else report.baseline = { approved: false, note: 'Candidate only; no persistent baseline compared or changed.' };
  report.passed = true;
}
main().catch(error => { report.passed = false; report.failure = String(error.stack || error); process.exitCode = 1; }).finally(async () => {
  await cleanup();
  await fs.mkdir(artifacts, { recursive: true });
  await fs.writeFile(path.join(artifacts, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(`${report.passed ? 'PASS' : 'FAIL'}: ${path.join(artifacts, 'report.json')}`);
  if (report.failure) console.error(report.failure);
});
