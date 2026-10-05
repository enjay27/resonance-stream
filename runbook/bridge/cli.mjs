#!/usr/bin/env node
// Command line of the bridge, for the notebooks (which are Python):
//   node cli.mjs serve [--port N] --out events.jsonl
//       Starts the broker and records what the app publishes. Prints {"ready":true,"port":N} on stdout, then
//       reads one JSON command per stdin line: {"send":"replay-chat","args":{"path":"..."}} and answers
//       {"ack":{...}} or {"error":"..."}. Stops when stdin closes or on SIGINT / SIGTERM.
//   node cli.mjs verify replay-chat --log events.jsonl --sample sample.jsonl
//       Checks the recording; prints {"scenario","ok","checks":[{id,title,ok,detail}]}; exit 1 when a check fails.
import { createInterface } from "node:readline";
import { readFileSync } from "node:fs";
import { Bridge, readRecording } from "./bridge.mjs";
import { parseSample, verifyReplayChat } from "./scenarios.mjs";

function options(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i += 2) {
    if (!argv[i].startsWith("--") || argv[i + 1] === undefined) throw new Error(`bad argument ${argv[i]}`);
    out[argv[i].slice(2)] = argv[i + 1];
  }
  return out;
}

async function serve(argv) {
  const opts = options(argv);
  if (!opts.out) throw new Error("serve needs --out <file>");
  const bridge = await Bridge.start({ port: Number(opts.port ?? 0), logFile: opts.out });
  const say = (value) => process.stdout.write(JSON.stringify(value) + "\n");
  say({ ready: true, port: bridge.port });
  const stop = async () => {
    await bridge.close();
    process.exit(0);
  };
  process.on("SIGINT", stop);
  process.on("SIGTERM", stop);
  const lines = createInterface({ input: process.stdin });
  lines.on("line", async (line) => {
    if (!line.trim()) return;
    try {
      const { send, args, timeout } = JSON.parse(line);
      say({ ack: await bridge.send(send, args ?? {}, { timeout: timeout ?? 10000 }) });
    } catch (e) {
      say({ error: e.message });
    }
  });
  lines.on("close", stop);
}

function verify(argv) {
  const [scenario, ...rest] = argv;
  if (scenario !== "replay-chat") throw new Error(`unknown scenario ${scenario}`);
  const opts = options(rest);
  if (!opts.log || !opts.sample) throw new Error("verify replay-chat needs --log and --sample");
  const checks = verifyReplayChat(readRecording(opts.log), parseSample(readFileSync(opts.sample, "utf8")));
  const ok = checks.every((c) => c.ok);
  console.log(JSON.stringify({ scenario, ok, checks }));
  process.exit(ok ? 0 : 1);
}

const [command, ...argv] = process.argv.slice(2);
try {
  if (command === "serve") await serve(argv);
  else if (command === "verify") verify(argv);
  else throw new Error("usage: cli.mjs serve --out FILE | verify replay-chat --log FILE --sample FILE");
} catch (e) {
  console.error(e.message);
  process.exit(2);
}
