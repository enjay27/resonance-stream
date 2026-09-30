#!/usr/bin/env node
// SessionStart hook for Graft (https://github.com/trailhq/Graft).
//
// Cloud sessions start from a fresh clone, so graft/ (git-ignored) is missing.
// Before handing off to graft's own session-start, this:
//   1. installs graft if the environment's setup script didn't;
//   2. records repo-only wiring (global:false) so graft's wiring refresh doesn't
//      write hooks into ~/.claude (trailhq/Graft#491, #497);
//   3. builds the structural graph (no LLM, no API key).
// Locally it only runs graft's session-start, which no-ops if graft isn't installed.
//
// Replaces graft's own SessionStart entry: re-running `graft init` adds that entry
// back to .claude/settings.json, so remove it again afterwards.
// It also resets .mcp.json's graft server to a bare `graft mcp`; point it back at
// .claude/helpers/graft-mcp.cjs, which waits for the install this hook does.
const fs = require('fs');
const path = require('path');
const { execSync, spawnSync } = require('child_process');

const GRAFT_VERSION = '0.21.1';
const dir = process.env.CLAUDE_PROJECT_DIR || process.cwd();
const quiet = (cmd) => execSync(cmd, { cwd: dir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });

if (process.env.CLAUDE_CODE_REMOTE === 'true') {
  try {
    let version;
    try {
      version = quiet('graft --version').trim();
    } catch {
      quiet(`npm i -g @nanonets/graft@${GRAFT_VERSION}`);
      version = quiet('graft --version').trim();
    }
    // Same shape graft's writeStamp() uses (dist/upkeep.js); re-check when upgrading graft.
    const stamp = path.join(dir, 'graft', '.cache', 'wiring-stamp.json');
    if (!fs.existsSync(stamp)) {
      fs.mkdirSync(path.dirname(stamp), { recursive: true });
      fs.writeFileSync(stamp, JSON.stringify({
        version,
        hosts: ['claude'],
        opts: { global: false, mcp: true, hooks: true, statusline: false },
        at: new Date().toISOString(),
      }, null, 2));
    }
    quiet('graft build');
  } catch {
    // Graft is an optimization; never block the session over it.
  }
}

const r = spawnSync(process.execPath, [path.join(dir, '.claude', 'helpers', 'graft-hooks.cjs'), 'session-start'], {
  stdio: 'inherit',
});
process.exit(r.status ?? 0);
