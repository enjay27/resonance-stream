use crate::set_timeout;
use leptos::prelude::{ElementChild, GetUntracked};
use leptos::*;
use leptos::context::use_context;
use leptos::control_flow::Show;
use leptos::prelude::{signal, ClassAttribute, Get, IntoAny, OnAttribute, Set};
use leptos::task::spawn_local;
use crate::store::{AppActions, AppSignals};
use crate::tauri_bridge::invoke;
use crate::ui_types::NetworkInterface;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

// Helper to pause the async loop without blocking the UI
async fn delay(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            // Tells the browser's window to wait 'ms' milliseconds, then call resolve()
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    // Wait for the JS promise to finish!
    let _ = JsFuture::from(promise).await;
}

#[component]
pub fn Troubleshooter() -> impl IntoView {
    let signals = use_context::<AppSignals>().expect("Signals missing");
    let actions = use_context::<AppActions>().expect("Actions missing");

    // "idle", "scanning", "success", "fail"
    let (status, set_status) = signal("idle".to_string());
    let (current_test, set_current_test) = signal("".to_string());
    let (progress, set_progress) = signal(0.0);

    let start_scan = move |_| {
        spawn_local(async move {
            set_status.set("scanning".to_string());
            set_progress.set(0.0);

            if let Ok(res) = invoke("get_network_interfaces", JsValue::NULL).await {
                if let Ok(list) = serde_wasm_bindgen::from_value::<Vec<NetworkInterface>>(res) {

                    for (i, iface) in list.iter().enumerate() {
                        set_current_test.set(format!("{} ({})", iface.name, iface.ip));
                        set_progress.set((i as f64 / list.len() as f64) * 100.0);

                        // 1. Temporarily save this adapter to config
                        signals.set_network_interface.set(iface.ip.clone());
                        actions.save_config.dispatch(());

                        // 2. Restart the backend sniffer!
                        let _ = invoke("restart_sniffer_command", JsValue::NULL).await;

                        // 3. Reset elapsed time
                        let mut elapsed = 0;
                        let timeout_ticks = 50; // 5 seconds

                        while elapsed < timeout_ticks {
                            delay(100).await;

                            // WOW! We just check if the state flipped to Active!
                            // As long as the game is running, background packets will trigger this instantly!
                            if signals.sniffer_state.get_untracked() == "Active" {
                                set_status.set("success".to_string());
                                set_progress.set(100.0);
                                return; // We found the working adapter!
                            }
                            elapsed += 1;
                        }
                    }
                }
            }

            // If we loop through everything and nothing worked...
            set_status.set("fail".to_string());
            set_progress.set(100.0);
            signals.set_network_interface.set("".to_string()); // Reset to auto
            actions.save_config.dispatch(());
            let _ = invoke("restart_sniffer_command", JsValue::NULL).await; // Restart in auto mode
        });
    };

    view! {
        <Show when=move || signals.show_troubleshooter.get()>
            <div class="modal modal-open backdrop-blur-sm z-[30000]">
                <div class="modal-box bg-base-300 border border-base-content/10 shadow-2xl w-full max-w-md p-6">

                    <div class="flex items-center gap-3 mb-4">
                        <div class="w-10 h-10 rounded-full bg-warning/20 text-warning flex items-center justify-center text-xl">"📡"</div>
                        <div>
                            <h2 class="text-lg font-black text-base-content">"네트워크 어댑터 복구"</h2>
                            <p class="text-xs text-base-content/60">"VPN이나 가상 네트워크 환경 문제 해결"</p>
                        </div>
                    </div>

                    {move || match status.get().as_str() {
                        "idle" => view! {
                            <div class="space-y-4">
                                <p class="text-sm">"게임 채팅이 인식되지 않나요? PC에 설치된 다른 네트워크 어댑터들을 하나씩 순회하며 게임 트래픽이 잡히는지 자동으로 테스트합니다."</p>
                                <button class="btn btn-warning btn-block font-bold" on:click=start_scan>"자동 복구 시작"</button>
                            </div>
                        }.into_any(),

                        "scanning" => view! {
                            <div class="space-y-4 text-center py-4">
                                <div class="text-xs font-bold text-success animate-pulse mb-2">"⚠️ 중요: 게임이 켜져 있는지 확인해 주세요!"</div>

                                <progress class="progress progress-warning w-full" value=move || progress.get().to_string() max="100"></progress>

                                <div class="bg-base-200 p-3 rounded-lg border border-base-content/10">
                                    <div class="text-[10px] text-base-content/50 uppercase font-bold mb-1">"현재 테스트 중인 어댑터"</div>
                                    <div class="text-sm font-mono text-warning break-all">{move || current_test.get()}</div>
                                </div>
                                <p class="text-[10px] opacity-50">"각 어댑터마다 최대 5초씩 대기하며 게임 패킷을 감지합니다..."</p>
                            </div>
                        }.into_any(),

                        "success" => view! {
                            <div class="space-y-4 text-center py-2">
                                <div class="text-4xl mb-2">"🎉"</div>
                                <h3 class="text-lg font-bold text-success">"어댑터 복구 완료!"</h3>
                                <p class="text-xs">"성공적으로 게임 채팅을 감지했습니다. 설정이 자동으로 저장되었습니다."</p>
                                <div class="text-[10px] bg-base-200 p-2 rounded text-success font-mono">{move || current_test.get()}</div>
                                <button class="btn btn-success btn-block mt-4" on:click=move |_| signals.set_show_troubleshooter.set(false)>"닫기"</button>
                            </div>
                        }.into_any(),

                        "fail" => view! {
                            <div class="space-y-4 text-center py-2">
                                <div class="text-4xl mb-2">"❌"</div>
                                <h3 class="text-lg font-bold text-error">"감지 실패"</h3>
                                <p class="text-xs">"모든 네트워크 어댑터를 확인했지만 게임 트래픽을 찾지 못했습니다. 게임이 켜져있고 로그인 된 상태인지 확인해주세요."</p>
                                <div class="flex gap-2 mt-4">
                                    <button class="btn btn-ghost flex-1" on:click=move |_| signals.set_show_troubleshooter.set(false)>"취소"</button>
                                    <button class="btn btn-warning flex-1" on:click=start_scan>"다시 시도"</button>
                                </div>
                            </div>
                        }.into_any(),

                        _ => view! { <div></div> }.into_any(),
                    }}
                </div>
                <div class="modal-backdrop bg-black/50" on:click=move |_| {
                    if status.get() != "scanning" { signals.set_show_troubleshooter.set(false); }
                }></div>
            </div>
        </Show>
    }
}