# Japanese study view (furigana), 2026-10-02

Branch `claude/japanese-study-furigana-ayn1k0`. Kade's ask: a mode that shows each
Japanese message (not the nickname) with furigana, translation hidden until the pointer
is over the row, in normal and compact mode; switched from the title-bar badge.

## What was built
- **Core** `crates/core/src/furigana.rs`: `spans_from_tokens` (pure, tested without a
  dictionary) puts the katakana reading -- as hiragana -- over the kanji only; okurigana
  stays plain (走った -> 走:はし + った), kana between kanji are anchors (取り込む), a word
  that does not line up gets one reading over all of it. `Furigana` = lindera 6.2 +
  embedded IPADIC; a line the analyser does not read back as itself comes back plain.
  `annotate()` shares one analyser. ~4 us a line, ~1 ms to load.
- **Types**: `RubySpan {text, reading?}`, `TranslationView` (on / off / study, default on).
- **Settings / wire**: `translation_view` in both `AppConfig`s + `ConfigSignals`.
  `use_translation` stays the master switch (settings); the view is only what rows do
  with the translations. With the translator off, "on" reads as "off"
  (`translation_view::effective`); study still works (furigana needs no translator).
- **Command** `annotate_furigana(texts) -> Vec<Vec<RubySpan>>` (async, no blocking pool).
- **UI**: title-bar badge (번역 ON / 번역 OFF / 공부 모드) with a picker; `ChatRow` asks
  for a row's furigana once the study view is on and caches by line (`ruby_view::RubyCache`);
  emphasis keywords are marked over the spans (`ruby_view::mark`); the translation is
  `HOVER_ONLY` (`hidden group-hover:block`), in compact too.

## Decisions and wrong turns
- **Engine.** kakasi (already a core dep) was tried first: whole-string hiragana only, and
  wrong in context (今日は -> こんにちは, 22時 -> 22と). lindera/IPADIC got those right.
  Known IPADIC miss: 一人 -> イチ ニン (ひとり).
- **The dictionary is downloaded at build time** (`lindera-ipadic`'s build.rs,
  `https://Lindera.dev/mecab-ipadic-2.7.0-20250920.tar.gz`, md5-checked). The cloud
  session's network policy blocks Lindera.dev, so core tests here ran on SourceForge's
  mecab-ipadic 2.7.0-20070801 re-encoded to UTF-8 (iconv EUC-JP), put in
  `LINDERA_BUILD_DICTIONARY_CACHE_DIR/6.2.0-fmt2/` and a scratch copy of lindera-ipadic
  with its md5 changed, applied by `--config patch.crates-io...` (never committed;
  Cargo.lock restored after each run). **CI uses the official pack**: if a reading in
  `furigana.rs`'s dictionary tests differs there, that is why.
- **Cost.** The embedded dictionary is ~46 MB: a scratch release binary was 48.7 MB.
  Unmeasured: the real exe/installer growth, and Windows CI build time.
  A build now needs Lindera.dev reachable (or `LINDERA_BUILD_DICTIONARY_CACHE_DIR` warm);
  consider caching it in CI if that site is flaky.
- **Reading text leaked into selections.** `<rt>reading</rt>` ends up in
  `Selection.toString()` (even with `user-select: none` on a script-made range), and the
  dictionary draft is built from the selection. The reading is now drawn with CSS
  `rt::before { content: attr(data-reading) }`, so it is not text; checked with a script
  range and a real mouse drag.
- **Per-row IPC** rather than a `ruby` field on `ChatMessage`: no wire/persistence change,
  and toggling the view needs no backfill. Two rows with the same line both ask once
  before the cache fills (harmless).
- The picker closes on mouse-leave (the title bar's `backdrop-filter` makes a `fixed`
  backdrop cover only the bar), and the title bar got `relative z-30` so the dropdown
  paints over the chat.

## Verified / not
Core gate, ui gate (wasm check + 105 tests), app cross-check (windows-gnu, `--tests`):
green here. Screenshotted with `ui-preview` (mocked `annotate_furigana`): badge + picker,
study rows with emphasis keyword, hover (normal and compact), off, translator-off, clean
selection. **NOT run**: `cargo tauri dev` / the real command on Windows, app tests,
real-dictionary readings on the official pack.
