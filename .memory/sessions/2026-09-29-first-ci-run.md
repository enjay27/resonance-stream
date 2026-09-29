# 2026-09-29 — first CI run

Run 36549688320 on cd84924. Linux (fmt, core 31/31, ui) green in ~1 min. Windows
built and linked `src-tauri` in 2m34s (cold) with no Npcap SDK — the phase-1
assumption held — then `test_full_translator_flow_with_mock` failed:
`"[AI Server Connection Error]" != "116 정찰 우측 은나포"`.

**Cause: the test was stale, not the refactor.** `translate_text` (`translator/core.rs`,
untouched by the refactor) has posted to llama.cpp's native `POST /completion` and read
`{"content": ...}` since aa06b82 (2026-03-08, "apply translate-gemma prompt"). The mock
still served only the OpenAI-style `POST /v1/chat/completions` with a `choices` body, so
it never answered, and the client reported a connection error. The test had never run
anywhere before CI existed.

**Fix:** the mock serves `POST /completion` with `{"content": " ... "}` (padded, so the
`trim()` in `translate_text` is exercised too). Verified on Linux by compiling `core.rs` +
the test into a scratch crate: old mock reproduces the exact CI assertion, new mock passes.
