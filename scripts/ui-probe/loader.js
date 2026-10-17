import init, { run, probe_snapshot } from './kinema_desktop.js';

// Only read-only instrumentation is exposed. Inputs go through canvas events.
window.probeLoadError = null;
try {
  await init();
  window.probeSnapshot = () => JSON.parse(probe_snapshot());
  await run(`${location.origin}/assets/`);
} catch (error) {
  window.probeLoadError = String(error?.stack || error);
  console.error(error);
}
