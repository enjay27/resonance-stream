//! The cheat sheet: class and dungeon names, the page of its popup window. The
//! Korean name is the main one; its Japanese names (official, then the fan names
//! players use) sit beside it, and a click on one copies it (to paste into the
//! game's chat). A class lists its two trees (specializations) under it; the dungeons are
//! grouped by season, and the seasons that have elapsed fold.

use crate::cheatsheet::{is_open, search, tree_lines, Entry, GroupHits, Section, SOURCE_NOTE};
use crate::components::icons::{self, icon};
use crate::utils::copy_to_clipboard;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// One entry: the Korean name is the main one, and every Japanese name it
/// has -- the official one, then the fan names -- is a chip that copies itself
/// on a click. `child` is `Some(last)` for a row hanging under its parent
/// (`last`: the parent's last child), drawn with tree lines; `None` for a class or
/// a dungeon.
#[component]
fn NameRow(
    entry: Entry,
    copied: ReadSignal<Option<&'static str>>,
    copy: Callback<&'static str>,
    #[prop(optional)] child: Option<bool>,
) -> impl IntoView {
    let small = child.is_some();
    view! {
        <div class=if small { "relative flex items-baseline gap-3 pl-8 pr-3 py-1" } else { "flex items-baseline gap-3 px-3 py-2" }>
            {child.map(|last| tree_lines(last).iter().map(|piece| view! {
                <span class=format!("pointer-events-none {piece}") aria-hidden="true"></span>
            }).collect_view())}
            <div class="flex-1 min-w-0 flex items-baseline gap-1.5">
                <span class=if small { "min-w-0 text-xs font-semibold break-words select-text" } else { "min-w-0 text-sm font-bold break-words select-text" }>{entry.ko}</span>
            </div>
            <div class="flex-1 min-w-0 flex flex-wrap items-center gap-1">
                {entry.ja.iter().enumerate().map(|(i, &name)| {
                    // The first is the official name; the rest are fan names, dashed.
                    let (border, tip) = if i == 0 {
                        ("border-transparent", "공식 표기 · 클릭하여 복사")
                    } else {
                        ("border-dashed border-base-content/25", "팬 표기 · 클릭하여 복사")
                    };
                    view! {
                        <button
                            class=format!("px-2 py-0.5 rounded-md border bg-base-content/5 text-left break-words hover:bg-success/15 hover:text-success transition-colors {border} {}", if small { "text-xs" } else { "text-sm" })
                            title=tip
                            on:click=move |_| copy.run(name)>
                            {name}
                            <span class="ml-1 text-success text-xs font-normal">
                                {move || if copied.get() == Some(name) { "✓" } else { "" }}
                            </span>
                        </button>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

/// One group of a section: its header (a title, and for a folded season a
/// button that opens it) and, while open, its entries as cards with their
/// children under them.
#[component]
fn GroupView(
    group: GroupHits,
    searching: bool,
    opened: RwSignal<Vec<&'static str>>,
    copied: ReadSignal<Option<&'static str>>,
    copy: Callback<&'static str>,
) -> impl IntoView {
    let GroupHits {
        group,
        hits,
        collapsible,
    } = group;
    let title = group.title;
    let count = hits.len();
    let open = move || is_open(collapsible, opened.get().contains(&title), searching);
    // A search shows its matches, so it cannot fold them.
    let can_toggle = collapsible && !searching;
    let toggle = move |_| {
        opened.update(|list| match list.iter().position(|t| *t == title) {
            Some(i) => {
                list.remove(i);
            }
            None => list.push(title),
        })
    };

    view! {
        <div class="flex flex-col gap-1.5">
            {(!title.is_empty()).then(|| view! {
                <button
                    class=if can_toggle { "flex items-center gap-1.5 px-1 pt-1 text-xs font-bold text-base-content/70 hover:text-base-content transition-colors" } else { "flex items-center gap-1.5 px-1 pt-1 text-xs font-bold text-base-content/70 cursor-default" }
                    disabled=!can_toggle
                    aria-expanded=move || open().to_string()
                    on:click=toggle>
                    {can_toggle.then(|| view! {
                        <span class=move || if open() { "inline-block transition-transform" } else { "inline-block transition-transform -rotate-90" }>{icon(icons::CHEVRON_DOWN, "size-3")}</span>
                    })}
                    <span>{title}</span>
                    <span class="font-normal text-base-content/40">{format!("({count})")}</span>
                </button>
            })}
            <Show when=open>
                {hits.clone().into_iter().map(|hit| {
                    let last = hit.children.len().saturating_sub(1);
                    let pad = if hit.children.is_empty() { "" } else { "pb-1" };
                    view! {
                        <div class=format!("rounded-lg bg-base-200 border border-base-content/5 {pad}")>
                            <NameRow entry=hit.entry copied=copied copy=copy />
                            {hit.children.into_iter().enumerate().map(|(i, child)| view! {
                                <NameRow entry=child copied=copied copy=copy
                                    child=i == last />
                            }).collect_view()}
                        </div>
                    }
                }).collect_view()}
            </Show>
        </div>
    }
}

#[component]
pub fn CheatSheetWindow() -> impl IntoView {
    let (section, set_section) = signal(Section::Classes);
    let (query, set_query) = signal(String::new());
    let (copied, set_copied) = signal(None::<&'static str>);
    // The folded seasons the user has opened (by title); a search opens its matches itself.
    let opened = RwSignal::new(Vec::<&'static str>::new());

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
            "일본어 이름을 누르면 복사됩니다. (점선 = 팬 표기)"
        </div>

        <div class="flex-1 overflow-y-auto custom-scrollbar p-3 flex flex-col gap-1.5">
            {move || {
                let q = query.get();
                let groups = search(section.get(), &q);
                if groups.is_empty() {
                    view! { <div class="text-xs text-base-content/50 text-center py-6">"검색 결과가 없습니다."</div> }.into_any()
                } else {
                    let searching = !q.trim().is_empty();
                    groups.into_iter().map(|group| view! {
                        <GroupView group=group searching=searching opened=opened copied=copied copy=copy_cb />
                    }).collect_view().into_any()
                }
            }}
        </div>

        <div class="text-[10px] text-base-content/40 px-3 py-2 border-t border-base-content/5">{SOURCE_NOTE}</div>
        </div>
    }
}
