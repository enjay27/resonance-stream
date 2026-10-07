//! One chat row: the sender, the text (translated, original, or the Study view's furigana) in the
//! normal or the compact layout, and the menus a click opens. The menus are in `menus`, the
//! furigana and the drawing of keywords in `ruby`.

mod menus;
mod ruby;

use self::menus::{MessageMenu, SenderMenu};
use self::ruby::{ask_ruby, cached, render_emphasized, render_original};
use crate::chat_view::{compact_original_class, relative_time, translation_pending};
use crate::readability::{
    box_name, channel_colors, needs_backing, row_palette, ORIGINAL_TEXT, TEXT_BOX,
};
use crate::store::AppSignals;
use crate::translation_view::{effective, shows_translation, HOVER_ONLY};
use crate::ui_types::{ChatMessage, RubySpan, TranslationView};
use crate::use_context;
use crate::utils::{format_time, is_japanese};
use crate::view_signals::{MenuKind, RowMenu};
use leptos::prelude::*;
use leptos::{component, view, IntoView};

#[component]
pub fn ChatRow(sig: ArcRwSignal<ChatMessage>) -> impl IntoView {
    // Owned by this row: freed when it unmounts. The store's Arc copy is
    // what events update.
    let sig = RwSignal::from(sig);
    let signals = use_context::<AppSignals>().expect("AppSignals missing");

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

    let channel_colors = move || sig.with(|m| channel_colors(m.channel));

    let display_time = move || {
        let raw_ts = sig.with(|m| m.timestamp);
        if signals.config.use_relative_time.get() {
            relative_time(raw_ts, signals.chat.current_time.get())
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
        if let Some(spans) = cached(&text) {
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
                                <SenderMenu sig=sig open=name_open pos=menu_pos />
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
                            <MessageMenu sig=sig open=text_open pos=menu_pos done=done set_done=set_done selection=selection />
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
                    <SenderMenu sig=sig open=name_open pos=menu_pos />
                    <MessageMenu sig=sig open=text_open pos=menu_pos done=done set_done=set_done selection=selection />
                </div>
            </Show>
        </Show>
    }
}
