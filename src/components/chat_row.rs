use crate::store::AppSignals;
use crate::types::ChatMessage;
use crate::use_context;
use crate::utils::{copy_to_clipboard, format_time, is_japanese};
use leptos::control_flow::Show;
use leptos::prelude::{ClassAttribute, Get, IntoAny, RwSignal};
use leptos::prelude::{CustomAttribute, Set, StyleAttribute};
use leptos::prelude::{ElementChild, GetUntracked, OnAttribute};
use leptos::{component, view, IntoView};

#[component]
pub fn ChatRow(sig: RwSignal<ChatMessage>) -> impl IntoView {
    let store = use_context::<AppSignals>().expect("Store missing");
    let is_active = move || store.active_menu_id.get() == Some(sig.get_untracked().pid);

    view! {
        <div class="group relative py-1 px-2 mb-0.5 border-l-2 transition-colors hover:bg-white/5"
             class:border-transparent=move || !is_active()
             class:border-bpsr-green=is_active
             style:z-index=move || if is_active() { "50" } else { "1" }>

            <div class="flex items-baseline gap-2 mb-1 opacity-90">
                <span class="text-[13px] font-black cursor-pointer hover:underline transition-all"
                    class:text-bpsr-green=move || store.search_term.get() == sig.get().nickname
                    on:click=move |ev| {
                        ev.stop_propagation();
                        store.set_active_menu_id.set(if is_active() { None } else { Some(sig.get_untracked().pid) });
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
                <span class="ml-auto text-[10px] text-gray-600">{format_time(sig.get().timestamp)}</span>
            </div>

            <div class="flex items-end gap-2">
                <div class="relative max-w-[85%] bg-zinc-900/50 border border-white/5 p-2 rounded-r-xl rounded-bl-xl shadow-lg">
                    <div class="text-[14px] leading-relaxed font-medium">
                        {move || if is_japanese(&sig.get().message) { view! { <span class="text-gray-500 mr-1">"[원문]"</span> }.into_any() } else { view! {}.into_any() }}
                        {move || sig.get().message.clone()}
                    </div>
                    {move || sig.get().translated.clone().map(|text| view! {
                        <div class="mt-1 text-bpsr-green font-bold text-[14px] border-t border-white/5 pt-1">
                            <span class="opacity-70 mr-1">"[번역]"</span> {text}
                        </div>
                    })}
                </div>
            </div>
        </div>
    }
}