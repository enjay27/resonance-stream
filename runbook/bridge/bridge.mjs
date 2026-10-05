// The test bridge, Node side: a local MQTT broker the app publishes to (`--bridge-url`), a recorder of
// what it says, and a client for waiting on events and sending commands. Topics and payloads are the
// ones in crates/core/src/bridge.rs. Nothing here listens beyond 127.0.0.1.
import { createServer } from "node:net";
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import { randomUUID } from "node:crypto";
import { Aedes } from "aedes";
import mqtt from "mqtt";

const HOST = "127.0.0.1";
const APP_TOPICS = "rs/app/#";

/** Reads a recording written by [Bridge]: one `{topic, received_at, message}` per line. */
export function readRecording(file) {
  return readFileSync(file, "utf8")
    .split("\n")
    .filter((line) => line.trim())
    .map((line) => JSON.parse(line));
}

function parsePayload(buffer) {
  const text = buffer.toString();
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

/**
 * A message test from a JSON spec, so a script in another language can say what it waits for.
 * `{"payload.state": "downloaded"}`: every dotted path must equal the value; a path may instead hold
 * `{"contains": text}`, `{"regex": pattern}` (a bad pattern matches nothing) or `{"not": value}`. `undefined` / `{}` match anything;
 * a plain string (the `rs/app/status` text) is compared as it is.
 */
export function matcher(spec) {
  if (spec === undefined || spec === null) return () => true;
  if (typeof spec !== "object") return (message) => message === spec;
  const test = (want, got) => {
    if (want !== null && typeof want === "object" && !Array.isArray(want)) {
      if ("not" in want) return JSON.stringify(got) !== JSON.stringify(want.not);
      if ("contains" in want) return typeof got === "string" && got.includes(want.contains);
      if ("regex" in want) {
        try {
          return typeof got === "string" && new RegExp(want.regex).test(got);
        } catch {
          return false;
        }
      }
    }
    return JSON.stringify(got) === JSON.stringify(want);
  };
  const at = (message, path) => path.split(".").reduce((value, key) => (value === null || value === undefined ? undefined : value[key]), message);
  return (message) => Object.entries(spec).every(([path, want]) => test(want, at(message, path)));
}

export class Bridge {
  /** Starts the broker (on `port`, or a free one) and a client that records `rs/app/#`. */
  static async start({ port = 0, logFile = null } = {}) {
    const broker = await Aedes.createBroker();
    const server = createServer(broker.handle);
    await new Promise((resolve, reject) => {
      server.once("error", reject);
      server.listen(port, HOST, resolve);
    });
    const bridge = new Bridge(broker, server, server.address().port, logFile);
    if (logFile) writeFileSync(logFile, "");
    bridge.client = await mqtt.connectAsync(`mqtt://${HOST}:${bridge.port}`, { clientId: "bridge-recorder" });
    await bridge.client.subscribeAsync(APP_TOPICS);
    bridge.client.on("message", (topic, payload) => bridge.#record(topic, payload));
    return bridge;
  }

  host = HOST;
  messages = [];
  #waiters = new Set();

  constructor(broker, server, port, logFile) {
    this.broker = broker;
    this.server = server;
    this.port = port;
    this.logFile = logFile;
  }

  get url() {
    return `mqtt://${this.host}:${this.port}`;
  }

  #record(topic, payload) {
    const record = { topic, received_at: Date.now(), message: parsePayload(payload) };
    this.messages.push(record);
    if (this.logFile) appendFileSync(this.logFile, JSON.stringify(record) + "\n");
    for (const waiter of [...this.#waiters]) waiter(record);
  }

  /**
   * Resolves with the `message` of the first recorded message on `topic` for which `where(message)` is true,
   * looking at what was already recorded first. Rejects after `timeout` ms.
   */
  expect(topic, { where = () => true, timeout = 10000 } = {}) {
    const found = this.messages.find((r) => r.topic === topic && where(r.message));
    if (found) return Promise.resolve(found.message);
    return new Promise((resolve, reject) => {
      const waiter = (record) => {
        if (record.topic === topic && where(record.message)) {
          done();
          resolve(record.message);
        }
      };
      const timer = setTimeout(() => {
        done();
        reject(new Error(`timed out after ${timeout} ms waiting for ${topic}`));
      }, timeout);
      const done = () => {
        clearTimeout(timer);
        this.#waiters.delete(waiter);
      };
      this.#waiters.add(waiter);
    });
  }

  /**
   * Each step `{topic, where?}` must be seen after the one before it; other messages in between are fine.
   * Resolves with the matched messages, rejects naming the first step that never came.
   */
  async expectSequence(steps, { timeout = 10000 } = {}) {
    const deadline = Date.now() + timeout;
    const matched = [];
    let from = 0;
    for (const [index, step] of steps.entries()) {
      const where = step.where ?? (() => true);
      const hit = () => this.messages.findIndex((r, i) => i >= from && r.topic === step.topic && where(r.message));
      let at = hit();
      while (at < 0 && Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, 20));
        at = hit();
      }
      if (at < 0) throw new Error(`step ${index + 1} of ${steps.length} (${step.topic}) never came within ${timeout} ms`);
      matched.push(this.messages[at].message);
      from = at + 1;
    }
    return matched;
  }

  /** Publishes `rs/test/command/<name>` with a fresh id and resolves with the app's ack (rejects on not-ok or no ack). */
  async send(name, args = {}, { timeout = 10000 } = {}) {
    const id = randomUUID();
    const waiting = this.expect(`rs/app/ack/${id}`, { timeout }).catch((e) => {
      throw new Error(`no ack for ${name}: ${e.message}`);
    });
    waiting.catch(() => {}); // reported by the await below
    await this.client.publishAsync(`rs/test/command/${name}`, JSON.stringify({ ...args, id }), { qos: 1 });
    const ack = await waiting;
    if (!ack.ok) throw new Error(`${name} refused: ${ack.error}`);
    return ack;
  }

  async close() {
    await this.client.endAsync(true);
    await new Promise((resolve) => this.broker.close(resolve));
    await new Promise((resolve) => this.server.close(resolve));
  }
}
