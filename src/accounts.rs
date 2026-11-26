//! The accounts on trackers, everyone's whichever project reads them: Settings → Accounts, its Trackers group, under
//! the code hosts — each account, what it reaches and how it signs in, Test and Change; one not reached says so with
//! Retry; Change opens its fields — the site, the email (Jira Cloud), the token — and what it reaches; Add a tracker.
//! The space's connect card opens the same form for a new account (`open_in_space`), drawn as a card's fields, and
//! links what it saves to the project. The accounts are kept in the plugin's `accounts.txt`, each token in
//! `~/.config/tokens/<key>` (the host for the first account on a site, as the Mac keeps internal hosts' tokens; a
//! second account's key is `host~name`).

use crate::jira::{self, Site};
use crate::model;
use crate::store::{Read, Store};
use crate::trackers::{self, SiteExt};
use artoo_plugin_sdk::core::labels;
use artoo_plugin_sdk::core::textedit::TextEdit;
use artoo_plugin_sdk::gpui::{
    self, AnyElement, App, ClickEvent, Context, Entity, FontWeight, KeyDownEvent, Render,
    SharedString, Window, div, prelude::*,
};
use artoo_plugin_sdk::kit::hover::{Hover, card_frame};
use artoo_plugin_sdk::kit::{controls, icons, input, motion, px, room, theme};
use artoo_plugin_sdk::shell::marks::{self, Mark};
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
    /// The key of the account it was opened for; None for a new one.
    key: Option<String>,
    /// The key the account takes when saved, settled by Save: the one it has, or a new one.
    id: String,
    /// The site, the email, the token, the label.
    fields: [String; 4],
    edits: [TextEdit; 4],
    /// The field with the keyboard.
    at: usize,
    why: Option<String>,
    /// The fields the site's answer marked: the site not reached, the email and token refused.
    marked: Vec<usize>,
    /// The site is being read before it is kept.
    reading: bool,
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
        let label = site.map(|s| s.label.clone()).unwrap_or_default();
        Form {
            key: site.map(|s| s.key().to_owned()),
            id: String::new(),
            edits: [
                TextEdit::new(url.clone()),
                TextEdit::new(email.clone()),
                TextEdit::new(String::new()),
                TextEdit::new(label.clone()),
            ],
            fields: [url, email, String::new(), label],
            at: 0,
            why: None,
            marked: Vec::new(),
            reading: false,
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
            3 => self.kind().id == trackers::JIRA && !self.fixed,
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

    /// The field after `at` (before it, `back`), round, skipping what is not asked: the label, the site, the email,
    /// the token, as they are drawn.
    fn step(&mut self, back: bool) {
        const ORDER: [usize; 4] = [3, 0, 1, 2];
        for _ in 0..4 {
            let now = ORDER.iter().position(|&f| f == self.at).unwrap_or(0);
            self.at = ORDER[(now + if back { 3 } else { 1 }) % 4];
            if self.asks(self.at) {
                break;
            }
        }
    }

    /// What the label field shows when empty: the site typed, which is what the account is called then.
    fn label_hint(&self) -> String {
        match self.host() {
            h if h.is_empty() => "Optional, to tell accounts apart".into(),
            h => h,
        }
    }

    /// Where a token is made for the kind typed: the link's words and its address, when it is known.
    fn token_page(&self) -> Option<(String, String)> {
        let kind = self.kind();
        let jira = kind.id == trackers::JIRA;
        let cloud = jira && trackers::cloud_host(&self.host());
        let host = self.host();
        match (&kind.token_page, cloud) {
            (Some((words, url)), _) => Some((words.clone(), url.clone())),
            (None, true) => Some((
                "Make one on id.atlassian.com".into(),
                "https://id.atlassian.com/manage-profile/security/api-tokens".into(),
            )),
            // Jira Server and Data Center: the profile's tokens on the site typed (the owner, 2026-11-26: the link
            // goes where the site says, and shows).
            (None, false) if jira && !host.is_empty() => Some((
                "Make one in your Jira profile".into(),
                server_token_page(&host),
            )),
            _ => None,
        }
    }

    /// The kind of tracker the site typed is: Trello's on `trello.com`, a plugin's on its hosts, else Jira.
    fn kind(&self) -> TrackerKind {
        trackers::kind(&trackers::kind_of(&self.host()))
    }
}

/// Where Jira Server and Data Center make a personal access token: the profile's tab on `host`.
fn server_token_page(host: &str) -> String {
    let base = if host.contains("://") {
        host.trim_end_matches('/').to_owned()
    } else {
        format!("https://{}", host.trim_end_matches('/'))
    };
    format!(
        "{base}/secure/ViewProfile.jspa?selectedTab=com.atlassian.pats.pats-plugin:jira-user-personal-access-tokens"
    )
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
        if self.form.as_ref().is_some_and(|f| f.key == host) {
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

    /// The form filled for a picture: `site=…&email=…&label=…&token=…`, and `save` presses Connect.
    pub fn snapshot_fill(&mut self, fill: &str, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        for pair in fill.split('&').filter(|p| !p.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let ix = match key {
                "site" => 0,
                "email" => 1,
                "token" => 2,
                "label" => 3,
                _ => continue,
            };
            form.fields[ix] = value.to_owned();
            form.edits[ix] = TextEdit::new(value.to_owned());
            form.at = ix;
        }
        if fill.split('&').any(|p| p == "save") {
            self.save(cx);
        }
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

    /// What the form holds as a site: the address, the email when asked, the token typed and the label.
    fn entered(form: &Form) -> (Site, String) {
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
        let label = match form.asks(3) {
            true => form.fields[3].trim().to_owned(),
            false => String::new(),
        };
        let mut site = Site {
            url,
            user: (!email.is_empty()).then_some(email),
            label,
            id: form.id.clone(),
            ..Default::default()
        };
        site.kind = trackers::kind_of(site.host());
        (site, form.fields[2].trim().to_owned())
    }

    /// Save: the site is read with the token first (`jira::probe`), and kept only when it answers; a site not reached
    /// marks its field, a token refused the email and token.
    fn save(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.reading {
            return;
        }
        let (mut site, token) = Self::entered(form);
        let host = site.host().to_owned();
        let sites = self.store.read(cx).sites.clone();
        let editing = (form.key.as_deref()).and_then(|k| sites.iter().find(|s| s.key() == k));
        // The account being edited keeps its key while it stays on its site; one typed with the email of an account
        // on that site is that account; any other is a new one beside them.
        let same = |a: &Site, b: &Site| {
            a.host() == b.host()
                && a.user.as_deref().unwrap_or_default().to_lowercase()
                    == b.user.as_deref().unwrap_or_default().to_lowercase()
        };
        let known = (editing.filter(|e| e.host() == host).cloned())
            .or_else(|| sites.iter().find(|s| same(s, &site)).cloned());
        site.id = match &known {
            Some(k) => k.key().to_owned(),
            None => jira::free_key(&sites, &site),
        };
        form.id = site.id.clone();
        form.marked.clear();
        if host.is_empty() {
            form.why =
                Some("A site is needed: jira.example.com, or your-team.atlassian.net".into());
            form.marked.push(0);
            return cx.notify();
        }
        if token.is_empty() && known.is_none() {
            form.why = Some("A token is needed for a new site".into());
            form.marked.push(2);
            return cx.notify();
        }
        form.why = None;
        form.reading = true;
        site.token = match token.is_empty() {
            true => known.map_or_else(String::new, |s| s.token),
            false => token,
        };
        let read = cx.background_spawn(async move { jira::probe(&site) });
        cx.spawn(async move |this, cx| {
            let read = read.await;
            this.update(cx, |a, cx| a.probed(read, cx)).ok();
        })
        .detach();
        cx.notify();
    }

    /// The site's answer to the first read: kept when it answered, else said on the form.
    fn probed(&mut self, read: Result<(), jira::Probe>, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        form.reading = false;
        match read {
            Ok(()) => self.keep(cx),
            Err(jira::Probe::Reach(said)) => {
                form.why = Some(format!("Can’t reach the site · {said}"));
                form.marked = vec![0];
                cx.notify();
            }
            Err(jira::Probe::Refused) => {
                let asked = form.asks(1);
                form.why = Some("The site refused this email and token".into());
                form.marked = if asked { vec![1, 2] } else { vec![2] };
                cx.notify();
            }
            Err(jira::Probe::Other(why)) => {
                form.why = Some(why);
                cx.notify();
            }
        }
    }

    /// The account, answered, kept in `accounts.txt`, its token (when one is typed) in `~/.config/tokens/<key>`; the
    /// boards read again.
    fn keep(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let (site, token) = Self::entered(form);
        let key = site.key().to_owned();
        if jira::fixture().is_some() {
            form.why = Some("The fixture's trackers are its trackers.txt".into());
            return cx.notify();
        }
        let replaced = form.key.clone();
        if let Err(why) = keep_site(
            replaced.as_deref(),
            Some(&site),
            (!token.is_empty()).then_some(&token),
        ) {
            form.why = Some(why);
            return cx.notify();
        }
        let project = form.project.clone();
        // Another account, or another token for this one: what was kept of its boards may not be theirs to see.
        let forgotten = match replaced {
            Some(old) if old != key => Some(old),
            _ => (!token.is_empty()).then(|| key.clone()),
        };
        self.close(cx);
        let linked = self.store.update(cx, |s, cx| {
            if let Some(host) = forgotten {
                s.forget_boards(&host, cx);
            }
            s.sites_again(cx);
            match project {
                Some(project) => s.link(&project, &key, true, cx),
                None => Ok(()),
            }
        });
        if let Err(why) = linked {
            self.frame.error(why.into(), cx);
        }
    }

    /// Remove: the account out of `accounts.txt`; its token stays where the Mac keeps it.
    fn remove(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.form.as_ref().and_then(|f| f.key.clone()) else {
            return self.close(cx);
        };
        if let Err(why) = keep_site(Some(&key), None, None) {
            if let Some(f) = self.form.as_mut() {
                f.why = Some(why);
            }
            return cx.notify();
        }
        self.close(cx);
        self.store.update(cx, |s, cx| {
            s.forget_boards(&key, cx);
            s.sites_again(cx);
        });
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
        form.marked.retain(|&m| m != at);
        match input::edit(&mut form.fields[at], &mut form.edits[at], key, cx) {
            input::Edit::Escape => {
                self.close(cx);
                return Typed::Leave;
            }
            input::Edit::Enter { .. } => {
                if at != 2 {
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
        let marked = form.marked.contains(&ix);
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
                .border_color(match (marked, active) {
                    (true, _) => theme::removed(),
                    (_, true) => theme::focus(),
                    _ => theme::island_border(),
                })
                .when(card && (active || marked), |d| {
                    d.shadow(vec![gpui::BoxShadow {
                        color: if marked {
                            theme::removed()
                        } else {
                            theme::focus()
                        }
                        .into(),
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
        let host = form.key.clone();
        let why = form.why.clone();
        let kind = form.kind();
        let jira = kind.id == trackers::JIRA;
        let page = form.token_page();
        let asks_user = form.asks(1);
        let (form_asks_label, label_hint) = (form.asks(3), form.label_hint());
        let reached = form.reading;
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
        let named = form_asks_label.then(|| {
            let field = self.field(3, &label_hint, false, cx);
            line(label("Label"), field)
        });
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
            .children(named)
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
            .children(reached.then(|| {
                div()
                    .text_size(px(12.))
                    .text_color(theme::faint())
                    .child("Reading the site…")
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(host.is_some(), |d| {
                        d.child(
                            crate::look::button("tracker-remove", "Remove")
                                .tip_two("Stop reading this tracker", "Its token stays")
                                .on_click(
                                    cx.listener(|this, _: &ClickEvent, _, cx| this.remove(cx)),
                                ),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        crate::look::button("tracker-cancel", "Cancel")
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
        let (why, kind, fixed, page, asks_user, reading) = (
            form.why.clone(),
            form.kind(),
            form.fixed,
            form.token_page(),
            form.asks(1),
            form.reading,
        );
        let (asks_label, label_hint) = (form.asks(3), form.label_hint());
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
        let named = asks_label.then(|| {
            let field = self.field(3, &label_hint, true, cx);
            let optional = div()
                .text_size(px(12.))
                .text_color(theme::faint())
                .child("optional")
                .into_any_element();
            labelled("Label".into(), Some(optional), field)
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
        let token_name = match (kind.id.as_str(), asks_user) {
            (trackers::JIRA, true) => "API token".to_owned(),
            (trackers::JIRA, false) => "Personal access token".to_owned(),
            _ => "Token".to_owned(),
        };
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
                        .children(named)
                        .children(site)
                        .children(user)
                        .child(labelled(token_name, made, token)),
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
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .text_size(px(12.))
                                .text_color(if reading {
                                    theme::faint()
                                } else {
                                    theme::removed()
                                })
                                .children(why.as_ref().map(|_| {
                                    icons::icon("alert")
                                        .size(px(12.))
                                        .flex_none()
                                        .text_color(theme::removed())
                                }))
                                .children(
                                    why.or_else(|| reading.then(|| "Reading the site…".into())),
                                ),
                        )
                        .child(
                            crate::look::button("tracker-space-cancel", "Cancel")
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
        let host = site.key().to_owned();
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
        // Trello's projects are its boards.
        let unit = kind.unit.clone();
        let readers = s.readers(&host);
        let since = s
            .last_reached(&host)
            .map(artoo_plugin_sdk::kit::format::clock);
        // The sign-in: what it is, then who.
        let (how, who) = match (&site.user, &kind.user) {
            (Some(email), _) if site.cloud() => ("API token".to_owned(), Some(email.clone())),
            (Some(_), Some((label, _))) => (format!("{label} and token"), None),
            _ if kind.id == trackers::JIRA => ("Personal access token".to_owned(), None),
            _ => ("Token".to_owned(), None),
        };
        let tested = self.tested.get(&host).cloned();
        let open = self
            .form
            .as_ref()
            .is_some_and(|f| f.key.as_deref() == Some(&host));
        let (h1, h2) = (host.clone(), host.clone());
        let detail = match (&bad, &tested) {
            (_, Some(Read::Done(who))) => div().text_color(theme::added()).child(who.clone()),
            (_, Some(Read::Failed(why))) => div().text_color(theme::removed()).child(why.clone()),
            (_, Some(Read::Reading)) => div().text_color(theme::faint()).child("Testing…"),
            (Some(_), None) => div()
                .flex()
                .items_center()
                .gap(px(5.))
                .text_color(theme::warning())
                .child(
                    icons::icon("alert")
                        .size(px(12.))
                        .text_color(theme::warning()),
                )
                .child("Can’t reach")
                .children(since.clone()),
            (None, None) => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().text_color(theme::faint()).child(how))
                .children(who.map(|w| div().text_color(theme::muted()).child(w))),
        };
        // The hover's title and quieter line: the boards still shown, else why there are none.
        let why = bad.clone().map(|why| {
            let line = match &since {
                Some(at) => format!("Its boards as read at {at}; Retry reads them again"),
                None => why,
            };
            (format!("Can’t reach {host}"), line)
        });
        let chips = (bad.is_none() && !readers.is_empty()).then(|| {
            let extra = readers.len().saturating_sub(3);
            div()
                .id(SharedString::from(format!("tracker-reads-{host}")))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .children(
                    readers
                        .iter()
                        .take(3)
                        .map(|path| project_mark(&marks::of(path))),
                )
                .children((extra > 0).then(|| {
                    div()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(format!("+{extra}"))
                }))
                .card(move |_, cx| {
                    let rows = (readers.iter())
                        .map(|p| (marks::of(p), labels::project_name(p)))
                        .collect();
                    cx.new(|_| ReadIn(rows)).into()
                })
        });
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
                                            .child(site.named()),
                                    )
                                    .children((!site.label.is_empty()).then(|| {
                                        div()
                                            .text_size(px(12.))
                                            .text_color(theme::faint())
                                            .child(site.host().to_owned())
                                    }))
                                    .children(projects.map(|n| {
                                        div()
                                            .id(SharedString::from(format!("tracker-count-{host}")))
                                            .flex()
                                            .items_center()
                                            .gap(px(3.))
                                            .text_size(px(12.))
                                            .text_color(theme::faint())
                                            .child(
                                                icons::icon("layers")
                                                    .size(px(11.))
                                                    .text_color(theme::faint()),
                                            )
                                            .child(n.to_string())
                                            .tip(artoo_plugin_sdk::kit::format::plural(n, &unit))
                                    })),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("tracker-detail-{host}")))
                                    .child(detail.text_size(px(12.)).truncate())
                                    .when_some(why, |d, (title, line)| d.tip_two(title, line)),
                            ),
                    )
                    .children(chips)
                    .children(
                        bad.is_some()
                            .then(|| div().text_color(theme::faint()).child("–")),
                    )
                    .child(
                        test_button(
                            SharedString::from(format!("tracker-test-{host}")),
                            bad.is_some(),
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
                        controls::row_button(
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
        let adding = (self.form.as_ref()).is_some_and(|f| f.key.is_none() && f.project.is_none());
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
                // As the design draws it: a button as wide as its words, its + first (the owner, 2026-11-26).
                div().pt(px(10.)).flex().child(
                    crate::look::small_button("tracker-add", "")
                        .child(icons::icon("plus").size(px(12.)).text_color(theme::muted()))
                        .child("Add a tracker")
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

/// A project in an account's row: the mark the rail draws for it.
fn project_mark(mark: &Mark) -> gpui::Div {
    mark.draw(20.).rounded(px(5.)).text_size(px(9.))
}

/// The card over an account's marks: every project that reads it, its mark and its name.
struct ReadIn(Vec<(Mark, String)>);

impl Render for ReadIn {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        card_frame()
            .w_auto()
            .min_w(px(200.))
            .max_w(px(300.))
            .px(px(10.))
            .py(px(8.))
            .gap(px(6.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child("Read in"),
            )
            .children(self.0.iter().map(|(mark, name)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(project_mark(mark))
                    .child(div().min_w_0().truncate().child(name.clone()))
            }))
    }
}

/// Test as a glyph with its sentence for a tooltip; a host not reached keeps its word, Retry.
fn test_button(id: SharedString, unreached: bool) -> gpui::Stateful<gpui::Div> {
    match unreached {
        true => controls::row_button(id, "Retry").tip("Read its boards again"),
        // On a button's ground beside Change, as the design draws it (the owner, 2026-11-26).
        false => {
            controls::row_icon_button(id, "activity").tip_two("Test", "Who the token is, read now")
        }
    }
}

/// `accounts.txt` with the line of account `old` (a key) replaced by `site`'s, or `site`'s added, or `old`'s line taken
/// out (`site` none); the token, when given, in `~/.config/tokens/<key>`. A later account's key is written on its
/// line, so it holds when the one before it is gone.
fn keep_site(old: Option<&str>, site: Option<&Site>, token: Option<&str>) -> Result<(), String> {
    let file = jira::settings_file().ok_or("No data folder")?;
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let mut entries = jira::entries(&text).into_iter();
    let mut lines: Vec<String> = Vec::new();
    let mut put = false;
    for l in text.lines() {
        if l.trim().is_empty() || l.trim_start().starts_with('#') {
            lines.push(l.to_owned());
            continue;
        }
        let Some((_, kept)) = entries.next() else {
            lines.push(l.to_owned());
            continue;
        };
        let same = Some(kept.key()) == old || site.is_some_and(|s| s.key() == kept.key());
        if same {
            if let (Some(site), false) = (site, put) {
                lines.push(jira::line_of(site));
                put = true;
            }
            continue;
        }
        lines.push(match kept.key() == kept.host() {
            true => l.to_owned(),
            false => jira::line_of(&kept),
        });
    }
    if let (Some(site), false) = (site, put) {
        lines.push(jira::line_of(site));
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
        let path = dir.join(site.map_or_else(String::new, |s| s.key().to_owned()));
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
