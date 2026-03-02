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

    // --- DYNAMIC TIME FORMATTER ---
    let display_time = move || {
        let raw_ts = sig.get().timestamp;

        // 1. Auto-detect Unit Mismatch
        // If the timestamp is > 10 billion (13 digits), it is in milliseconds.
        // Otherwise (10 digits), it is already in seconds.
        let msg_secs = if raw_ts > 10_000_000_000 { raw_ts / 1000 } else { raw_ts };

        if signals.use_relative_time.get() {
            let current_raw = signals.current_time.get();
            let current_secs = if current_raw > 10_000_000_000 { current_raw / 1000 } else { current_raw };

            // 2. Calculate difference safely
            let diff_secs = if current_secs > msg_secs { current_secs - msg_secs } else { 0 };

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
                // If the message is incredibly old, show days
                format!("{}d", diff_secs / 86400)
            }
        } else {
            // Fallback to absolute mm:ss format
            format_time(raw_ts)
        }
    };

    view! {
        <Show
            when=move || signals.compact_mode.get()
            fallback=move || view! {
                // ==========================================
                // STANDARD VIEW (Stacked)
                // ==========================================
                <div class="flex flex-col items-start px-2 py-1 group transition-colors hover:bg-base-content/5">
                    <div class="opacity-90 mb-1 flex gap-2 items-center">
                        <span
                            class=move || {
                                let color_class = if signals.search_term.get() == sig.get().nickname {
                                    "text-success underline decoration-2"
                                } else {
                                    channel_colors().0
                                };
                                format!("font-black cursor-pointer transition-all hover:brightness-125 tracking-wide {}", color_class)
                            }
                            style=move || format!(
                                "font-size: {}px; text-shadow: -1px -1px 0 #000, 1px -1px 0 #000, -1px 1px 0 #000, 1px 1px 0 #000;",
                                signals.font_size.get().saturating_sub(1).max(10)
                            )
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
                                </div>
                            </Portal>
                        </Show>

                        <span class="text-base-content/50 font-bold text-[10px]">
                            "Lv." {move || sig.get().level}
                        </span>
                        // 1. Time string replaced here
                        <time class="ml-2 text-base-content/50 opacity-70 text-[10px]">
                            {display_time}
                        </time>
                    </div>

                    <div class="flex items-center gap-2 w-full">
                        <div class=move || format!(
                            "px-3 py-2 w-fit max-w-[85%] bg-base-200 border-y border-r border-base-content/5 border-l-[3px] rounded-md text-base-content shadow-sm transition-all {}",
                            channel_colors().1
                        )>
                            <div class="leading-relaxed font-bold"
                                style=move || format!("font-size: {}px;", signals.font_size.get())>
                                {move || {
                                    let show_original_prefix = is_japanese(&sig.get().message) && signals.use_translation.get();
                                    if show_original_prefix {
                                        view! { <span class="text-base-content/50 mr-1.5 font-bold">[원문]</span> }.into_any()
                                    } else {
                                        view! {}.into_any()
                                    }
                                }}
                                {move || render_emphasized(&sig.get().message, &signals.emphasis_keywords.get())}
                            </div>

                            {move || sig.get().translated.clone().map(|text| view! {
                                <div class="mt-1.5 pt-1.5 border-t border-base-content/10 text-success font-bold animate-in slide-in-from-top-1 duration-200"
                                    style=move || format!("font-size: {}px;", signals.font_size.get())>
                                     <span class="opacity-70 mr-1.5 font-bold">[번역]</span>
                                     {render_emphasized(&text, &signals.emphasis_keywords.get())}
                                </div>
                            })}
                        </div>

                        <div class="opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                            <button class="btn btn-ghost btn-xs text-[10px] text-base-content/50 h-6 min-h-0 px-2 hover:bg-base-content/10 hover:text-base-content"
                                on:click=move |_| copy_to_clipboard(&sig.get_untracked().message)>
                                "COPY"
                            </button>
                        </div>
                    </div>
                </div>
            }
        >
            // ==========================================
            // COMPACT VIEW (Inline Flattened)
            // ==========================================
            <div class="flex flex-row items-baseline gap-2 px-2 py-0.5 group transition-colors hover:bg-base-content/5 w-full">

                // 1. Nickname
                <span
                    class=move || {
                        let color_class = if signals.search_term.get() == sig.get().nickname {
                            "text-success underline decoration-2"
                        } else {
                            channel_colors().0
                        };
                        format!("font-black cursor-pointer transition-all hover:brightness-125 tracking-wide flex-shrink-0 {}", color_class)
                    }
                    style=move || format!(
                        "font-size: {}px; text-shadow: -1px -1px 0 #000, 1px -1px 0 #000, -1px 1px 0 #000, 1px 1px 0 #000;",
                        signals.font_size.get().saturating_sub(2).max(10)
                    )
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
                        </div>
                    </Portal>
                </Show>

                // 2. Message Body & Actions
                <div class="flex-1 min-w-0 flex flex-wrap items-baseline gap-x-1.5 group/msg">
                    {move || {
                        let msg = sig.get();
                        let emphasized_msg = msg.clone();
                        let has_translation = msg.translated.is_some();
                        let hide_orig_pref = signals.hide_original_in_compact.get();

                        // Define the original message view
                        let original_view = view! {
                            <span class=move || format!(
                                "text-[12px] text-base-content font-bold leading-snug break-words opacity-90 {}",
                                if hide_orig_pref && has_translation { "hidden group-hover/msg:inline" } else { "inline" }
                            )
                            style=move || format!("font-size: {}px;", signals.font_size.get().saturating_sub(2).max(10))>
                                {move || {
                                    // Only show the [원문] badge if we are NOT in the "Hide Original" mode
                                    if !hide_orig_pref && is_japanese(&msg.message) && signals.use_translation.get() {
                                        view! { <span class="text-base-content/50 mr-1 font-bold">[원문]</span> }.into_any()
                                    } else {
                                        view! {}.into_any()
                                    }
                                }}
                                {render_emphasized(&emphasized_msg.message, &signals.emphasis_keywords.get())}
                            </span>
                        };

                        // Define the translated message view
                        let translated_view = msg.translated.clone().map(|text| {
                            view! {
                                <span class=move || format!(
                                    "text-[12px] text-success font-bold leading-snug break-words {}",
                                    if hide_orig_pref { "inline group-hover/msg:hidden" } else { "inline" }
                                )
                                style=move || format!("font-size: {}px;", signals.font_size.get().saturating_sub(2).max(10))>
                                    // Only show the [번역] badge if we are NOT in the "Hide Original" mode
                                    <Show when=move || !hide_orig_pref>
                                        <span class="opacity-70 mr-1 font-bold">[번역]</span>
                                    </Show>
                                    {render_emphasized(&text, &signals.emphasis_keywords.get())}
                                </span>
                            }
                        });

                        view! {
                            {original_view}
                            {translated_view}
                        }
                    }}

                    // 3. Time / Copy Action Swapper (Inline with text)
                    <div class="inline-flex items-center self-center flex-shrink-0 ml-1">
                        <time class="text-[10px] text-base-content/40 whitespace-nowrap block group-hover:hidden">
                            {display_time}
                        </time>

                        <button class="hidden group-hover:flex btn btn-ghost btn-xs text-[10px] font-bold text-base-content/50 h-5 min-h-0 px-1.5 py-0 hover:bg-base-content/10 hover:text-base-content leading-none"
                            on:click=move |_| copy_to_clipboard(&sig.get_untracked().message)>
                            "COPY"
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

fn render_emphasized(text: &str, keywords: &[String]) -> impl IntoView {
    let shadow_style = "text-shadow: -1px -1px 0 #000, 1px -1px 0 #000, -1px 1px 0 #000, 1px 1px 0 #000;";

    if keywords.is_empty() || text.is_empty() {
        return view! { <span class="font-bold" style=shadow_style>{text.to_string()}</span> }.into_any();
    }

    let mut views = Vec::new();
    let mut current_text = text;

    while !current_text.is_empty() {
        let mut earliest_find = None;
        for kw in keywords {
            if kw.is_empty() { continue; }
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
                    views.push(view! {
                        <span class="font-bold" style=shadow_style>{before.to_string()}</span>
                    }.into_any());
                }
                // Emphasis keywords keep their specific color but gain the shadow
                views.push(view! {
                    <span class="text-warning font-black drop-shadow-md mx-0.5" style=shadow_style>
                        {kw.to_string()}
                    </span>
                }.into_any());
                current_text = &current_text[idx + kw.len()..];
            },
            None => {
                views.push(view! {
                    <span class="font-bold" style=shadow_style>{current_text.to_string()}</span>
                }.into_any());
                break;
            }
        }
    }

    views.into_view().into_any()
}