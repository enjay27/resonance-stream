// A stand-in for a test-env build of Resonance Stream, ONLY for runner.test.mjs: it reads the same flags, writes the
// status file, connects to the bridge broker and answers the commands the way src-tauri/src/bridge.rs does. It is this
// author's reading of src-tauri -- it proves the runner, nothing about the real app.
//   FAKE_APP_LEAD_IN_MS   wait before the first replayed line (default 50; the real app waits 2000)
//   FAKE_APP_DROP_CHAT=1  a bug to catch: replays without publishing the chat lines
//   FAKE_APP_IGNORE_QUIT=1  a bug to catch: acks quit but keeps running
import { readFileSync, writeFileSync } from "node:fs";
import mqtt from "mqtt";

const flags = {};
const argv = process.argv.slice(2);
for (let i = 0; i < argv.length; i++) {
  if (!argv[i].startsWith("--")) continue;
  const name = argv[i].slice(2);
  flags[name] = argv[i + 1] && !argv[i + 1].startsWith("--") ? argv[++i] : true;
}
const broker = new URL(flags["bridge-url"]);
const client = await mqtt.connectAsync(`mqtt://${broker.hostname}:${broker.port}`, {
  will: { topic: "rs/app/status", payload: "offline", retain: true, qos: 1 },
});
let seq = 0;
const emit = (name, payload) =>
  client.publishAsync(`rs/app/event/${name}`, JSON.stringify({ seq: seq++, t_ms: Date.now(), name, payload }), { qos: 1 });
const system = (message, level = "info") => emit("system-event", { pid: seq, level, source: "Replay", message });
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const emote = (t) => t.replace(/<sprite=\d+>/g, "[이모지]").replace(/emojiPic=\d+/g, "[스티커]");

async function replay(path) {
  const entries = readFileSync(path, "utf8").split(/\r?\n/).filter((l) => l.trim() && !l.startsWith("#")).map((l) => JSON.parse(l));
  await system(`Replaying ${entries.length} chat lines from ${path}`);
  await sleep(Number(process.env.FAKE_APP_LEAD_IN_MS ?? 50));
  for (const [i, e] of entries.entries()) {
    await sleep(e.delay_ms ?? 0);
    if (!process.env.FAKE_APP_DROP_CHAT) {
      await emit("packet-event", { pid: i + 1, channel: e.channel ?? "WORLD", nickname: e.nickname ?? "", level: e.level ?? 0, message: emote(e.text) });
    }
  }
  await system("Replay finished");
}

await client.subscribeAsync("rs/test/command/+");
client.on("message", async (topic, payload) => {
  const { id, ...args } = JSON.parse(payload.toString());
  const ack = (ok = true, error) => client.publishAsync(`rs/app/ack/${id}`, JSON.stringify(ok ? { id, ok } : { id, ok, error }), { qos: 1 });
  switch (topic.split("/").at(-1)) {
    case "ping":
      return ack();
    case "replay-chat":
      await ack();
      return void replay(args.path);
    case "quit":
      await ack();
      await sleep(50);
      if (!process.env.FAKE_APP_IGNORE_QUIT) process.exit(0);
      return;
    default:
      return ack(false, `unknown command ${topic}`);
  }
});

await client.publishAsync("rs/app/status", "online", { retain: true, qos: 1 });
if (flags["status-file"]) writeFileSync(flags["status-file"], JSON.stringify({ ready: true, pid: process.pid, flags: Object.keys(flags) }));
