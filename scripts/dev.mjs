import { spawn } from 'node:child_process';

const proxyUrl = 'http://127.0.0.1:3000/api/sponsor';
const proxyCommand = process.platform === 'win32' ? 'npx.cmd' : 'npx';
const appCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
let stopping = false;

function stop(child) {
  if (!child || child.exitCode !== null) return;
  child.kill('SIGTERM');
}

const proxy = spawn(
  proxyCommand,
  ['--yes', 'vercel@latest', 'dev', '--cwd', 'services/adtention-proxy', '--listen', '127.0.0.1:3000', '--project', 'r2modmac-sponsor-proxy', '--yes'],
  { stdio: 'inherit' },
);

async function waitForProxy() {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (proxy.exitCode !== null) throw new Error('The local sponsor proxy stopped before it was ready.');
    try {
      const response = await fetch(proxyUrl, { method: 'GET' });
      if (response.status === 204 || response.ok) return;
    } catch {
      // The function is still booting.
    }
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  throw new Error('Timed out while starting the local sponsor proxy.');
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
