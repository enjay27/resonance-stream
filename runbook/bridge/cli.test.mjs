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
    app.publishAsync("rs/app/event/chat-message-update", JSON.stringify({ seq, t_ms: t0, name: "chat-message-update", payload: { pid: seq, channel: "WORLD", nickname: "a", message, level: 1 } }));
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
