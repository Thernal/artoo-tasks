//! Settings → Accounts, its Trackers group, under the code hosts: each tracker the plugin reads, what it reaches and
//! how it signs in, Test and Change; a tracker not reached says so with Retry; Change opens its fields — the site,
//! the email (Jira Cloud), the token — and what it reaches; Add a tracker. The sites are kept in the plugin's
//! `trackers.txt`, each token in `~/.config/tokens/<host>` as the Mac keeps internal hosts' tokens.

use crate::jira::{self, Site};
use crate::model;
use crate::store::{Read, Store};
use artoo_plugin_sdk::core::textedit::TextEdit;
use artoo_plugin_sdk::gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, KeyDownEvent, SharedString, Window,
    div, prelude::*,
};
use artoo_plugin_sdk::kit::hover::Hover;
use artoo_plugin_sdk::kit::{controls, icons, input, px, theme};
use artoo_plugin_sdk::shell::{Frame, Typed};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// The fields' input: the keys while one of them is typed into.
pub const INPUT: &str = "tasks.accounts";

thread_local! {
    /// The row to open when the group is drawn next: the first run's Connect… asks for it before Settings opens.
    static WANTED: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The row of `tracker` (`jira`, a host) opened when Settings → Accounts shows next.
pub fn open_row(tracker: Option<String>, _cx: &mut App) {
    WANTED.with(|w| *w.borrow_mut() = tracker);
}

/// A tracker's fields as Change shows them.
struct Form {
    /// The host it was opened for; None for a new one.
    host: Option<String>,
    fields: [String; 3],
    edits: [TextEdit; 3],
    /// The field with the keyboard: the site, the email, the token.
    at: usize,
    why: Option<String>,
}

impl Form {
    fn new(site: Option<&Site>) -> Self {
        let url = site.map(|s| s.url.clone()).unwrap_or_default();
        let email = site.and_then(|s| s.email.clone()).unwrap_or_default();
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
        }
    }
}

pub struct Accounts {
    store: Entity<Store>,
    frame: Rc<dyn Frame>,
    form: Option<Form>,
    /// Test's answer by host: who the token is, or why not.
    tested: HashMap<String, Read<String>>,
}

impl Accounts {
    pub fn new(store: Entity<Store>, frame: Rc<dyn Frame>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Accounts {
            store,
            frame,
            form: None,
            tested: HashMap::new(),
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
        let email = form.fields[1].trim().to_owned();
        let token = form.fields[2].trim().to_owned();
        let site = Site {
            url: url.clone(),
            email: (!email.is_empty()).then_some(email.clone()),
            token: String::new(),
        };
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
        let line = if email.is_empty() {
            url
        } else {
            format!("{url} {email}")
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
        self.close(cx);
        self.store.update(cx, |s, cx| s.sites_again(cx));
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
            form.at = if key.modifiers.shift {
                (form.at + 2) % 3
            } else {
                (form.at + 1) % 3
            };
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
                    form.at += 1;
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

    fn field(&self, ix: usize, placeholder: &'static str, cx: &mut Context<Self>) -> AnyElement {
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
                .h(px(28.))
                .px(px(8.))
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
        let trello = form.fields[0].contains("trello.com");
        let cloud = !trello
            && (form.fields[0].contains("atlassian.net") || !form.fields[1].trim().is_empty());
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
        let (make, make_url) = if trello {
            (
                "Make them in Trello's Power-Up admin",
                "https://trello.com/power-ups/admin",
            )
        } else if cloud {
            (
                "Make one on id.atlassian.com",
                "https://id.atlassian.com/manage-profile/security/api-tokens",
            )
        } else {
            ("Make one in your Jira profile", "")
        };
        let site = self.field(
            0,
            "your-team.atlassian.net, jira.example.com or trello.com",
            cx,
        );
        let email = self.field(
            1,
            if trello {
                "Your Trello API key"
            } else {
                "For Jira Cloud: the email the token is of"
            },
            cx,
        );
        let token = self.field(
            2,
            if host.is_some() {
                "Leave empty to keep the token"
            } else {
                "API token or personal access token"
            },
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
            .child(line(label(if trello { "API key" } else { "Email" }), email))
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
                                .on_click(move |_, _, cx| cx.open_url(make_url)),
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
        let tracker = site.tracker();
        // Trello's projects are its boards.
        let unit = if site.trello() { "board" } else { "project" };
        let what = match projects {
            Some(n) => format!(
                "{tracker} · {}",
                artoo_plugin_sdk::kit::format::plural(n, unit)
            ),
            None => tracker.to_owned(),
        };
        let how = match &site.email {
            Some(_) if site.trello() => "API key and token".to_owned(),
            Some(email) => format!("API token · {email}"),
            None => "Personal access token".to_owned(),
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
                    .child(mark("J", 2))
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
            .when(open, |d| d.child(self.form(cx)))
            .into_any_element()
    }

    fn draw(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(want) = WANTED.with(|w| w.borrow_mut().take()) {
            let host = self
                .store
                .read(cx)
                .sites
                .iter()
                .find(|s| s.host() == want)
                .map(|s| s.host().to_owned());
            if self.form.is_none() {
                let site = host.as_ref().and_then(|h| self.store.read(cx).site_of(h));
                self.form = Some(Form::new(site.as_ref()));
                self.frame.take_keyboard(INPUT, cx);
            }
        }
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
        let adding = self.form.as_ref().is_some_and(|f| f.host.is_none());
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
                            .child(mark("J", 2))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme::text())
                                    .child("A Jira site"),
                            ),
                    )
                    .child(self.form(cx)),
            );
        } else {
            out = out.child(
                div().pt(px(10.)).child(
                    crate::look::small_button("tracker-add", "Add a tracker")
                        .child(icons::icon("plus").size(px(12.)).text_color(theme::muted()))
                        .tip("Jira Cloud, Server or Data Center, or Trello")
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
fn mark(letter: &'static str, hue: u32) -> impl IntoElement {
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
            email: None,
            token: String::new(),
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

/// The group's rows for Settings → Accounts, and its fields' input.
pub fn parts(
    store: Entity<Store>,
    frame: Rc<dyn Frame>,
    cx: &mut App,
) -> (
    artoo_plugin_sdk::shell::settings::Rows,
    artoo_plugin_sdk::shell::Input,
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
    (rows, input)
}
