use crate::store::AppSignals;
use crate::tauri_bridge::invoke;
use crate::ui_types::ChatMessage;
use crate::use_context;
use crate::utils::{copy_to_clipboard, format_time, is_japanese};
use leptos::portal::Portal;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use leptos::{component, view, IntoView};

#[component]
pub fn ChatRow(sig: RwSignal<ChatMessage>) -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");

    Effect::new(move |_| {
        if sig.get().translated.is_some() {
            if signals.is_at_bottom.get_untracked() {
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
        Memo::new(move |_| signals.active_menu_id.get() == Some(sig.get_untracked().pid));
    let (menu_pos, set_menu_pos) = signal((0, 0));

    let channel_colors = move || match sig.get().channel.as_str() {
        "WORLD" => ("text-purple-500", "border-l-purple-500"),
        "GUILD" => ("text-emerald-500", "border-l-emerald-500"),
        "PARTY" => ("text-sky-500", "border-l-sky-500"),
        "LOCAL" => ("text-base-content/70", "border-l-base-content/50"),
        "SYSTEM" => ("text-warning", "border-l-warning"),
        _ => ("text-base-content", "border-l-base-content"),
    };

    let display_time = move || {
        let raw_ts = sig.get().timestamp;
        let msg_secs = if raw_ts > 10_000_000_000 {
            raw_ts / 1000
        } else {
            raw_ts
        };

        if signals.use_relative_time.get() {
            let current_raw = signals.current_time.get();
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

    view! {
        <Show when=move || !(sig.get().is_blocked && signals.hide_blocked_messages.get())>
            <Show
                when=move || signals.compact_mode.get()
                fallback=move || view! {
                    // ==========================================
                    // STANDARD VIEW (Stacked)
                    // ==========================================
                    <div class="flex flex-col items-start px-2 group transition-colors hover:bg-base-content/5"
                         style=move || format!("padding-top: {0}px; padding-bottom: {0}px;", signals.message_spacing.get())>
                        <div class="opacity-90 mb-1 flex gap-2 items-center">
                            // 1. NICKNAME BUBBLE
                            <span
                                class=move || {
                                    let color_class = if signals.search_term.get() == sig.get().nickname {
                                        "text-success underline decoration-2"
                                    } else {
                                        channel_colors().0
                                    };
                                    // ADDED: bg-base-200 and padding to create a solid pill shape!
                                    format!("font-black cursor-pointer transition-all hover:brightness-125 tracking-wide bg-base-200 px-1.5 py-0.5 rounded-md shadow-sm border border-base-content/5 {}", color_class)
                                }
                                style=move || format!("font-size: {}px;", signals.font_size.get().saturating_sub(1).max(10))
                                on:click=move |ev| {
                                    ev.stop_propagation();
                                    if is_active.get() {
                                        signals.set_active_menu_id.set(None);
                                    } else {
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

                                        <button class="btn btn-ghost btn-sm justify-start text-xs font-normal h-8 min-h-0 px-2 text-error"
                                            on:click=move |_| {
                                                let target_uid = sig.get_untracked().uid;
                                                let target_name = sig.get_untracked().nickname.clone();
                                                let blocked_name = sig.get_untracked().nickname.clone();

                                                spawn_local(async move {
                                                    let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                                        "uid": target_uid,
                                                        "nickname": target_name
                                                    })).unwrap();
                                                    let _ = invoke("block_user_command", args).await;
                                                });

                                                signals.set_blocked_users.update(|map| { map.insert(target_uid, blocked_name); });
                                                signals.set_active_menu_id.set(None);
                                            }>
                                            "🚫 Block User"
                                        </button>
                                    </div>
                                </Portal>
                            </Show>

                            <span class="text-base-content/50 font-bold text-[10px] bg-base-200 px-1 rounded border border-base-content/5">
                                "Lv." {move || sig.get().level}
                            </span>
                            <time class="ml-1 text-base-content/50 opacity-70 text-[10px] bg-base-200 px-1 rounded border border-base-content/5">
                                {display_time}
                            </time>
                        </div>

                        // 2. MESSAGE BUBBLE
                        <div class="flex items-center gap-2 w-full mt-0.5">
                            <div class=move || format!(
                                "px-3 py-2 w-fit max-w-[85%] bg-base-200 border-y border-r border-base-content/5 border-l-[3px] rounded-md text-base-content shadow-sm transition-all {}",
                                channel_colors().1
                            )>
                                {move || {
                                    let msg = sig.get();

                                    if msg.is_blocked {
                                        view! {
                                            <div class="italic opacity-50 text-base-content/50 font-bold"
                                                style=move || format!("font-size: {}px;", signals.font_size.get())>
                                                "(차단된 사용자의 메시지입니다)"
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <>
                                                <div class="leading-relaxed font-bold"
                                                    style=move || format!("font-size: {}px;", signals.font_size.get())>
                                                    {
                                                        let show_original_prefix = is_japanese(&msg.message) && signals.use_translation.get();
                                                        if show_original_prefix {
                                                            view! { <span class="text-base-content/50 mr-1.5 font-bold">"[원문]"</span> }.into_any()
                                                        } else {
                                                            view! {}.into_any()
                                                        }
                                                    }
                                                    {render_emphasized(&msg.message, &signals.emphasis_keywords.get())}
                                                </div>

                                                {msg.translated.clone().map(|text| view! {
                                                    <div class="mt-1.5 pt-1.5 border-t border-base-content/10 text-success font-bold animate-in slide-in-from-top-1 duration-200"
                                                        style=move || format!("font-size: {}px;", signals.font_size.get())>
                                                         <span class="opacity-70 mr-1.5 font-bold">"[번역]"</span>
                                                         {render_emphasized(&text, &signals.emphasis_keywords.get())}
                                                    </div>
                                                })}
                                            </>
                                        }.into_any()
                                    }
                                }}
                            </div>

                            <div class="opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                                <button class="btn btn-ghost btn-xs text-[10px] text-base-content/50 h-6 min-h-0 px-2 hover:bg-base-content/10 hover:text-base-content bg-base-200 rounded-md shadow-sm"
                                    on:click=move |_| copy_to_clipboard(&sig.get_untracked().message)>
                                    "COPY"
                                </button>
                            </div>
                        </div>
                    </div>
                }
            >
                // ==========================================
                // COMPACT VIEW (Inline Wrapping)
                // ==========================================
                // 1. Parent is now a standard block with generous line-height for wrapping bubbles
                <div class="block px-2 group transition-colors hover:bg-base-content/5 w-full leading-[1.7] text-left break-words"
                     style=move || format!("padding-top: {0}px; padding-bottom: {0}px;", signals.message_spacing.get())>

                    // 2. NICKNAME BUBBLE (inline-block so it flows like text)
                    <span
                        class=move || {
                            let color_class = if signals.search_term.get() == sig.get().nickname {
                                "text-success underline decoration-2"
                            } else {
                                channel_colors().0
                            };
                            format!("font-black cursor-pointer transition-all hover:brightness-125 tracking-wide bg-base-200 px-1.5 py-0.5 rounded-md shadow-sm border border-base-content/5 inline-block align-baseline mr-1.5 {}", color_class)
                        }
                        style=move || format!("font-size: {}px;", signals.font_size.get().saturating_sub(2).max(10))
                        on:click=move |ev| {
                            ev.stop_propagation();
                            if is_active.get() {
                                signals.set_active_menu_id.set(None);
                            } else {
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

                                <button class="btn btn-ghost btn-sm justify-start text-xs font-normal h-8 min-h-0 px-2 text-error"
                                    on:click=move |_| {
                                        let target_uid = sig.get_untracked().uid;
                                        let target_name = sig.get_untracked().nickname.clone();
                                        let blocked_name = sig.get_untracked().nickname.clone();

                                        spawn_local(async move {
                                            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                                                "uid": target_uid,
                                                "nickname": target_name
                                            })).unwrap();
                                            let _ = invoke("block_user_command", args).await;
                                        });

                                        signals.set_blocked_users.update(|map| { map.insert(target_uid, blocked_name); });
                                        signals.set_active_menu_id.set(None);
                                    }>
                                    "🚫 Block User"
                                </button>
                            </div>
                        </Portal>
                    </Show>

                    // 3. MESSAGE BODY (Inline and wrapping)
                    {move || {
                        let msg = sig.get();

                        if msg.is_blocked {
                            view! {
                                <span class="italic opacity-50 text-base-content/50 font-bold inline align-baseline"
                                      style=move || format!("font-size: {}px;", signals.font_size.get().saturating_sub(2).max(10))>
                                    "(차단된 사용자의 메시지입니다)"
                                </span>
                            }.into_any()
                        } else {
                            let emphasized_msg = msg.clone();
                            let has_translation = msg.translated.is_some();
                            let hide_orig_pref = signals.hide_original_in_compact.get();

                            // Original message view (inline, with box-decoration-clone to wrap backgrounds beautifully)
                            let original_view = view! {
                                <span class=move || format!(
                                    "text-base-content font-bold opacity-90 box-decoration-clone bg-base-200 px-1.5 py-0.5 rounded-md shadow-sm border-y border-r border-base-content/5 border-l-[3px] inline align-baseline {} {}",
                                    if hide_orig_pref && has_translation { "hidden group-hover:inline" } else { "inline" },
                                    channel_colors().1
                                )
                                style=move || format!("font-size: {}px;", signals.font_size.get().saturating_sub(2).max(10))>
                                    {
                                        if !hide_orig_pref && is_japanese(&msg.message) && signals.use_translation.get() {
                                            view! { <span class="text-base-content/50 mr-1 font-bold">"[원문]"</span> }.into_any()
                                        } else {
                                            view! {}.into_any()
                                        }
                                    }
                                    {render_emphasized(&emphasized_msg.message, &signals.emphasis_keywords.get())}
                                </span>
                            };

                            // Translated message view
                            let translated_view = msg.translated.clone().map(|text| {
                                view! {
                                    <span class=move || format!(
                                        "text-success font-bold box-decoration-clone bg-base-200 px-1.5 py-0.5 rounded-md shadow-sm border border-base-content/5 inline align-baseline ml-1 {}",
                                        if hide_orig_pref { "inline group-hover:hidden" } else { "inline" }
                                    )
                                    style=move || format!("font-size: {}px;", signals.font_size.get().saturating_sub(2).max(10))>
                                        <Show when=move || !hide_orig_pref>
                                            <span class="opacity-70 mr-1 font-bold">"[번역]"</span>
                                        </Show>
                                        {render_emphasized(&text, &signals.emphasis_keywords.get())}
                                    </span>
                                }
                            });

                            view! {
                                {original_view}
                                {translated_view}
                            }.into_any()
                        }
                    }}

                    // 4. TIMESTAMP & COPY BUTTON (Inline-block at the end of the text)
                    <div class="inline-block align-baseline ml-2 opacity-90 bg-base-200 rounded-md shadow-sm">
                        <time class="text-[10px] text-base-content/50 whitespace-nowrap block group-hover:hidden min-h-0 px-1.5 py-0">
                            {display_time}
                        </time>

                        // --- NEW: HIDE COPY BUTTON ON BLOCKED MESSAGES ---
                        <Show when=move || !sig.get().is_blocked>
                            <button class="hidden group-hover:flex btn btn-ghost btn-xs text-[10px] font-bold text-base-content/50 h-5 min-h-0 px-1.5 py-0 hover:bg-base-content/10 hover:text-base-content leading-none"
                                on:click=move |_| copy_to_clipboard(&sig.get_untracked().message)>
                                "COPY"
                            </button>
                        </Show>
                    </div>
                </div>
            </Show>
        </Show>
    }
}

// CLEANED UP: No more messy text-shadows needed!
fn render_emphasized(text: &str, keywords: &[String]) -> impl IntoView {
    if keywords.is_empty() || text.is_empty() {
        return view! { <span class="font-bold">{text.to_string()}</span> }.into_any();
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
                            <span class="font-bold">{before.to_string()}</span>
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
                        <span class="font-bold">{current_text.to_string()}</span>
                    }
                    .into_any(),
                );
                break;
            }
        }
    }

    views.into_view().into_any()
}
