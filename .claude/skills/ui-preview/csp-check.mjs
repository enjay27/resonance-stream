// Serves the real Trunk build with a Content-Security-Policy header and drives the app, recording violations.
//   usage: node csp-check.mjs <dist dir> <ui-preview web dir (mock.js, config.js)> <policy file> <label>
// See SKILL.md, "Checking a Content-Security-Policy". The screenshot lands next to this script.
import http from 'node:http';
import { readFileSync, existsSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { execSync } from 'node:child_process';
import { extname, join } from 'node:path';
const require = createRequire(execSync('npm root -g').toString().trim() + '/');
const { chromium } = require('playwright');

const [dist, previewWeb, policyFile, label = 'run'] = process.argv.slice(2);
const policy = readFileSync(policyFile, 'utf8').trim();
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm', '.mp3': 'audio/mpeg' };

const server = http.createServer((req, res) => {
  let p = decodeURIComponent(req.url.split('?')[0]);
  if (p === '/') p = '/index.html';
  const file = join(dist, p);
  if (!existsSync(file) || !statSync(file).isFile()) { res.writeHead(404); res.end('nf'); return; }
  res.writeHead(200, { 'content-type': types[extname(file)] || 'application/octet-stream', 'content-security-policy': policy });
  res.end(readFileSync(file));
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

const exe = ['/opt/pw-browsers/chromium'].find(existsSync);
const browser = await chromium.launch(exe ? { executablePath: exe } : {});
const page = await browser.newPage({ viewport: { width: 900, height: 640 } });
const problems = [];
page.on('pageerror', (e) => problems.push(`pageerror: ${e.message}`));
page.on('console', (m) => { if (m.type() === 'error') problems.push(`console: ${m.text().slice(0, 220)}`); });
page.on('requestfailed', (r) => problems.push(`requestfailed: ${r.url()} ${r.failure()?.errorText}`));
// The mock bridge and config come from the ui-preview build; init scripts run outside the page's policy.
await page.addInitScript({ content: readFileSync(join(previewWeb, 'config.js'), 'utf8') });
await page.addInitScript({ content: readFileSync(join(previewWeb, 'mock.js'), 'utf8') });
await page.addInitScript(() => {
  window.__csp = [];
  document.addEventListener('securitypolicyviolation', (e) => window.__csp.push(`${e.effectiveDirective} blocked ${e.blockedURI || '(inline)'} ${e.sample ? '| ' + e.sample.slice(0, 60) : ''}`));
});

await page.goto(base + '/');
await page.waitForTimeout(2500);
const rendered = await page.evaluate(() => document.body.innerText.trim().length);
const hasTabs = await page.locator('text=전체').count();
await page.screenshot({ path: `${process.argv[1].replace(/[^/]*$/, '')}csp-${label}-main.png` });

if (hasTabs) {
  // settings: every category, then close
  await page.getByRole('button', { name: '⚙️' }).click().catch(() => {});
  await page.waitForTimeout(500);
  for (const b of await page.locator('.modal-box nav li button').all()) { await b.click(); await page.waitForTimeout(200); }
  await page.locator('.modal-box button', { hasText: '✕' }).click().catch(() => {});
  await page.waitForTimeout(300);
  // dictionary editor, search, favorites, cheat sheet, whatever the toolbar opens
  for (const sel of ['button[title*="사전"]', 'button[title*="검색"]', 'button[title*="즐겨찾기"]', 'button[title*="치트"]']) {
    const el = page.locator(sel).first();
    if (await el.count()) { await el.click().catch(() => {}); await page.waitForTimeout(400); await page.keyboard.press('Escape'); await page.waitForTimeout(200); }
  }
  // audio: the ping the app plays, loaded the way the page would
  await page.evaluate(() => { try { const a = new Audio('/public/ping.mp3'); a.load(); } catch (e) {} });
  await page.waitForTimeout(500);
  // dynamic inline styles and eval, directly
  await page.evaluate(() => { const d = document.createElement('div'); d.setAttribute('style', 'opacity:.5'); document.body.appendChild(d); d.style.color = 'red'; });
}
const violations = await page.evaluate(() => window.__csp);
console.log(`[${label}] rendered text length: ${rendered}; main view present: ${hasTabs > 0}`);
console.log(`[${label}] violations: ${violations.length}`); violations.slice(0, 12).forEach((v) => console.log('   ' + v));
const other = problems.filter((p) => !/Content Security Policy|Refused to/.test(p));
console.log(`[${label}] other page problems: ${other.length}`); other.slice(0, 8).forEach((p) => console.log('   ' + p));
await browser.close(); server.close();
