//! Favorite messages: copy one to the clipboard, or give it a global shortcut
//! that pastes it into the game's chat box (the backend does the pasting).

use crate::config_signals::ConfigSignals;
use crate::favorites::{
    add_tab, delete_tab, fill_with_defaults, locate, tab_summary, DEFAULT_TAB, DEFAULT_TAB_LABEL,
    MAX_TAB_NAME_CHARS,
};
use crate::favorites_sync;
use crate::shortcut_keys::{
    accelerator_from_event, display, find_conflict, tab_switch_accelerator, Conflict, Rejected,
};
use crate::store::AppSignals;
use crate::ui_types::{default_favorite_messages, FavoriteMessage};
use crate::utils::copy_to_clipboard;
use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn FavoritesWindow() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let ConfigSignals {
        favorite_messages,
        set_favorite_messages,
        favorite_tabs,
        set_favorite_tabs,
        tab_switch_modifier,
        tab_switch_key,
        ..
    } = signals.config;

    // The tab on show (its stored name; empty is the default tab) and the two
    // questions the strip can ask: add a tab, or confirm deleting one.
    let (active_tab, set_active_tab) = signal(DEFAULT_TAB.to_string());
    let (adding_tab, set_adding_tab) = signal(false);
    let (new_tab_name, set_new_tab_name) = signal(String::new());
    let (new_tab_error, set_new_tab_error) = signal(None::<&'static str>);
    let (deleting_tab, set_deleting_tab) = signal(None::<String>);

    // Index being edited; `len()` means a new entry not in the list yet.
    let (editing, set_editing) = signal(None::<usize>);
    let (edit_text, set_edit_text) = signal(String::new());
    let (edit_note, set_edit_note) = signal(String::new());
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
        set_edit_note.set(fav.note);
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
        let mut entry = FavoriteMessage {
            text,
            note: edit_note.get_untracked().trim().to_string(),
            shortcut: edit_shortcut.get_untracked(),
            tab: active_tab.get_untracked(),
        };
        set_favorite_messages.update(|list| match list.get_mut(index) {
            Some(slot) => {
                entry.tab = slot.tab.clone();
                *slot = entry;
            }
            None => list.push(entry),
        });
        cancel_edit();
        // The backend re-registers the shortcuts when the saved list changes.
        favorites_sync::save(signals.config);
    };

    let delete = move |index: usize| {
        set_favorite_messages.update(|list| {
            if index < list.len() {
                list.remove(index);
            }
        });
        set_pending_delete.set(None);
        cancel_edit();
        favorites_sync::save(signals.config);
    };

    let switch_tab = move |name: String| {
        cancel_edit();
        set_pending_delete.set(None);
        set_active_tab.set(name);
    };

    let open_add_tab = move || {
        set_new_tab_name.set(String::new());
        set_new_tab_error.set(None);
        set_adding_tab.set(true);
    };

    // `fill`: also file the default messages under the new tab.
    let confirm_add_tab = move |fill: bool| {
        let mut tabs = favorite_tabs.get_untracked();
        match add_tab(&mut tabs, &new_tab_name.get_untracked()) {
            Ok(name) => {
                set_favorite_tabs.set(tabs);
                if fill {
                    set_favorite_messages.update(|list| {
                        fill_with_defaults(list, &name);
                    });
                }
                switch_tab(name);
                set_adding_tab.set(false);
                favorites_sync::save(signals.config);
            }
            Err(e) => set_new_tab_error.set(Some(e.message())),
        }
    };

    let confirm_delete_tab = move |name: String| {
        let mut tabs = favorite_tabs.get_untracked();
        let mut list = favorite_messages.get_untracked();
        delete_tab(&mut tabs, &mut list, &name);
        set_favorite_tabs.set(tabs);
        set_favorite_messages.set(list);
        set_deleting_tab.set(None);
        switch_tab(DEFAULT_TAB.to_string());
        favorites_sync::save(signals.config);
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
                    tab_switch_modifier.get_untracked(),
                    &tab_switch_key.get_untracked(),
                );
                match find_conflict(&accelerator, own, &others, tab.as_deref()) {
                    Some(Conflict::TabSwitch) => set_edit_error.set(Some(format!(
                        "{}: 탭 전환 단축키와 겹칩니다.",
                        display(&accelerator)
                    ))),
                    Some(Conflict::Favorite(i)) => {
                        let (tab, place) = favorite_messages
                            .with_untracked(|list| locate(list, i))
                            .unwrap_or_default();
                        set_edit_error.set(Some(format!(
                            "{}: [{}] {}번 메시지가 이미 사용 중입니다.",
                            display(&accelerator),
                            tab,
                            place
                        )))
                    }
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
                <input
                    type="text"
                    class="input input-bordered input-xs w-full"
                    placeholder="메모 (예: 뜻) -- 선택"
                    prop:value=move || edit_note.get()
                    on:input=move |ev| set_edit_note.set(event_target_value(&ev))
                />
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
        <div class="relative h-full flex flex-col bg-base-100 text-base-content overflow-hidden">
            // --- TABS: the default tab, the user's, and "+" ---
            <div class="flex items-center gap-1 px-3 pt-2 overflow-x-auto custom-scrollbar" role="tablist">
                {move || {
                    let mut names = vec![DEFAULT_TAB.to_string()];
                    names.extend(favorite_tabs.get());
                    names.into_iter().map(|name| {
                        let is_active = {
                            let name = name.clone();
                            move || active_tab.get() == name
                        };
                        let label = if name == DEFAULT_TAB { DEFAULT_TAB_LABEL.to_string() } else { name.clone() };
                        let removable = name != DEFAULT_TAB;
                        let pick = name.clone();
                        let ask_delete = name.clone();
                        let is_active_btn = is_active.clone();
                        view! {
                            <div class=move || format!(
                                "flex items-center shrink-0 rounded-md text-xs whitespace-nowrap transition-colors {}",
                                if is_active() { "bg-base-100 shadow-sm font-bold text-base-content" } else { "text-base-content/60 hover:text-base-content hover:bg-base-content/5" }
                            )>
                                <button class="h-7 px-2.5" role="tab"
                                    aria-selected=move || is_active_btn().to_string()
                                    on:click=move |_| switch_tab(pick.clone())>
                                    {label}
                                </button>
                                <Show when={
                                    let is_active = is_active.clone();
                                    move || removable && is_active()
                                }>
                                    <button class="h-7 pr-2 text-base-content/50 hover:text-error" title="탭 삭제"
                                        on:click={
                                            let ask_delete = ask_delete.clone();
                                            move |_| set_deleting_tab.set(Some(ask_delete.clone()))
                                        }>
                                        "✕"
                                    </button>
                                </Show>
                            </div>
                        }
                    }).collect_view()
                }}
                <button class="btn btn-ghost btn-xs btn-square shrink-0" title="탭 추가"
                    on:click=move |_| open_add_tab()>
                    "＋"
                </button>
            </div>

            <div class="text-[10px] text-base-content/60 px-3 pt-2">
                "📋 클립보드로 복사 · 단축키: 게임 채팅창이 열린 상태에서 누르면 메시지를 붙여넣습니다."
            </div>

            <div class="flex-1 overflow-y-auto custom-scrollbar p-3 flex flex-col gap-2">
                <For
                    each={move || {
                        let tab = active_tab.get();
                        favorite_messages.get().into_iter().enumerate()
                            .filter(|(_, f)| f.tab == tab)
                            .collect::<Vec<_>>()
                    }}
                    key=|(i, f)| (*i, f.text.clone(), f.note.clone(), f.shortcut.clone(), f.tab.clone())
                    children=move |(index, fav)| {
                        let text = fav.text.clone();
                        let shortcut = fav.shortcut.clone();
                        let note = fav.note.clone();
                        view! {
                            <Show
                                when=move || editing.get() == Some(index)
                                fallback=move || {
                                    let text = text.clone();
                                    let copy_text = text.clone();
                                    let shortcut = shortcut.clone();
                                    let note = note.clone();
                                    view! {
                                        <div class="flex items-start gap-2 p-2 rounded-lg bg-base-200 border border-base-content/5">
                                            <div class="flex-1 min-w-0">
                                                <div class="text-sm whitespace-pre-wrap break-words select-text">{text}</div>
                                                {(!note.is_empty()).then(|| view! {
                                                    <div class="text-[11px] text-base-content/50 break-words">{note}</div>
                                                })}
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

            // --- ASK: new tab name, and whether to fill it with the defaults ---
            <Show when=move || adding_tab.get()>
                <div class="absolute inset-0 z-20 grid place-items-center bg-base-300/80 p-4">
                    <div class="w-full max-w-xs flex flex-col gap-2 p-3 rounded-lg bg-base-200 border border-base-content/10 shadow-xl">
                        <h3 class="text-sm font-black">"새 탭"</h3>
                        <input type="text" class="input input-bordered input-sm w-full" placeholder="탭 이름"
                            maxlength=MAX_TAB_NAME_CHARS.to_string()
                            prop:value=move || new_tab_name.get()
                            on:input=move |ev| { set_new_tab_name.set(event_target_value(&ev)); set_new_tab_error.set(None); }
                            on:keydown=move |ev| match ev.key().as_str() {
                                "Enter" => confirm_add_tab(false),
                                "Escape" => set_adding_tab.set(false),
                                _ => {}
                            }
                        />
                        {move || new_tab_error.get().map(|e| view! {
                            <div class="text-[10px] text-error bg-error/10 p-1.5 rounded">{e}</div>
                        })}
                        <p class="text-[11px] text-base-content/70">
                            {format!("기본 문구 {}개(인사말 등)를 이 탭에 채워 넣을까요?", default_favorite_messages().len())}
                        </p>
                        <div class="flex flex-wrap justify-end gap-1">
                            <button class="btn btn-ghost btn-xs" on:click=move |_| set_adding_tab.set(false)>"취소"</button>
                            <button class="btn btn-outline btn-xs" on:click=move |_| confirm_add_tab(false)>"빈 탭으로 추가"</button>
                            <button class="btn btn-success btn-xs" on:click=move |_| confirm_add_tab(true)>"기본 문구로 채우기"</button>
                        </div>
                    </div>
                </div>
            </Show>

            // --- WARN: deleting a tab deletes its messages ---
            <Show when=move || deleting_tab.get().is_some()>
                {move || {
                    let name = deleting_tab.get().unwrap_or_default();
                    let (count, keyed) = favorite_messages.with(|l| tab_summary(l, &name));
                    let target = name.clone();
                    view! {
                        <div class="absolute inset-0 z-20 grid place-items-center bg-base-300/80 p-4">
                            <div class="w-full max-w-xs flex flex-col gap-2 p-3 rounded-lg bg-base-200 border border-error/40 shadow-xl">
                                <h3 class="text-sm font-black text-error">{format!("'{name}' 탭을 삭제할까요?")}</h3>
                                <p class="text-[11px] text-base-content/80">
                                    {format!("이 탭의 메시지 {count}개가 모두 삭제되며 되돌릴 수 없습니다.")}
                                    {(keyed > 0).then(|| format!(" 지정된 단축키 {keyed}개도 해제됩니다."))}
                                </p>
                                <div class="flex justify-end gap-1">
                                    <button class="btn btn-ghost btn-xs" on:click=move |_| set_deleting_tab.set(None)>"취소"</button>
                                    <button class="btn btn-error btn-xs" on:click=move |_| confirm_delete_tab(target.clone())>"삭제"</button>
                                </div>
                            </div>
                        </div>
                    }
                }}
            </Show>
        </div>
    }
}
