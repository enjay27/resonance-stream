import test from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import mqtt from "mqtt";

const CLI = new URL("./cli.mjs", import.meta.url).pathname;
const SAMPLE = '{"delay_ms":0,"channel":"WORLD","nickname":"a","level":1,"text":"x"}\n';

function serve(out) {
  const child = spawn(process.execPath, [CLI, "serve", "--out", out], { stdio: ["pipe", "pipe", "inherit"] });
  const lines = [];
  const waiters = [];
  let buffer = "";
  child.stdout.on("data", (chunk) => {
    buffer += chunk;
    let at;
    while ((at = buffer.indexOf("\n")) >= 0) {
      lines.push(JSON.parse(buffer.slice(0, at)));
      buffer = buffer.slice(at + 1);
    }
    while (waiters.length && lines.length) waiters.shift()(lines.shift());
  });
  const next = () => (lines.length ? Promise.resolve(lines.shift()) : new Promise((r) => waiters.push(r)));
  return { child, next };
}

test("serve records the app's events, forwards commands and stops when stdin closes; verify reads the recording", async () => {
  const dir = mkdtempSync(join(tmpdir(), "bridge-cli-"));
  const out = join(dir, "events.jsonl");
  const { child, next } = serve(out);
  const { port } = await next();
  assert.ok(port > 0);

  const app = await mqtt.connectAsync(`mqtt://127.0.0.1:${port}`);
  await app.subscribeAsync("rs/test/command/+");
  app.on("message", (_t, p) => {
    const { id } = JSON.parse(p.toString());
    app.publish(`rs/app/ack/${id}`, JSON.stringify({ id, ok: true }));
  });
  child.stdin.write(JSON.stringify({ send: "ping", args: {} }) + "\n");
  assert.equal((await next()).ack.ok, true);
  child.stdin.write(JSON.stringify({ send: "rm-rf" }) + "\n");
  // the stand-in acks everything; a refusal is covered in bridge.test.mjs
  assert.equal((await next()).ack.ok, true);

  const t0 = Date.now();
  const chat = (seq, message) =>
    app.publishAsync("rs/app/event/packet-event", JSON.stringify({ seq, t_ms: t0, name: "packet-event", payload: { pid: seq, channel: "WORLD", nickname: "a", message, level: 1 } }));
  await chat(1, "x");
  await app.publishAsync("rs/app/event/system-event", JSON.stringify({ seq: 2, t_ms: t0, name: "system-event", payload: { level: "info", source: "Replay", message: "Replay finished" } }));
  await new Promise((r) => setTimeout(r, 200));
  await app.endAsync();

  const exited = new Promise((r) => child.on("exit", r));
  child.stdin.end();
  assert.equal(await exited, 0);

  const sample = join(dir, "sample.jsonl");
  writeFileSync(sample, SAMPLE);
  const ok = spawnSync(process.execPath, [CLI, "verify", "replay-chat", "--log", out, "--sample", sample], { encoding: "utf8" });
  assert.equal(ok.status, 0, ok.stdout);
  assert.equal(JSON.parse(ok.stdout).ok, true);

  writeFileSync(sample, SAMPLE + SAMPLE);
  const bad = spawnSync(process.execPath, [CLI, "verify", "replay-chat", "--log", out, "--sample", sample], { encoding: "utf8" });
  assert.equal(bad.status, 1);
  assert.equal(JSON.parse(bad.stdout).checks.find((c) => c.id === "B1").ok, false);
});

test("a bad command line exits 2 with a reason", () => {
  const r = spawnSync(process.execPath, [CLI, "verify", "nope"], { encoding: "utf8" });
  assert.equal(r.status, 2);
  assert.match(r.stderr, /unknown scenario nope/);
});

test("run replay-chat does the whole test by itself and exits 0 (a stand-in app)", () => {
  const dir = mkdtempSync(join(tmpdir(), "bridge-run-"));
  const sample = join(dir, "sample.jsonl");
  writeFileSync(sample, '{"delay_ms":0,"channel":"WORLD","nickname":"a","level":1,"text":"x"}\n{"delay_ms":200,"channel":"GUILD","nickname":"b","level":1,"text":"y<sprite=1>"}\n');
  const fake = new URL("./fake_app.mjs", import.meta.url).pathname;
  const args = [CLI, "run", "replay-chat", "--exe", process.execPath, "--exe-arg", fake, "--sample", sample, "--work", join(dir, "work")];
  const ok = spawnSync(process.execPath, args, { encoding: "utf8", env: { ...process.env, FAKE_APP_LEAD_IN_MS: "20" }, timeout: 60000 });
  assert.equal(ok.status, 0, ok.stderr);
  const report = JSON.parse(ok.stdout);
  assert.equal(report.ok, true);
  assert.match(ok.stderr, /\[PASS\] Q1/);
  const bad = spawnSync(process.execPath, args, { encoding: "utf8", env: { ...process.env, FAKE_APP_LEAD_IN_MS: "20", FAKE_APP_DROP_CHAT: "1" }, timeout: 60000 });
  assert.equal(bad.status, 1);
});

test("run without --exe is a bad command line", () => {
  const r = spawnSync(process.execPath, [CLI, "run", "replay-chat"], { encoding: "utf8" });
  assert.equal(r.status, 2);
  assert.match(r.stderr, /needs --exe/);
});

test("serve answers expect and expect_sequence from stdin", async () => {
  const dir = mkdtempSync(join(tmpdir(), "bridge-cli-"));
  const { child, next } = serve(join(dir, "events.jsonl"));
  const { port } = await next();
  const app = await mqtt.connectAsync(`mqtt://127.0.0.1:${port}`);
  const emit = (seq, name, payload) =>
    app.publishAsync(`rs/app/event/${name}`, JSON.stringify({ seq, t_ms: Date.now(), name, payload }));
  await emit(0, "update-state", { state: "available:0.6.9" });
  await emit(1, "update-state", { state: "downloading" });
  await emit(2, "update-state", { state: "downloaded" });
  const ask = async (command) => {
    child.stdin.write(JSON.stringify(command) + "\n");
    return next();
  };
  const one = await ask({ expect: { topic: "rs/app/event/update-state", match: { "payload.state": "downloading" }, timeout: 2000 } });
  assert.equal(one.found.seq, 1);
  const none = await ask({ expect: { topic: "rs/app/event/update-state", match: { "payload.state": "error" }, timeout: 150 } });
  assert.match(none.error, /timed out/);
  const seq = await ask({
    expect_sequence: {
      steps: ["available", "downloading", "downloaded"].map((p) => ({ topic: "rs/app/event/update-state", match: { "payload.state": { regex: "^" + p } } })),
      timeout: 2000,
    },
  });
  assert.deepEqual(seq.found.map((m) => m.seq), [0, 1, 2]);
  const wrong = await ask({
    expect_sequence: { steps: [{ topic: "rs/app/event/update-state", match: { "payload.state": "downloaded" } }, { topic: "rs/app/event/update-state", match: { "payload.state": "downloading" } }], timeout: 150 },
  });
  assert.match(wrong.error, /step 2/);
  await app.endAsync();
  child.stdin.end();
});
