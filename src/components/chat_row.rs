use crate::store::AppSignals;
use crate::types::ChatMessage;
use crate::use_context;
use crate::utils::{copy_to_clipboard, format_time, is_japanese};
use leptos::prelude::*;
use leptos::{component, view, IntoView};
use leptos::portal::Portal;

#[component]
pub fn ChatRow(sig: RwSignal<ChatMessage>) -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");

    // Memoize state for performance on your high-end desktop
    let is_active = Memo::new(move |_| signals.active_menu_id.get() == Some(sig.get_untracked().pid));

    let (menu_pos, set_menu_pos) = signal((0, 0));

    view! {
        // chat-start alignment matches your previous bubble flow
        <div class="chat chat-start px-2 py-1 group transition-colors hover:bg-white/5">

            // --- HEADER: Nickname(Romaji) + Level + Time ---
            <div class="chat-header opacity-90 mb-1 flex gap-2 items-baseline">
                <span class="text-[13px] font-black cursor-pointer hover:underline transition-all"
                    class=("text-bpsr-green", move || signals.search_term.get() == sig.get().nickname)
                    on:click=move |ev| {
                        ev.stop_propagation();
                        if is_active.get() {
                            signals.set_active_menu_id.set(None);
                        } else {
                            // Save the exact screen coordinates of the click
                            set_menu_pos.set((ev.client_x(), ev.client_y()));
                            signals.set_active_menu_id.set(Some(sig.get_untracked().pid));
                        }
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

                <Show when=move || is_active.get()>
                    <Portal>
                        // FIXED positioning based on the captured mouse coordinates
                        <div class="fixed z-50 bg-base-300 border border-white/10 rounded-lg shadow-2xl p-1 flex flex-col min-w-[130px] animate-in fade-in zoom-in-95 duration-100"
                             style=move || {
                                 let (x, y) = menu_pos.get();
                                 // Add a slight 8px offset so the menu doesn't spawn exactly under the cursor
                                 format!("top: {}px; left: {}px;", y + 8, x + 8)
                             }
                             on:click=move |ev| ev.stop_propagation()>

                            <button class="btn btn-ghost btn-sm justify-start text-xs font-normal h-8 min-h-0 px-2"
                                on:click=move |_| {
                                    copy_to_clipboard(&sig.get_untracked().nickname);
                                    signals.set_active_menu_id.set(None);
                                }>
                                "📋 Copy Name"
                            </button>

                            <button class="btn btn-ghost btn-sm justify-start text-xs font-normal h-8 min-h-0 px-2"
                                on:click=move |_| {
                                    let n = sig.get_untracked().nickname;
                                    if signals.search_term.get_untracked() == n {
                                        signals.set_search_term.set("".into());
                                    } else {
                                        signals.set_search_term.set(n);
                                    }
                                    signals.set_active_menu_id.set(None);
                                }>
                                "🔍 Filter Chat"
                            </button>
                        </div>
                    </Portal>
                </Show>

                <span class="text-[10px] text-gray-500 font-bold">"Lv." {move || sig.get().level}</span>
                <time class="ml-2 text-[10px] text-gray-600 opacity-50">{move || format_time(sig.get().timestamp)}</time>
            </div>

            // --- BUBBLE: Styled with DaisyUI + Custom BPSR Accents ---
            <div class="chat-bubble bg-zinc-900/80 border border-white/5 text-neutral-content min-h-0 shadow-lg transition-all">
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