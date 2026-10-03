//! The cheat sheet: class and dungeon names in Japanese and Korean, the page of
//! its popup window. A class lists its two trees (specializations) under it.
//! Click a Japanese name to copy it (to paste into the game's chat).

use crate::cheatsheet::{search, Entry, Section, SOURCE_NOTE};
use crate::utils::copy_to_clipboard;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// One name in both languages: the Japanese one copies on a click. `branch` is
/// the tree connector in front of a specialization (empty for a class or a dungeon).
#[component]
fn NameRow(
    entry: Entry,
    copied: ReadSignal<Option<&'static str>>,
    copy: Callback<&'static str>,
    #[prop(optional)] branch: &'static str,
) -> impl IntoView {
    let small = !branch.is_empty();
    view! {
        <div class=if small { "flex items-center gap-3 pl-3 pr-3 py-1" } else { "flex items-center gap-3 px-3 py-2" }>
            <div class="flex-1 min-w-0 flex items-baseline gap-1.5">
                <span class="w-3 shrink-0 text-base-content/30 text-xs select-none">{branch}</span>
                <button class=if small { "min-w-0 text-left text-xs font-semibold break-words hover:text-success transition-colors" } else { "min-w-0 text-left text-sm font-bold break-words hover:text-success transition-colors" }
                    title="클릭하여 복사"
                    on:click=move |_| copy.run(entry.ja)>
                    {entry.ja}
                    <span class="ml-1 text-success text-xs font-normal">
                        {move || if copied.get() == Some(entry.ja) { "✓" } else { "" }}
                    </span>
                </button>
                {(!entry.also.is_empty()).then(|| view! {
                    <span class="text-[10px] text-base-content/50 break-words">{entry.also}</span>
                })}
            </div>
            <span class=if small { "flex-1 min-w-0 text-xs text-base-content/70 break-words select-text" } else { "flex-1 min-w-0 text-sm text-base-content/80 break-words select-text" }>{entry.ko}</span>
        </div>
    }
}

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

    let copy_cb = Callback::new(copy);

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
                    rows.into_iter().map(|hit| {
                        let last = hit.trees.len().saturating_sub(1);
                        let pad = if hit.trees.is_empty() { "" } else { "pb-1" };
                        view! {
                            <div class=format!("rounded-lg bg-base-200 border border-base-content/5 {pad}")>
                                <NameRow entry=hit.entry copied=copied copy=copy_cb />
                                {hit.trees.into_iter().enumerate().map(|(i, tree)| view! {
                                    <NameRow entry=tree copied=copied copy=copy_cb
                                        branch=if i == last { "└" } else { "├" } />
                                }).collect_view()}
                            </div>
                        }
                    }).collect_view().into_any()
                }
            }}
        </div>

        <div class="text-[10px] text-base-content/40 px-3 py-2 border-t border-base-content/5">{SOURCE_NOTE}</div>
        </div>
    }
}
