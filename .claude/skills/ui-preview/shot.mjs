// Drives the served ui and saves screenshots. Copy and edit the steps per task.
//   usage: node shot.mjs <out_dir> [width] [height]
import { createRequire } from 'node:module';
import { execSync } from 'node:child_process';
import { existsSync } from 'node:fs';
const require = createRequire(execSync('npm root -g').toString().trim() + '/');
const { chromium } = require('playwright');

const [out = '.', w = '900', h = '640'] = process.argv.slice(2);
const exe = ['/opt/pw-browsers/chromium'].find(existsSync);
const browser = await chromium.launch(exe ? { executablePath: exe } : {});
const page = await browser.newPage({ viewport: { width: +w, height: +h } });
page.on('pageerror', (e) => console.log('pageerror:', e.message));
page.on('console', (m) => m.type() === 'error' && !/404/.test(m.text()) && console.log('console:', m.text()));

await page.goto('http://127.0.0.1:8765/');
await page.waitForTimeout(1500); // hydration
await page.screenshot({ path: `${out}/main.png` });

// --- example: open settings and visit each category ---
await page.getByRole('button', { name: '⚙️' }).click();
await page.waitForTimeout(400);
for (const [i, b] of (await page.locator('.modal-box nav li button').all()).entries()) {
  await b.click();
  await page.waitForTimeout(250);
  await page.screenshot({ path: `${out}/settings-${i}.png` });
}
await page.locator('.modal-box button', { hasText: '✕' }).click();
await page.waitForTimeout(300);

console.log(await page.evaluate(() => window.__callLog().join('\n')));
await browser.close();
