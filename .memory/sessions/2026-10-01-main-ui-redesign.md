# Main UI redesign: A (normal) + CB (compact), 2026-10-01

Branch `candidate/main-ui-a-cb`: pushed, **no PR**. Kade runs it on Windows first and
then says whether it becomes a PR.

## How it was chosen
Candidates were built as real Leptos variants in a scratch worktree (local branches
`candidate/{a,b,c,ca,cb,cc}`, never pushed) and screenshotted with `ui-preview`, with
mock chat injected through `page.route('**/mock.js')`. Kade chose **A** for normal mode
(header line, translation first, original smaller below; line icons; colour-dot tabs;
Korean status pills) and **CB** for compact (game-subtitle captions; the bar shows on
hover only). Asked during the task: normal mode also gets the text box automatically
when `overlay_opacity < 0.5` ("auto backing").

## Readability guarantee
`src/readability.rs`: the text box is `bg-black/70`. Tests check the worst case, pure
white behind the box, with the WCAG 2 formula: main text >= 7:1, original
(`text-white/75`) and every channel name (Tailwind -300 shades, hex values taken from
v3) >= 4.5:1. The class strings are tied to the tested alphas by a test.
Screenshots: compact at 0 % opacity over white, normal at 30 % over a bright gradient.

Bars (Kade: "add it as your recommendation"): below the same 0.5 opacity the title
bar and tab bar turn near solid in the theme's own colour (`bg-base-300/95`,
`title_bar_bg` / `nav_bar_bg`), so the theme's text keeps its designed contrast;
at the default opacity they look as before. The compact hover bar was 95 % already.

## Other pure pieces (tested)
- `status_view`: sniffer/translator state -> tone + Korean label.
- `Tab::dot_class` replaced `Tab::icon`/`Tab::colors` (removed with their test lines:
  tabs show dots now, not emoji).
- `compact_original_class(hide, has_translation)`: display classes only; the
  one-display-class guard is kept.
- `translation_pending`: shows a "..." on a Japanese line waiting for its translation.

## Open
- Not run on Windows (`cargo tauri dev`): transparency, drag regions (compact bar is
  `absolute` and invisible until hover), the always-on-top pin, real timestamps.
- `format_time` multiplies by 1000: the mock's millisecond timestamps show odd times;
  real ones are seconds (not a change of this branch).
