//! The connect card, in the space: what a project reading no tracker shows, and what `+` beside Boards opens for
//! another. The accounts kept that the project does not read yet, each with Use; then a row a kind (Jira, Trello, a
//! plugin's) to add an account of, its form opening under it in the same card (`Accounts::space_form`). An account
//! is everyone's; Use and a form saved make this project read it.

use crate::accounts;
use crate::page::Page;
use crate::trackers::{self, SiteExt};
use artoo_plugin_sdk::gpui::{
    self, AnyElement, Context, FontWeight, SharedString, div, prelude::*,
};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, px, theme};

/// The card's width, as the design system's cards are drawn in the middle of a page.
const WIDTH: f32 = 480.;

/// What a kind asks for an account, in words: `A site, an email and an API token`, `An API key and a token`.
fn asks(kind: &artoo_tasks_api::TrackerKind) -> String {
    if kind.id == trackers::JIRA {
        return "A site and a token; on Cloud, an email too".into();
    }
    let a = |word: &str| match word.chars().next().map(|c| c.to_ascii_lowercase()) {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    };
    let own = kind.hosts.iter().any(|h| !h.starts_with('*'));
    match (kind.user.as_ref().map(|(user, _)| user), own) {
        (Some(user), true) => format!("{} {user} and a token", capital(a(user))),
        (Some(user), false) => format!("A site, {} {user} and a token", a(user)),
        (None, true) => "A token".into(),
        (None, false) => "A site and a token".into(),
    }
}

fn capital(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// A row of the card: a mark, its name and what it is, and its button.
fn row(
    id: SharedString,
    mark: AnyElement,
    name: String,
    about: String,
    end: Option<AnyElement>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(16.))
        .py(px(10.))
        .child(mark)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .truncate()
                        .child(name),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .truncate()
                        .child(about),
                ),
        )
        .children(end)
}

/// A group's heading in the card: `YOUR ACCOUNTS`, `ADD AN ACCOUNT`; ruled above when a group is over it.
fn heading(text: &str, ruled: bool) -> gpui::Div {
    div()
        .px(px(16.))
        .pt(px(14.))
        .when(ruled, |d| {
            d.border_t_1().border_color(theme::island_border())
        })
        .child(controls::rows_heading(text))
}

impl Page {
    /// The connect card for project `project`; `more` when it reads a tracker already, so the board is a step back.
    pub(crate) fn connect_page(
        &mut self,
        project: &str,
        more: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accounts = self.accounts.clone();
        let form = (accounts.as_ref()).and_then(|a| a.update(cx, |a, cx| a.space_form(cx)));
        let opened = (accounts.as_ref()).and_then(|a| a.read(cx).space_kind());
        // While an account is being added, the card is its form alone, so Connect stays in sight.
        let kept = match opened {
            Some(_) => Vec::new(),
            None => self.store.read(cx).sites_not_in(project),
        };
        let ruled = !kept.is_empty();
        let mut card = div()
            .w(px(WIDTH))
            .max_w_full()
            .flex()
            .flex_col()
            .rounded(px(12.))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .overflow_hidden();
        if !kept.is_empty() {
            card = card.child(heading("Your accounts", false));
            for site in kept {
                let host = site.host().to_owned();
                let kind = trackers::kind(&site.kind);
                // Jira Cloud's user is an email, said; another kind's (Trello's API key) is not.
                let about = match (&site.user, site.cloud()) {
                    (Some(user), true) => format!("{user} · Jira Cloud"),
                    _ => site.tracker(),
                };
                let (store, project) = (self.store.clone(), project.to_owned());
                let link = host.clone();
                let us = cx.entity().downgrade();
                let end = crate::look::small_button(format!("tasks-use-{host}"), "Use")
                    .h(px(28.))
                    .tip("Read its boards in this project")
                    .on_click(move |_, _, cx| {
                        let linked = store.update(cx, |s, cx| s.link(&project, &link, true, cx));
                        us.update(cx, |p, cx| {
                            if let Err(why) = linked {
                                p.frame.error(why.into(), cx);
                            }
                            p.connected(cx);
                        })
                        .ok();
                    })
                    .into_any_element();
                card = card.child(row(
                    format!("tasks-account-{host}").into(),
                    accounts::mark(&kind).into_any_element(),
                    host,
                    about,
                    Some(end),
                ));
            }
        }
        card = card.child(heading("Add an account", ruled));
        let mut form = form;
        // Jira first, as the most asked for; the others as they claim hosts.
        let mut kinds = trackers::kinds();
        kinds.sort_by_key(|k| k.id != trackers::JIRA);
        for kind in kinds {
            let open = opened.as_deref() == Some(kind.id.as_str());
            if opened.is_some() && !open {
                continue;
            }
            let end = (!open).then(|| {
                let (accounts, id, project) =
                    (accounts.clone(), kind.id.clone(), project.to_owned());
                crate::look::small_button(format!("tasks-add-{}", kind.id), "Add…")
                    .h(px(28.))
                    .on_click(move |_, _, cx| {
                        if let Some(a) = &accounts {
                            a.update(cx, |a, cx| a.open_in_space(&id, project.clone(), cx));
                        }
                    })
                    .into_any_element()
            });
            card = card.child(row(
                format!("tasks-kind-{}", kind.id).into(),
                accounts::mark(&kind).into_any_element(),
                kind.name.clone(),
                asks(&kind),
                end,
            ));
            if open {
                card = card.children(form.take());
            }
        }
        let back = more.then(|| {
            div().pt(px(4.)).child(
                crate::look::small_button("tasks-connect-back", "Back to the boards")
                    .h(px(28.))
                    .on_click(cx.listener(|p, _, _, cx| p.connected(cx))),
            )
        });
        (self.page_frame_own())
            .child(
                div()
                    .id("tasks-connect")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .items_center()
                    .px(px(24.))
                    .py(px(40.))
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("Connect a tracker"),
                    )
                    .child(
                        div()
                            .max_w(px(WIDTH))
                            .text_center()
                            .text_size(px(13.))
                            .text_color(theme::muted())
                            .child(
                                "Its boards and issues show here, beside the code and the merge \
                                 requests that do them. An account is kept once, for every project.",
                            ),
                    )
                    .child(div().pt(px(10.)).child(card))
                    .children(back),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_says_what_an_account_asks() {
        assert_eq!(
            asks(&trackers::jira()),
            "A site and a token; on Cloud, an email too"
        );
        assert_eq!(asks(&trackers::trello()), "An API key and a token");
    }
}
