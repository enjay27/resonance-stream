---
paths:
  - "src/**"
---

## Conventions (ui crate)

- **State lives in `AppSignals` (context), not in component locals**, when more than one
  component touches it. Components read it with `use_context::<AppSignals>()` and reach a
  signal through its group: `signals.config` (settings), `.service`, `.setup`, `.updates`,
  `.chat`, `.ui`. A new signal goes in the group it describes.
- **A component under `<Show>` is re-created each time it shows.** State that must
  survive closing a modal (typed input, an `Action`'s last result) is created by the
  parent outside the `<Show>` and passed down as props — see `components/settings/`.
- Helpers that need many signals take `signals: AppSignals` and destructure only the
  fields they use (`let ChatSignals { a, set_b, .. } = signals.chat;`).
- **A setting that lives in `config.json` is a field of `AppConfig` (`crates/types`) and a
  signal of `signals.config` (`ConfigSignals`, named like the field).** Adding one means adding
  it to `AppConfig` (with its default and a value in `app_config_full.json`) and to
  `ConfigSignals` -- `to_config` / `apply` list every field, so forgetting is a compile
  error. Its load-time quirks (a saved value that is
  clamped or replaced) live in `apply`, not in `hydration.rs`.
