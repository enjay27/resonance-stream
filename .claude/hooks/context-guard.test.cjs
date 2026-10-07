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

test("window size: configured, else 1M (the default window of current Claude models)", () => {
  assert.strictEqual(g.windowSize({ CLAUDE_CONTEXT_WINDOW: "200000" }), 200000);
  assert.strictEqual(g.windowSize({}), 1000000);
  assert.strictEqual(g.windowSize({ CLAUDE_CONTEXT_WINDOW: "junk" }), 1000000);
});

test("thresholds: 200k / 400k tokens, never later than 40% / 60% of the window", () => {
  assert.deepStrictEqual(g.thresholds(1000000, {}), { warn: 200000, handoff: 400000 });
  assert.deepStrictEqual(g.thresholds(200000, {}), { warn: 80000, handoff: 120000 });
  assert.deepStrictEqual(g.thresholds(500000, {}), { warn: 200000, handoff: 300000 });
  assert.deepStrictEqual(
    g.thresholds(1000000, { CONTEXT_WARN_TOKENS: "150000", CONTEXT_HANDOFF_TOKENS: "300000" }),
    { warn: 150000, handoff: 300000 },
  );
  assert.deepStrictEqual(
    g.thresholds(1000000, { CONTEXT_WARN_PCT: "10", CONTEXT_HANDOFF_PCT: "20" }),
    { warn: 100000, handoff: 200000 },
  );
});

test("levels, in tokens", () => {
  assert.strictEqual(g.levelFor(199999, 200000, 400000), 0);
  assert.strictEqual(g.levelFor(200000, 200000, 400000), 1);
  assert.strictEqual(g.levelFor(400000, 200000, 400000), 2);
});

const prompt = { hook_event_name: "UserPromptSubmit", session_id: "s1" };

test("quiet below 200k in a 1M session (the old 200k guess warned at 120k here)", () => {
  assert.strictEqual(g.decide(prompt, entry(0, 149000), 0, {}), null);
});

test("warns once at 200k, then stays quiet at the same level", () => {
  const text = entry(0, 209000); // 210k of 1M
  const first = g.decide(prompt, text, 0, {});
  assert.strictEqual(first.level, 1);
  assert.match(first.output.hookSpecificOutput.additionalContext, /210k tokens \(21% of 1M\)/);
  assert.match(first.output.hookSpecificOutput.additionalContext, /warning line 200k/);
  assert.match(first.output.hookSpecificOutput.additionalContext, /recommended at 400k/);
  assert.strictEqual(first.output.systemMessage, "Context 210k tokens (21%) used");
  assert.strictEqual(g.decide(prompt, text, 1, {}), null);
});

test("recommends a handoff at 400k even after the warning", () => {
  const out = g.decide(prompt, entry(0, 409000), 1, {});
  assert.strictEqual(out.level, 2);
  assert.match(out.output.hookSpecificOutput.additionalContext, /past the 400k handoff line/);
  assert.match(out.output.hookSpecificOutput.additionalContext, /recommend a handoff/);
  assert.match(out.output.hookSpecificOutput.additionalContext, /session-handoff/);
  assert.strictEqual(out.output.systemMessage, "Context 410k tokens (41%) used: handoff recommended");
});

test("a 200k window warns at 40% and recommends the handoff at 60%", () => {
  const env = { CLAUDE_CONTEXT_WINDOW: "200000" };
  assert.strictEqual(g.decide(prompt, entry(0, 78000), 0, env), null); // 79k: just under 80k
  const warn = g.decide(prompt, entry(0, 89000), 0, env);
  assert.strictEqual(warn.level, 1);
  assert.match(warn.output.hookSpecificOutput.additionalContext, /90k tokens \(45% of 200k\)/);
  assert.strictEqual(g.decide(prompt, entry(0, 124000), 1, env).level, 2);
});

test("re-arms silently when usage drops after a compaction", () => {
  const out = g.decide(prompt, entry(0, 30000), 2, {});
  assert.deepStrictEqual(out, { level: 0, output: null });
});

test("custom thresholds", () => {
  const out = g.decide(prompt, entry(0, 99000), 0, { CONTEXT_WARN_TOKENS: "50000", CONTEXT_HANDOFF_TOKENS: "90000" });
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
  fs.writeFileSync(transcript, [user, entry(0, 209000)].join("\n") + "\n");
  const input = JSON.stringify({ ...prompt, session_id: "e2e", transcript_path: transcript, scratchpad_dir: dir });
  const run = () => execFileSync(process.execPath, [path.join(__dirname, "context-guard.cjs")], { input, encoding: "utf8" });

  const first = JSON.parse(run());
  assert.strictEqual(first.systemMessage, "Context 210k tokens (21%) used");
  assert.strictEqual(run(), ""); // same level: quiet

  fs.appendFileSync(transcript, entry(0, 450000) + "\n");
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
