#!/usr/bin/env bash
# Builds the ui crate to wasm and serves it on http://127.0.0.1:8765 with a
# mocked Tauri bridge (mock.js), so a headless browser can drive and
# screenshot the real UI on Linux. See SKILL.md.
#   usage: preview.sh [out_dir]      (default: $TMPDIR/ui-preview)
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
HERE=$(cd "$(dirname "$0")" && pwd)
OUT=${1:-${TMPDIR:-/tmp}/ui-preview}
WEB="$OUT/web"
mkdir -p "$OUT/bin" "$WEB"
cd "$ROOT"

# 1. Tools: wasm target, wasm-bindgen-cli pinned to Cargo.lock, tailwind + daisyUI.
rustup target add wasm32-unknown-unknown >/dev/null 2>&1
WBV=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"/\1/p')
BINDGEN="$OUT/bin/bin/wasm-bindgen"
if ! { [ -x "$BINDGEN" ] && "$BINDGEN" --version | grep -q "$WBV"; }; then
    cargo install -q wasm-bindgen-cli --version "$WBV" --locked --root "$OUT/bin"
fi
[ -d node_modules/tailwindcss ] || npm ci --silent

# 2. A config.json the ui accepts: serialize its own AppConfig with a
#    throwaway test (so new fields never go stale here). ui_types.rs is
#    restored from a backup whatever happens.
cp src/ui_types.rs "$OUT/ui_types.rs.bak"
trap 'cp "$OUT/ui_types.rs.bak" src/ui_types.rs' EXIT
cat >> src/ui_types.rs <<'RS'
#[test]
fn zz_ui_preview_config() {
    let c = AppConfig { init_done: true, font_size: 14, message_spacing: 4, alert_volume: 0.5,
        overlay_opacity: 0.9, active_tab: ALL_TAB.into(), ..Default::default() };
    println!("UI_PREVIEW_CFG={}", serde_json::to_string(&c).unwrap());
}
RS
CFG=$(cargo test -q -p resonance-stream-ui zz_ui_preview_config -- --nocapture 2>/dev/null \
      | sed -n 's/^UI_PREVIEW_CFG=//p')
cp "$OUT/ui_types.rs.bak" src/ui_types.rs
trap - EXIT
[ -n "$CFG" ] || { echo "could not generate the mock config" >&2; exit 1; }

# 3. Build: wasm + JS glue, CSS, static assets, page.
cargo build -q -p resonance-stream-ui --target wasm32-unknown-unknown
"$BINDGEN" --target web --no-typescript --out-dir "$WEB" \
    target/wasm32-unknown-unknown/debug/resonance-stream-ui.wasm
npx @tailwindcss/cli -i style/input.css -o "$WEB/output.css" 2>/dev/null
cp -r public/. "$WEB/"
echo "window.__MOCK_CONFIG__ = $CFG;" > "$WEB/config.js"
cp "$HERE/mock.js" "$WEB/mock.js"
cat > "$WEB/index.html" <<'HTML'
<!doctype html><html><head><meta charset="utf-8"><link rel="stylesheet" href="output.css">
<script src="config.js"></script><script src="mock.js"></script></head><body>
<script type="module">import init from './resonance-stream-ui.js'; init();</script>
</body></html>
HTML

# 4. Serve (restart if already running).
pkill -f "http-server $WEB" 2>/dev/null || true
(npx -y http-server "$WEB" -p 8765 -s -c-1 >/dev/null 2>&1 &)
sleep 2
echo "serving $WEB on http://127.0.0.1:8765"
