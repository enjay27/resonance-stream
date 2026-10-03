//! Favorite messages as a table, one row per message: a global shortcut that
//! pastes it into the game's chat box (the backend does the pasting), the
//! message (Japanese) over its reminder (Korean), copy and delete. Every box is
//! edited in place -- click it, type, press Enter or click away.

use crate::config_signals::ConfigSignals;
use crate::favorites::{
    add_tab, delete_tab, fill_with_defaults, indices_in_tab, locate, new_message, set_field,
    tab_summary, Field, DEFAULT_TAB, DEFAULT_TAB_LABEL, MAX_TAB_NAME_CHARS,
};
use crate::favorites_sync;
use crate::shortcut_keys::{
    accelerator_from_event, display, find_conflict, tab_switch_accelerator, Conflict, Rejected,
};
use crate::store::AppSignals;
use crate::ui_types::default_favorite_messages;
use crate::utils::copy_to_clipboard;
use leptos::ev::KeyboardEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// The box that is recording a shortcut.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// A message's, by its place in the whole list.
    Row(usize),
    /// The blank row's.
    Draft,
}

/// The message box and the reminder box: plain text until pointed at.
const BOX_TEXT: &str = "w-full h-6 px-1.5 rounded-md bg-transparent text-sm border border-transparent hover:border-base-content/20 focus:border-success focus:bg-base-200 outline-none truncate";
const BOX_NOTE: &str = "w-full h-5 px-1.5 rounded-md bg-transparent text-[11px] text-base-content/60 border border-transparent hover:border-base-content/20 focus:border-success focus:bg-base-200 focus:text-base-content outline-none truncate";

/// What a box holds now, read without tracking (an event handler).
fn value_of_untracked(
    list: ReadSignal<Vec<crate::ui_types::FavoriteMessage>>,
    index: usize,
    field: Field,
) -> String {
    list.with_untracked(|l| {
        l.get(index)
            .map(|f| match field {
                Field::Text => f.text.clone(),
                Field::Note => f.note.clone(),
                Field::Shortcut => f.shortcut.clone(),
            })
            .unwrap_or_default()
    })
}

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

    // The box recording a shortcut: a row's (by its place in the whole list --
    // the key of the table's rows) or the blank row's.
    let (recording, set_recording) = signal(None::<Slot>);
    let (error, set_error) = signal(None::<String>);
    let (copied, set_copied) = signal(None::<usize>);
    let (pending_delete, set_pending_delete) = signal(None::<usize>);
    // The blank row at the end: it becomes a message once it has text.
    let (draft_text, set_draft_text) = signal(String::new());
    let (draft_note, set_draft_note) = signal(String::new());
    let (draft_shortcut, set_draft_shortcut) = signal(String::new());

    let reset_boxes = move || {
        set_recording.set(None);
        set_error.set(None);
        set_pending_delete.set(None);
    };

    // What a box holds now (tracked: a box follows the list).
    let value_of = move |index: usize, field: Field| {
        favorite_messages.with(|list| {
            list.get(index)
                .map(|f| match field {
                    Field::Text => f.text.clone(),
                    Field::Note => f.note.clone(),
                    Field::Shortcut => f.shortcut.clone(),
                })
                .unwrap_or_default()
        })
    };

    // A box was edited: keep it, and save when it changed. The backend
    // re-registers the shortcuts when the saved list changes.
    let edit = move |index: usize, field: Field, value: &str| {
        let mut result = Ok(false);
        set_favorite_messages.update(|list| result = set_field(list, index, field, value));
        match result {
            Ok(true) => {
                set_error.set(None);
                favorites_sync::save(signals.config);
            }
            Ok(false) => {}
            Err(e) => set_error.set(Some(e.message().to_string())),
        }
    };

    // The blank row got its text: it joins the tab, and a new blank row follows.
    let add_from_draft = move || match new_message(
        &active_tab.get_untracked(),
        &draft_text.get_untracked(),
        &draft_note.get_untracked(),
        &draft_shortcut.get_untracked(),
    ) {
        Ok(message) => {
            set_favorite_messages.update(|list| list.push(message));
            set_draft_text.set(String::new());
            set_draft_note.set(String::new());
            set_draft_shortcut.set(String::new());
            set_error.set(None);
            favorites_sync::save(signals.config);
        }
        Err(_) => set_draft_text.set(String::new()),
    };

    let delete = move |index: usize| {
        set_favorite_messages.update(|list| {
            if index < list.len() {
                list.remove(index);
            }
        });
        set_pending_delete.set(None);
        favorites_sync::save(signals.config);
    };

    let switch_tab = move |name: String| {
        reset_boxes();
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

    // A shortcut box has the focus and is waiting for keys: Esc cancels,
    // Backspace / Delete clears it, a combination sets it unless it is taken.
    let on_record_key = move |slot: Slot, ev: KeyboardEvent| {
        if recording.get_untracked() != Some(slot) {
            return;
        }
        ev.prevent_default();
        ev.stop_propagation();
        let key = ev.key();
        if key == "Escape" {
            set_recording.set(None);
            return;
        }
        let apply = move |accelerator: &str| {
            match slot {
                Slot::Row(index) => edit(index, Field::Shortcut, accelerator),
                Slot::Draft => set_draft_shortcut.set(accelerator.to_string()),
            }
            set_recording.set(None);
        };
        if key == "Backspace" || key == "Delete" {
            set_error.set(None);
            apply("");
            return;
        }
        let ctrl = ev.ctrl_key() || ev.meta_key();
        match accelerator_from_event(&ev.code(), ctrl, ev.alt_key(), ev.shift_key()) {
            Ok(accelerator) => {
                let own = match slot {
                    Slot::Row(index) => index,
                    Slot::Draft => usize::MAX,
                };
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
                    Some(Conflict::TabSwitch) => set_error.set(Some(format!(
                        "{}: 탭 전환 단축키와 겹칩니다.",
                        display(&accelerator)
                    ))),
                    Some(Conflict::Favorite(i)) => {
                        let (tab, place) = favorite_messages
                            .with_untracked(|list| locate(list, i))
                            .unwrap_or_default();
                        set_error.set(Some(format!(
                            "{}: [{}] {}번 메시지가 이미 사용 중입니다.",
                            display(&accelerator),
                            if tab.is_empty() {
                                DEFAULT_TAB_LABEL
                            } else {
                                &tab
                            },
                            place
                        )))
                    }
                    None => {
                        set_error.set(None);
                        apply(&accelerator);
                    }
                }
            }
            Err(Rejected::ModifierOnly) => {}
            Err(Rejected::NeedsModifier) => set_error.set(Some(
                "Ctrl / Alt / Shift 와 함께 누르세요. (F1~F12는 단독 사용 가능)".to_string(),
            )),
            Err(Rejected::Unsupported) => {
                set_error.set(Some("지원하지 않는 키입니다.".to_string()))
            }
        }
    };

    // The shortcut box of a row (or of the blank row).
    let shortcut_box = move |slot: Slot| {
        let held = move || match slot {
            Slot::Row(index) => value_of(index, Field::Shortcut),
            Slot::Draft => draft_shortcut.get(),
        };
        view! {
            <button
                class=move || format!(
                    "w-full h-7 px-1 rounded-md text-[11px] font-mono border border-transparent hover:border-base-content/20 hover:bg-base-content/5 focus:border-success focus:bg-success/10 focus:text-success transition-colors truncate {}",
                    if held().is_empty() { "text-base-content/40" } else { "text-base-content" }
                )
                title="누르고 단축키 조합을 입력하세요 (Esc 취소 · Backspace 해제)"
                on:click=move |_| { set_error.set(None); set_recording.set(Some(slot)); }
                on:blur=move |_| if recording.get_untracked() == Some(slot) { set_recording.set(None) }
                on:keydown=move |ev| on_record_key(slot, ev)
            >
                {move || {
                    if recording.get() == Some(slot) {
                        "키를 누르세요...".to_string()
                    } else {
                        let held = held();
                        if held.is_empty() { "단축키 미지정".to_string() } else { display(&held) }
                    }
                }}
            </button>
        }
    };

    // A message or reminder box of a row: click to edit, Enter or click away
    // to keep, Esc to undo.
    let text_box = move |index: usize,
                         field: Field,
                         class: &'static str,
                         placeholder: &'static str| {
        view! {
            <input type="text" class=class placeholder=placeholder
                prop:value=move || value_of(index, field)
                on:change=move |ev| {
                    let input = event_target::<web_sys::HtmlInputElement>(&ev);
                    edit(index, field, &input.value());
                    // What was kept (trimmed), or the old text when it was refused.
                    input.set_value(&value_of_untracked(favorite_messages, index, field));
                }
                on:keydown=move |ev| {
                    let key = ev.key();
                    if key == "Enter" || key == "Escape" {
                        let input = event_target::<web_sys::HtmlInputElement>(&ev);
                        if key == "Escape" {
                            input.set_value(&value_of_untracked(favorite_messages, index, field));
                        }
                        let _ = input.blur();
                    }
                }
            />
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
                "칸을 눌러 고치기 (Enter 저장 · Esc 취소) · 단축키: 게임 채팅창에서 누르면 붙여넣기"
            </div>
            {move || error.get().map(|e| view! {
                <div class="mx-3 mt-2 text-[10px] text-error bg-error/10 p-1.5 rounded">{e}</div>
            })}

            <div class="flex-1 min-h-0 overflow-y-auto custom-scrollbar px-3 pb-3 pt-1">
                <table class="w-full table-fixed border-collapse text-left">
                    <colgroup>
                        <col class="w-[7.5rem]" />
                        <col />
                        <col class="w-[4.5rem]" />
                    </colgroup>
                    <thead class="sticky top-0 z-10 bg-base-100 text-[10px] text-base-content/50">
                        <tr>
                            <th class="font-normal px-1 py-1">"단축키"</th>
                            <th class="font-normal px-1 py-1">"메시지 (일본어) / 메모 (한국어)"</th>
                            <th></th>
                        </tr>
                    </thead>
                    // One body per message: its two lines are one row.
                    <For
                        each={move || favorite_messages.with(|l| indices_in_tab(l, &active_tab.get()))}
                        key=|index| *index
                        children=move |index| view! {
                            <tbody class="border-b border-base-content/5">
                                <tr>
                                    <td rowspan="2" class="align-middle px-0.5">{shortcut_box(Slot::Row(index))}</td>
                                    <td class="pt-0.5 px-0.5">
                                        {text_box(index, Field::Text, BOX_TEXT, "메시지 (일본어)")}
                                    </td>
                                    <td rowspan="2" class="align-middle text-right whitespace-nowrap">
                                        <button class="btn btn-ghost btn-xs px-1" title="복사"
                                            class:text-success=move || copied.get() == Some(index)
                                            on:click=move |_| copy(index, value_of(index, Field::Text))>
                                            {move || if copied.get() == Some(index) { "✓" } else { "📋" }}
                                        </button>
                                        <button class="btn btn-ghost btn-xs px-1" title="삭제 (두 번 누르면 삭제)"
                                            class:text-error=move || pending_delete.get() == Some(index)
                                            on:click=move |_| {
                                                if pending_delete.get_untracked() == Some(index) {
                                                    delete(index);
                                                } else {
                                                    set_pending_delete.set(Some(index));
                                                }
                                            }
                                            on:blur=move |_| if pending_delete.get_untracked() == Some(index) { set_pending_delete.set(None) }>
                                            {move || if pending_delete.get() == Some(index) { "삭제?" } else { "🗑" }}
                                        </button>
                                    </td>
                                </tr>
                                <tr>
                                    <td class="pb-0.5 px-0.5">
                                        {text_box(index, Field::Note, BOX_NOTE, "메모 (한국어) -- 선택")}
                                    </td>
                                </tr>
                            </tbody>
                        }
                    />
                    // The blank row: type a message and press Enter to add it.
                    <tbody>
                        <tr>
                            <td rowspan="2" class="align-middle px-0.5">{shortcut_box(Slot::Draft)}</td>
                            <td class="pt-0.5 px-0.5">
                                <input type="text" class=BOX_TEXT placeholder="+ 새 메시지 (일본어) -- Enter로 추가"
                                    prop:value=move || draft_text.get()
                                    on:input=move |ev| set_draft_text.set(event_target_value(&ev))
                                    on:change=move |_| add_from_draft()
                                    on:keydown=move |ev| if ev.key() == "Enter" {
                                        let _ = event_target::<web_sys::HtmlInputElement>(&ev).blur();
                                    }
                                />
                            </td>
                            <td rowspan="2"></td>
                        </tr>
                        <tr>
                            <td class="pb-0.5 px-0.5">
                                <input type="text" class=BOX_NOTE placeholder="메모 (한국어) -- 선택"
                                    prop:value=move || draft_note.get()
                                    on:input=move |ev| set_draft_note.set(event_target_value(&ev))
                                />
                            </td>
                        </tr>
                    </tbody>
                </table>
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
