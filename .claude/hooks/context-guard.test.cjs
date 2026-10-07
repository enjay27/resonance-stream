// Run: node --test hooks/context-guard.test.cjs
"use strict";

const test = require("node:test");
const assert = require("node:assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");
const g = require("./context-guard.cjs");

function entry(input, cacheRead, extra = {}) {
  return JSON.stringify({
    type: "assistant",
    isSidechain: false,
    message: {
      model: "claude-opus-5-5",
      usage: { input_tokens: input, cache_creation_input_tokens: 1000, cache_read_input_tokens: cacheRead, output_tokens: 500 },
    },
    ...extra,
  });
}

const user = JSON.stringify({ type: "user", message: { role: "user", content: "hi" } });

test("reads the last main-thread usage, skipping subagents and broken lines", () => {
  const text = [
    entry(10, 50000),
    user,
    entry(2, 120000),
    entry(5, 900000, { isSidechain: true }),
    '{"cut off',
    "",
  ].join("\n");
  assert.strictEqual(g.contextTokens(text), 2 + 1000 + 120000);
});

test("no usage yet means no answer", () => {
  assert.strictEqual(g.contextTokens(user + "\n"), null);
});

test("window size: configured, default, and 1M inferred", () => {
  assert.strictEqual(g.windowSize(50000, { CLAUDE_CONTEXT_WINDOW: "1000000" }), 1000000);
  assert.strictEqual(g.windowSize(150000, {}), 200000);
  assert.strictEqual(g.windowSize(469000, {}), 1000000);
});

test("levels", () => {
  assert.strictEqual(g.levelFor(59, 60, 80), 0);
  assert.strictEqual(g.levelFor(60, 60, 80), 1);
  assert.strictEqual(g.levelFor(80, 60, 80), 2);
});

const prompt = { hook_event_name: "UserPromptSubmit", session_id: "s1" };

test("quiet below the warning line", () => {
  assert.strictEqual(g.decide(prompt, entry(0, 100000), 0, {}), null);
});

test("warns once at 60%, then stays quiet at the same level", () => {
  const text = entry(0, 125000); // 126k of 200k = 63%
  const first = g.decide(prompt, text, 0, {});
  assert.strictEqual(first.level, 1);
  assert.match(first.output.hookSpecificOutput.additionalContext, /63% \(126k of 200k tokens\)/);
  assert.strictEqual(first.output.systemMessage, "Context 63% used");
  assert.strictEqual(g.decide(prompt, text, 1, {}), null);
});

test("recommends a handoff at 80% even after the 60% warning", () => {
  const out = g.decide(prompt, entry(0, 165000), 1, {});
  assert.strictEqual(out.level, 2);
  assert.match(out.output.hookSpecificOutput.additionalContext, /recommend a handoff/);
  assert.match(out.output.hookSpecificOutput.additionalContext, /session-handoff/);
  assert.strictEqual(out.output.systemMessage, "Context 83% used: handoff recommended");
});

test("re-arms silently when usage drops after a compaction", () => {
  const out = g.decide(prompt, entry(0, 30000), 2, {});
  assert.deepStrictEqual(out, { level: 0, output: null });
});

test("custom thresholds", () => {
  const out = g.decide(prompt, entry(0, 99000), 0, { CONTEXT_WARN_PCT: "40", CONTEXT_HANDOFF_PCT: "50" });
  assert.strictEqual(out.level, 2);
});

test("after compaction, SessionStart tells Claude to offer a handoff", () => {
  const out = g.decide({ hook_event_name: "SessionStart", source: "compact" }, "", 2, {});
  assert.strictEqual(out.level, 0);
  assert.match(out.output.hookSpecificOutput.additionalContext, /just compacted/);
  assert.strictEqual(g.decide({ hook_event_name: "SessionStart", source: "startup" }, "", 0, {}), null);
});

test("end to end through stdin, with state kept between prompts", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "cg-"));
  const transcript = path.join(dir, "t.jsonl");
  fs.writeFileSync(transcript, [user, entry(0, 125000)].join("\n") + "\n");
  const input = JSON.stringify({ ...prompt, session_id: "e2e", transcript_path: transcript, scratchpad_dir: dir });
  const run = () => execFileSync(process.execPath, [path.join(__dirname, "context-guard.cjs")], { input, encoding: "utf8" });

  const first = JSON.parse(run());
  assert.strictEqual(first.systemMessage, "Context 63% used");
  assert.strictEqual(run(), ""); // same level: quiet

  fs.appendFileSync(transcript, entry(0, 170000) + "\n");
  assert.match(JSON.parse(run()).systemMessage, /handoff recommended/);
});

test("bad input never throws or prints", () => {
  const out = execFileSync(process.execPath, [path.join(__dirname, "context-guard.cjs")], {
    input: "not json",
    encoding: "utf8",
  });
  assert.strictEqual(out, "");
  const missing = JSON.stringify({ ...prompt, transcript_path: "/nope/x.jsonl" });
  assert.strictEqual(
    execFileSync(process.execPath, [path.join(__dirname, "context-guard.cjs")], { input: missing, encoding: "utf8" }),
    "",
  );
});
