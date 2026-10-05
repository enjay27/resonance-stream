import test from "node:test";
import assert from "node:assert/strict";
import { parseSample, expectedText, verifyReplayChat } from "./scenarios.mjs";

const SAMPLE = [
  '# a comment',
  '{"delay_ms": 0, "channel": "WORLD", "nickname": "ミナト", "level": 60, "text": "こんにちは"}',
  '',
  '{"delay_ms": 1500, "channel": "PARTY", "nickname": "ゆう", "level": 60, "text": "回復お願いします！<sprite=3>"}',
  '{"delay_ms": 600, "channel": "WORLD", "nickname": "Kenji", "level": 60, "text": "emojiPic=9"}',
].join("\n");

// What the app publishes for those three lines, as the bridge records them.
function recording(overrides = {}) {
  const chat = (seq, t_ms, channel, nickname, message, level = 60) => ({
    topic: "rs/app/event/chat-message-update",
    message: { seq, t_ms, name: "chat-message-update", payload: { pid: seq, channel, nickname, message, level } },
  });
  const system = (seq, t_ms, message, level = "info") => ({
    topic: "rs/app/event/system-event",
    message: { seq, t_ms, name: "system-event", payload: { pid: 100 + seq, level, source: "Replay", message } },
  });
  return [
    system(0, 1000, "Replaying 3 chat lines from x.jsonl"),
    chat(1, 3000, "WORLD", "ミナト", "こんにちは"),
    chat(2, 4500, "PARTY", "ゆう", "回復お願いします！[이모지]"),
    chat(3, 5100, "WORLD", "Kenji", "[스티커]"),
    system(4, 5100, "Replay finished"),
  ].map((r, i) => overrides[i] ?? r);
}

const byId = (checks) => Object.fromEntries(checks.map((c) => [c.id, c]));

test("a sample file is JSON Lines; blank lines and # lines are skipped", () => {
  const entries = parseSample(SAMPLE);
  assert.equal(entries.length, 3);
  assert.equal(entries[1].channel, "PARTY");
  assert.throws(() => parseSample('{"nope": 1}'), /line 1: no "text"/);
  assert.throws(() => parseSample("not json"), /line 1: not JSON/);
});

test("the backend turns emotes into placeholders", () => {
  assert.equal(expectedText("hi<sprite=3>"), "hi[이모지]");
  assert.equal(expectedText("emojiPic=9"), "[스티커]");
  assert.equal(expectedText("plain"), "plain");
});

test("a faithful replay passes every check", () => {
  const checks = verifyReplayChat(recording(), parseSample(SAMPLE));
  assert.deepEqual(checks.filter((c) => !c.ok), []);
  assert.deepEqual(Object.keys(byId(checks)), ["B1", "B2", "B3", "B4", "B5"]);
});

test("a missing chat event fails the count", () => {
  const rec = recording();
  rec.splice(2, 1);
  const checks = byId(verifyReplayChat(rec, parseSample(SAMPLE)));
  assert.equal(checks.B1.ok, false);
  assert.match(checks.B1.detail, /2 of 3/);
});

test("a wrong channel, name, level or text fails the content check and says which line", () => {
  for (const [index, patch] of [
    [1, { channel: "GUILD" }],
    [1, { nickname: "x" }],
    [1, { level: 0 }],
    [1, { message: "こんばんは" }],
  ]) {
    const rec = recording();
    rec[index].message.payload = { ...rec[index].message.payload, ...patch };
    const checks = byId(verifyReplayChat(rec, parseSample(SAMPLE)));
    assert.equal(checks.B2.ok, false, JSON.stringify(patch));
    assert.match(checks.B2.detail, /line 1/);
  }
});

test("a translation update re-sending a chat message is not a second line", () => {
  const rec = recording();
  rec.splice(3, 0, { ...rec[1], message: { ...rec[1].message, seq: 9, payload: { ...rec[1].message.payload, translated: "안녕" } } });
  assert.deepEqual(verifyReplayChat(rec, parseSample(SAMPLE)).filter((c) => !c.ok), []);
});

test("no 'Replay finished' after the last line fails", () => {
  const rec = recording();
  rec.pop();
  assert.equal(byId(verifyReplayChat(rec, parseSample(SAMPLE))).B3.ok, false);
  const early = recording();
  early[4].message.seq = 2; // said before the last line
  assert.equal(byId(verifyReplayChat(early, parseSample(SAMPLE))).B3.ok, false);
});

test("a replay warning or error fails the quiet check", () => {
  const rec = recording();
  rec.push({ topic: "rs/app/event/system-event", message: { seq: 5, t_ms: 6000, name: "system-event", payload: { level: "error", source: "Replay", message: "Cannot read x" } } });
  const checks = byId(verifyReplayChat(rec, parseSample(SAMPLE)));
  assert.equal(checks.B4.ok, false);
  assert.match(checks.B4.detail, /Cannot read x/);
});

test("lines that all arrive at once fail the timing check", () => {
  const rec = recording();
  rec[2].message.t_ms = 3000;
  rec[3].message.t_ms = 3000;
  assert.equal(byId(verifyReplayChat(rec, parseSample(SAMPLE))).B5.ok, false);
});
