use crate::chat_view::{compact_original_class, translation_pending};
use crate::components::icons::{self, icon};
use crate::favorites::add_from_chat;
use crate::readability::{box_name, needs_backing, row_palette, ORIGINAL_TEXT, TEXT_BOX};
use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use crate::ui_types::{Channel, ChatMessage};
use crate::use_context;
use crate::utils::{copy_to_clipboard, format_time};
use leptos::portal::Portal;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use leptos::{component, view, IntoView};

#[component]
pub fn ChatRow(sig: ArcRwSignal<ChatMessage>) -> impl IntoView {
    // Owned by this row: freed when it unmounts. The store's Arc copy is
    // what events update.
    let sig = RwSignal::from(sig);
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");

    // Star: save this message (translation as the note) to the favorites.
    let (starred, set_starred) = signal(false);
    let save_favorite = move || {
        let (text, translated) = sig.with_untracked(|m| (m.message.clone(), m.translated.clone()));
        let mut added = false;
        signals
            .config
            .set_favorite_messages
            .update(|list| added = add_from_chat(list, &text, translated.as_deref()));
        if added {
            actions.save_config.dispatch(());
        }
        set_starred.set(true);
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(1200).await;
            set_starred.set(false);
        });
    };
    let star_button = move |class: &'static str| {
        view! {
            <Show when=move || !sig.with(|m| m.is_blocked)>
                <button class=class title="자주 쓰는 메시지에 추가"
                    on:click=move |_| save_favorite()>
                    {move || if starred.get() { "✓" } else { "⭐" }}
                </button>
            </Show>
        }
        .into_any()
    };

    Effect::new(move |_| {
        if sig.with(|m| m.translated.is_some()) {
            if signals.chat.is_at_bottom.get_untracked() {
                request_animation_frame(move || {
                    if let Some(window) = web_sys::window() {
                        if let Some(doc) = window.document() {
                            if let Some(el) = doc.get_element_by_id("chat-scroll-container") {
                                el.set_scroll_top(el.scroll_height());
                            }
                        }
                    }
                });
            }
        }
    });

    let is_active =
        Memo::new(move |_| signals.ui.active_menu_id.get() == Some(sig.with_untracked(|m| m.pid)));
    let (menu_pos, set_menu_pos) = signal((0, 0));

    let channel_colors = move || {
        sig.with(|m| match m.channel {
            Channel::World => ("text-purple-500", "border-l-purple-500"),
            Channel::Guild => ("text-emerald-500", "border-l-emerald-500"),
            Channel::Party => ("text-sky-500", "border-l-sky-500"),
            Channel::Local => ("text-base-content/70", "border-l-base-content/50"),
            Channel::Beginner => ("text-amber-500", "border-l-amber-500"),
        })
    };

    let display_time = move || {
        let raw_ts = sig.with(|m| m.timestamp);
        let msg_secs = if raw_ts > 10_000_000_000 {
            raw_ts / 1000
        } else {
            raw_ts
        };

        if signals.config.use_relative_time.get() {
            let current_raw = signals.chat.current_time.get();
            let current_secs = if current_raw > 10_000_000_000 {
                current_raw / 1000
            } else {
                current_raw
            };
            let diff_secs = if current_secs > msg_secs {
                current_secs - msg_secs
            } else {
                0
            };

            if diff_secs < 10 {
                "now".to_string()
            } else if diff_secs < 60 {
                format!("{}s", diff_secs)
            } else if diff_secs < 3600 {
                format!("{}m", diff_secs / 60)
            } else if diff_secs < 86400 {
                let hours = diff_secs / 3600;
                let mins = (diff_secs % 3600) / 60;
                if mins > 0 {
                    format!("{}h {}m", hours, mins)
                } else {
                    format!("{}h", hours)
                }
            } else {
                format!("{}d", diff_secs / 86400)
            }
        } else {
            format_time(raw_ts)
        }
    };

    // Sender-name click: open / close this row's menu at the pointer.
    let toggle_menu = move |ev: web_sys::MouseEvent| {
        ev.stop_propagation();
        if is_active.get() {
            signals.ui.set_active_menu_id.set(None);
        } else {
            set_menu_pos.set((ev.client_x(), ev.client_y()));
            signals
                .ui
                .set_active_menu_id
                .set(Some(sig.with_untracked(|m| m.pid)));
        }
    };
    // Underlined when the chat is filtered to this sender.
    let filtered_to_sender = move || {
        signals
            .chat
            .search_term
            .with(|s| sig.with(|m| *s == m.nickname))
    };

    let name_menu = move || {
        view! {
            <Show when=move || is_active.get()>
                <Portal>
                    <div class="fixed z-50 bg-base-300 border border-white/10 rounded-lg shadow-2xl p-1 flex flex-col min-w-[130px] animate-in fade-in zoom-in-95 duration-100"
                         style=move || {
                             let (x, y) = menu_pos.get();
                             format!("top: {}px; left: {}px;", y + 8, x + 8)
                         }
                         on:click=move |ev| ev.stop_propagation()>

                        <button class="btn btn-ghost btn-sm justify-start text-xs font-normal h-8 min-h-0 px-2"
                            on:click=move |_| {
                                sig.with_untracked(|m| copy_to_clipboard(&m.nickname));
                                signals.ui.set_active_menu_id.set(None);
                            }>
                            "📋 Copy Name"
                        </button>

                        <button class="btn btn-ghost btn-sm justify-start text-xs font-normal h-8 min-h-0 px-2"
                            on:click=move |_| {
                                let n = sig.with_untracked(|m| m.nickname.clone());
                                if signals.chat.search_term.get_untracked() == n {
                                    signals.chat.set_search_term.set("".into());
                                } else {
                                    signals.chat.set_search_term.set(n);
                                }
                                signals.ui.set_active_menu_id.set(None);
                            }>
                            "🔍 Filter Chat"
                        </button>

                        <button class="btn btn-ghost btn-sm justify-start text-xs font-normal h-8 min-h-0 px-2 text-error"
                            on:click=move |_| {
                                let target_uid = sig.with_untracked(|m| m.uid);
                                let target_name = sig.with_untracked(|m| m.nickname.clone());
                                let blocked_name = sig.with_untracked(|m| m.nickname.clone());

                                spawn_local(async move {
                                    let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                        "uid": target_uid,
                                        "nickname": target_name
                                    })).unwrap();
                                    let _ = invoke("block_user_command", args).await;
                                });

                                signals.config.set_blocked_users.update(|map| { map.insert(target_uid, blocked_name); });
                                signals.ui.set_active_menu_id.set(None);
                            }>
                            "🚫 Block User"
                        </button>
                    </div>
                </Portal>
            </Show>
        }
        .into_any()
    };

    // Star + copy, shown on hover.
    let hover_actions = move |class: &'static str| {
        view! {
            <div class=class>
                {star_button("btn btn-ghost btn-xs h-5 min-h-0 px-1.5 text-[10px]")}
                <Show when=move || !sig.with(|m| m.is_blocked)>
                    <button class="btn btn-ghost btn-xs h-5 min-h-0 px-1.5 opacity-70 hover:opacity-100" title="원문 복사"
                        on:click=move |_| sig.with_untracked(|m| copy_to_clipboard(&m.message))>
                        {icon(icons::COPY, "size-3")}
                    </button>
                </Show>
            </div>
        }
        .into_any()
    };

    // Normal rows sit on the text box once the window is see-through.
    let palette =
        Memo::new(move |_| row_palette(needs_backing(signals.config.overlay_opacity.get())));
    let pending =
        move || sig.with(|m| translation_pending(m, signals.config.use_translation.get()));
    let dots = || view! { <span class="loading loading-dots loading-xs ml-1.5 align-middle opacity-50"></span> };

    view! {
        <Show when=move || !(sig.with(|m| m.is_blocked) && signals.config.hide_blocked_messages.get())>
            <Show
                when=move || signals.config.compact_mode.get()
                fallback=move || view! {
                    // ==========================================
                    // NORMAL VIEW: header line, translation first, original below
                    // ==========================================
                    <div class=move || format!("group mx-1 border-l-2 rounded-r-md transition-colors {}",
                            channel_colors().1)
                         style=move || format!("padding-top: {0}px; padding-bottom: {0}px;", signals.config.message_spacing.get() + 2)>
                        <div class=move || {
                            let p = palette.get();
                            if p.container.is_empty() {
                                "pl-3 pr-2 rounded-r-md hover:bg-base-content/5".to_string()
                            } else {
                                format!("ml-1.5 px-2.5 py-1 w-fit max-w-[calc(100%-0.5rem)] {}", p.container)
                            }
                        }>
                            <div class="flex items-center gap-2 text-[11px] leading-5 min-w-0">
                                <span
                                    class=move || {
                                        let color = if filtered_to_sender() {
                                            "text-success underline decoration-2"
                                        } else if palette.get().container.is_empty() {
                                            channel_colors().0
                                        } else {
                                            sig.with(|m| box_name(m.channel).0)
                                        };
                                        format!("font-bold cursor-pointer hover:underline truncate {color}")
                                    }
                                    style=move || format!("font-size: {}px;", signals.config.font_size.get().saturating_sub(2).max(10))
                                    on:click=toggle_menu
                                >
                                    {move || sig.with(|m| m.nickname.clone())}
                                    {move || sig.with(|m| m.nickname_romaji.clone()).map(|r| view! {
                                        <span class=move || format!("ml-1 font-normal {}", palette.get().meta)>{r}</span>
                                    })}
                                </span>
                                {name_menu()}
                                <span class=move || format!("tabular-nums {}", palette.get().meta)>"Lv." {move || sig.with(|m| m.level)}</span>
                                <time class=move || format!("tabular-nums {}", palette.get().meta)>{display_time}</time>
                                {hover_actions("ml-auto flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity")}
                            </div>

                            {move || {
                                let msg = sig.get();
                                let p = palette.get();
                                let fs = signals.config.font_size.get();
                                let kw = signals.config.emphasis_keywords.get();
                                if msg.is_blocked {
                                    view! {
                                        <div class=format!("italic opacity-60 {}", p.text) style=format!("font-size: {}px;", fs)>
                                            "(차단된 사용자의 메시지입니다)"
                                        </div>
                                    }.into_any()
                                } else if let Some(text) = msg.translated.clone() {
                                    view! {
                                        <div class=format!("leading-snug font-medium animate-in fade-in duration-200 {}", p.text) style=format!("font-size: {}px;", fs)>
                                            {render_emphasized(&text, &kw)}
                                        </div>
                                        <div class=format!("leading-snug mt-0.5 {}", p.original) style=format!("font-size: {}px;", fs.saturating_sub(2).max(10))>
                                            {render_emphasized(&msg.message, &kw)}
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class=format!("leading-snug font-medium {}", p.text) style=format!("font-size: {}px;", fs)>
                                            {render_emphasized(&msg.message, &kw)}
                                            {pending().then(dots)}
                                        </div>
                                    }.into_any()
                                }
                            }}
                        </div>
                    </div>
                }
            >
                // ==========================================
                // COMPACT VIEW: subtitle captions on the text box
                // ==========================================
                <div class="group relative px-2"
                     style=move || format!("padding-top: {0}px; padding-bottom: {0}px; font-size: {1}px;",
                         signals.config.message_spacing.get().saturating_sub(2),
                         signals.config.font_size.get().saturating_sub(1).max(10))>
                    <div class=format!("inline-block max-w-full px-2.5 py-1 leading-snug {TEXT_BOX}")>
                        <span
                            class=move || {
                                let color = if filtered_to_sender() { "text-success underline" } else { sig.with(|m| box_name(m.channel).0) };
                                format!("font-semibold cursor-pointer hover:underline {color}")
                            }
                            on:click=toggle_menu
                        >{move || sig.with(|m| m.nickname.clone())}</span>
                        <span class="text-white/40 mr-1">":"</span>
                        {move || {
                            let msg = sig.get();
                            let kw = signals.config.emphasis_keywords.get();
                            if msg.is_blocked {
                                view! { <span class="italic text-white/60">"(차단된 사용자의 메시지)"</span> }.into_any()
                            } else if let Some(text) = msg.translated.clone() {
                                let hide = signals.config.hide_original_in_compact.get();
                                view! {
                                    <span>{render_emphasized(&text, &kw)}</span>
                                    <div class=format!("text-[0.85em] {ORIGINAL_TEXT} {}", compact_original_class(hide, true))>
                                        {render_emphasized(&msg.message, &kw)}
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <span>{render_emphasized(&msg.message, &kw)}</span>
                                    {pending().then(dots)}
                                }.into_any()
                            }
                        }}
                    </div>
                    {name_menu()}
                    {hover_actions("absolute right-1 top-0.5 hidden group-hover:flex gap-0.5 bg-base-300 rounded-md shadow border border-base-content/10 z-10")}
                </div>
            </Show>
        </Show>
    }
}

// CLEANED UP: No more messy text-shadows needed!
fn render_emphasized(text: &str, keywords: &[String]) -> impl IntoView {
    if keywords.is_empty() || text.is_empty() {
        return view! { <span>{text.to_string()}</span> }.into_any();
    }

    let mut views = Vec::new();
    let mut current_text = text;

    while !current_text.is_empty() {
        let mut earliest_find = None;
        for kw in keywords {
            if kw.is_empty() {
                continue;
            }
            if let Some(idx) = current_text.find(kw) {
                if earliest_find.map_or(true, |(e_idx, _)| idx < e_idx) {
                    earliest_find = Some((idx, kw));
                }
            }
        }

        match earliest_find {
            Some((idx, kw)) => {
                let before = &current_text[..idx];
                if !before.is_empty() {
                    views.push(
                        view! {
                            <span>{before.to_string()}</span>
                        }
                        .into_any(),
                    );
                }
                // Emphasis keywords keep their warning color, but no shadow needed.
                views.push(
                    view! {
                        <span class="text-warning font-black mx-0.5">
                            {kw.to_string()}
                        </span>
                    }
                    .into_any(),
                );
                current_text = &current_text[idx + kw.len()..];
            }
            None => {
                views.push(
                    view! {
                        <span>{current_text.to_string()}</span>
                    }
                    .into_any(),
                );
                break;
            }
        }
    }

    views.into_view().into_any()
}
