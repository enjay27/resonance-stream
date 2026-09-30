//! Favorite messages: copy one to the clipboard, or give it a global shortcut
//! that pastes it into the game's chat box (the backend does the pasting).

use crate::shortcut_keys::{
    accelerator_from_event, display, find_conflict, tab_switch_accelerator, Conflict, Rejected,
};
use crate::store::{AppActions, AppSignals};
use crate::ui_types::FavoriteMessage;
use crate::utils::copy_to_clipboard;
use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn FavoritesModal() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");
    let AppSignals {
        favorite_messages,
        set_favorite_messages,
        show_favorites,
        set_show_favorites,
        tab_switch_modifier,
        tab_switch_key,
        ..
    } = signals;

    // Index being edited; `len()` means a new entry not in the list yet.
    let (editing, set_editing) = signal(None::<usize>);
    let (edit_text, set_edit_text) = signal(String::new());
    let (edit_shortcut, set_edit_shortcut) = signal(String::new());
    let (recording, set_recording) = signal(false);
    let (edit_error, set_edit_error) = signal(None::<String>);
    let (copied, set_copied) = signal(None::<usize>);
    let (pending_delete, set_pending_delete) = signal(None::<usize>);

    let start_edit = move |index: usize| {
        let fav = favorite_messages
            .get_untracked()
            .get(index)
            .cloned()
            .unwrap_or_default();
        set_edit_text.set(fav.text);
        set_edit_shortcut.set(fav.shortcut);
        set_edit_error.set(None);
        set_recording.set(false);
        set_pending_delete.set(None);
        set_editing.set(Some(index));
    };

    let cancel_edit = move || {
        set_editing.set(None);
        set_recording.set(false);
        set_edit_error.set(None);
    };

    let commit_edit = move || {
        let Some(index) = editing.get_untracked() else {
            return;
        };
        let text = edit_text.get_untracked().trim().to_string();
        if text.is_empty() {
            set_edit_error.set(Some("메시지를 입력하세요.".to_string()));
            return;
        }
        let entry = FavoriteMessage {
            text,
            shortcut: edit_shortcut.get_untracked(),
        };
        set_favorite_messages.update(|list| match list.get_mut(index) {
            Some(slot) => *slot = entry,
            None => list.push(entry),
        });
        cancel_edit();
        // The backend re-registers the shortcuts when the saved list changes.
        actions.save_config.dispatch(());
    };

    let delete = move |index: usize| {
        set_favorite_messages.update(|list| {
            if index < list.len() {
                list.remove(index);
            }
        });
        set_pending_delete.set(None);
        cancel_edit();
        actions.save_config.dispatch(());
    };

    let copy = move |index: usize, text: String| {
        copy_to_clipboard(&text);
        set_copied.set(Some(index));
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(1200).await;
            if copied.get_untracked() == Some(index) {
                set_copied.set(None);
            }
        });
    };

    let on_record_key = move |ev: KeyboardEvent| {
        if !recording.get_untracked() {
            return;
        }
        ev.prevent_default();
        ev.stop_propagation();
        if ev.key() == "Escape" {
            set_recording.set(false);
            return;
        }
        let ctrl = ev.ctrl_key() || ev.meta_key();
        match accelerator_from_event(&ev.code(), ctrl, ev.alt_key(), ev.shift_key()) {
            Ok(accelerator) => {
                let own = editing.get_untracked().unwrap_or(usize::MAX);
                let others: Vec<String> = favorite_messages
                    .get_untracked()
                    .into_iter()
                    .map(|f| f.shortcut)
                    .collect();
                let tab = tab_switch_accelerator(
                    &tab_switch_modifier.get_untracked(),
                    &tab_switch_key.get_untracked(),
                );
                match find_conflict(&accelerator, own, &others, tab.as_deref()) {
                    Some(Conflict::TabSwitch) => set_edit_error.set(Some(format!(
                        "{}: 탭 전환 단축키와 겹칩니다.",
                        display(&accelerator)
                    ))),
                    Some(Conflict::Favorite(i)) => set_edit_error.set(Some(format!(
                        "{}: {}번 메시지가 이미 사용 중입니다.",
                        display(&accelerator),
                        i + 1
                    ))),
                    None => {
                        set_edit_shortcut.set(accelerator);
                        set_edit_error.set(None);
                        set_recording.set(false);
                    }
                }
            }
            Err(Rejected::ModifierOnly) => {}
            Err(Rejected::NeedsModifier) => set_edit_error.set(Some(
                "Ctrl / Alt / Shift 와 함께 누르세요. (F1~F12는 단독 사용 가능)".to_string(),
            )),
            Err(Rejected::Unsupported) => {
                set_edit_error.set(Some("지원하지 않는 키입니다.".to_string()))
            }
        }
    };

    let editor = move || {
        view! {
            <div class="flex flex-col gap-2 p-3 rounded-lg bg-base-100 border border-success/40">
                <textarea
                    class="textarea textarea-bordered textarea-sm w-full min-h-16 text-sm"
                    placeholder="메시지"
                    prop:value=move || edit_text.get()
                    on:input=move |ev| set_edit_text.set(event_target_value(&ev))
                ></textarea>
                <div class="flex items-center gap-1">
                    <span class="text-[10px] font-bold text-base-content/70 shrink-0">"단축키"</span>
                    <button
                        class="btn btn-outline btn-xs flex-1 font-bold focus:border-success focus:text-success focus:bg-success/10"
                        on:click=move |_| set_recording.set(true)
                        on:blur=move |_| set_recording.set(false)
                        on:keydown=on_record_key
                    >
                        {move || {
                            if recording.get() {
                                "키 조합을 누르세요... (Esc 취소)".to_string()
                            } else {
                                let s = edit_shortcut.get();
                                if s.is_empty() { "지정되지 않음".to_string() } else { display(&s) }
                            }
                        }}
                    </button>
                    <div class="tooltip tooltip-top" data-tip="단축키 해제">
                        <button
                            class="btn btn-outline btn-xs btn-error w-7 p-0 font-black"
                            on:click=move |_| {
                                set_edit_shortcut.set(String::new());
                                set_recording.set(false);
                                set_edit_error.set(None);
                            }
                        >
                            "✕"
                        </button>
                    </div>
                </div>
                {move || edit_error.get().map(|e| view! {
                    <div class="text-[10px] text-error bg-error/10 p-1.5 rounded">{e}</div>
                })}
                <div class="flex justify-end gap-1">
                    <button class="btn btn-ghost btn-xs" on:click=move |_| cancel_edit()>"취소"</button>
                    <button class="btn btn-success btn-xs" on:click=move |_| commit_edit()>"저장"</button>
                </div>
            </div>
        }
    };

    view! {
        <Show when=move || show_favorites.get()>
            <div class="modal modal-open backdrop-blur-sm z-[30000]">
                <div class="modal-box bg-base-300 border border-base-content/10 w-11/12 max-w-lg p-0 overflow-hidden shadow-2xl flex flex-col max-h-[80vh]">

                    <div class="flex items-center justify-between p-3 border-b border-base-content/5 bg-base-200">
                        <h2 class="text-sm font-black tracking-widest text-base-content">"자주 쓰는 메시지"</h2>
                        <button class="btn btn-ghost btn-xs text-xl"
                            on:click=move |_| set_show_favorites.set(false)>"✕"</button>
                    </div>

                    <div class="text-[10px] text-base-content/60 px-3 pt-2">
                        "📋 클립보드로 복사 · 단축키: 게임 채팅창이 열린 상태에서 누르면 메시지를 붙여넣습니다."
                    </div>

                    <div class="flex-1 overflow-y-auto custom-scrollbar p-3 flex flex-col gap-2">
                        <For
                            each={move || favorite_messages.get().into_iter().enumerate().collect::<Vec<_>>()}
                            key=|(i, f)| (*i, f.text.clone(), f.shortcut.clone())
                            children=move |(index, fav)| {
                                let text = fav.text.clone();
                                let shortcut = fav.shortcut.clone();
                                view! {
                                    <Show
                                        when=move || editing.get() == Some(index)
                                        fallback=move || {
                                            let text = text.clone();
                                            let copy_text = text.clone();
                                            let shortcut = shortcut.clone();
                                            view! {
                                                <div class="flex items-start gap-2 p-2 rounded-lg bg-base-200 border border-base-content/5">
                                                    <div class="flex-1 min-w-0">
                                                        <div class="text-sm whitespace-pre-wrap break-words select-text">{text}</div>
                                                        {(!shortcut.is_empty()).then(|| view! {
                                                            <span class="badge badge-ghost badge-xs font-mono mt-1">{display(&shortcut)}</span>
                                                        })}
                                                    </div>
                                                    <div class="flex gap-0.5 shrink-0">
                                                        <div class="tooltip tooltip-left" data-tip="복사">
                                                            <button class="btn btn-ghost btn-xs"
                                                                class:text-success=move || copied.get() == Some(index)
                                                                on:click=move |_| copy(index, copy_text.clone())>
                                                                {move || if copied.get() == Some(index) { "✓" } else { "📋" }}
                                                            </button>
                                                        </div>
                                                        <div class="tooltip tooltip-left" data-tip="편집">
                                                            <button class="btn btn-ghost btn-xs"
                                                                on:click=move |_| start_edit(index)>"✏️"</button>
                                                        </div>
                                                        <div class="tooltip tooltip-left" data-tip="삭제">
                                                            <button class="btn btn-ghost btn-xs"
                                                                class:text-error=move || pending_delete.get() == Some(index)
                                                                on:click=move |_| {
                                                                    if pending_delete.get_untracked() == Some(index) {
                                                                        delete(index);
                                                                    } else {
                                                                        set_pending_delete.set(Some(index));
                                                                    }
                                                                }>
                                                                {move || if pending_delete.get() == Some(index) { "삭제?" } else { "🗑" }}
                                                            </button>
                                                        </div>
                                                    </div>
                                                </div>
                                            }
                                        }
                                    >
                                        {editor()}
                                    </Show>
                                }
                            }
                        />

                        <Show
                            when=move || editing.get() == Some(favorite_messages.with(|l| l.len()))
                            fallback=move || view! {
                                <button class="btn btn-outline btn-sm btn-block border-dashed"
                                    on:click=move |_| start_edit(favorite_messages.with_untracked(|l| l.len()))>
                                    "+ 추가"
                                </button>
                            }
                        >
                            {editor()}
                        </Show>
                    </div>
                </div>
                <div class="modal-backdrop" on:click=move |_| set_show_favorites.set(false)></div>
            </div>
        </Show>
    }
}
