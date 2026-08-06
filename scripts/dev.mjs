import { spawn, spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const repoRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const workerDir = path.join(repoRoot, 'services/adtention-worker');
const proxyUrl = 'http://127.0.0.1:3000/api/sponsor';
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const appCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
let stopping = false;

function stop(child) {
  if (!child || child.exitCode !== null) return;
  child.kill('SIGTERM');
}

// wrangler is a local devDependency of the worker, unlike the old `npx
// vercel@latest` invocation this replaces, which re-resolved and could
// re-download the CLI from the registry on every single `npm run dev`.
// The only network call now is this one-time install on a fresh clone.
if (!existsSync(path.join(workerDir, 'node_modules'))) {
  console.log('[dev] Installing services/adtention-worker dependencies (first run only)...');
  const install = spawnSync(npmCommand, ['install'], { cwd: workerDir, stdio: 'inherit' });
  if (install.status !== 0) {
    console.error('[dev] Failed to install services/adtention-worker dependencies.');
    process.exit(install.status ?? 1);
  }
}

const proxy = spawn(npmCommand, ['run', 'dev'], { stdio: 'inherit', cwd: workerDir });

async function waitForProxy() {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (proxy.exitCode !== null) throw new Error('The local sponsor Worker stopped before it was ready.');
    try {
      const response = await fetch(proxyUrl, { method: 'GET' });
      if (response.status === 204 || response.ok) return;
    } catch {
      // The Worker is still booting.
    }
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  throw new Error('Timed out while starting the local sponsor Worker.');
}

try {
  await waitForProxy();
  const app = spawn(appCommand, ['exec', 'tauri', 'dev'], {
    stdio: 'inherit',
    env: { ...process.env, R2MODMAC_SPONSOR_PROXY_URL: proxyUrl },
  });

  const shutdown = () => {
    if (stopping) return;
    stopping = true;
    stop(app);
    stop(proxy);
  };
  process.on('SIGINT', shutdown);
  process.on('SIGTERM', shutdown);

  app.on('exit', (code) => {
    shutdown();
    process.exitCode = code ?? 0;
  });
} catch (error) {
  stop(proxy);
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
}
