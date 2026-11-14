//! The accounts on trackers, everyone's whichever project reads them: Settings → Accounts, its Trackers group, under
//! the code hosts — each account, what it reaches and how it signs in, Test and Change; one not reached says so with
//! Retry; Change opens its fields — the site, the email (Jira Cloud), the token — and what it reaches; Add a tracker.
//! The space's connect card opens the same form for a new account (`open_in_space`), drawn as a card's fields, and
//! links what it saves to the project. The sites are kept in the plugin's `trackers.txt`, each token in
//! `~/.config/tokens/<host>` as the Mac keeps internal hosts' tokens.

use crate::jira::{self, Site};
use crate::model;
use crate::store::{Read, Store};
use crate::trackers::{self, SiteExt};
use artoo_plugin_sdk::core::textedit::TextEdit;
use artoo_plugin_sdk::gpui::{
    self, AnyElement, App, ClickEvent, Context, Entity, FontWeight, KeyDownEvent, SharedString,
    Window, div, prelude::*,
};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, icons, input, motion, px, room, theme};
use artoo_plugin_sdk::shell::{Frame, Typed};
use artoo_tasks_api::TrackerKind;
use std::collections::HashMap;
use std::rc::Rc;

/// The fields' input: the keys while one of them is typed into.
pub const INPUT: &str = "tasks.accounts";

/// The fold of a form's Email field.
const EMAIL: &str = "tracker-email";

/// A tracker's fields as Change shows them.
struct Form {
    /// The host it was opened for; None for a new one.
    host: Option<String>,
    fields: [String; 3],
    edits: [TextEdit; 3],
    /// The field with the keyboard: the site, the email, the token.
    at: usize,
    why: Option<String>,
    /// The project whose space opened it: what it saves, that project reads.
    project: Option<String>,
    /// The site is the kind's own (Trello's): not asked.
    fixed: bool,
}

impl Form {
    fn new(site: Option<&Site>) -> Self {
        let url = site.map(|s| s.url.clone()).unwrap_or_default();
        Self::filled(site, url)
    }

    /// A new site's form with `url` typed: a kind's own host, its first run's Connect… asked for it.
    fn filled(site: Option<&Site>, url: String) -> Self {
        let email = site.and_then(|s| s.user.clone()).unwrap_or_default();
        Form {
            host: site.map(|s| s.host().to_owned()),
            edits: [
                TextEdit::new(url.clone()),
                TextEdit::new(email.clone()),
                TextEdit::new(String::new()),
            ],
            fields: [url, email, String::new()],
            at: 0,
            why: None,
            project: None,
            fixed: false,
        }
    }

    /// Whether field `ix` is asked: the site unless it is the kind's own, the user when the kind has one — Jira's
    /// email on a Cloud host alone, as a server refuses a token sent with one.
    fn asks(&self, ix: usize) -> bool {
        match ix {
            0 => !self.fixed,
            1 => {
                let kind = self.kind();
                kind.user.is_some()
                    && (kind.id != trackers::JIRA || trackers::cloud_host(&self.host()))
            }
            _ => true,
        }
    }

    /// The host of the site typed: `jira.example.com` of `https://jira.example.com/`.
    fn host(&self) -> String {
        let url = self.fields[0].trim();
        let site = Site {
            url: if url.contains("://") {
                url.to_owned()
            } else {
                format!("https://{url}")
            },
            ..Default::default()
        };
        site.host().to_owned()
    }

    /// The field after `at` (before it, `back`), round, skipping what is not asked.
    fn step(&mut self, back: bool) {
        for _ in 0..3 {
            self.at = (self.at + if back { 2 } else { 1 }) % 3;
            if self.asks(self.at) {
                break;
            }
        }
    }

    /// Where a token is made for the kind typed: the link's words and its address, when it is known.
    fn token_page(&self) -> Option<(String, String)> {
        let kind = self.kind();
        let jira = kind.id == trackers::JIRA;
        let cloud = jira && trackers::cloud_host(&self.host());
        match (&kind.token_page, cloud) {
            (Some((words, url)), _) => Some((words.clone(), url.clone())),
            (None, true) => Some((
                "Make one on id.atlassian.com".into(),
                "https://id.atlassian.com/manage-profile/security/api-tokens".into(),
            )),
            _ => None,
        }
    }

    /// The kind of tracker the site typed is: Trello's on `trello.com`, a plugin's on its hosts, else Jira.
    fn kind(&self) -> TrackerKind {
        trackers::kind(&trackers::kind_of(&self.host()))
    }
}

pub struct Accounts {
    store: Entity<Store>,
    frame: Rc<dyn Frame>,
    form: Option<Form>,
    /// Test's answer by host: who the token is, or why not.
    tested: HashMap<String, Read<String>>,
    /// Its folds: a form opening under its row, Jira's Email coming as the site says Cloud (#110).
    motion: motion::Marks,
    folds: motion::Folds,
    heights: room::Heights,
    /// Whether the Email field was drawn last: its fold moves when that changes.
    asked_user: bool,
}

impl Accounts {
    pub fn new(store: Entity<Store>, frame: Rc<dyn Frame>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Accounts {
            store,
            frame,
            form: None,
            tested: HashMap::new(),
            motion: motion::Marks::default(),
            folds: motion::Folds::default(),
            heights: room::Heights::default(),
            asked_user: false,
        }
    }

    pub fn typing(&self) -> bool {
        self.form.is_some()
    }

    fn open(&mut self, host: Option<String>, cx: &mut Context<Self>) {
        if self.form.as_ref().is_some_and(|f| f.host == host) {
            self.form = None;
            self.frame.leave_keyboard(INPUT, cx);
            return cx.notify();
        }
        let site = host.as_ref().and_then(|h| self.store.read(cx).site_of(h));
        self.form = Some(Form::new(site.as_ref()));
        self.moved(&Self::form_key(host.as_deref()), cx);
        if let Some(h) = host {
            self.store.update(cx, |s, cx| s.read_kinds(&h, "", cx));
        }
        self.frame.take_keyboard(INPUT, cx);
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.form = None;
        self.frame.leave_keyboard(INPUT, cx);
        cx.notify();
    }

    /// The fold of the form under site `host`'s row, or of a new site's (`None`).
    fn form_key(host: Option<&str>) -> String {
        format!("tracker-form-{}", host.unwrap_or("new"))
    }

    /// Fold `key` set moving, and drawn still once it is over.
    fn moved(&mut self, key: &str, cx: &mut Context<Self>) {
        self.motion.mark(key);
        motion::redraw_after(cx);
    }

    fn folding(
        &self,
        key: &str,
        open: bool,
        content: impl IntoElement + 'static,
    ) -> Option<AnyElement> {
        motion::folding(
            &self.folds,
            &self.heights,
            self.motion.recent(key),
            key,
            open,
            content,
        )
    }

    /// The Email field, folding in as the site typed says Cloud and out as it says otherwise.
    fn email_fold(
        &mut self,
        asks: bool,
        field: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if asks != self.asked_user {
            self.asked_user = asks;
            self.moved(EMAIL, cx);
        }
        self.folding(EMAIL, asks, field?)
    }

    /// A new account of kind `kind` (`jira`, `trello`, a plugin's) asked in project `project`'s space: its form, on
    /// the kind's own site when it has one.
    pub fn open_in_space(&mut self, kind: &str, project: String, cx: &mut Context<Self>) {
        let own = (trackers::kinds().into_iter())
            .find(|k| k.id == kind)
            .and_then(|k| k.hosts.iter().find(|h| !h.starts_with('*')).cloned());
        let mut form = match &own {
            Some(host) => Form::filled(None, format!("https://{host}")),
            None => Form::new(None),
        };
        form.project = Some(project);
        form.fixed = own.is_some();
        if form.fixed {
            form.step(false);
        }
        self.form = Some(form);
        self.frame.take_keyboard(INPUT, cx);
        cx.notify();
    }

    /// The space's form closed, if it is open there.
    pub fn close_in_space(&mut self, cx: &mut Context<Self>) {
        if self.form.as_ref().is_some_and(|f| f.project.is_some()) {
            self.close(cx);
        }
    }

    /// The kind of the account being added in a space, while its form is open there.
    pub fn space_kind(&self) -> Option<String> {
        (self.form.as_ref())
            .filter(|f| f.project.is_some())
            .map(|f| f.kind().id)
    }

    /// Test: who the token is on the site, read now.
    fn test(&mut self, host: &str, cx: &mut Context<Self>) {
        let Some(site) = self.store.read(cx).site_of(host) else {
            return;
        };
        self.tested.insert(host.to_owned(), Read::Reading);
        let h = host.to_owned();
        let read = cx.background_spawn(async move { model::myself(&site) });
        cx.spawn(async move |this, cx| {
            let me = read.await;
            this.update(cx, |a, cx| {
                a.tested.insert(
                    h,
                    match me {
                        Ok(p) => Read::Done(format!("Signed in as {}", p.name)),
                        Err(why) => Read::Failed(why),
                    },
                );
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Save: the site kept in `trackers.txt`, its token (when one is typed) in `~/.config/tokens/<host>`; the boards
    /// read again.
    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let url = form.fields[0].trim().trim_end_matches('/').to_owned();
        let url = if url.is_empty() || url.contains("://") {
            url
        } else {
            format!("https://{url}")
        };
        // A field not asked is not kept: an email typed before the site said it is a server's.
        let email = match form.asks(1) {
            true => form.fields[1].trim().to_owned(),
            false => String::new(),
        };
        let token = form.fields[2].trim().to_owned();
        let mut site = Site {
            url: url.clone(),
            user: (!email.is_empty()).then_some(email.clone()),
            ..Default::default()
        };
        site.kind = trackers::kind_of(site.host());
        let host = site.host().to_owned();
        if host.is_empty() {
            form.why =
                Some("A site is needed: jira.example.com, or your-team.atlassian.net".into());
            return cx.notify();
        }
        if token.is_empty() && form.host.as_deref() != Some(host.as_str()) {
            form.why = Some("A token is needed for a new site".into());
            return cx.notify();
        }
        if jira::fixture().is_some() {
            form.why = Some("The fixture's trackers are its trackers.txt".into());
            return cx.notify();
        }
        // A kind other than Jira is said on the line, so a site whose plugin is gone is not read as Jira.
        let kind = (site.kind != trackers::JIRA).then(|| format!(" kind={}", site.kind));
        let line = match email.is_empty() {
            true => format!("{url}{}", kind.unwrap_or_default()),
            false => format!("{url} {email}{}", kind.unwrap_or_default()),
        };
        let replaced = form.host.clone();
        if let Err(why) = keep_site(
            replaced.as_deref(),
            Some(&line),
            &host,
            (!token.is_empty()).then_some(&token),
        ) {
            form.why = Some(why);
            return cx.notify();
        }
        let project = form.project.clone();
        self.close(cx);
        let linked = self.store.update(cx, |s, cx| {
            s.sites_again(cx);
            match project {
                Some(project) => s.link(&project, &host, true, cx),
                None => Ok(()),
            }
        });
        if let Err(why) = linked {
            self.frame.error(why.into(), cx);
        }
    }

    /// Remove: the site out of `trackers.txt`; its token stays where the Mac keeps it.
    fn remove(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.form.as_ref().and_then(|f| f.host.clone()) else {
            return self.close(cx);
        };
        if let Err(why) = keep_site(Some(&host), None, &host, None) {
            if let Some(f) = self.form.as_mut() {
                f.why = Some(why);
            }
            return cx.notify();
        }
        self.close(cx);
        self.store.update(cx, |s, cx| s.sites_again(cx));
    }

    pub fn key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> Typed {
        let key = &event.keystroke;
        let Some(form) = self.form.as_mut() else {
            return Typed::Leave;
        };
        if key.key == "tab" {
            form.step(key.modifiers.shift);
            cx.notify();
            return Typed::Kept;
        }
        let at = form.at;
        match input::edit(&mut form.fields[at], &mut form.edits[at], key, cx) {
            input::Edit::Escape => {
                self.close(cx);
                return Typed::Leave;
            }
            input::Edit::Enter { .. } => {
                if at < 2 {
                    form.step(false);
                } else {
                    self.save(cx);
                    return if self.form.is_some() {
                        Typed::Kept
                    } else {
                        Typed::Leave
                    };
                }
            }
            _ => {}
        }
        cx.notify();
        Typed::Kept
    }

    /// Field `ix`; `card` drawn as a card's field (32 px, a ring while it has the keyboard), else as a row's.
    fn field(
        &self,
        ix: usize,
        placeholder: &str,
        card: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self.form.as_ref() else {
            return div().into_any_element();
        };
        let active = form.at == ix && self.frame.typing(cx) == Some(INPUT);
        let text = form.fields[ix].clone();
        let look = input::Look {
            mask: ix == 2,
            ..Default::default()
        };
        let edit = &form.edits[ix];
        let body = input::field(&text, active, placeholder, &look, || {
            input::text_body(&text, edit, look.clone(), Default::default(), None).into_any_element()
        });
        artoo_plugin_sdk::shell::input_box(
            div()
                .id(SharedString::from(format!("tracker-field-{ix}")))
                .flex_1()
                .min_w_0()
                .h(px(if card { 32. } else { 28. }))
                .px(px(if card { 10. } else { 8. }))
                .flex()
                .items_center()
                .rounded(px(6.))
                .bg(theme::base())
                .border_1()
                .border_color(if active {
                    theme::focus()
                } else {
                    theme::island_border()
                })
                .when(card && active, |d| {
                    d.shadow(vec![gpui::BoxShadow {
                        color: theme::focus().into(),
                        offset: gpui::point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(1.),
                        inset: false,
                    }])
                })
                .text_size(px(13.))
                .cursor_text()
                .child(body)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(f) = this.form.as_mut() {
                        f.at = ix;
                    }
                    this.frame.take_keyboard(INPUT, cx);
                    cx.notify();
                })),
            INPUT,
            &self.frame,
        )
        .into_any_element()
    }

    /// A tracker's fields under its row: Site, Email, API token, what it reaches; Remove, Cancel, Save.
    fn form(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(form) = self.form.as_ref() else {
            return div().into_any_element();
        };
        let host = form.host.clone();
        let why = form.why.clone();
        let kind = form.kind();
        let jira = kind.id == trackers::JIRA;
        let page = form.token_page();
        let asks_user = form.asks(1);
        let reaches = host
            .as_ref()
            .map(|h| match self.store.read(cx).projects.get(h) {
                Some(Read::Done(p)) => {
                    let words: Vec<String> =
                        p.iter().take(6).map(|(k, n)| format!("{n} {k}")).collect();
                    let more = p.len().saturating_sub(6);
                    (
                        true,
                        if more > 0 {
                            format!("{} · {more} more", words.join(" · "))
                        } else {
                            words.join(" · ")
                        },
                    )
                }
                Some(Read::Failed(why)) => (false, why.clone()),
                _ => (true, "Reading…".into()),
            });
        let label = |t: &str| {
            div()
                .w(px(84.))
                .flex_none()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(t.to_owned())
        };
        let line = |l, v: AnyElement| div().flex().items_center().gap(px(10.)).child(l).child(v);
        let (make, make_url) = match page {
            Some(page) => page,
            None if jira => ("Make one in your Jira profile".into(), String::new()),
            None => (String::new(), String::new()),
        };
        let site = self.field(
            0,
            "your-team.atlassian.net, jira.example.com or trello.com",
            false,
            cx,
        );
        let user = (kind.user.clone()).map(|(name, hint)| {
            let field = self.field(1, &hint, false, cx);
            line(label(&name), field).into_any_element()
        });
        let user = self.email_fold(asks_user, user, cx);
        let token = self.field(
            2,
            if host.is_some() {
                "Leave empty to keep the token"
            } else {
                "API token or personal access token"
            },
            false,
            cx,
        );
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .px(px(14.))
            .py(px(12.))
            .bg(theme::base())
            .border_t_1()
            .border_color(theme::island_border())
            .child(line(label("Site"), site))
            .children(user)
            .child(line(
                label("API token"),
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(token)
                    .when(!make_url.is_empty(), |d| {
                        d.child(
                            div()
                                .id("tracker-make-token")
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .text_size(px(12.))
                                .text_color(theme::accent())
                                .cursor_pointer()
                                .child(make)
                                .child(
                                    icons::icon("external")
                                        .size(px(11.))
                                        .text_color(theme::accent()),
                                )
                                .on_click(move |_, _, cx| cx.open_url(&make_url)),
                        )
                    })
                    .into_any_element(),
            ))
            .children(reaches.map(|(ok, words)| {
                line(
                    label("Reaches"),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(if ok { theme::text() } else { theme::removed() })
                        .child(
                            icons::icon(if ok { "check" } else { "alert" })
                                .size(px(12.))
                                .text_color(if ok { theme::added() } else { theme::removed() }),
                        )
                        .child(words)
                        .into_any_element(),
                )
            }))
            .children(why.map(|w| {
                div()
                    .text_size(px(12.))
                    .text_color(theme::removed())
                    .child(w)
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(host.is_some(), |d| {
                        d.child(
                            crate::look::small_button("tracker-remove", "Remove")
                                .tip("Stop reading this tracker; its token stays")
                                .on_click(
                                    cx.listener(|this, _: &ClickEvent, _, cx| this.remove(cx)),
                                ),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        crate::look::small_button("tracker-cancel", "Cancel")
                            .tip("esc")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close(cx))),
                    )
                    .child(
                        controls::primary_button("tracker-save", "Save", "↵")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx))),
                    ),
            )
            .into_any_element()
    }

    /// The form opened in a space, as the design system draws a card's fields: each label over its field, where
    /// the token is made beside its label; what went wrong, Cancel and Connect at the foot.
    pub fn space_form(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let form = self.form.as_ref().filter(|f| f.project.is_some())?;
        let (why, kind, fixed, page, asks_user) = (
            form.why.clone(),
            form.kind(),
            form.fixed,
            form.token_page(),
            form.asks(1),
        );
        let labelled = |label: String, aside: Option<AnyElement>, field: AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(12.))
                                .text_color(theme::muted())
                                .child(label),
                        )
                        .children(aside),
                )
                .child(div().flex().child(field))
        };
        let made = page.map(|(words, url)| {
            div()
                .id("tracker-make-token")
                .flex()
                .items_center()
                .gap(px(4.))
                .text_size(px(12.))
                .text_color(theme::accent())
                .cursor_pointer()
                .child(words)
                .child(
                    icons::icon("external")
                        .size(px(11.))
                        .text_color(theme::accent()),
                )
                .on_click(move |_, _, cx| cx.open_url(&url))
                .into_any_element()
        });
        let site = (!fixed).then(|| {
            let field = self.field(0, "your-team.atlassian.net or jira.example.com", true, cx);
            labelled("Site".into(), None, field)
        });
        let user = (kind.user.clone()).map(|(label, hint)| {
            let field = self.field(1, &hint, true, cx);
            labelled(label, None, field).into_any_element()
        });
        let user = self.email_fold(asks_user, user, cx);
        let hint = match kind.id.as_str() {
            trackers::JIRA => "API token or personal access token".to_owned(),
            _ => format!("Your {} token", kind.name),
        };
        let token = self.field(2, &hint, true, cx);
        Some(
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .px(px(16.))
                        .pt(px(4.))
                        .pb(px(14.))
                        .children(site)
                        .children(user)
                        .child(labelled("Token".into(), made, token)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(16.))
                        .py(px(12.))
                        .border_t_1()
                        .border_color(theme::island_border())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(12.))
                                .text_color(theme::removed())
                                .children(why),
                        )
                        .child(
                            crate::look::small_button("tracker-space-cancel", "Cancel")
                                .h(px(28.))
                                .tip("esc")
                                .on_click(
                                    cx.listener(|this, _: &ClickEvent, _, cx| this.close(cx)),
                                ),
                        )
                        .child(
                            controls::primary_button("tracker-space-save", "Connect", "↵")
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save(cx))),
                        ),
                )
                .into_any_element(),
        )
    }

    /// One tracker's row: its mark, host and what it reaches, how it signs in or why it is not reached; Test (Retry)
    /// and Change.
    fn row(&mut self, site: &Site, cx: &mut Context<Self>) -> AnyElement {
        let host = site.host().to_owned();
        // Its projects, read once when the row is first shown: the count beside Jira.
        self.store.update(cx, |s, cx| s.read_kinds(&host, "", cx));
        let s = self.store.read(cx);
        let bad = s
            .unread
            .iter()
            .find(|(h, _)| *h == host)
            .map(|(_, w)| w.clone());
        let projects = match s.projects.get(&host) {
            Some(Read::Done(p)) => Some(p.len()),
            _ => None,
        };
        let kind = trackers::kind(&site.kind);
        let tracker = kind.name.clone();
        // Trello's projects are its boards.
        let unit = kind.unit.as_str();
        let what = match projects {
            Some(n) => format!(
                "{tracker} · {}",
                artoo_plugin_sdk::kit::format::plural(n, unit)
            ),
            None => tracker.clone(),
        };
        let how = match (&site.user, &kind.user) {
            (Some(email), _) if site.cloud() => format!("API token · {email}"),
            (Some(_), Some((label, _))) => format!("{label} and token"),
            _ if kind.id == trackers::JIRA => "Personal access token".to_owned(),
            _ => "Token".to_owned(),
        };
        let tested = self.tested.get(&host).cloned();
        let open = self
            .form
            .as_ref()
            .is_some_and(|f| f.host.as_deref() == Some(&host));
        let (h1, h2) = (host.clone(), host.clone());
        let detail = match (&bad, &tested) {
            (_, Some(Read::Done(who))) => div().text_color(theme::added()).child(who.clone()),
            (_, Some(Read::Failed(why))) => div().text_color(theme::removed()).child(why.clone()),
            (_, Some(Read::Reading)) => div().text_color(theme::faint()).child("Testing…"),
            (Some(why), None) => div()
                .text_color(theme::removed())
                .child(format!("Can’t reach {host} · {why}")),
            (None, None) => div().text_color(theme::muted()).child(how),
        };
        div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(theme::island_border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .min_h(px(52.))
                    .py(px(6.))
                    .child(mark(&kind))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .text_size(px(13.))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme::text())
                                            .child(host.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(theme::faint())
                                            .child(what),
                                    ),
                            )
                            .child(detail.text_size(px(12.)).truncate()),
                    )
                    .child(
                        crate::look::small_button(
                            SharedString::from(format!("tracker-test-{host}")),
                            if bad.is_some() { "Retry" } else { "Test" },
                        )
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| {
                                this.test(&h1, cx);
                                this.store.update(cx, |s, cx| {
                                    if !s.unread.is_empty() {
                                        s.read_boards_again(cx);
                                    }
                                });
                            },
                        )),
                    )
                    .child(
                        crate::look::small_button(
                            SharedString::from(format!("tracker-change-{host}")),
                            if open { "Close" } else { "Change" },
                        )
                        .on_click(cx.listener(
                            move |this, _: &ClickEvent, _, cx| this.open(Some(h2.clone()), cx),
                        )),
                    ),
            )
            .when(open, |d| {
                let form = self.form(cx);
                d.children(self.folding(&Self::form_key(Some(&host)), true, form))
            })
            .into_any_element()
    }

    fn draw(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let sites = self.store.read(cx).sites.clone();
        let missing = self.store.read(cx).missing.clone();
        let mut out = div()
            .flex()
            .flex_col()
            .pt(px(18.))
            .child(controls::rows_heading("Trackers"));
        for site in &sites {
            out = out.child(self.row(site, cx));
        }
        for why in missing {
            out = out.child(
                div()
                    .py(px(8.))
                    .text_size(px(12.))
                    .text_color(theme::removed())
                    .border_b_1()
                    .border_color(theme::island_border())
                    .child(why),
            );
        }
        // A new account's form in a space is the space's.
        let adding = (self.form.as_ref()).is_some_and(|f| f.host.is_none() && f.project.is_none());
        let adding_kind = (self.form.as_ref()).map_or_else(trackers::jira, Form::kind);
        if adding {
            out = out.child(
                div()
                    .flex()
                    .flex_col()
                    .border_b_1()
                    .border_color(theme::island_border())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .h(px(44.))
                            .child(mark(&adding_kind))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme::text())
                                    .child(format!("A {} site", adding_kind.name)),
                            ),
                    )
                    .children({
                        let form = self.form(cx);
                        self.folding(&Self::form_key(None), true, form)
                    }),
            );
        } else {
            out = out.child(
                div().pt(px(10.)).child(
                    crate::look::small_button("tracker-add", "Add a tracker")
                        .child(icons::icon("plus").size(px(12.)).text_color(theme::muted()))
                        .tip(adds())
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open(None, cx))),
                ),
            );
        }
        out.into_any_element()
    }
}

impl Render for Accounts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draw(cx)
    }
}

/// A tracker's mark: its letter on a tint of code colour `hue`.
/// The trackers Add takes, in words: `Jira Cloud, Server or Data Center, or Trello`.
fn adds() -> String {
    let others: Vec<String> = (trackers::kinds().into_iter())
        .filter(|k| k.id != trackers::JIRA)
        .map(|k| k.name)
        .collect();
    match others.split_last() {
        None => "Jira Cloud, Server or Data Center".into(),
        Some((last, [])) => format!("Jira Cloud, Server or Data Center, or {last}"),
        Some((last, rest)) => format!(
            "Jira Cloud, Server or Data Center, {}, or {last}",
            rest.join(", ")
        ),
    }
}

/// A tracker kind's mark: its name's first letter, on Jira's blue, Trello's or another's own hue.
pub fn mark(kind: &TrackerKind) -> impl IntoElement {
    let letter: String = kind.name.chars().take(1).collect();
    let hue = match kind.id.as_str() {
        trackers::JIRA => 2,
        trackers::TRELLO => 4,
        _ => 6,
    };
    div()
        .flex_none()
        .size(px(28.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .bg(crate::look::tint(theme::code(hue), 0.2))
        .text_color(theme::code(hue))
        .font_weight(FontWeight::BOLD)
        .child(letter)
}

/// `trackers.txt` with the line of `old` (a host) replaced by `line`, or `line` added, or the host's line taken out
/// when `line` is None; `token` written to `~/.config/tokens/<host>`, readable by you alone.
fn keep_site(
    old: Option<&str>,
    line: Option<&str>,
    host: &str,
    token: Option<&str>,
) -> Result<(), String> {
    let file = jira::settings_file().ok_or("No data folder")?;
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let mut lines: Vec<String> = Vec::new();
    let mut put = false;
    for l in text.lines() {
        let named = l.split_whitespace().next().map(|u| Site {
            url: u.trim_end_matches('/').to_owned(),
            ..Default::default()
        });
        let same = named.is_some_and(|s| Some(s.host()) == old || s.host() == host);
        if same && !l.trim_start().starts_with('#') {
            if let (Some(line), false) = (line, put) {
                lines.push(line.to_owned());
                put = true;
            }
            continue;
        }
        lines.push(l.to_owned());
    }
    if let (Some(line), false) = (line, put) {
        lines.push(line.to_owned());
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file, lines.join("\n") + "\n").map_err(|e| e.to_string())?;
    if let Some(token) = token {
        let home = std::env::var_os("HOME").ok_or("No home folder")?;
        let dir = std::path::PathBuf::from(home)
            .join(".config")
            .join("tokens");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(host);
        std::fs::write(&path, format!("{token}\n")).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// The group's rows for Settings → Accounts, its fields' input, and the accounts the space's connect card draws.
pub fn parts(
    store: Entity<Store>,
    frame: Rc<dyn Frame>,
    cx: &mut App,
) -> (
    artoo_plugin_sdk::shell::settings::Rows,
    artoo_plugin_sdk::shell::Input,
    Entity<Accounts>,
) {
    let view = cx.new(|cx| Accounts::new(store, frame.clone(), cx));
    let redraw = frame.clone();
    cx.observe(&view, move |_, cx| redraw.redraw(cx)).detach();
    let rows = artoo_plugin_sdk::shell::settings::Rows::new(
        artoo_plugin_sdk::hosts::connect::SETTINGS,
        10,
        view.clone(),
    );
    let (key, left) = (view.clone(), view.clone());
    let input = artoo_plugin_sdk::shell::Input {
        id: INPUT,
        open: crate::kept_open(&view, Accounts::typing, cx),
        key: Rc::new(move |event, _, cx| key.update(cx, |a, cx| a.key(event, cx))),
        left: Rc::new(move |cx| left.update(cx, |_, cx| cx.notify())),
        selected: Rc::new(|_| None),
        caret: Rc::new(|_| true),
    };
    (rows, input, view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jiras_email_is_asked_on_cloud_alone() {
        let asks = |url: &str| {
            let mut form = Form::new(None);
            form.fields[0] = url.into();
            form.asks(1)
        };
        assert!(asks("acme.atlassian.net"));
        assert!(asks("https://acme.atlassian.net/"));
        assert!(!asks("jira.example.com"));
        assert!(!asks("https://jira.example.com"));
        // Trello's user is its API key, asked on its own host.
        assert!(asks("trello.com"));
    }
}
