#!/usr/bin/env node
'use strict';
// Linux-only feasibility probe. Never inherits a display or launches the real app.
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const { spawn, execFile } = require('node:child_process');
const { promisify } = require('node:util');
const assert = require('node:assert/strict');
const { decode, crop, encode, diff, assertMatch, differenceImage } = require('./pixels.cjs');
const exec = promisify(execFile);
const args = process.argv.slice(2);
const artifacts = path.resolve(args[0] && !args[0].startsWith('--') ? args.shift() : 'target/ui-probe/native-artifacts');
let inputProof = false, gallery = false, light = false, lab = false, scene, reduceMotion = false, hover, focusPoint;
while (args.length) {
  const arg = args.shift();
  if (arg === '--input') inputProof = true;
  else if (arg === '--gallery') gallery = true;
  else if (arg === '--lab') lab = true;
  else if (arg === '--reduce-motion') reduceMotion = true;
  else if (arg === '--scene') {
    scene = args.shift();
    assert.ok(['components', 'browse', 'menus', 'sheet', 'sheet-ready', 'detail', 'series', 'series-streams', 'states'].includes(scene), '--scene requires a lab scene name');
  } else if (arg === '--light') light = true;
  else if (arg === '--hover' || arg === '--focus') {
    const value = args.shift();
    assert.ok(/^\d+,\d+$/.test(value || ''), `${arg} requires X,Y`);
    if (arg === '--hover') hover = value.split(',').map(Number);
    else focusPoint = value.split(',').map(Number);
  } else throw new Error(`Unknown argument: ${arg}`);
}
assert.ok(!(lab && gallery), '--lab and --gallery are mutually exclusive');
assert.ok(!scene || lab, '--scene requires --lab');
assert.ok(!reduceMotion || lab, '--reduce-motion requires --lab');
assert.ok(!inputProof || !(gallery || lab), '--input coordinates apply only to the counter specimen');
assert.ok(!light || gallery || lab, '--light requires --gallery or --lab');
assert.ok(!hover || gallery || lab, '--hover requires --gallery or --lab');
assert.ok(!focusPoint || gallery || lab, '--focus requires --gallery or --lab');
const binary = path.resolve('target/debug/ui-probe');
// xdotool's legacy WM_NAME search can transliterate the em dash differently
// from getwindowname's UTF-8 _NET_WM_NAME. Match the ASCII fixture marker.
const fixtureTitle = '^Kinema.*FIXTURE ONLY$';
const children = [];
const report = { scope: 'isolated Xvfb fixture-window feasibility only; not personal desktop, full application, native performance or cross-platform acceptance', passed: false };
let home, env, cleaning = false;
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
function start(command, args, extra = {}) {
  const child = spawn(command, args, { env, detached: true, stdio: ['ignore', 'pipe', 'pipe'], ...extra });
  children.push(child);
  return child;
}
function logs(child) {
  const output = { stdout: '', stderr: '' };
  child.stdout?.on('data', data => { output.stdout += data; });
  child.stderr?.on('data', data => { output.stderr += data; });
  child.on('error', error => { output.error = String(error); });
  child.on('exit', (code, signal) => { output.exit = { code, signal }; });
  return output;
}
async function cleanup() {
  if (cleaning) return;
  cleaning = true;
  for (const child of children.toReversed()) {
    if (child.exitCode === null && child.signalCode === null && child.pid) {
      try { process.kill(-child.pid, 'SIGTERM'); } catch {}
    }
  }
  await pause(300);
  for (const child of children) {
    if (child.exitCode === null && child.signalCode === null && child.pid) {
      try { process.kill(-child.pid, 'SIGKILL'); } catch {}
    }
  }
  if (home) await fs.rm(home, { recursive: true, force: true });
}
for (const signal of ['SIGINT','SIGTERM']) process.once(signal, () => cleanup().finally(() => process.exit(signal === 'SIGINT' ? 130 : 143)));
async function main() {
  if (process.platform !== 'linux') throw new Error('This bounded feasibility script is Linux/Xvfb only');
  await fs.access(binary);
  await fs.mkdir(artifacts, { recursive: true });
  home = await fs.mkdtemp(path.join(os.tmpdir(), 'kinema-ui-probe-native-'));
  await fs.chmod(home, 0o700);
  env = { ...process.env, HOME: home, XDG_CONFIG_HOME: home, XDG_CACHE_HOME: home, XDG_DATA_HOME: home, XDG_RUNTIME_DIR: home, XDG_SESSION_TYPE: 'x11' };
  // Do not even pass the personal display to the server or fixture child.
  delete env.DISPLAY; delete env.WAYLAND_DISPLAY; delete env.WAYLAND_SOCKET;
  delete env.XAUTHORITY; delete env.DBUS_SESSION_BUS_ADDRESS; delete env.DBUS_SESSION_BUS_PID;
  const xvfb = start('Xvfb', ['-displayfd', '3', '-screen', '0', '1920x1200x24', '-nolisten', 'tcp', '-ac', '-noreset'], { stdio: ['ignore', 'pipe', 'pipe', 'pipe'] });
  report.xvfb = logs(xvfb);
  const displayNumber = await new Promise((resolve, reject) => {
    let text = '';
    const timer = setTimeout(() => reject(new Error('Xvfb display allocation timeout')), 5000);
    xvfb.stdio[3].on('data', data => {
      text += data;
      if (/^\d+\n/.test(text)) { clearTimeout(timer); resolve(text.trim()); }
    });
    xvfb.once('error', error => { clearTimeout(timer); reject(error); });
    xvfb.once('exit', code => { clearTimeout(timer); reject(new Error(`Xvfb exited ${code}`)); });
  });
  if (!/^\d+$/.test(displayNumber)) throw new Error('Invalid owned display number');
  if (displayNumber === '0') throw new Error('Refusing display :0 even if Xvfb allocated it; no capture attempted');
  env.DISPLAY = `:${displayNumber}`;
  report.ownedDisplay = env.DISPLAY;
  const icd = process.env.KINEMA_UI_PROBE_VULKAN_ICD;
  let fixtureEnv = env;
  if (icd) {
    await fs.access(icd);
    fixtureEnv = { ...env, VK_DRIVER_FILES: icd, VK_ICD_FILENAMES: icd };
    report.vulkanIcdOverride = icd;
  }
  const appArgs = lab ? ['--lab', ...(scene ? ['--scene', scene] : []), ...(light ? ['--light'] : []), ...(reduceMotion ? ['--reduce-motion'] : [])] : gallery ? ['--gallery', ...(light ? ['--light'] : [])] : [];
  report.scenario = lab ? `lab-${scene || 'components'}-${light ? 'light' : 'dark'}` : gallery ? `gallery-${light ? 'light' : 'dark'}` : 'counter';
  const app = start(binary, appArgs, { env: fixtureEnv });
  report.fixture = logs(app);
  let windowId;
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    if (report.fixture.exit || report.fixture.error) throw new Error(`Fixture failed before painted window: ${JSON.stringify(report.fixture)}`);
    try {
      const { stdout } = await exec('xdotool', ['search', '--onlyvisible', '--name', fixtureTitle], { env, timeout: 1000 });
      windowId = stdout.trim().split('\n')[0];
      if (/^\d+$/.test(windowId)) break;
    } catch {}
    if (!report.driverMapped && Date.now() > deadline - 12000) {
      try {
        const id = (await exec('xdotool', ['search', '--name', fixtureTitle], { env, timeout: 1000 })).stdout.trim().split('\n')[0];
        if (/^\d+$/.test(id)) {
          await exec('xdotool', ['windowmap', '--sync', id], { env, timeout: 2000 });
          report.driverMapped = { windowId: id, reason: 'Owned fixture existed unmapped without a window manager' };
        }
      } catch {}
    }
    await pause(200);
  }
  if (!windowId) {
    try {
      const ids = (await exec('xdotool', ['search', '--name', '.*'], { env, timeout: 2000 })).stdout.trim().split('\n').filter(id => /^\d+$/.test(id));
      report.ownedWindows = [];
      for (const id of ids.slice(0, 20)) {
        const name = (await exec('xdotool', ['getwindowname', id], { env, timeout: 1000 })).stdout.trim();
        report.ownedWindows.push({ id, name });
      }
    } catch (error) { report.windowEnumerationError = String(error); }
    throw new Error('No visible fixture window within 15 seconds');
  }
  report.windowId = windowId;
  report.windowTitle = (await exec('xdotool', ['getwindowname', windowId], { env, timeout: 2000 })).stdout.trim();
  assert.match(report.windowTitle, /^Kinema.*FIXTURE ONLY$/, 'Unexpected fixture title');
  if (lab) assert.ok(report.windowTitle.includes('Design Lab'), 'Binary did not open requested lab');
  if (gallery) assert.ok(report.windowTitle.includes('Graphite Gallery'), 'Binary did not open the requested gallery scenario');
  report.geometry = (await exec('xdotool', ['getwindowgeometry', '--shell', windowId], { env, timeout: 2000 })).stdout;
  if (lab) {
    // Xvfb starts its pointer at screen center, which otherwise discloses a card.
    await exec('xdotool', ['mousemove', '--window', windowId, '4', '796'], { env, timeout: 2000 });
  }
  // No render-ready adapter exists for native pixels: this is a candidate image only.
  await pause(1500);
  if (report.fixture.exit) throw new Error(`Fixture exited before capture: ${JSON.stringify(report.fixture.exit)}`);
  const png = path.join(artifacts, 'native-first-frame.png');
  await exec('import', ['-silent', '-window', windowId, '-depth', '8', `PNG32:${png}`], { env, timeout: 10000 });
  const image = decode(await fs.readFile(png));
  const colors = new Set();
  for (let i = 0; i < image.pixels.length; i += 4) colors.add(image.pixels.readUInt32BE(i));
  report.capture = { path: png, width: image.width, height: image.height, colors: colors.size, renderReadyAcknowledgment: false, needsAgentImageReview: true };
  if (colors.size < 17) throw new Error('Captured candidate has insufficient painted detail');
  if (hover) {
    assert.ok(hover[0] < image.width && hover[1] < image.height, 'Hover point outside fixture');
    await exec('xdotool', ['mousemove', '--window', windowId, ...hover.map(String)], { env, timeout: 2000 });
    if (lab) {
      await pause(80);
      const midPath = path.join(artifacts, 'native-hover-mid.png');
      await exec('import', ['-silent', '-window', windowId, '-depth', '8', `PNG32:${midPath}`], { env, timeout: 5000 });
      report.hoverMid = { path: midPath, nominalDelayMs: 80, needsAgentImageReview: true };
      await pause(220);
    } else await pause(250);
    const hoveredPath = path.join(artifacts, 'native-hover.png');
    await exec('import', ['-silent', '-window', windowId, '-depth', '8', `PNG32:${hoveredPath}`], { env, timeout: 5000 });
    const hovered = decode(await fs.readFile(hoveredPath));
    report.hover = { point: hover, path: hoveredPath, diff: diff(image, hovered), minimumDelayMs: lab ? 300 : 250, needsAgentImageReview: true };
    if (lab && scene === 'browse' && hover[0] >= 20 && hover[0] < 217 && hover[1] >= 68 && hover[1] < 363) {
      // First-card artwork only: no chips, disclosure panel or focus ring in this ROI.
      const artworkBounds = { x: 44, y: 112, width: 144, height: 64 };
      report.hover.artwork = { bounds: artworkBounds, diff: diff(crop(image, artworkBounds), crop(hovered, artworkBounds)) };
      assert.ok(report.hover.artwork.diff.changedPixels > 0, 'Hover did not visibly zoom first-card artwork');
    }
  }
  if (focusPoint) {
    assert.ok(focusPoint[0] < image.width && focusPoint[1] < image.height, 'Focus point outside fixture');
    if (lab) {
      assert.equal(scene || 'components', 'browse', 'Lab keyboard-focus capture is defined only for browse');
      await exec('xdotool', ['windowfocus', '--sync', windowId, 'mousemove', '--window', windowId, String(image.width - 4), String(image.height - 4), 'key', '--clearmodifiers', ...Array(8).fill('Tab')], { env, timeout: 3000 });
    } else {
      await exec('xdotool', ['mousemove', '--window', windowId, ...focusPoint.map(String), 'click', '1'], { env, timeout: 2000 });
    }
    if (!lab) await exec('xdotool', ['windowfocus', '--sync', windowId, 'mousemove', '--window', windowId, String(image.width - 4), String(image.height - 4)], { env, timeout: 2000 });
    await pause(250);
    const focusedPath = path.join(artifacts, 'native-focused.png');
    await exec('import', ['-silent', '-window', windowId, '-depth', '8', `PNG32:${focusedPath}`], { env, timeout: 5000 });
    report.focus = { point: focusPoint, keyboardOnly: lab, pointerOutsideCards: true, path: focusedPath, needsAgentImageReview: true };
  }
  if (inputProof) {
    assert.equal(image.width, 800); assert.equal(image.height, 600);
    const counterBounds = { x: 32, y: 136, width: 240, height: 40 };
    report.input = { oracle: 'Reviewed 800x600 image coordinates, not a native semantic snapshot; counter values require image review', counterBounds, counterGlyphRegionChecked: false };
    const capture = async name => {
      await pause(250);
      const file = path.join(artifacts, `${name}.png`);
      await exec('import', ['-silent', '-window', windowId, '-depth', '8', `PNG32:${file}`], { env, timeout: 5000 });
      return decode(await fs.readFile(file));
    };
    const saveRoi = async (name, frame) => {
      const roi = crop(frame, counterBounds);
      await fs.writeFile(path.join(artifacts, `${name}-counter.png`), encode(roi));
      return roi;
    };
    const initialRoi = await saveRoi('initial', image);
    report.input.stability = assertMatch(image, await capture('native-first-frame-repeat'));
    await exec('xdotool', ['mousemove', '--window', windowId, '152', '276', 'click', '1'], { env, timeout: 2000 });
    const disabledRoi = await saveRoi('disabled', await capture('native-disabled'));
    report.input.disabledCounterDiff = assertMatch(initialRoi, disabledRoi);
    await exec('xdotool', ['mousemove', '--window', windowId, '152', '216', 'click', '1'], { env, timeout: 2000 });
    const clickedRoi = await saveRoi('clicked', await capture('native-clicked'));
    report.input.pointerCounterDiff = diff(initialRoi, clickedRoi);
    await fs.writeFile(path.join(artifacts, 'native-clicked-counter-diff.png'), differenceImage(initialRoi, clickedRoi));
    assert.ok(report.input.pointerCounterDiff.changedPixels > 0, 'Pointer did not repaint counter ROI');
    await exec('xdotool', ['windowfocus', '--sync', windowId, 'key', 'Tab', 'Return'], { env, timeout: 2000 });
    const keyboardRoi = await saveRoi('keyboard', await capture('native-keyboard'));
    report.input.keyboardCounterDiff = diff(clickedRoi, keyboardRoi);
    await fs.writeFile(path.join(artifacts, 'native-keyboard-counter-diff.png'), differenceImage(clickedRoi, keyboardRoi));
    assert.ok(report.input.keyboardCounterDiff.changedPixels > 0, 'Keyboard did not repaint counter ROI');
    const corrupt = { ...image, pixels: Buffer.from(image.pixels) }; corrupt.pixels[0] ^= 255;
    assert.throws(() => assertMatch(image, corrupt), /Pixel mismatch/);
    report.input.negativeControlRejected = true;
    report.input.counterGlyphRegionChecked = true;
    report.input.baseline = 'Candidate screenshots only; no accepted baseline read or modified';
  }
  report.passed = true;
}
main().catch(error => { report.failure = String(error.stack || error); process.exitCode = 1; }).finally(async () => {
  await cleanup();
  await fs.mkdir(artifacts, { recursive: true });
  await fs.writeFile(path.join(artifacts, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(`${report.passed ? 'CANDIDATE CAPTURED' : 'BLOCKED'}: ${path.join(artifacts, 'report.json')}`);
  if (report.failure) console.error(report.failure);
});
