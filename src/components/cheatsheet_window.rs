//! The cheat sheet: class and dungeon names in Japanese and Korean, the page of
//! its popup window. Click a Japanese name to copy it (to paste into the
//! game's chat).

use crate::cheatsheet::{search, Section, SOURCE_NOTE};
use crate::utils::copy_to_clipboard;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn CheatSheetWindow() -> impl IntoView {
    let (section, set_section) = signal(Section::Classes);
    let (query, set_query) = signal(String::new());
    let (copied, set_copied) = signal(None::<&'static str>);

    let copy = move |ja: &'static str| {
        copy_to_clipboard(ja);
        set_copied.set(Some(ja));
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(1200).await;
            if copied.get_untracked() == Some(ja) {
                set_copied.set(None);
            }
        });
    };

    view! {
        <div class="h-full flex flex-col bg-base-100 text-base-content">
        <div class="flex items-center gap-2 px-3 pt-2">
            <div class="flex items-center gap-1" role="tablist">
                {Section::ALL.into_iter().map(|s| view! {
                    <button role="tab"
                        class=move || format!(
                            "h-7 px-3 rounded-md text-xs whitespace-nowrap transition-colors {}",
                            if section.get() == s { "bg-base-100 shadow-sm font-bold text-base-content" } else { "text-base-content/60 hover:text-base-content hover:bg-base-content/5" }
                        )
                        aria-selected=move || (section.get() == s).to_string()
                        on:click=move |_| set_section.set(s)>
                        {s.label()}
                    </button>
                }).collect_view()}
            </div>
            <input type="text" class="input input-bordered input-xs flex-1 min-w-0" placeholder="검색 (일본어 / 한국어)"
                prop:value=move || query.get()
                on:input=move |ev| set_query.set(event_target_value(&ev))
            />
        </div>

        <div class="text-[10px] text-base-content/60 px-3 pt-2">
            "일본어 이름을 누르면 복사됩니다."
        </div>

        <div class="flex-1 overflow-y-auto custom-scrollbar p-3 flex flex-col gap-1.5">
            {move || {
                let rows = search(section.get(), &query.get());
                if rows.is_empty() {
                    view! { <div class="text-xs text-base-content/50 text-center py-6">"검색 결과가 없습니다."</div> }.into_any()
                } else {
                    rows.into_iter().map(|e| view! {
                        <div class="flex items-center gap-3 px-3 py-2 rounded-lg bg-base-200 border border-base-content/5">
                            <button class="flex-1 min-w-0 text-left text-sm font-bold break-words hover:text-success transition-colors"
                                title="클릭하여 복사"
                                on:click=move |_| copy(e.ja)>
                                {e.ja}
                                <span class="ml-1 text-success text-xs font-normal">
                                    {move || if copied.get() == Some(e.ja) { "✓" } else { "" }}
                                </span>
                            </button>
                            <span class="flex-1 min-w-0 text-sm text-base-content/80 break-words select-text">{e.ko}</span>
                        </div>
                    }).collect_view().into_any()
                }
            }}
        </div>

        <div class="text-[10px] text-base-content/40 px-3 py-2 border-t border-base-content/5">{SOURCE_NOTE}</div>
        </div>
    }
}
