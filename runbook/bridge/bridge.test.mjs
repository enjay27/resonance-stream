import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import mqtt from "mqtt";
import { Bridge, readRecording } from "./bridge.mjs";

// A stand-in for the app: connects to the broker like it does, publishes events,
// and answers commands the way src-tauri/src/bridge.rs does. NOT the app.
async function fakeApp(port, { answer = true } = {}) {
  const client = await mqtt.connectAsync(`mqtt://127.0.0.1:${port}`);
  await client.subscribeAsync("rs/test/command/+");
  const received = [];
  client.on("message", (topic, payload) => {
    const body = JSON.parse(payload.toString());
    received.push({ topic, body });
    if (answer) client.publish(`rs/app/ack/${body.id}`, JSON.stringify({ id: body.id, ok: true }));
  });
  let seq = 0;
  const emit = (name, payload) =>
    client.publishAsync(`rs/app/event/${name}`, JSON.stringify({ seq: seq++, t_ms: Date.now(), name, payload }));
  return { client, emit, received };
}

const tmp = () => mkdtempSync(join(tmpdir(), "bridge-"));

test("the bridge records what the app publishes, in a JSONL file", async () => {
  const log = join(tmp(), "events.jsonl");
  const bridge = await Bridge.start({ logFile: log });
  const app = await fakeApp(bridge.port);
  await app.emit("sniffer-state", { running: true });
  await bridge.expect("rs/app/event/sniffer-state", { timeout: 2000 });
  await app.client.endAsync();
  await bridge.close();
  const lines = readFileSync(log, "utf8").trim().split("\n").map((l) => JSON.parse(l));
  assert.equal(lines.length, 1);
  assert.equal(lines[0].topic, "rs/app/event/sniffer-state");
  assert.deepEqual(lines[0].message.payload, { running: true });
  assert.deepEqual(readRecording(log), lines);
});

test("expect waits for a message that matches, and times out otherwise", async () => {
  const bridge = await Bridge.start();
  const app = await fakeApp(bridge.port);
  const waiting = bridge.expect("rs/app/event/system-event", {
    where: (m) => m.payload.message === "Replay finished",
    timeout: 2000,
  });
  await app.emit("system-event", { message: "other" });
  await app.emit("system-event", { message: "Replay finished" });
  assert.equal((await waiting).payload.message, "Replay finished");
  await assert.rejects(bridge.expect("rs/app/event/nothing", { timeout: 100 }), /timed out/);
  await app.client.endAsync();
  await bridge.close();
});

test("expect also sees a message that came before it was called", async () => {
  const bridge = await Bridge.start();
  const app = await fakeApp(bridge.port);
  await app.emit("translator-state", { state: "ready" });
  await bridge.expect("rs/app/event/translator-state", { timeout: 2000 });
  const again = await bridge.expect("rs/app/event/translator-state", { timeout: 2000 });
  assert.equal(again.payload.state, "ready");
  await app.client.endAsync();
  await bridge.close();
});

test("expectSequence needs the steps in order, other messages in between are fine", async () => {
  const bridge = await Bridge.start();
  const app = await fakeApp(bridge.port);
  await app.emit("chat-message-update", { n: 1 });
  await app.emit("sniffer-state", {});
  await app.emit("chat-message-update", { n: 2 });
  await app.emit("system-event", { message: "done" });
  const steps = [
    { topic: "rs/app/event/chat-message-update", where: (m) => m.payload.n === 1 },
    { topic: "rs/app/event/chat-message-update", where: (m) => m.payload.n === 2 },
    { topic: "rs/app/event/system-event" },
  ];
  assert.equal((await bridge.expectSequence(steps, { timeout: 2000 })).length, 3);
  const wrongOrder = [steps[1], steps[0]];
  await assert.rejects(bridge.expectSequence(wrongOrder, { timeout: 150 }), /step 2/);
  await app.client.endAsync();
  await bridge.close();
});

test("send publishes a command with an id and returns the app's ack", async () => {
  const bridge = await Bridge.start();
  const app = await fakeApp(bridge.port);
  const ack = await bridge.send("replay-chat", { path: "C:/x.jsonl" }, { timeout: 2000 });
  assert.equal(ack.ok, true);
  assert.equal(app.received[0].topic, "rs/test/command/replay-chat");
  assert.equal(app.received[0].body.path, "C:/x.jsonl");
  assert.equal(app.received[0].body.id, ack.id);
  await app.client.endAsync();
  await bridge.close();
});

test("send fails when the app does not answer, and when it refuses", async () => {
  const bridge = await Bridge.start();
  const silent = await fakeApp(bridge.port, { answer: false });
  await assert.rejects(bridge.send("ping", {}, { timeout: 150 }), /no ack/);
  await silent.client.endAsync();

  const refuser = await mqtt.connectAsync(`mqtt://127.0.0.1:${bridge.port}`);
  await refuser.subscribeAsync("rs/test/command/+");
  refuser.on("message", (_t, p) => {
    const { id } = JSON.parse(p.toString());
    refuser.publish(`rs/app/ack/${id}`, JSON.stringify({ id, ok: false, error: "unknown command" }));
  });
  await assert.rejects(bridge.send("rm-rf", {}, { timeout: 2000 }), /unknown command/);
  await refuser.endAsync();
  await bridge.close();
});

test("the broker only listens on this machine", async () => {
  const bridge = await Bridge.start();
  assert.equal(bridge.host, "127.0.0.1");
  assert.ok(bridge.port > 0);
  await bridge.close();
});

import { matcher } from "./bridge.mjs";

test("a matcher is a JSON spec: dotted paths that must equal, contain or match a value", () => {
  const m = { name: "update-state", payload: { state: "available:0.6.9", n: 3, nested: { ok: true } } };
  assert.equal(matcher({})(m), true);
  assert.equal(matcher({ name: "update-state" })(m), true);
  assert.equal(matcher({ "payload.state": "available:0.6.9", "payload.nested.ok": true })(m), true);
  assert.equal(matcher({ "payload.n": 4 })(m), false);
  assert.equal(matcher({ "payload.missing": "x" })(m), false);
  assert.equal(matcher({ "payload.state": { contains: "0.6.9" } })(m), true);
  assert.equal(matcher({ "payload.state": { contains: "0.7" } })(m), false);
  assert.equal(matcher({ "payload.state": { regex: "^available:\\d+\\.\\d+\\.\\d+$" } })(m), true);
  assert.equal(matcher({ "payload.state": { regex: "^error" } })(m), false);
  assert.equal(matcher({ "payload.state": { regex: "(" } })(m), false, "a bad pattern matches nothing");
  assert.equal(matcher({ "payload.n": { not: 4 } })(m), true);
  assert.equal(matcher({ "payload.n": { not: 3 } })(m), false);
  assert.equal(matcher({ "payload.missing": { not: 3 } })(m), true, "a missing value is not that value");
  assert.equal(matcher(undefined)(m), true);
  // a plain-text message (the status topic) is compared as it is
  assert.equal(matcher("online")("online"), true);
  assert.equal(matcher("online")("offline"), false);
});
