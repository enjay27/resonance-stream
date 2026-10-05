// Runs a whole scenario with no one at the keyboard: starts the broker, starts the app pointed at it, drives the app
// with commands over the bridge, checks what it published, and tells it to quit. Nothing here is a prompt.
import { spawn } from "node:child_process";
import { closeSync, mkdirSync, openSync, readFileSync, rmSync } from "node:fs";
import { join } from "node:path";
import { Bridge } from "./bridge.mjs";
import { parseSample, verifyReplayChat } from "./scenarios.mjs";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
/** src-tauri/src/services/sniffer/replay.rs LEAD_IN */
const LEAD_IN_MS = 2000;

/** The flags of an isolated run (runbook/runbook/mockfeed.py `flag_args`), without `--replay-chat`: the bridge starts it. */
export function appFlags({ workDir, statusFile, logFile, bridgeUrl }) {
  return [
    "--data-dir", join(workDir, "data"), "--fresh", "--assume-setup-done", "--no-capture", "--no-translator",
    "--no-update-check", "--no-popups", "--no-window-state",
    "--status-file", statusFile, "--log-file", logFile, "--bridge-url", bridgeUrl,
  ];
}

function readStatus(file) {
  try {
    return JSON.parse(readFileSync(file, "utf8"));
  } catch {
    return null;
  }
}

/**
 * `command` is `{file, args}`: the exe (or what stands in for it) and arguments that come before the flags.
 * Resolves with `{ok, checks, recording, workDir}`; never rejects for an app that misbehaves -- that is a failed check.
 */
export async function runReplayChat({
  command, sample, workDir, env = process.env, readyTimeout = 120000, finishSlack = 30000, quitTimeout = 15000, log = () => {},
}) {
  const entries = parseSample(readFileSync(sample, "utf8"));
  rmSync(workDir, { recursive: true, force: true });
  mkdirSync(workDir, { recursive: true });
  const recording = join(workDir, "events.jsonl");
  const statusFile = join(workDir, "status.json");
  const logFile = join(workDir, "app.log");
  const checks = [];
  const add = (id, title, ok, detail = "") => {
    checks.push({ id, title, ok, detail });
    log(`[${ok ? "PASS" : "FAIL"}] ${id} ${title}${detail ? ` -- ${detail}` : ""}`);
    return ok;
  };
  const finish = () => ({ ok: checks.every((c) => c.ok), checks, recording, workDir });

  const bridge = await Bridge.start({ logFile: recording });
  const out = openSync(join(workDir, "app.stdout.txt"), "w");
  const child = spawn(command.file, [...command.args, ...appFlags({ workDir, statusFile, logFile, bridgeUrl: bridge.url })], {
    stdio: ["ignore", out, out], env,
  });
  let exit = null;
  const exited = new Promise((resolve) => child.on("exit", (code, signal) => resolve((exit = { code, signal }))));
  child.on("error", (e) => (exit = { code: null, signal: null, error: e.message }));
  const stop = async () => {
    if (exit === null) child.kill();
    await Promise.race([exited, sleep(3000)]);
    closeSync(out);
    await bridge.close();
  };

  try {
    // A1: ready, as the status file says (the bridge connects a moment earlier).
    const deadline = Date.now() + readyTimeout;
    while (Date.now() < deadline && exit === null && !readStatus(statusFile)?.ready) await sleep(200);
    const ready = readStatus(statusFile)?.ready === true;
    const why = ready ? `pid ${readStatus(statusFile).pid}` : exit ? `the app exited with code ${exit.code ?? exit.error ?? exit.signal}` : `not ready after ${readyTimeout} ms`;
    if (!add("A1", "the app started and became ready", ready, why)) return finish();

    try {
      await bridge.expect("rs/app/status", { where: (m) => m === "online", timeout: 10000 });
      add("A2", "the app connected to the bridge", true, bridge.url);
    } catch (e) {
      add("A2", "the app connected to the bridge", false, e.message);
      return finish();
    }

    const command_ = async (id, title, name, args = {}) => {
      try {
        await bridge.send(name, args, { timeout: 10000 });
        return add(id, title, true);
      } catch (e) {
        return add(id, title, false, e.message);
      }
    };
    if (!(await command_("B0", "the app answers a command sent over the bridge", "ping"))) return finish();
    if (!(await command_("A3", "the app accepted replay-chat", "replay-chat", { path: sample }))) return finish();

    const total = LEAD_IN_MS + entries.reduce((sum, e) => sum + (e.delay_ms ?? 0), 0);
    try {
      await bridge.expect("rs/app/event/system-event", { where: (m) => m.payload?.message === "Replay finished", timeout: total + finishSlack });
    } catch {
      // B3 below says it was not there
    }
    for (const c of verifyReplayChat(bridge.messages, entries)) add(c.id, c.title, c.ok, c.detail);

    let quitDetail = "";
    try {
      await bridge.send("quit", {}, { timeout: 10000 });
    } catch (e) {
      quitDetail = e.message;
    }
    const quit = await Promise.race([exited.then(() => true), sleep(quitTimeout).then(() => false)]);
    add("Q1", "the app quit when told to", quit && !quitDetail, quit ? quitDetail : `still running ${quitTimeout} ms after quit`);
    return finish();
  } finally {
    await stop();
  }
}
