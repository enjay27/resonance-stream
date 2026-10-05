// What the app is expected to publish for a scenario, checked over a recording (see bridge.mjs). Pure.

/** The lines of a `--replay-chat` file (JSON Lines; blank and `#` lines skipped). */
export function parseSample(text) {
  const entries = [];
  text.split(/\r?\n/).forEach((raw, index) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    let entry;
    try {
      entry = JSON.parse(line);
    } catch (e) {
      throw new Error(`line ${index + 1}: not JSON (${e.message})`);
    }
    if (entry === null || typeof entry !== "object" || !("text" in entry)) throw new Error(`line ${index + 1}: no "text"`);
    entries.push(entry);
  });
  return entries;
}

/** The text the backend sends for a replayed line: emotes become placeholders (crates/core/src/capture). */
export function expectedText(text) {
  return text.replace(/<sprite=\d+>/g, "[이모지]").replace(/emojiPic=\d+/g, "[스티커]");
}

const events = (recording, name) =>
  recording.filter((r) => r.topic === `rs/app/event/${name}`).map((r) => r.message);

/** The B checks for `--replay-chat`: what the app *published*, not what the window showed. */
export function verifyReplayChat(recording, entries) {
  const checks = [];
  const add = (id, title, ok, detail = "") => checks.push({ id, title, ok, detail });

  // The app publishes a new chat line as `packet-event` (src-tauri/src/events.rs `store_and_emit`); `chat-message-update`
  // only re-sends a line whose blocked flag changed, so it is not read here. The same pid twice is one line.
  const seen = new Set();
  const chat = events(recording, "packet-event").filter((m) => {
    const key = m.payload.pid ?? `seq${m.seq}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  add("B1", `${entries.length} chat messages were published`, chat.length === entries.length, `${chat.length} of ${entries.length} packet-event messages`);

  const wrong = [];
  entries.forEach((entry, i) => {
    const got = chat[i]?.payload;
    if (!got) return wrong.push(`line ${i + 1}: missing`);
    const want = { channel: (entry.channel ?? "WORLD").toUpperCase(), nickname: entry.nickname ?? "", level: entry.level ?? 0, message: expectedText(entry.text) };
    for (const [field, value] of Object.entries(want)) {
      if (got[field] !== value) wrong.push(`line ${i + 1}: ${field} ${JSON.stringify(got[field])}, expected ${JSON.stringify(value)}`);
    }
  });
  add("B2", "each message has the right channel, sender, level and text, in order", wrong.length === 0, wrong.slice(0, 5).join("; "));

  const system = events(recording, "system-event");
  const finished = system.find((m) => m.payload.message === "Replay finished");
  const lastChat = chat.at(-1);
  add("B3", "'Replay finished' came after the last line", Boolean(finished) && Boolean(lastChat) && finished.seq > lastChat.seq,
    finished ? `seq ${finished.seq}, last line seq ${lastChat?.seq}` : "no such system-event");

  const problems = system.filter((m) => m.payload.source === "Replay" && ["warn", "error"].includes(m.payload.level));
  add("B4", "no replay warning or error", problems.length === 0, problems.map((m) => m.payload.message).join("; "));

  // The replay sleeps `delay_ms` before each line, so a line cannot come sooner than that after the one before it.
  const early = [];
  for (let i = 1; i < Math.min(chat.length, entries.length); i++) {
    const gap = chat[i].t_ms - chat[i - 1].t_ms;
    const need = (entries[i].delay_ms ?? 0) * 0.8 - 100;
    if (gap < need) early.push(`line ${i + 1}: ${gap} ms after the one before, wanted about ${entries[i].delay_ms} ms`);
  }
  add("B5", "the lines came one after another, at the pace of the file", early.length === 0, early.slice(0, 3).join("; "));
  return checks;
}
