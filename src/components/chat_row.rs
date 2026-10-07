use crate::chat_view::{compact_original_class, translation_pending};
use crate::dictionary_edit::draft;
use crate::favorites::add_from_chat;
use crate::favorites_sync;
use crate::readability::{box_name, needs_backing, row_palette, ORIGINAL_TEXT, TEXT_BOX};
use crate::ruby_view::{mark, RubyBatch, RubyCache, RUBY_BATCH_MAX};
use crate::store::AppSignals;
use crate::tauri_bridge::invoke;
use crate::translation_view::{effective, shows_translation, HOVER_ONLY};
use crate::ui_types::{Channel, ChatMessage, RubySpan, TranslationView};
use crate::use_context;
use crate::utils::{copy_to_clipboard, format_time, is_japanese};
use crate::view_signals::{DictDraft, MenuKind, RowMenu};
use leptos::portal::Portal;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use leptos::{component, view, IntoView};
use std::cell::RefCell;
use std::time::Duration;

/// Lines already given furigana: a row that is built again (a tab switch,
/// paging) does not ask the backend again.
const RUBY_CACHE_LINES: usize = 2000;

/// How long rows wait for each other before the furigana of all of them is asked in one call.
const RUBY_WINDOW: Duration = Duration::from_millis(50);

type RubyTarget = WriteSignal<Option<Vec<RubySpan>>>;

thread_local! {
    static RUBY_CACHE: RefCell<RubyCache> = RefCell::new(RubyCache::new(RUBY_CACHE_LINES));
    static RUBY_BATCH: RefCell<RubyBatch<RubyTarget>> = RefCell::new(RubyBatch::new());
}

/// Asks for the furigana of a row's line. Rows built together (a tab switch, paging, turning the
/// Study view on) are answered by one `annotate_furigana` call: the first request opens a short
/// window, and everything asked inside it goes out together.
fn ask_ruby(text: &str, target: RubyTarget) {
    let opens = RUBY_BATCH.with(|batch| batch.borrow_mut().add(text, target));
    if opens {
        set_timeout(flush_ruby, RUBY_WINDOW);
    }
}

fn flush_ruby() {
    let calls = RUBY_BATCH.with(|batch| batch.borrow_mut().drain(RUBY_BATCH_MAX));
    for call in calls {
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({ "texts": call.lines() }))
                .unwrap();
            let Ok(answer) = invoke("annotate_furigana", args).await else {
                return;
            };
            let Ok(lines) = serde_wasm_bindgen::from_value::<Vec<Vec<RubySpan>>>(answer) else {
                return;
            };
            for (line, spans, rows) in call.answer(lines) {
                RUBY_CACHE.with(|cache| cache.borrow_mut().put(&line, spans.clone()));
                // A row that was removed meanwhile is gone: its signal takes nothing.
                for target in rows {
                    let _ = target.try_set(Some(spans.clone()));
                }
            }
        });
    }
}

#[component]
pub fn ChatRow(sig: ArcRwSignal<ChatMessage>) -> impl IntoView {
    // Owned by this row: freed when it unmounts. The store's Arc copy is
    // what events update.
    let sig = RwSignal::from(sig);
    let signals = use_context::<AppSignals>().expect("AppSignals missing");

    // Star: save this message (translation as the note) to the favorites.
    let save_favorite = move || {
        let (text, translated) = sig.with_untracked(|m| (m.message.clone(), m.translated.clone()));
        let mut added = false;
        signals
            .config
            .set_favorite_messages
            .update(|list| added = add_from_chat(list, &text, translated.as_deref()));
        if added {
            favorites_sync::save(signals.config);
        }
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

    let pid = sig.with_untracked(|m| m.pid);
    let menu_open = move |kind: MenuKind| {
        Memo::new(move |_| signals.ui.active_menu.get() == Some(RowMenu { pid, kind }))
    };
    let name_open = menu_open(MenuKind::Sender);
    let text_open = menu_open(MenuKind::Message);
    let (menu_pos, set_menu_pos) = signal((0, 0));
    // The text selected in the message when its menu opened (the buttons of
    // the menu would clear it), and where the pointer went down (drag-to-scroll).
    let (selection, set_selection) = signal(String::new());
    let (pointer_down, set_pointer_down) = signal((0, 0));
    // "✓ ..." shown on a menu item for a moment before the menu closes.
    let (done, set_done) = signal(None::<&'static str>);

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

    // Click on the sender's name or on the message text: open / close that
    // menu at the pointer. A double click selects a word, so it (re)opens the
    // menu with that word instead of closing it.
    let click_menu = move |kind: MenuKind| {
        move |ev: web_sys::MouseEvent| {
            ev.stop_propagation();
            if kind == MenuKind::Message && sig.with_untracked(|m| m.is_blocked) {
                return;
            }
            // Drag-to-scroll ends in a click: not a request for a menu.
            if signals.config.drag_to_scroll.get_untracked() {
                let (x, y) = pointer_down.get_untracked();
                if (ev.client_x() - x).abs() > 4 || (ev.client_y() - y).abs() > 4 {
                    return;
                }
            }
            if kind == MenuKind::Message {
                let picked = web_sys::window()
                    .and_then(|w| w.get_selection().ok().flatten())
                    .map(|s| String::from(s.to_string()))
                    .unwrap_or_default();
                set_selection.set(picked);
            }
            let target = RowMenu { pid, kind };
            set_menu_pos.set((ev.client_x(), ev.client_y()));
            set_done.set(None);
            signals.ui.set_active_menu.update(|open| {
                *open = if ev.detail() >= 2 {
                    Some(target)
                } else {
                    RowMenu::toggled(*open, target)
                }
            });
        }
    };
    let toggle_name = click_menu(MenuKind::Sender);
    let toggle_text = click_menu(MenuKind::Message);
    let note_pointer_down =
        move |ev: web_sys::MouseEvent| set_pointer_down.set((ev.client_x(), ev.client_y()));
    // Underlined when the chat is filtered to this sender.
    let filtered_to_sender = move || {
        signals
            .chat
            .search_term
            .with(|s| sig.with(|m| *s == m.nickname))
    };

    let name_menu = move || {
        view! {
            <Show when=move || name_open.get()>
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
                                signals.ui.set_active_menu.set(None);
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
                                signals.ui.set_active_menu.set(None);
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
                                signals.ui.set_active_menu.set(None);
                            }>
                            "🚫 Block User"
                        </button>
                    </div>
                </Portal>
            </Show>
        }
        .into_any()
    };

    // The message's menu: copy, favorite, add to the dictionary.
    let text_menu = move || {
        // Runs `action`, shows `label` as done, and closes the menu a moment later.
        let finish = move |label: &'static str| {
            set_done.set(Some(label));
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(700).await;
                if done.get_untracked() == Some(label) {
                    set_done.set(None);
                    if text_open.get_untracked() {
                        signals.ui.set_active_menu.set(None);
                    }
                }
            });
        };
        let item = "btn btn-ghost btn-sm justify-start text-xs font-normal h-8 min-h-0 px-2";
        view! {
            <Show when=move || text_open.get()>
                <Portal>
                    <div class="fixed z-50 bg-base-300 border border-white/10 rounded-lg shadow-2xl p-1 flex flex-col min-w-[150px] animate-in fade-in zoom-in-95 duration-100"
                         style=move || {
                             let (x, y) = menu_pos.get();
                             format!("top: {}px; left: {}px;", y + 8, x + 8)
                         }
                         on:click=move |ev| ev.stop_propagation()>
                        <button class=item
                            on:click=move |_| {
                                sig.with_untracked(|m| copy_to_clipboard(&m.message));
                                finish("copy");
                            }>
                            {move || if done.get() == Some("copy") { "✓ 복사됨" } else { "📋 메시지 복사" }}
                        </button>
                        <Show when=move || sig.with(|m| m.translated.is_some())>
                            <button class=item
                                on:click=move |_| {
                                    sig.with_untracked(|m| copy_to_clipboard(m.translated.as_deref().unwrap_or_default()));
                                    finish("copy-translation");
                                }>
                                {move || if done.get() == Some("copy-translation") { "✓ 복사됨" } else { "📋 번역 복사" }}
                            </button>
                        </Show>
                        <button class=item
                            on:click=move |_| {
                                save_favorite();
                                finish("favorite");
                            }>
                            {move || if done.get() == Some("favorite") { "✓ 추가됨" } else { "⭐ 자주 쓰는 메시지에 추가" }}
                        </button>
                        <button class=item
                            on:click=move |_| {
                                let (key, value) = sig.with_untracked(|m| {
                                    draft(&selection.get_untracked(), &m.message, m.translated.as_deref())
                                });
                                signals.ui.set_dict_draft.set(Some(DictDraft { key, value }));
                                signals.ui.set_active_menu.set(None);
                            }>
                            "📖 사전에 추가"
                        </button>
                    </div>
                </Portal>
            </Show>
        }
        .into_any()
    };

    // Hover on the message text: a pointer and a line under it.
    let text_class = move || {
        if sig.with(|m| m.is_blocked) {
            ""
        } else {
            "cursor-pointer hover:underline decoration-1 underline-offset-4"
        }
    };

    // Normal rows sit on the text box once the window is see-through.
    let palette =
        Memo::new(move |_| row_palette(needs_backing(signals.config.overlay_opacity.get())));
    // What this row shows of the translation: on, off, or the study view.
    let mode = move || {
        effective(
            signals.config.translation_view.get(),
            signals.config.use_translation.get(),
        )
    };
    let pending = move || {
        sig.with(|m| {
            translation_pending(
                m,
                signals.config.use_translation.get() && shows_translation(mode()),
            )
        })
    };

    // Study view: the furigana of this row's line, asked for once, when the
    // view is on and the line is Japanese.
    let (ruby, set_ruby) = signal(None::<Vec<RubySpan>>);
    Effect::new(move |_| {
        if mode() != TranslationView::Study || ruby.with_untracked(Option::is_some) {
            return;
        }
        let (text, blocked) = sig.with_untracked(|m| (m.message.clone(), m.is_blocked));
        if blocked || !is_japanese(&text) {
            return;
        }
        if let Some(spans) = RUBY_CACHE.with(|c| c.borrow().get(&text).cloned()) {
            set_ruby.set(Some(spans));
            return;
        }
        ask_ruby(&text, set_ruby);
    });
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
                            format!("{} {}", p.layout, p.container)
                        }>
                            <div class="flex items-center gap-2 text-[11px] leading-5 min-w-0">
                                <span
                                    class=move || {
                                        let color = if filtered_to_sender() {
                                            "text-success underline decoration-2"
                                        } else if !palette.get().backed {
                                            channel_colors().0
                                        } else {
                                            sig.with(|m| box_name(m.channel).0)
                                        };
                                        format!("font-bold cursor-pointer hover:underline truncate {color}")
                                    }
                                    style=move || format!("font-size: {}px;", signals.config.font_size.get().saturating_sub(2).max(10))
                                    on:click=toggle_name
                                >
                                    {move || sig.with(|m| m.nickname.clone())}
                                    {move || sig.with(|m| m.nickname_romaji.clone()).map(|r| view! {
                                        <span class=move || format!("ml-1 font-normal {}", palette.get().meta)>{r}</span>
                                    })}
                                </span>
                                {name_menu()}
                                <span class=move || format!("tabular-nums {}", palette.get().meta)>"Lv." {move || sig.with(|m| m.level)}</span>
                                <time class=move || format!("tabular-nums {}", palette.get().meta)>{display_time}</time>
                            </div>

                            <div class=text_class on:click=toggle_text on:mousedown=note_pointer_down>
                            {move || {
                                let msg = sig.get();
                                let p = palette.get();
                                let fs = signals.config.font_size.get();
                                let kw = signals.config.emphasis_keywords.get();
                                let mode = mode();
                                if msg.is_blocked {
                                    view! {
                                        <div class=format!("italic opacity-60 {}", p.text) style=format!("font-size: {}px;", fs)>
                                            "(차단된 사용자의 메시지입니다)"
                                        </div>
                                    }.into_any()
                                } else if mode == TranslationView::Study {
                                    // The Japanese with furigana; the translation only under the pointer.
                                    let translated = msg.translated.clone();
                                    view! {
                                        <div class=format!("leading-snug font-medium {}", p.text) style=format!("font-size: {}px;", fs)>
                                            {render_original(&msg.message, ruby.get(), &kw)}
                                        </div>
                                        {translated.map(|text| view! {
                                            <div class=format!("leading-snug mt-0.5 animate-in fade-in duration-200 {} {HOVER_ONLY}", p.original) style=format!("font-size: {}px;", fs.saturating_sub(2).max(10))>
                                                {render_emphasized(&text, &kw)}
                                            </div>
                                        })}
                                    }.into_any()
                                } else if let Some(text) = msg.translated.clone().filter(|_| shows_translation(mode)) {
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
                            {text_menu()}
                        </div>
                    </div>
                }
            >
                // ==========================================
                // COMPACT VIEW: subtitle captions on the text box
                // ==========================================
                <div class="group relative flex items-start gap-1 px-2"
                     style=move || format!("padding-top: {0}px; padding-bottom: {0}px; font-size: {1}px;",
                         signals.config.message_spacing.get().saturating_sub(2),
                         signals.config.font_size.get().saturating_sub(1).max(10))>
                    <div class=format!("min-w-0 px-2.5 py-1 leading-snug {TEXT_BOX}")>
                        <span
                            class=move || {
                                let color = if filtered_to_sender() { "text-success underline" } else { sig.with(|m| box_name(m.channel).0) };
                                format!("font-semibold cursor-pointer hover:underline {color}")
                            }
                            on:click=toggle_name
                        >{move || sig.with(|m| m.nickname.clone())}</span>
                        <span class="text-white/40 mr-1">":"</span>
                        <span class=text_class on:click=toggle_text on:mousedown=note_pointer_down>
                        {move || {
                            let msg = sig.get();
                            let kw = signals.config.emphasis_keywords.get();
                            let mode = mode();
                            if msg.is_blocked {
                                view! { <span class="italic text-white/60">"(차단된 사용자의 메시지)"</span> }.into_any()
                            } else if mode == TranslationView::Study {
                                let translated = msg.translated.clone();
                                view! {
                                    <span>{render_original(&msg.message, ruby.get(), &kw)}</span>
                                    {translated.map(|text| view! {
                                        <div class=format!("text-[0.85em] {ORIGINAL_TEXT} {HOVER_ONLY}")>
                                            {render_emphasized(&text, &kw)}
                                        </div>
                                    })}
                                }.into_any()
                            } else if let Some(text) = msg.translated.clone().filter(|_| shows_translation(mode)) {
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
                        </span>
                    </div>
                    {name_menu()}
                    {text_menu()}
                </div>
            </Show>
        </Show>
    }
}

/// A message's original: with furigana once the backend has answered, as
/// plain text until then (and for a line without Japanese).
fn render_original(text: &str, spans: Option<Vec<RubySpan>>, keywords: &[String]) -> AnyView {
    match spans {
        Some(spans) => render_ruby(&spans, keywords),
        None => render_emphasized(text, keywords).into_any(),
    }
}

/// Spans as `<ruby>` (reading over the kanji) and plain text, the emphasis
/// keywords marked the way `render_emphasized` marks them.
fn render_ruby(spans: &[RubySpan], keywords: &[String]) -> AnyView {
    mark(spans, keywords)
        .into_iter()
        .map(|piece| {
            let class = if piece.emphasized {
                "text-warning font-black mx-0.5"
            } else {
                ""
            };
            match piece.reading {
                Some(reading) => view! {
                    <ruby class=class>{piece.text}<rt data-reading=reading></rt></ruby>
                }
                .into_any(),
                None => view! { <span class=class>{piece.text}</span> }.into_any(),
            }
        })
        .collect_view()
        .into_any()
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
