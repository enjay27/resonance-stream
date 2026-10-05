#!/usr/bin/env node
// Command line of the bridge, for the notebooks (which are Python):
//   node cli.mjs serve [--port N] --out events.jsonl
//       Starts the broker and records what the app publishes. Prints {"ready":true,"port":N} on stdout, then
//       reads one JSON command per stdin line and answers each with one line:
//         {"send":"replay-chat","args":{"path":"..."}}                      -> {"ack":{...}} | {"error":"..."}
//         {"expect":{"topic":"rs/app/event/x","match":{"payload.k":"v"},"timeout":5000}}   -> {"found":<message>} | {"error"}
//         {"expect_sequence":{"steps":[{"topic":..,"match":..},..],"timeout":5000}}        -> {"found":[<message>,..]} | {"error"}
//       (`match`: see matcher() in bridge.mjs). Stops when stdin closes or on SIGINT / SIGTERM.
//   node cli.mjs run replay-chat --exe APP.exe [--sample FILE] [--work DIR] [--exe-arg ARG]
//       The whole test with no one at the keyboard: starts the broker and the app, drives it with commands over the
//       bridge (ping, replay-chat, quit), checks what it published and prints the same JSON as `verify`.
//       Progress goes to stderr. Exit 0 when every check passes, 1 when one fails, 2 for a bad command line.
//   node cli.mjs verify replay-chat --log events.jsonl --sample sample.jsonl
//       Checks the recording; prints {"scenario","ok","checks":[{id,title,ok,detail}]}; exit 1 when a check fails.
import { createInterface } from "node:readline";
import { readFileSync } from "node:fs";
import { Bridge, matcher, readRecording } from "./bridge.mjs";
import { parseSample, verifyReplayChat } from "./scenarios.mjs";
import { runReplayChat } from "./runner.mjs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

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
      const command = JSON.parse(line);
      if (command.send) {
        say({ ack: await bridge.send(command.send, command.args ?? {}, { timeout: command.timeout ?? 10000 }) });
      } else if (command.expect) {
        const { topic, match, timeout } = command.expect;
        say({ found: await bridge.expect(topic, { where: matcher(match), timeout: timeout ?? 10000 }) });
      } else if (command.expect_sequence) {
        const { steps, timeout } = command.expect_sequence;
        const found = await bridge.expectSequence(steps.map((s) => ({ topic: s.topic, where: matcher(s.match) })), { timeout: timeout ?? 10000 });
        say({ found });
      } else {
        say({ error: "unknown command: expected send, expect or expect_sequence" });
      }
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

async function run(argv) {
  const [scenario, ...rest] = argv;
  if (scenario !== "replay-chat") throw new Error(`unknown scenario ${scenario}`);
  const opts = options(rest);
  if (!opts.exe) throw new Error("run replay-chat needs --exe <path of a test-env build>");
  const sample = opts.sample ?? fileURLToPath(new URL("../../crates/core/testdata/replay-sample.jsonl", import.meta.url));
  const result = await runReplayChat({
    command: { file: opts.exe, args: opts["exe-arg"] ? [opts["exe-arg"]] : [] },
    sample,
    workDir: opts.work ?? join(tmpdir(), "resonance-bridge-replay"),
    log: (line) => console.error(line),
  });
  console.log(JSON.stringify({ scenario, ...result }));
  process.exit(result.ok ? 0 : 1);
}

const [command, ...argv] = process.argv.slice(2);
try {
  if (command === "serve") await serve(argv);
  else if (command === "verify") verify(argv);
  else if (command === "run") await run(argv);
  else throw new Error("usage: cli.mjs serve --out FILE | run replay-chat --exe APP | verify replay-chat --log FILE --sample FILE");
} catch (e) {
  console.error(e.message);
  process.exit(2);
}
