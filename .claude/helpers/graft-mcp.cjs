#!/usr/bin/env node
// Launches `graft mcp` for .mcp.json.
//
// In cloud sessions graft is installed at session start (by the environment's
// setup script or by .claude/hooks/graft-bootstrap.cjs), and that install can
// finish after Claude Code spawns MCP servers, which then fail with
// "Executable not found in $PATH: graft". Wait for the install instead.
const path = require('path');
const { spawn, spawnSync } = require('child_process');

const WAIT_MS = 25000; // stays under Claude Code's default 30s MCP startup timeout
const POLL_MS = 500;
const win = process.platform === 'win32';

// Where `npm i -g` puts graft in the cloud image; not always on PATH at MCP spawn.
if (!win) process.env.PATH = ['/opt/node22/bin', process.env.PATH].join(path.delimiter);

const installed = () => spawnSync('graft', ['--version'], { stdio: 'ignore', shell: win }).status === 0;

(async () => {
  const deadline = Date.now() + (process.env.CLAUDE_CODE_REMOTE === 'true' ? WAIT_MS : 0);
  while (!installed()) {
    if (Date.now() >= deadline) {
      console.error('graft is not installed');
      process.exit(1);
    }
    await new Promise((r) => setTimeout(r, POLL_MS));
  }
  const child = spawn('graft', ['mcp', ...process.argv.slice(2)], { stdio: 'inherit', shell: win });
  for (const sig of ['SIGINT', 'SIGTERM']) process.on(sig, () => child.kill(sig));
  child.on('exit', (code, signal) => process.exit(signal ? 1 : code ?? 0));
})();
