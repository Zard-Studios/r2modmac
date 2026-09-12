import { spawn } from 'node:child_process';
const npmCliPath = process.env.npm_execpath;
const npmCommand = npmCliPath ? process.execPath : process.platform === 'win32' ? 'npm.cmd' : 'npm';
const npmArgsPrefix = npmCliPath ? [npmCliPath] : [];
const npmNeedsShell = !npmCliPath && process.platform === 'win32';

function runNpm(args, options) {
  return spawn(npmCommand, [...npmArgsPrefix, ...args], {
    ...options,
    ...(npmNeedsShell ? { shell: true } : {}),
  });
}

const app = runNpm(['exec', 'tauri', 'dev'], { stdio: 'inherit' });
app.once('error', (error) => {
  console.error(`[dev] Failed to start Tauri: ${error.message}`);
  process.exitCode = 1;
});
app.once('exit', (code) => {
  process.exitCode = code ?? 0;
});
