#!/usr/bin/env node
// context-guard: warns when a Claude Code session's context is filling up, so Kade can choose
// a handoff and a fresh session before quality drops or auto-compaction rewrites the history.
//
// Hooks do not receive context usage directly, but they receive `transcript_path`. Every
// assistant entry in the transcript carries the API `usage` of that request; its input side
// (input + cache creation + cache read) is exactly what the status line reports as context used.
//
// Events:
//   UserPromptSubmit         at WARN% and again at HANDOFF%: tells Claude to stop and offer the
//                            choices (handoff + new session, /compact, continue), and shows Kade a
//                            one-line warning. Each level fires once until usage drops again.
//   SessionStart (compact)   after a compaction: tells Claude to say so and offer a handoff.
//
// Settings (environment):
//   CLAUDE_CONTEXT_WINDOW    window size in tokens. Default 200000; a session already past
//                            200000 tokens is treated as 1000000 (a 1M-context model).
//   CONTEXT_WARN_PCT         default 60
//   CONTEXT_HANDOFF_PCT      default 80
//
// The hook never blocks a prompt: on any error it prints nothing and exits 0.
// Source of truth: github.com/enjay27/claude-skills (hooks/). Repositories vendor a copy.

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");

const TAIL_BYTES = 2 * 1024 * 1024;

function readTail(file, bytes = TAIL_BYTES) {
  const stat = fs.statSync(file);
  const start = Math.max(0, stat.size - bytes);
  const fd = fs.openSync(file, "r");
  try {
    const buf = Buffer.alloc(stat.size - start);
    fs.readSync(fd, buf, 0, buf.length, start);
    return buf.toString("utf8");
  } finally {
    fs.closeSync(fd);
  }
}

// Tokens in context after the most recent main-thread request, or null if none yet.
function contextTokens(transcriptText) {
  const lines = transcriptText.split("\n");
  for (let i = lines.length - 1; i >= 0; i--) {
    const line = lines[i].trim();
    if (!line || line[0] !== "{") continue; // the first line of a tail may be cut
    let entry;
    try {
      entry = JSON.parse(line);
    } catch {
      continue;
    }
    if (entry.isSidechain) continue; // subagent requests have their own context
    const usage = entry && entry.message && entry.message.usage;
    if (!usage || typeof usage.input_tokens !== "number") continue;
    return (
      usage.input_tokens +
      (usage.cache_creation_input_tokens || 0) +
      (usage.cache_read_input_tokens || 0)
    );
  }
  return null;
}

function windowSize(tokens, env) {
  const configured = parseInt(env.CLAUDE_CONTEXT_WINDOW || "", 10);
  if (configured > 0) return configured;
  return tokens > 200000 ? 1000000 : 200000;
}

function levelFor(pct, warn, handoff) {
  if (pct >= handoff) return 2;
  if (pct >= warn) return 1;
  return 0;
}

function stateFile(sessionId, dir) {
  const safe = String(sessionId || "unknown").replace(/[^A-Za-z0-9_-]/g, "_");
  return path.join(dir || os.tmpdir(), `context-guard-${safe}.json`);
}

function readLevel(file) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8")).level || 0;
  } catch {
    return 0;
  }
}

function writeLevel(file, level) {
  try {
    fs.writeFileSync(file, JSON.stringify({ level }));
  } catch {
    /* a missing state file only means the warning may repeat */
  }
}

function fmt(n) {
  if (n >= 1000000 && n % 1000000 === 0) return `${n / 1000000}M`;
  return n >= 1000 ? `${Math.round(n / 1000)}k` : String(n);
}

const CHOICES =
  "Offer Kade three choices and let him pick: " +
  "(1) handoff and a new session (use the session-handoff skill), " +
  "(2) /compact with a focus he names, " +
  "(3) continue as is.";

// Pure decision: what to emit for one event. Returns {output, level} or null.
function decide(input, transcriptText, previousLevel, env) {
  const event = input.hook_event_name;

  if (event === "SessionStart") {
    if (input.source !== "compact") return null;
    return {
      level: 0,
      output: {
        hookSpecificOutput: {
          hookEventName: "SessionStart",
          additionalContext:
            "context-guard: this session was just compacted, so earlier detail is now a summary. " +
            "In your next reply, tell Kade this in one line before anything else. " +
            CHOICES,
        },
      },
    };
  }

  if (event !== "UserPromptSubmit") return null;
  const tokens = contextTokens(transcriptText);
  if (tokens === null) return null;

  const size = windowSize(tokens, env);
  const pct = Math.round((tokens / size) * 100);
  const warn = parseInt(env.CONTEXT_WARN_PCT || "60", 10);
  const handoff = parseInt(env.CONTEXT_HANDOFF_PCT || "80", 10);
  const level = levelFor(pct, warn, handoff);

  if (level <= previousLevel) {
    // re-arm after usage drops (e.g. after /compact), stay quiet otherwise
    return level < previousLevel ? { level, output: null } : null;
  }

  const used = `${pct}% (${fmt(tokens)} of ${fmt(size)} tokens)`;
  const context =
    level === 2
      ? `context-guard: the context window is ${used} full, past the ${handoff}% handoff line. ` +
        "Before doing anything for this prompt, tell Kade the number and recommend a handoff. " +
        "Do not start new multi-step work until he answers. " +
        CHOICES
      : `context-guard: the context window is ${used} full (warning line ${warn}%). ` +
        "Answer this prompt, then at the end add one line telling Kade the number and that a " +
        "handoff will be recommended at " +
        `${handoff}%. If this prompt starts a large new task, ask first. ` +
        CHOICES;

  return {
    level,
    output: {
      hookSpecificOutput: { hookEventName: "UserPromptSubmit", additionalContext: context },
      systemMessage:
        level === 2
          ? `Context ${pct}% used: handoff recommended`
          : `Context ${pct}% used`,
    },
  };
}

function main() {
  let raw = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk) => (raw += chunk));
  process.stdin.on("end", () => {
    try {
      const input = JSON.parse(raw || "{}");
      const file = stateFile(input.session_id, input.scratchpad_dir);
      const transcript =
        input.hook_event_name === "UserPromptSubmit" && input.transcript_path
          ? readTail(input.transcript_path)
          : "";
      const result = decide(input, transcript, readLevel(file), process.env);
      if (!result) return;
      writeLevel(file, result.level);
      if (result.output) process.stdout.write(JSON.stringify(result.output));
    } catch {
      /* never block a prompt because of this hook */
    }
  });
}

if (require.main === module) main();

module.exports = { contextTokens, windowSize, levelFor, decide, stateFile };
