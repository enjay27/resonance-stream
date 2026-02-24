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

    let is_active = Memo::new(move |_| signals.active_menu_id.get() == Some(sig.get_untracked().pid));
    let (menu_pos, set_menu_pos) = signal((0, 0));

    let channel_colors = move || match sig.get().channel.as_str() {
        "WORLD" => ("text-purple-500", "border-l-purple-500"),
        "GUILD" => ("text-emerald-500", "border-l-emerald-500"),
        "PARTY" => ("text-sky-500", "border-l-sky-500"),
        "LOCAL" => ("text-base-content/70", "border-l-base-content/50"),
        "SYSTEM" => ("text-warning", "border-l-warning"),
        _ => ("text-base-content", "border-l-base-content"),
    };

    view! {
        // chat-start alignment matches your previous bubble flow
        <div class=move || format!(
            "flex flex-col items-start px-2 group transition-colors hover:bg-base-content/5 {}",
            if signals.compact_mode.get() { "py-0.5" } else { "py-1" }
        )>

            // --- HEADER: Nickname(Romaji) + Level + Time ---
            <div class="opacity-90 mb-1 flex gap-2 items-baseline">
                <span
                    class=move || {
                        let size_class = if signals.compact_mode.get() { "text-[12px]" } else { "text-[13px]" };
                        let color_class = if signals.search_term.get() == sig.get().nickname {
                            "text-success underline decoration-2"
                        } else {
                            channel_colors().0
                        };

                        // Combine static classes with the dynamic variables into one string
                        format!("font-black cursor-pointer hover:underline transition-all {} {}", size_class, color_class)
                    }
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

                <span class=move || format!("text-gray-500 font-bold {}", if signals.compact_mode.get() { "text-[9px]" } else { "text-[10px]" })>
                    "Lv." {move || sig.get().level}
                </span>
                <time class=move || format!("ml-2 text-gray-500 opacity-70 {}", if signals.compact_mode.get() { "text-[9px]" } else { "text-[10px]" })>
                    {move || format_time(sig.get().timestamp)}
                </time>
            </div>

            <div class="flex items-center gap-2 w-full">
                // --- BUBBLE: Styled with DaisyUI + Custom BPSR Accents ---
                <div class=move || format!(
                    "px-3 w-fit max-w-[85%] bg-base-200 border-y border-r border-base-content/5 border-l-[3px] rounded-md text-base-content shadow-sm transition-all {} {}",
                    channel_colors().1,
                    if signals.compact_mode.get() { "py-1" } else { "py-2" } // Dynamic bubble padding
                )>
                    <div class=move || if signals.compact_mode.get() { "text-[12px] leading-snug" } else { "text-[14px] leading-relaxed" }>
                        {move || if is_japanese(&sig.get().message) {
                            view! { <span class="text-gray-500 mr-1.5 font-bold">[원문]</span> }.into_any()
                        } else {
                            view! {}.into_any()
                        }}
                        {move || sig.get().message.clone()}
                    </div>

                    {move || sig.get().translated.clone().map(|text| view! {
                        <div class=move || format!(
                            "mt-1.5 pt-1.5 border-t border-base-content/10 text-success font-bold animate-in slide-in-from-top-1 duration-200 {}",
                            if signals.compact_mode.get() { "text-[12px]" } else { "text-[14px]" }
                        )>
                             <span class="opacity-70 mr-1.5 font-bold">[번역]</span>
                             {text}
                        </div>
                    })}
                </div>

                // --- ACTION BAR: Visible on hover for copy/search ---
                <div class="opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                    <button class="btn btn-ghost btn-xs text-[10px] text-gray-500 h-6 min-h-0 px-2 hover:bg-base-content/10 hover:text-base-content"
                        on:click=move |_| copy_to_clipboard(&sig.get_untracked().message)>
                        "COPY"
                    </button>
                </div>
            </div>
        </div>
    }
}