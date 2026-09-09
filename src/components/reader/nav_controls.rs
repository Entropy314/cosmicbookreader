use leptos::prelude::*;

use crate::hooks::use_comic_reader::ReaderState;

#[component]
pub fn NavControls(reader: ReaderState, advance: Callback<bool>) -> impl IntoView {
    // Reading right-to-left, the next page lies to the left.
    let forward_is_right = move || !reader.rtl.get();

    view! {
        <div class="nav-zones" class:hidden=move || reader.page_data.with(|page| page.is_none())>
            <div
                class="nav-zone nav-zone-left"
                on:click=move |_| advance.run(!forward_is_right())
                title=move || if forward_is_right() { "Previous page" } else { "Next page" }
            />
            // Middle zone: toggle toolbar
            <div
                class="nav-zone nav-zone-middle"
                on:click=move |_| reader.toggle_toolbar()
            />
            <div
                class="nav-zone nav-zone-right"
                on:click=move |_| advance.run(forward_is_right())
                title=move || if forward_is_right() { "Next page" } else { "Previous page" }
            />
        </div>
    }
}
