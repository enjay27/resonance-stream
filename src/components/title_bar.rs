use crate::chat_view::Tab;
use crate::components::icons::{self, icon};
use crate::readability::title_bar_bg;
use crate::status_view::{sniffer_status, translator_status};
use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use crate::translation_view::{effective, hint, label, pill_class};
use crate::ui_types::{SnifferState, TranslationView, TranslatorState};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::IntoView;
use wasm_bindgen::JsValue;

const PILL: &str =
    "flex items-center gap-1.5 h-5 px-2 rounded-full text-[10px] font-semibold border transition-colors";

#[component]
pub fn TitleBar() -> impl IntoView {
    let store = use_context::<AppSignals>().expect("Store missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");
    let sniffer = move || sniffer_status(store.service.sniffer_state.get());
    let translator = move || translator_status(store.service.translator_state.get());
    // The translation badge: what rows show of the translation (on / off / study).
    let view_now = move || {
        effective(
            store.config.translation_view.get(),
            store.config.use_translation.get(),
        )
    };
    let (picker_open, set_picker_open) = signal(false);

    view! {
        <div class=move || format!("relative z-[60] flex items-center h-8 pl-3 backdrop-blur-md border-b border-base-content/5 select-none transition-colors {}", title_bar_bg(store.config.overlay_opacity.get())) data-tauri-drag-region>
            // --- LEFT: app name, version, start-up status ---
            <div class="flex items-baseline gap-2 min-w-0 flex-1 pointer-events-none">
                <span class="text-[11px] font-bold text-base-content/80 truncate">"Resonance Stream"</span>
                <span class="text-[10px] text-base-content/40">{concat!("v", env!("CARGO_PKG_VERSION"))}</span>
                <span class="text-[10px] text-base-content/50 truncate hidden min-[520px]:inline">
                    "· " {move || store.setup.status_text.get()}
                </span>
            </div>

            // --- RIGHT: service pills ---
            <div class="flex items-center gap-1.5 no-drag">
                <div class=move || format!("{PILL} {}", sniffer().0.pill_class())
                    title="패킷 캡처 상태 (꺼짐·오류일 때 클릭하면 진단)"
                    on:click=move |_| {
                        // Open the troubleshooter when capture is broken or off
                        if matches!(store.service.sniffer_state.get(), SnifferState::Error | SnifferState::Off) {
                            store.ui.set_show_troubleshooter.set(true);
                        }
                    }>
                    {icon(icons::RADIO, "size-3")}
                    <span>{move || sniffer().1}</span>
                    <span class=move || format!("size-1.5 rounded-full {}", sniffer().0.dot_class())></span>
                </div>
                <Show when=move || store.config.init_done.get()>
                    <div class="relative" on:mouseleave=move |_| set_picker_open.set(false)>
                        <button class=move || format!("{PILL} {}", pill_class(view_now()))
                            title="번역 표시 방식 (번역 ON / 번역 OFF / 공부 모드)"
                            on:click=move |_| set_picker_open.update(|open| *open = !*open)>
                            <span>{move || label(view_now())}</span>
                        </button>
                        <Show when=move || picker_open.get()>
                            <div class="absolute right-0 top-5 pt-1 z-50">
                                <div class="w-60 bg-base-300 border border-white/10 rounded-lg shadow-2xl p-1 flex flex-col">
                                    {TranslationView::ALL.iter().copied().map(|choice| {
                                        // "On" needs the translator; it is switched on in settings.
                                        let unavailable = move || {
                                            choice == TranslationView::On
                                                && !store.config.use_translation.get()
                                        };
                                        view! {
                                            <button
                                                class=move || format!("btn btn-ghost btn-sm justify-start flex-col items-start gap-0 h-auto min-h-0 py-1.5 px-2 font-normal {}",
                                                    if view_now() == choice { "bg-base-content/10" } else { "" })
                                                disabled=unavailable
                                                on:click=move |_| {
                                                    store.config.set_translation_view.set(choice);
                                                    actions.save_config.dispatch(());
                                                    set_picker_open.set(false);
                                                }>
                                                <span class="text-xs font-semibold">{label(choice)}</span>
                                                <span class="text-[10px] text-base-content/50">
                                                    {move || if unavailable() { "설정에서 번역을 켜면 쓸 수 있어요" } else { hint(choice) }}
                                                </span>
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                            </div>
                        </Show>
                    </div>
                </Show>
                <Show when=move || store.config.use_translation.get()>
                    <div class=move || format!("{PILL} {}", translator().0.pill_class())
                        title="번역 엔진 상태 (오류일 때 클릭하면 내용 표시)"
                        on:click=move |_| {
                            if store.service.translator_state.get() == TranslatorState::Error {
                                if let Some(w) = web_sys::window() {
                                    let _ = w.alert_with_message(&store.service.translator_error.get());
                                }
                            }
                        }>
                        {icon(icons::LANGUAGES, "size-3")}
                        <span>{move || translator().1}</span>
                        <span class=move || format!("size-1.5 rounded-full {}", translator().0.dot_class())></span>
                    </div>
                </Show>
            </div>

            // --- Window controls ---
            <div class="flex h-8 ml-2 no-drag">
                // Class and dungeon names (JP / KO), right next to minimize.
                <button class="w-10 h-full grid place-items-center text-base-content/60 hover:bg-base-content/10 hover:text-base-content transition-colors" title="직업 · 던전 이름 (일본어 / 한국어)"
                    on:click=move |_| store.ui.set_show_cheatsheet.set(true)>
                    {icon(icons::BOOK, "size-3.5")}
                </button>
                <button class="w-10 h-full grid place-items-center text-base-content/60 hover:bg-base-content/10 hover:text-base-content transition-colors" title="최소화"
                    on:click=move |_| { spawn_local(async { let _ = invoke("minimize_window", JsValue::NULL).await; }); }>
                    {icon(icons::MINUS, "size-3.5")}
                </button>
                // Compact mode sits next to close: easy to hit.
                <Show when=move || store.config.init_done.get()>
                    <button class="w-10 h-full grid place-items-center text-base-content/60 hover:bg-base-content/10 hover:text-base-content transition-colors" title="컴팩트 모드"
                        on:click=move |_| {
                            store.config.set_compact_mode.set(true);
                            if store.config.active_tab.get_untracked() != Tab::System.label() {
                                store.config.set_active_tab.set(Tab::Custom.label().to_string());
                            }
                            actions.save_config.dispatch(());
                        }>
                        {icon(icons::SHRINK, "size-3.5")}
                    </button>
                </Show>
                <button class="w-10 h-full grid place-items-center text-base-content/60 hover:bg-error hover:text-error-content transition-colors" title="닫기"
                    on:click=move |_| { spawn_local(async { let _ = invoke("close_window", JsValue::NULL).await; }); }>
                    {icon(icons::CLOSE, "size-3.5")}
                </button>
            </div>
        </div>
    }
}
