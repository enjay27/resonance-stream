use crate::store::AppSignals;
use crate::types::ChatMessage;
use crate::use_context;
use crate::utils::{copy_to_clipboard, format_time, is_japanese};
use leptos::prelude::*;
use leptos::{component, view, IntoView};

#[component]
pub fn ChatRow(sig: RwSignal<ChatMessage>) -> impl IntoView {
    let store = use_context::<AppSignals>().expect("Store missing");

    // Memoize state for performance on your high-end desktop
    let is_active = Memo::new(move |_| store.active_menu_id.get() == Some(sig.get_untracked().pid));

    view! {
        // chat-start alignment matches your previous bubble flow
        <div class="chat chat-start px-2 py-1 group transition-colors hover:bg-white/5">

            // --- HEADER: Nickname(Romaji) + Level + Time ---
            <div class="chat-header opacity-90 mb-1 flex gap-2 items-baseline">
                <span class="text-[13px] font-black cursor-pointer hover:underline transition-all"
                    class=("text-bpsr-green", move || store.search_term.get() == sig.get().nickname)
                    on:click=move |ev| {
                        ev.stop_propagation();
                        store.set_active_menu_id.set(if is_active.get() { None } else { Some(sig.get_untracked().pid) });
                    }
                >
                    {move || {
                        let p = sig.get();
                        match p.nickname_romaji {
                            Some(r) => format!("{}({})", p.nickname, r),
                            None => p.nickname.clone()
                        }
                    }}
                </span>

                <span class="text-[10px] text-gray-500 font-bold">"Lv." {move || sig.get().level}</span>
                <time class="ml-2 text-[10px] text-gray-600 opacity-50">{move || format_time(sig.get().timestamp)}</time>
            </div>

            // --- BUBBLE: Styled with DaisyUI + Custom BPSR Accents ---
            <div class=move || format!(
                "chat-bubble bg-zinc-900/80 border text-neutral-content min-h-0 shadow-lg transition-all {}",
                if is_active.get() { "border-bpsr-green ring-1 ring-bpsr-green/30" } else { "border-white/5" }
            )>
                // Original Message with [원문] tag
                <div class="text-[14px] leading-relaxed">
                    {move || if is_japanese(&sig.get().message) {
                        view! { <span class="text-gray-500 mr-1.5 font-bold">[원문]</span> }.into_any()
                    } else {
                        view! {}.into_any()
                    }}
                    {move || sig.get().message.clone()}
                </div>

                // Translation Result with [번역] tag
                {move || sig.get().translated.clone().map(|text| view! {
                    <div class="mt-1.5 pt-1.5 border-t border-white/10 text-bpsr-green font-bold text-[14px] animate-in slide-in-from-top-1 duration-200">
                         <span class="opacity-70 mr-1.5 font-bold">[번역]</span>
                         {text}
                    </div>
                })}
            </div>

            // --- ACTION BAR: Visible on hover for copy/search ---
            <div class="chat-footer opacity-0 group-hover:opacity-100 transition-opacity pt-1">
                <button class="btn btn-ghost btn-xs text-[10px] text-gray-500"
                    on:click=move |_| copy_to_clipboard(&sig.get_untracked().message)>
                    "COPY"
                </button>
            </div>
        </div>
    }
}