import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runReplayChat } from "./runner.mjs";

const FAKE = new URL("./fake_app.mjs", import.meta.url).pathname;
const SAMPLE = [
  '{"delay_ms": 0, "channel": "WORLD", "nickname": "a", "level": 60, "text": "こんにちは"}',
  '{"delay_ms": 300, "channel": "PARTY", "nickname": "b", "level": 60, "text": "回復<sprite=3>"}',
  '{"delay_ms": 300, "channel": "GUILD", "nickname": "c", "level": 60, "text": "emojiPic=9"}',
].join("\n");

function setup() {
  const dir = mkdtempSync(join(tmpdir(), "runner-"));
  const sample = join(dir, "sample.jsonl");
  writeFileSync(sample, SAMPLE);
  return { dir, sample };
}
const run = (env, extra = {}) => {
  const { dir, sample } = setup();
  return runReplayChat({
    command: { file: process.execPath, args: [FAKE] },
    sample,
    workDir: join(dir, "work"),
    env: { ...process.env, ...env },
    readyTimeout: 15000,
    quitTimeout: 3000,
    finishSlack: 5000,
    ...extra,
  });
};
const failed = (r) => r.checks.filter((c) => !c.ok).map((c) => c.id);

test("the runner starts the app, drives replay-chat over the bridge, checks it and quits it -- no prompts", async () => {
  const r = await run({});
  assert.deepEqual(failed(r), []);
  assert.equal(r.ok, true);
  assert.deepEqual(r.checks.map((c) => c.id), ["A1", "A2", "B0", "A3", "B1", "B2", "B3", "B4", "B5", "Q1"]);
  assert.ok(existsSync(r.recording));
});

test("an app that replays without publishing the lines fails B1-B2 and still quits", async () => {
  const r = await run({ FAKE_APP_DROP_CHAT: "1" });
  assert.deepEqual(failed(r), ["B1", "B2", "B3"]);
  assert.equal(r.ok, false);
  assert.equal(r.checks.find((c) => c.id === "Q1").ok, true);
});

test("an app that ignores quit is killed and Q1 fails", async () => {
  const r = await run({ FAKE_APP_IGNORE_QUIT: "1" }, { quitTimeout: 500 });
  assert.deepEqual(failed(r), ["Q1"]);
});

test("an app that never connects fails A1 without hanging", async () => {
  const { dir, sample } = setup();
  const r = await runReplayChat({
    command: { file: process.execPath, args: ["-e", "process.exit(3)", "--"] },
    sample,
    workDir: join(dir, "work"),
    readyTimeout: 3000,
  });
  assert.equal(r.ok, false);
  assert.equal(r.checks[0].id, "A1");
  assert.match(r.checks[0].detail, /exited with code 3/);
});
