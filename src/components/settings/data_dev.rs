use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use crate::ui_types::NetworkInterface;
use leptos::prelude::*;
use leptos::reactive::spawn_local;
use wasm_bindgen::JsValue;

/// Dictionary sync/edit, chat export, app-data folder, debug options.
/// The actions and the interface list are owned by `Settings` so their state
/// (last export result, in-flight sync) survives closing the modal, as before the split.
#[component]
pub fn DataDevSection(
    interfaces: ReadSignal<Vec<NetworkInterface>>,
    sync_dict_action: Action<(), String>,
    save_chat_action: Action<(), String>,
) -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("AppSignals missing");
    let actions = use_context::<AppActions>().expect("AppActions missing");
    let is_syncing = sync_dict_action.pending();
    let is_saving_chat = save_chat_action.pending();

    view! {
        // ==========================================
        // SECTION: DATA & DEVELOPER
        // ==========================================
        <section class="space-y-3">
            <h3 class="text-[10px] font-bold text-warning uppercase tracking-widest opacity-80">
                "데이터 및 개발자 (Data & Dev)"
            </h3>

            <div class="bg-base-200 p-3 rounded-xl border border-base-content/5 space-y-4">
                // Sync Dictionary Option
                <div class="flex items-center justify-between">
                    <div class="flex flex-col">
                        <span class="text-xs font-bold text-base-content/80">"사용자 사전 동기화"</span>
                        <span class="text-[9px] opacity-60">"GitHub에서 최신 단어장을 불러옵니다."</span>
                    </div>
                    <button class="btn btn-xs btn-outline relative"
                        class:btn-success=move || signals.dict_update_available.get()
                        disabled=move || is_syncing.get()
                        on:click=move |_| {
                            sync_dict_action.dispatch(());
                            signals.set_dict_update_available.set(false);
                        }
                    >
                        <Show when=move || signals.dict_update_available.get()>
                            <span class="absolute -top-1 -right-1 flex h-2 w-2">
                              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-success opacity-75"></span>
                              <span class="relative inline-flex rounded-full h-2 w-2 bg-success"></span>
                            </span>
                        </Show>

                        {move || if is_syncing.get() {
                            view! { <span class="loading loading-spinner loading-xs"></span> }.into_any()
                        } else {
                            view! { "업데이트" }.into_any()
                        }}
                    </button>
                </div>

                <div class="divider m-0 opacity-10"></div>

                // Dictionary Modal
                <div class="flex items-center justify-between">
                    <div class="flex flex-col">
                        <span class="text-xs font-bold text-base-content/80">"사용자 사전 편집"</span>
                        <span class="text-[9px] opacity-60">"번역 사전을 확인하고 직접 수정합니다."</span>
                    </div>
                    <button class="btn btn-xs btn-outline"
                        on:click=move |_| {
                            signals.set_show_dictionary.set(true);
                        }
                    >
                        "사전 열기"
                    </button>
                </div>

                <div class="divider m-0 opacity-10"></div>

                <div class="flex items-center justify-between">
                    <label class="label cursor-pointer px-0">
                        <div class="flex flex-col">
                            <span class="text-xs font-bold text-base-content/80">"사전 자동 동기화"</span>
                            <span class="text-[9px] opacity-60">"시작 시 사용자 사전 최신 버전을 체크하고 다운받습니다."</span>
                            <span class="text-[9px] opacity-60">"(사용자 사전 직접 수정 시 체크 해제해주세요)"</span>
                        </div>
                        <input type="checkbox" class="toggle toggle-warning toggle-sm"
                            prop:checked=move || signals.auto_sync_latest_dict.get()
                            on:change=move |ev| {
                                let checked = event_target_checked(&ev);
                                signals.set_auto_sync_latest_dict.set(checked);
                                actions.save_config.dispatch(());
                            }
                        />
                    </label>
                </div>

                <div class="divider m-0 opacity-10"></div>

                <div class="flex items-center justify-between">
                    <div class="flex flex-col">
                        <span class="text-xs font-bold text-base-content/80">"대화 기록 저장"</span>
                        <span class="text-[9px] opacity-60">"현재 대화 내용을 텍스트로 내보냅니다."</span>
                    </div>
                    <button class="btn btn-xs btn-outline w-16"
                        disabled=move || is_saving_chat.get()
                        on:click=move |_| { save_chat_action.dispatch(()); }
                    >
                        {move || if is_saving_chat.get() {
                            view! { <span class="loading loading-spinner loading-xs"></span> }.into_any()
                        } else if let Some(res) = save_chat_action.value().get() {
                            // Displays "저장 완료" (Saved) or "저장 실패" (Failed) temporarily
                            view! { {res} }.into_any()
                        } else {
                            view! { "저장" }.into_any()
                        }}
                    </button>
                </div>

                <div class="divider m-0 opacity-10"></div>

                // Chat log retention: daily files in chat_logs/, pruned at
                // start-up, on save and when the day changes.
                <div class="flex items-center justify-between">
                    <div class="flex flex-col">
                        <span class="text-xs font-bold text-base-content/80">"채팅 로그 보관 기간"</span>
                        <span class="text-[9px] opacity-60">"지난 채팅 로그(chat_logs)를 설정한 일수만 남기고 삭제합니다. 0 = 삭제 안 함"</span>
                    </div>
                    <div class="flex items-center gap-1">
                        <input type="number" min="0" max="3650" step="1"
                            class="input input-xs input-bordered w-16 text-right"
                            prop:value=move || signals.chat_log_retention_days.get().to_string()
                            on:change=move |ev| {
                                let days = event_target_value(&ev).trim().parse::<u32>().unwrap_or(0).min(3650);
                                signals.set_chat_log_retention_days.set(days);
                                actions.save_config.dispatch(());
                            }
                        />
                        <span class="text-[10px] opacity-60">"일"</span>
                    </div>
                </div>

                <div class="divider m-0 opacity-10"></div>

                // --- NEW: Open AppData Directory ---
                <div class="flex items-center justify-between">
                    <div class="flex flex-col">
                        <span class="text-xs font-bold text-base-content/80">"앱 데이터 폴더 열기"</span>
                        <span class="text-[9px] opacity-60">"설정 및 로그 파일이 저장된 폴더를 엽니다."</span>
                    </div>
                    <button class="btn btn-xs btn-outline"
                        on:click=move |_| {
                            spawn_local(async {
                                let _ = invoke("open_app_data_folder", JsValue::NULL).await;
                            });
                        }
                    >
                        "폴더 열기"
                    </button>
                </div>

                <div class="divider m-0 opacity-10"></div>

                <div class="flex items-center justify-between">
                    <div class="flex flex-col">
                        <span class="text-xs font-bold text-warning">"디버그 모드 (Debug Mode)"</span>
                        <span class="text-[9px] opacity-60">"시스템 탭 및 개발자 도구 활성화"</span>
                    </div>
                    <input type="checkbox" class="toggle toggle-warning toggle-sm"
                        prop:checked=move || signals.debug_mode.get()
                        on:change=move |ev| {
                            signals.set_debug_mode.set(event_target_checked(&ev));
                            actions.save_config.dispatch(());
                        }
                    />
                </div>

                // --- REVEALED ONLY IN DEBUG MODE ---
                <Show when=move || signals.debug_mode.get()>
                    <div class="p-3 bg-warning/5 border border-warning/20 rounded-lg space-y-3 mt-2 animate-in fade-in slide-in-from-top-2 duration-200">

                        // 1. Log Level Select
                        <div class="flex items-center justify-between">
                            <div class="flex flex-col">
                                <span class="text-[11px] font-bold text-base-content/80">"로그 레벨 (Log Level)"</span>
                            </div>
                            <select class="select select-bordered select-xs w-24 text-xs font-bold bg-base-100"
                                prop:value=move || signals.log_level.get()
                                on:change=move |ev| {
                                    signals.set_log_level.set(event_target_value(&ev));
                                    actions.save_config.dispatch(());
                                }>
                                <option value="trace">"TRACE"</option>
                                <option value="debug">"DEBUG"</option>
                                <option value="info">"INFO"</option>
                                <option value="warn">"WARN"</option>
                                <option value="error">"ERROR"</option>
                            </select>
                        </div>

                        <div class="divider m-0 opacity-10"></div>

                        // 2. Network Interface Manual Selection
                        <div class="flex items-center justify-between">
                            <div class="flex flex-col">
                                <span class="text-[11px] font-bold text-base-content/80">"네트워크 어댑터 (Network Interface)"</span>
                                <span class="text-[9px] text-warning/80 italic">"VPN 사용 시 패킷 캡처 실패 해결용"</span>
                            </div>
                            <select class="select select-bordered select-xs w-36 text-[10px] font-bold bg-base-100"
                                prop:value=move || signals.network_interface.get()
                                on:change=move |ev| {
                                    signals.set_network_interface.set(event_target_value(&ev));
                                    actions.save_config.dispatch(());
                                    signals.set_restart_required.set(true); // Requires sniffer restart
                                }>
                                <option value="">"Auto-Detect (권장)"</option>
                                <For
                                    each=move || interfaces.get()
                                    key=|iface| iface.ip.clone()
                                    children=move |iface| {
                                        view! {
                                            <option value=iface.ip.clone()>
                                                {format!("{} ({})", iface.name, iface.ip)}
                                            </option>
                                        }
                                    }
                                />
                            </select>
                        </div>

                        // 3. Data Factory (Save Chatting Log)
                        <div class="flex items-center justify-between">
                            <div class="flex flex-col">
                                <span class="text-[11px] font-black text-warning uppercase">"Data Factory"</span>
                                <span class="text-[9px] text-base-content/60 italic">"채팅 로그 원본 저장 (dataset_raw.jsonl)"</span>
                            </div>
                            <input type="checkbox" class="checkbox checkbox-warning checkbox-xs"
                                prop:checked=move || signals.archive_chat.get()
                                on:change=move |ev| {
                                    signals.set_archive_chat.set(event_target_checked(&ev));
                                    actions.save_config.dispatch(());
                                }
                            />
                        </div>
                    </div>
                </Show>
            </div>
        </section>
    }
}
