use crate::use_context;
use leptos::prelude::{CustomAttribute, ReadSignal, Set, StyleAttribute};
use leptos::prelude::{ElementChild, GetUntracked, OnAttribute};
use leptos::{component, view, IntoView};
use leptos::control_flow::Show;
use leptos::prelude::{ClassAttribute, Get, RwSignal};
use leptos::task::spawn_local;
use crate::app;
use crate::store::GlobalStore;
use crate::types::ChatMessage;
use crate::utils::{copy_to_clipboard, format_time, is_japanese};

#[component]
pub fn ChatRow(sig: RwSignal<ChatMessage>) -> impl IntoView {
    let store = use_context::<GlobalStore>()
        .expect("GlobalStore context missing");

    let msg = sig.get();
    let msg = sig.get();
    let pid = msg.pid;
    let is_jp = is_japanese(&msg.message);
    let is_active = move || store.active_menu_id.get() == Some(pid);
    view! {
        <div class="chat-row" data-channel=move || sig.get().channel.clone()
             style:z-index=move || if is_active() { "10001" } else { "1" }>

            <div class="msg-header">
                // Restore Nickname Click & Active Class
                <span class=move || if store.search_term.get() == sig.get().nickname { "nickname active" } else { "nickname" }
                    on:click=move |ev| {
                        ev.stop_propagation();
                        if is_active() { store.set_active_menu_id.set(None); }
                        else { store.set_active_menu_id.set(Some(pid)); }
                    }
                >
                    {move || {
                        let p = sig.get();
                        match p.nickname_romaji {
                            Some(romaji) => format!("{}({})", p.nickname, romaji),
                            None => p.nickname.clone()
                        }
                    }}
                </span>

                // Restore Context Menu
                <Show when=is_active>
                    <div class="context-menu" on:click=move |ev| ev.stop_propagation()>
                        <button class="menu-item" on:click=move |_| {
                            copy_to_clipboard(&sig.get_untracked().nickname);
                            store.set_active_menu_id.set(None);
                        }>
                            <span class="menu-icon">"📋"</span>"Copy Name"
                        </button>
                        <button class="menu-item" on:click=move |_| {
                            let n = sig.get_untracked().nickname;
                            if store.search_term.get_untracked() == n { store.set_search_term.set("".into()); }
                            else { store.set_search_term.set(n); }
                            store.set_active_menu_id.set(None);
                        }>
                            <span class="menu-icon">"🔍"</span>"Filter Chat"
                        </button>
                    </div>
                </Show>

                <span class="lvl">"Lv." {move || sig.get().level}</span>
                <span class="time">{format_time(msg.timestamp)}</span>
            </div>

            <div class="msg-wrapper">
                <div class="msg-body" class:has-translation=move || sig.get().translated.is_some()>
                    // Restore [원문] and [번역] Labels
                    <div class="original">
                        {if is_jp { "[원문] " } else { "" }} {move || sig.get().message.clone()}
                    </div>
                    {move || sig.get().translated.clone().map(|text| view! {
                        <div class="translated">"[번역] " {text}</div>
                    })}
                </div>
                <button class="copy-btn" on:click=move |ev| {
                    ev.stop_propagation();
                    copy_to_clipboard(&sig.get().message.clone());
                }> "📋" </button>
            </div>
        </div>
    }
}