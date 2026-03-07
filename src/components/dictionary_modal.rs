use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::BTreeMap;
use wasm_bindgen::JsValue;
use crate::store::AppSignals;
use crate::tauri_bridge::invoke;

#[component]
pub fn DictionaryModal() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");

    let (dict, set_dict) = signal(BTreeMap::<String, BTreeMap<String, String>>::new());
    let (active_category, set_active_category) = signal("chat".to_string());
    let (version, set_version) = signal("0.0.0".to_string());

    // Signals for adding a new word
    let (new_key, set_new_key) = signal(String::new());
    let (new_val, set_new_val) = signal(String::new());

    // Signals for the Inline Editing feature
    // editing_target: Option<(Category, OriginalKey, "key" | "value")>
    let (editing_target, set_editing_target) = signal(None::<(String, String, String)>);
    let (edit_value, set_edit_value) = signal(String::new());

    // Fetch the dictionary data when the modal opens
    Effect::new(move |_| {
        if signals.show_dictionary.get() {
            spawn_local(async move {
                if let Ok(v) = invoke("get_dict_version", JsValue::NULL).await {
                    if let Some(v_str) = v.as_string() {
                        set_version.set(v_str);
                    }
                }

                if let Ok(json_val) = invoke("get_local_dictionary", JsValue::NULL).await {
                    if let Some(json_str) = json_val.as_string() {
                        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap_or(serde_json::json!({}));

                        let mut new_dict = BTreeMap::new();

                        if let Some(obj) = parsed.as_object() {
                            for (cat, items) in obj {
                                let mut cat_map = BTreeMap::new();
                                if let Some(items_obj) = items.as_object() {
                                    for (k, val) in items_obj {
                                        if let Some(val_str) = val.as_str() {
                                            cat_map.insert(k.clone(), val_str.to_string());
                                        }
                                    }
                                }
                                new_dict.insert(cat.clone(), cat_map);
                            }
                        }
                        set_dict.set(new_dict);
                    }
                }
            });
        }
    });

    // Save the changes to the disk
    let save_dict = move |_| {
        let current_dict = dict.get_untracked();
        let json_payload = serde_json::to_string_pretty(&current_dict).unwrap();

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&serde_json::json!({
                "content": json_payload
            })).unwrap();
            let _ = invoke("save_local_dictionary", args).await;

            signals.set_restart_required.set(true);
            signals.set_show_dictionary.set(false);
        });
    };

    let delete_word = move |category: String, key: String| {
        set_dict.update(|d| {
            if let Some(cat) = d.get_mut(&category) {
                cat.remove(&key);
            }
        });
    };

    // Commit the inline edit and update the dictionary state
    let commit_edit = move || {
        if let Some((cat, orig_key, field)) = editing_target.get_untracked() {
            let new_text = edit_value.get_untracked().trim().to_string();

            // Clear editing state immediately to prevent blur/enter conflict
            set_editing_target.set(None);

            if new_text.is_empty() {
                return;
            }

            set_dict.update(|d| {
                if let Some(cat_map) = d.get_mut(&cat) {
                    if field == "key" && orig_key != new_text {
                        if let Some(val) = cat_map.remove(&orig_key) {
                            cat_map.insert(new_text, val);
                        }
                    } else if field == "value" {
                        cat_map.insert(orig_key, new_text);
                    }
                }
            });
        }
    };

    view! {
        <Show when=move || signals.show_dictionary.get()>
            <div class="modal modal-open backdrop-blur-sm transition-all duration-300 z-[30000]">
                <div class="modal-box bg-base-300 border border-base-content/10 w-11/12 max-w-4xl p-0 overflow-hidden shadow-2xl flex flex-col h-[80vh] animate-in zoom-in duration-200">

                    // --- HEADER ---
                    <div class="flex items-center justify-between p-4 border-b border-base-content/5 bg-base-200">
                        <div class="flex items-center gap-3">
                            <h2 class="text-sm font-black tracking-widest text-base-content">"사용자 사전 편집기"</h2>
                            <span class="badge badge-info badge-sm font-mono opacity-80">{move || format!("v{}", version.get())}</span>
                        </div>
                        <button class="btn btn-ghost btn-xs text-xl"
                                on:click=move |_| signals.set_show_dictionary.set(false)>"✕"</button>
                    </div>

                    // --- MAIN CONTENT AREA ---
                    <div class="flex flex-1 overflow-hidden">
                        // SIDEBAR: Categories
                        <div class="w-40 bg-base-200/50 border-r border-base-content/5 overflow-y-auto p-2">
                            <ul class="menu menu-xs w-full gap-1">
                                <For
                                    each={move || dict.get().keys().cloned().collect::<Vec<_>>()}
                                    key=|cat| cat.clone()
                                    children=move |cat| {
                                        let cat_clone = cat.clone();
                                        let click_clone = cat.clone();
                                        let display_name = cat.clone().to_uppercase();
                                        view! {
                                            <li>
                                                <a
                                                    class:active=move || active_category.get() == cat_clone
                                                    class="font-bold tracking-widest text-[10px]"
                                                    on:click=move |_| set_active_category.set(click_clone.clone())
                                                >
                                                    {display_name}
                                                </a>
                                            </li>
                                        }
                                    }
                                />
                            </ul>
                        </div>

                        // MAIN PANEL: Words List
                        <div class="flex-1 flex flex-col bg-base-100 overflow-hidden">

                            // Add New Word Row
                            <div class="p-3 bg-base-200 border-b border-base-content/5 flex gap-2">
                                <input type="text" class="input input-xs input-bordered flex-1" placeholder="원문 (예: よろです)"
                                    prop:value=move || new_key.get()
                                    on:input=move |ev| set_new_key.set(event_target_value(&ev)) />
                                <input type="text" class="input input-xs input-bordered flex-1" placeholder="번역 (예: 잘부탁해요)"
                                    prop:value=move || new_val.get()
                                    on:input=move |ev| set_new_val.set(event_target_value(&ev)) />
                                <button class="btn btn-xs btn-success font-bold"
                                    on:click=move |_| {
                                        let k = new_key.get_untracked().trim().to_string();
                                        let v = new_val.get_untracked().trim().to_string();
                                        let cat = active_category.get_untracked();
                                        if !k.is_empty() && !v.is_empty() {
                                            set_dict.update(|d| {
                                                d.entry(cat).or_default().insert(k, v);
                                            });
                                            set_new_key.set(String::new());
                                            set_new_val.set(String::new());
                                        }
                                    }>
                                    "단어 추가"
                                </button>
                            </div>

                            // Word Table
                            <div class="flex-1 overflow-y-auto custom-scrollbar p-0">
                                <table class="table table-xs table-pin-rows table-fixed w-full">
                                    <thead class="bg-base-300">
                                        <tr>
                                            <th>"원문 (키)"</th>
                                            <th>"번역 (값)"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {move || {
                                            let current_cat = active_category.get();
                                            let current_dict = dict.get();

                                            if let Some(words) = current_dict.get(&current_cat) {
                                                words.iter().map(|(k, v)| {
                                                    let key_display = k.clone();
                                                    let val_display = v.clone();
                                                    let cat_del = current_cat.clone();
                                                    let key_del = k.clone();

                                                    // Closures to check if THIS specific row is being edited
                                                    let cat_k = current_cat.clone();
                                                    let orig_k = k.clone();
                                                    let is_editing_key = move || {
                                                        editing_target.get().map(|(c, ok, f)| c == cat_k && ok == orig_k && f == "key").unwrap_or(false)
                                                    };

                                                    let cat_v = current_cat.clone();
                                                    let orig_v = k.clone();
                                                    let is_editing_val = move || {
                                                        editing_target.get().map(|(c, ok, f)| c == cat_v && ok == orig_v && f == "value").unwrap_or(false)
                                                    };

                                                    view! {
                                                        <tr class="hover:bg-base-200/50 transition-colors">

                                                            // --- INLINE EDIT: KEY ---
                                                            <td class="font-mono text-xs max-w-xs truncate cursor-pointer hover:bg-base-content/10"
                                                                title="클릭하여 수정"
                                                                on:click={
                                                                    let c = current_cat.clone();
                                                                    let ok = k.clone();
                                                                    let text = k.clone();
                                                                    move |_| {
                                                                        let target = editing_target.get_untracked();
                                                                        let is_currently_editing = target.as_ref().map(|(cat, o_k, f)| cat == &c && o_k == &ok && f == "key").unwrap_or(false);

                                                                        if !is_currently_editing {
                                                                            set_edit_value.set(text.clone());
                                                                            set_editing_target.set(Some((c.clone(), ok.clone(), "key".to_string())));
                                                                        }
                                                                    }
                                                                }
                                                            >
                                                                <Show when=is_editing_key fallback=move || view! { {key_display.clone()} }>
                                                                    <input type="text" class="input input-xs input-bordered w-full bg-base-100"
                                                                        autofocus
                                                                        prop:value=move || edit_value.get()
                                                                        on:input=move |ev| set_edit_value.set(event_target_value(&ev))
                                                                        on:blur=move |_| commit_edit()
                                                                        on:keydown=move |ev| {
                                                                            if ev.key() == "Enter" { commit_edit(); }
                                                                            else if ev.key() == "Escape" { set_editing_target.set(None); }
                                                                        }
                                                                    />
                                                                </Show>
                                                            </td>

                                                            // --- INLINE EDIT: VALUE ---
                                                            <td class="text-xs max-w-xs truncate cursor-pointer hover:bg-base-content/10"
                                                                title="클릭하여 수정"
                                                                on:click={
                                                                    let c = current_cat.clone();
                                                                    let ok = k.clone();
                                                                    let text = v.clone();
                                                                    move |_| {
                                                                        let target = editing_target.get_untracked();
                                                                        let is_currently_editing = target.as_ref().map(|(cat, o_k, f)| cat == &c && o_k == &ok && f == "value").unwrap_or(false);

                                                                        if !is_currently_editing {
                                                                            set_edit_value.set(text.clone());
                                                                            set_editing_target.set(Some((c.clone(), ok.clone(), "value".to_string())));
                                                                        }
                                                                    }
                                                                }
                                                            >
                                                                <Show when=is_editing_val fallback=move || view! { {val_display.clone()} }>
                                                                    <input type="text" class="input input-xs input-bordered w-full bg-base-100"
                                                                        autofocus
                                                                        prop:value=move || edit_value.get()
                                                                        on:input=move |ev| set_edit_value.set(event_target_value(&ev))
                                                                        on:blur=move |_| commit_edit()
                                                                        on:keydown=move |ev| {
                                                                            if ev.key() == "Enter" { commit_edit(); }
                                                                            else if ev.key() == "Escape" { set_editing_target.set(None); }
                                                                        }
                                                                    />
                                                                </Show>
                                                            </td>

                                                            // --- ACTION: DELETE ---
                                                            <td class="text-center">
                                                                <button class="btn btn-ghost btn-xs text-error hover:bg-error/20"
                                                                    on:click=move |_| delete_word(cat_del.clone(), key_del.clone())>
                                                                    "삭제"
                                                                </button>
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view().into_any()
                                            } else {
                                                view! { <tr><td colspan="3" class="text-center py-4 opacity-50">"비어 있습니다."</td></tr> }.into_any()
                                            }
                                        }}
                                    </tbody>
                                </table>
                            </div>
                        </div>
                    </div>

                    // --- FOOTER ---
                    <div class="bg-base-200 p-3 border-t border-base-content/5 flex justify-between items-center">
                        <span class="text-[10px] text-warning">"⚠️ 서버에서 사전을 동기화하면 수정한 내용이 초기화될 수 있습니다."</span>
                        <div class="flex gap-2">
                            <button class="btn btn-ghost btn-sm" on:click=move |_| signals.set_show_dictionary.set(false)>"취소"</button>
                            <button class="btn btn-success btn-sm font-bold shadow-lg" on:click=save_dict>"수정 사항 저장"</button>
                        </div>
                    </div>

                </div>
                <div class="modal-backdrop bg-black/40" on:click=move |_| signals.set_show_dictionary.set(false)></div>
            </div>
        </Show>
    }
}