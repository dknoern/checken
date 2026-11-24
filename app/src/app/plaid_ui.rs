//! Bank connections through Plaid: connect a bank (new accounts, or feed
//! existing ones), update all connected accounts, and the Plaid settings.
//! The network work runs on a background thread; its results come back as
//! messages and are applied here. See crate::plaid for the details.

use super::*;
use crate::plaid::{self, Access, Applied, BankAccount, Billing, Endpoint, LinkOutcome, PlaidItem, PlaidLink, SyncResult};
use std::sync::mpsc::{Receiver, Sender, channel};

/// What a bank account in a new connection should become.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Choice {
    Skip,
    New(AccountKind),
    Existing(u32),
}

/// The "Connect accounts" window shown after Plaid hands back a connection.
pub struct ConnectDialog {
    item_id: String,
    institution: String,
    accounts: Vec<BankAccount>,
    choices: Vec<Choice>,
}

/// What to carry on with once the person has subscribed to Bank Sync.
#[derive(Clone, Debug, PartialEq)]
pub enum After {
    Connect(Option<u32>),
    ConnectFromServer,
    Refresh(Option<String>),
    Nothing,
}

/// The "Turn on Bank Sync" window: shown the first time a bank is connected
/// (or updated) on a server that charges for it.
pub struct SubscribeDialog {
    pub billing: Billing,
    pub after: After,
    /// Stripe's payment page is open in the browser; waiting for it.
    pub checkout: Option<String>,
}

pub enum Msg {
    Progress(String),
    /// The server issued this copy of Checken its key: keep it.
    Key(String),
    /// Bank Sync needs a subscription first.
    NeedSubscription(Billing, After),
    /// Stripe's payment page is open in the browser.
    CheckoutOpened(String),
    /// Stripe confirmed the subscription.
    Subscribed,
    /// Show Plaid Link (a page on 127.0.0.1) in Checken's own window.
    OpenLink(String),
    Connected { item_id: String, institution: String, accounts: Vec<BankAccount> },
    Synced { item_id: String, result: Result<SyncResult, String> },
    Failed(String),
    /// A background job finished (with nothing more to report).
    Idle,
}

#[derive(Default)]
pub struct PlaidUi {
    rx: Option<Receiver<Msg>>,
    /// What's running now, shown in the status line.
    pub busy: Option<String>,
    connect: Option<ConnectDialog>,
    /// Accounts made by a connection, waiting for their first download to set
    /// their starting balance to the bank's (Checken account → Plaid account).
    fresh: Vec<(u32, String)>,
    /// The account a "Connect to Bank" started from, offered first.
    target: Option<u32>,
    settings: Option<(String, String)>,
    /// Updates finished in this run of the job, for the summary.
    summary: Applied,
    failures: Vec<String>,
    /// An update asked for while another job was running; it runs next.
    queued: Option<Option<String>>,
    /// Plaid Link's native window while a bank is being connected.
    link_window: Option<super::link_window::LinkWindow>,
    /// Set when the person closes that window, so the wait for Plaid ends.
    link_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// A check run closes the Plaid window this long after starting.
    pub test_close_after: Option<std::time::Instant>,
    ctx: Option<egui::Context>,
    pub subscribe: Option<SubscribeDialog>,
    /// What to do when the current job ends (after subscribing).
    after: Option<After>,
    /// Set when the person stops waiting for Stripe.
    billing_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The subscription as last read for the Bank Connections window.
    pub billing: Option<Result<Option<Billing>, String>>,
    billing_rx: Option<Receiver<(Result<Option<Billing>, String>, Option<String>)>>,
}

/// Before bank work: registers this Checken with the server if needed and
/// checks its Bank Sync subscription. False means stop: the person has to
/// subscribe first (the window for that opens) or the server didn't answer.
fn gate(server: &mut Endpoint, after: After, say: &dyn Fn(Msg)) -> bool {
    match plaid::check_access(server) {
        Err(e) => {
            say(Msg::Failed(format!("The Plaid server at {server} didn't answer: {e}")));
            false
        }
        Ok((access, key)) => {
            if let Some(k) = key {
                say(Msg::Key(k));
            }
            match access {
                Access::Ready => true,
                Access::NeedsSubscription(b) => {
                    say(Msg::NeedSubscription(b, after));
                    false
                }
            }
        }
    }
}

impl App {
    fn plaid_job(&mut self, what: &str, ctx: &egui::Context, job: impl FnOnce(&Sender<Msg>) + Send + 'static) {
        if self.plaid.busy.is_some() {
            self.status = "Still busy with the bank; try again in a moment.".into();
            return;
        }
        let (tx, rx) = channel();
        self.plaid.ctx = Some(ctx.clone());
        self.plaid.rx = Some(rx);
        self.plaid.busy = Some(what.to_string());
        self.plaid.summary = Applied::default();
        self.plaid.failures.clear();
        let wake = ctx.clone();
        std::thread::spawn(move || {
            job(&tx);
            let _ = tx.send(Msg::Idle);
            wake.request_repaint();
        });
    }

    /// File > Connect a Bank Account: Plaid Link in the browser, then choose
    /// what each bank account becomes. The server keeps the bank's access
    /// token; Checken only learns the connection's item id.
    pub(super) fn plaid_connect(&mut self, ctx: &egui::Context, target: Option<u32>) {
        self.plaid.target = target;
        let mut server = self.ledger.plaid.endpoint();
        let wake = ctx.clone();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.plaid.link_cancel = cancel.clone();
        self.plaid_job("Connecting to your bank in the Plaid window…", ctx, move |tx| {
            let say = |m: Msg| {
                let _ = tx.send(m);
                wake.request_repaint();
            };
            if !gate(&mut server, After::Connect(target), &say) {
                return;
            }
            let link_token = match plaid::create_link_token(&server) {
                Ok(t) => t,
                Err(e) => return say(Msg::Failed(format!("The Plaid server at {server} didn't answer: {e}"))),
            };
            // Link shows in Checken's own window (opened on the main thread).
            let open = |url: &str| {
                let _ = tx.send(Msg::OpenLink(url.to_string()));
                wake.request_repaint();
            };
            let outcome = match plaid::run_link(&link_token, &open, &cancel) {
                Ok(o) => o,
                Err(e) => return say(Msg::Failed(e)),
            };
            let (public_token, institution) = match outcome {
                LinkOutcome::Connected { public_token, institution } => (public_token, institution),
                LinkOutcome::Cancelled(why) => return say(Msg::Failed(why)),
            };
            say(Msg::Progress("Connected. Reading your accounts…".into()));
            let item_id = match plaid::exchange(&server, &public_token) {
                Ok(v) => v,
                Err(e) => return say(Msg::Failed(e)),
            };
            match plaid::accounts(&server, &item_id) {
                Ok((accounts, name)) => say(Msg::Connected { item_id, institution: if name.is_empty() { institution } else { name }, accounts }),
                Err(e) => say(Msg::Failed(e)),
            }
        });
    }

    /// Adds a bank the server already holds (say, one connected in the Plaid
    /// demo at localhost:3000) that Checken isn't using yet.
    pub(super) fn plaid_connect_from_server(&mut self, ctx: &egui::Context) {
        let mut server = self.ledger.plaid.endpoint();
        let known: Vec<String> = self.ledger.plaid.items.iter().map(|i| i.item_id.clone()).collect();
        let wake = ctx.clone();
        self.plaid_job("Looking for banks connected on the Plaid server…", ctx, move |tx| {
            let say = |m: Msg| {
                let _ = tx.send(m);
                wake.request_repaint();
            };
            if !gate(&mut server, After::ConnectFromServer, &say) {
                return;
            }
            let _ = plaid::hand_over_legacy_tokens(&server, &known);
            let items = match plaid::server_items(&server) {
                Ok(v) => v,
                Err(e) => return say(Msg::Failed(format!("The Plaid server at {server} didn't answer: {e}"))),
            };
            let Some((item_id, _)) = items.into_iter().find(|(id, _)| !known.contains(id)) else {
                return say(Msg::Failed("Every bank on the Plaid server is already connected to Checken. To add another, use Connect a Bank Account.".into()));
            };
            match plaid::accounts(&server, &item_id) {
                Ok((accounts, name)) => say(Msg::Connected { item_id, institution: name, accounts }),
                Err(e) => say(Msg::Failed(e)),
            }
        });
    }

    /// Downloads what's new for every connection (or one).
    pub(super) fn plaid_refresh(&mut self, ctx: &egui::Context, only: Option<String>) {
        let items: Vec<PlaidItem> = self.ledger.plaid.items.iter().filter(|i| only.as_ref().is_none_or(|o| *o == i.item_id)).cloned().collect();
        if items.is_empty() {
            self.status = "No bank accounts are connected yet. Use Connect a Bank Account (File menu or ⋯).".into();
            return;
        }
        // Busy with something else (say, reading a new connection's accounts):
        // this update runs as soon as that finishes.
        if self.plaid.busy.is_some() {
            self.plaid.queued = Some(only);
            return;
        }
        let mut server = self.ledger.plaid.endpoint();
        let in_use: Vec<String> = self.ledger.plaid.items.iter().map(|i| i.item_id.clone()).collect();
        let wake = ctx.clone();
        let what = if items.len() == 1 { format!("Updating {} from the bank…", items[0].institution) } else { format!("Updating {} bank connections…", items.len()) };
        self.plaid_job(&what, ctx, move |tx| {
            let say = |m: Msg| {
                let _ = tx.send(m);
                wake.request_repaint();
            };
            if !gate(&mut server, After::Refresh(only), &say) {
                return;
            }
            // Keys an older Checken kept go to the server first (once). Keys
            // for connections that are gone are dropped, and a problem here
            // is only mentioned: the update itself goes ahead.
            if let Err(e) = plaid::hand_over_legacy_tokens(&server, &in_use) {
                say(Msg::Failed(format!("(A bank key Checken saved earlier couldn't be handed to the Plaid server yet: {e})")));
            }
            for item in items {
                let result = plaid::sync(&server, &item.item_id, &item.cursor);
                say(Msg::Synced { item_id: item.item_id, result });
            }
        });
    }

    /// Stops syncing an account (its transactions stay). When none of a
    /// connection's accounts are left, the bank is disconnected at Plaid.
    pub(super) fn plaid_unlink(&mut self, account: u32) {
        let Some(link) = self.ledger.plaid.links.remove(&account) else { return };
        self.plaid_forget_if_unused(&link.item_id);
        self.status = format!("{} no longer updates from the bank; its transactions stay.", self.ledger.account_name(account));
        self.dirty = true;
    }

    /// Disconnects a connection none of whose accounts Checken uses any more:
    /// the server removes it at Plaid and forgets its key.
    pub(super) fn plaid_forget_if_unused(&mut self, item_id: &str) {
        if self.ledger.plaid.links.values().any(|l| l.item_id == item_id) {
            return;
        }
        self.ledger.plaid.items.retain(|i| i.item_id != item_id);
        let server = self.ledger.plaid.endpoint();
        let item = item_id.to_string();
        std::thread::spawn(move || {
            let _ = plaid::remove_item(&server, &item);
        });
    }

    /// Picks up messages from the background job; call every frame.
    pub(super) fn plaid_poll(&mut self) {
        if let (Some(t), Some(w)) = (self.plaid.test_close_after, self.plaid.link_window.as_mut()) {
            if t.elapsed() > std::time::Duration::from_secs(6) {
                self.plaid.test_close_after = None;
                w.close();
            }
        }
        // The person closed the Plaid window: stop waiting for it.
        if self.plaid.link_window.as_ref().is_some_and(|w| !w.is_open()) {
            self.plaid.link_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            self.plaid.link_window = None;
        }
        if let Some(rx) = &self.plaid.billing_rx {
            if let Ok((status, key)) = rx.try_recv() {
                self.plaid.billing_rx = None;
                if let Some(k) = key {
                    self.ledger.plaid.subscriber_key = k;
                    self.dirty = true;
                }
                self.plaid.billing = Some(status);
            }
        }
        let Some(rx) = &self.plaid.rx else { return };
        let msgs: Vec<Msg> = rx.try_iter().collect();
        for m in msgs {
            match m {
                Msg::OpenLink(url) => {
                    self.plaid.link_window = super::link_window::LinkWindow::open(&url);
                    if self.plaid.link_window.is_none() && cfg!(target_os = "macos") {
                        // No window could be made: fall back to the browser.
                        let _ = std::process::Command::new("/usr/bin/open").arg(&url).spawn();
                    }
                }
                Msg::Progress(p) => {
                    // Plaid is done with the person: its window goes away.
                    if let Some(mut w) = self.plaid.link_window.take() {
                        w.close();
                    }
                    self.plaid.busy = Some(p);
                }
                Msg::Failed(e) => {
                    if let Some(mut w) = self.plaid.link_window.take() {
                        w.close();
                    }
                    self.plaid.failures.push(e);
                }
                Msg::Connected { item_id, institution, accounts } => {
                    let choices = self.default_choices(&item_id, &accounts);
                    let institution = if institution.is_empty() { "your bank".to_string() } else { institution };
                    self.plaid.connect = Some(ConnectDialog { item_id, institution, accounts, choices });
                    // A check run (CHECKEN_PLAID=demo-sync) accepts the suggestions and downloads.
                    if self.plaid_auto_accept {
                        self.plaid_auto_accept = false;
                        if let (Some(d), Some(ctx)) = (self.plaid.connect.take(), self.plaid_ctx.clone()) {
                            self.plaid_finish_connect(&ctx, d);
                        }
                    }
                }
                Msg::Synced { item_id, result } => self.plaid_apply(&item_id, result),
                Msg::Key(k) => {
                    self.ledger.plaid.subscriber_key = k;
                    self.dirty = true;
                }
                Msg::NeedSubscription(billing, after) => {
                    self.plaid.subscribe = Some(SubscribeDialog { billing, after, checkout: None });
                }
                Msg::CheckoutOpened(url) => {
                    if let Some(d) = self.plaid.subscribe.as_mut() {
                        d.checkout = Some(url);
                    }
                }
                Msg::Subscribed => {
                    let after = self.plaid.subscribe.take().map(|d| d.after).unwrap_or(After::Nothing);
                    self.plaid.billing = None;
                    self.status = "Bank Sync is on. Thank you!".into();
                    self.plaid.after = Some(after);
                }
                Msg::Idle => {
                    if let Some(mut w) = self.plaid.link_window.take() {
                        w.close();
                    }
                    self.plaid.rx = None;
                    self.plaid.busy = None;
                    let after = self.plaid.after.take();
                    if let (Some(after), Some(ctx)) = (after.clone(), self.plaid.ctx.clone()) {
                        // Just subscribed: carry on with what the person asked for.
                        match after {
                            After::Connect(target) => self.plaid_connect(&ctx, target),
                            After::ConnectFromServer => self.plaid_connect_from_server(&ctx),
                            After::Refresh(only) => self.plaid_refresh(&ctx, only),
                            After::Nothing => {}
                        }
                    } else if let (Some(only), Some(ctx)) = (self.plaid.queued.take(), self.plaid.ctx.clone()) {
                        self.plaid_refresh(&ctx, only);
                    } else if self.plaid.subscribe.is_none() || !self.plaid.failures.is_empty() {
                        self.plaid_report();
                    }
                }
            }
        }
    }

    fn plaid_apply(&mut self, item_id: &str, result: Result<SyncResult, String>) {
        let name = self.ledger.plaid.item(item_id).map(|i| i.institution.clone()).unwrap_or_default();
        match result {
            Err(e) => {
                if let Some(i) = self.ledger.plaid.items.iter_mut().find(|i| i.item_id == item_id) {
                    i.last_error = Some(e.clone());
                }
                self.plaid.failures.push(format!("{name}: {e}"));
                self.dirty = true;
            }
            Ok(res) => {
                // On a connection's first download, every linked account that has
                // no transactions yet (new, or an empty one the person picked)
                // starts where the bank says it is now.
                let first_download = self.ledger.plaid.item(item_id).is_some_and(|i| i.cursor.is_empty());
                let mut fresh: Vec<(u32, String)> = std::mem::take(&mut self.plaid.fresh);
                if first_download {
                    for (acct, link) in self.ledger.plaid.links.iter().filter(|(_, l)| l.item_id == item_id) {
                        let empty = !self.ledger.transactions.iter().any(|t| t.account == *acct || t.transfer == Some(*acct));
                        if empty && !fresh.iter().any(|(a, _)| a == acct) {
                            fresh.push((*acct, link.account_id.clone()));
                        }
                    }
                }
                let out = plaid::apply(&mut self.ledger, item_id, &res);
                for (acct, plaid_id) in fresh {
                    let bank = res.balances.iter().find(|b| b.account_id == plaid_id).and_then(|b| b.signed_balance());
                    let in_txns: f64 = self.ledger.transactions.iter().map(|t| Ledger::effect_on(t, acct)).sum();
                    let first = self.ledger.transactions.iter().filter(|t| t.account == acct || t.transfer == Some(acct)).map(|t| t.date).min();
                    if let Some(a) = self.ledger.accounts.iter_mut().find(|a| a.id == acct) {
                        if let Some(b) = bank {
                            a.opening_balance = ((b - in_txns) * 100.0).round() / 100.0;
                        }
                        if let Some(d) = first {
                            a.opening_date = d;
                        }
                    }
                }
                let s = &mut self.plaid.summary;
                s.added += out.added;
                s.updated += out.updated;
                s.removed += out.removed;
                s.matched += out.matched;
                s.pending_skipped += out.pending_skipped;
                self.dirty = true;
                self.budget_buf_month = None;
            }
        }
    }

    fn plaid_report(&mut self) {
        if self.plaid.connect.is_some() {
            return;
        }
        let s = &self.plaid.summary;
        let mut msg = if s.added + s.updated + s.removed + s.matched > 0 || self.plaid.failures.is_empty() {
            let mut parts = vec![format!("{} new transaction{}", s.added, if s.added == 1 { "" } else { "s" })];
            if s.matched > 0 {
                parts.push(format!("{} matched to ones already here", s.matched));
            }
            if s.updated > 0 {
                parts.push(format!("{} updated", s.updated));
            }
            if s.removed > 0 {
                parts.push(format!("{} removed by the bank", s.removed));
            }
            if s.pending_skipped > 0 {
                parts.push(format!("{} pending (they'll come once they post)", s.pending_skipped));
            }
            format!("Bank update: {}.", parts.join(", "))
        } else {
            String::new()
        };
        if !self.plaid.failures.is_empty() {
            if !msg.is_empty() {
                msg.push(' ');
            }
            msg.push_str(&self.plaid.failures.join(" "));
        }
        if !msg.is_empty() {
            self.status = msg;
        }
    }

    /// Bank accounts go into an unlinked Checken account with the same name
    /// or the same last four digits; otherwise into a new account.
    fn default_choices(&self, item_id: &str, accounts: &[BankAccount]) -> Vec<Choice> {
        let mut used: Vec<u32> = Vec::new();
        let mut target = self.plaid.target;
        accounts
            .iter()
            .map(|b| {
                // Already linked from an earlier connection of this bank.
                if let Some((acct, _)) = self.ledger.plaid.links.iter().find(|(_, l)| l.item_id == item_id && l.account_id == b.account_id) {
                    used.push(*acct);
                    return Choice::Existing(*acct);
                }
                let free = |a: &Account| !a.closed && !self.ledger.plaid.links.contains_key(&a.id) && !used.contains(&a.id);
                let by_name = self.ledger.accounts.iter().find(|a| {
                    free(a) && (a.name.eq_ignore_ascii_case(&b.name) || (!b.mask.is_empty() && a.name.contains(&b.mask)) || (!b.official_name.is_empty() && a.name.eq_ignore_ascii_case(&b.official_name)))
                });
                if let Some(a) = by_name {
                    used.push(a.id);
                    return Choice::Existing(a.id);
                }
                if let Some(t) = target.take().filter(|t| self.ledger.account(*t).is_some_and(free)) {
                    used.push(t);
                    return Choice::Existing(t);
                }
                Choice::New(b.checken_kind())
            })
            .collect()
    }

    /// The "Connect accounts from <bank>" window.
    pub(super) fn plaid_dialogs(&mut self, ctx: &egui::Context) {
        self.plaid_connect_dialog(ctx);
        self.plaid_settings_dialog(ctx);
        self.plaid_subscribe_dialog(ctx);
    }

    /// Opens Stripe's payment page in the browser and waits (up to 30
    /// minutes) for Stripe to confirm, checking every few seconds; then
    /// carries on with what the person was doing.
    fn plaid_start_checkout(&mut self, ctx: &egui::Context) {
        let server = self.ledger.plaid.endpoint();
        let wake = ctx.clone();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.plaid.billing_cancel = cancel.clone();
        self.plaid_job("Waiting for your payment with Stripe in the browser…", ctx, move |tx| {
            let say = |m: Msg| {
                let _ = tx.send(m);
                wake.request_repaint();
            };
            let url = match plaid::checkout_url(&server) {
                Ok(Some(u)) => u,
                Ok(None) => return say(Msg::Subscribed),
                Err(e) => return say(Msg::Failed(format!("Couldn't start the payment: {e}"))),
            };
            if let Err(e) = plaid::open_in_browser(&url) {
                say(Msg::Failed(e));
            }
            say(Msg::CheckoutOpened(url));
            let start = std::time::Instant::now();
            while start.elapsed() < std::time::Duration::from_secs(30 * 60) {
                for _ in 0..12 {
                    if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                if let Ok(Some(b)) = plaid::billing_status(&server) {
                    if b.active {
                        return say(Msg::Subscribed);
                    }
                }
            }
            say(Msg::Failed("Stopped waiting for the payment. If you paid, try connecting again in a minute.".into()));
        });
    }

    /// The "Turn on Bank Sync" window.
    fn plaid_subscribe_dialog(&mut self, ctx: &egui::Context) {
        let Some(d) = self.plaid.subscribe.as_ref() else { return };
        let price = d.billing.price_text();
        let waiting = d.checkout.clone();
        let lapsed = !matches!(d.billing.status.as_str(), "" | "none");
        let mut subscribe = false;
        let mut reopen = false;
        let mut close = false;
        let width = 560.0;
        egui::Modal::new(egui::Id::unique("plaid_subscribe")).frame(dialog_frame()).show(ctx, |ui| {
            ui.set_width(width);
            egui::Frame::new().inner_margin(Margin { left: 28, right: 28, top: 24, bottom: 20 }).show(ui, |ui| {
                ui.set_width(width - 56.0);
                ui.label(RichText::new(if lapsed { "Renew Bank Sync" } else { "Turn on Bank Sync" }).size(22.0).color(NAVY));
                ui.add_space(8.0);
                ui.label(
                    "Checken is free for everything on your computer: entering transactions, budgets, reports, and \
                     importing Quicken, Mint, CSV and QFX/OFX files.",
                );
                ui.add_space(6.0);
                ui.label(format!(
                    "Downloading transactions straight from your bank goes through Plaid, which charges for every connected bank. \
                     Bank Sync covers that for {price}."
                ));
                ui.add_space(10.0);
                for line in [
                    "Connect as many banks and cards as you like",
                    "New transactions with one click (⟳ or ⌘R)",
                    "Cancel anytime from Bank Connections",
                ] {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("✓").color(GREEN).strong());
                        ui.label(line);
                    });
                }
                ui.add_space(10.0);
                ui.label(RichText::new("You pay with Stripe in your web browser; Checken never sees your card.").color(MUTED));
                ui.add_space(16.0);
                if waiting.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Finish paying in your browser. Checken carries on as soon as Stripe confirms.").color(NAVY));
                    });
                    ui.add_space(12.0);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if waiting.is_none() {
                        let b = egui::Button::new(RichText::new(format!("Subscribe for {price}")).size(15.0).color(Color32::WHITE))
                            .fill(ACCENT)
                            .stroke(Stroke::NONE)
                            .min_size(egui::vec2(190.0, 32.0))
                            .corner_radius(6.0);
                        if ui.add_enabled(self.plaid.busy.is_none(), b).clicked() {
                            subscribe = true;
                        }
                        if ui.add(egui::Button::new(RichText::new("Not Now").size(15.0)).min_size(egui::vec2(96.0, 32.0)).corner_radius(6.0)).clicked() {
                            close = true;
                        }
                    } else {
                        if ui.add(egui::Button::new(RichText::new("Cancel").size(15.0)).min_size(egui::vec2(96.0, 32.0)).corner_radius(6.0)).clicked() {
                            close = true;
                        }
                        if ui.add(egui::Button::new(RichText::new("Open the Payment Page Again").size(15.0)).min_size(egui::vec2(96.0, 32.0)).corner_radius(6.0)).clicked() {
                            reopen = true;
                        }
                    }
                });
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if close {
            self.plaid.billing_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
            self.plaid.subscribe = None;
            self.status = "Bank Sync isn't on, so no bank was connected. Everything else in Checken works as before.".into();
        } else if reopen {
            if let Some(url) = waiting {
                if let Err(e) = plaid::open_in_browser(&url) {
                    self.status = e;
                }
            }
        } else if subscribe {
            self.plaid_start_checkout(ctx);
        }
    }

    /// Reads the subscription for the Bank Connections window, in the background.
    fn plaid_check_billing(&mut self, ctx: &egui::Context) {
        if self.plaid.billing_rx.is_some() || self.shot.is_some() {
            return;
        }
        let (tx, rx) = channel();
        self.plaid.billing_rx = Some(rx);
        self.plaid.billing = None;
        let mut server = self.ledger.plaid.endpoint();
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let result = plaid::check_access(&mut server).and_then(|(_, key)| plaid::billing_status(&server).map(|b| (b, key)));
            let _ = match result {
                Ok((b, key)) => tx.send((Ok(b), key)),
                Err(e) => tx.send((Err(e), None)),
            };
            wake.request_repaint();
        });
    }

    /// Stripe's customer portal in the browser.
    fn plaid_manage_subscription(&mut self, ctx: &egui::Context) {
        let server = self.ledger.plaid.endpoint();
        let wake = ctx.clone();
        self.plaid_job("Opening your subscription with Stripe…", ctx, move |tx| {
            let r = plaid::portal_url(&server).and_then(|u| plaid::open_in_browser(&u));
            if let Err(e) = r {
                let _ = tx.send(Msg::Failed(format!("Couldn't open the subscription page: {e}")));
            }
            wake.request_repaint();
        });
    }

    fn plaid_connect_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.plaid.connect.take() else { return };
        let mut close = false;
        let mut go = false;
        let width = 760.0;
        let accts: Vec<(u32, String)> = self
            .ledger
            .accounts
            .iter()
            .filter(|a| !a.closed && self.ledger.plaid.links.get(&a.id).is_none_or(|l| l.item_id == d.item_id))
            .map(|a| (a.id, a.name.clone()))
            .collect();
        egui::Modal::new(egui::Id::unique("plaid_connect")).frame(dialog_frame()).show(ctx, |ui| {
            ui.set_width(width);
            egui::Frame::new().inner_margin(Margin { left: 26, right: 26, top: 22, bottom: 18 }).show(ui, |ui| {
                ui.set_width(width - 52.0);
                ui.label(RichText::new(format!("Connect accounts from {}", d.institution)).size(22.0).color(NAVY));
                ui.add_space(6.0);
                ui.label(RichText::new("Choose where each bank account's transactions go: a new Checken account, an account you already have (its history stays; matching transactions aren't added twice), or nowhere.").color(MUTED));
                ui.add_space(12.0);
                egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                    egui::Grid::new("plaid_accts").num_columns(4).spacing([16.0, 8.0]).striped(true).show(ui, |ui| {
                        for h in ["At the bank", "Bank balance", "Goes into", "Kind"] {
                            ui.label(RichText::new(h).color(MUTED));
                        }
                        ui.end_row();
                        for (i, b) in d.accounts.iter().enumerate() {
                            ui.allocate_ui_with_layout(egui::vec2(220.0, 22.0), Layout::left_to_right(Align::Center), |ui| {
                                ui.set_min_width(220.0);
                                ui.add(egui::Label::new(b.label()).truncate()).on_hover_text(format!("{}\n{} {}", b.official_name, b.kind, b.subtype));
                            });
                            let bal = b.signed_balance();
                            ui.label(bal.map(money).unwrap_or_else(|| "—".into()));
                            let cur = d.choices[i];
                            let shown = match cur {
                                Choice::Skip => "Don't download".to_string(),
                                Choice::New(_) => format!("New account: {}", b.name),
                                Choice::Existing(id) => self.ledger.account_name(id),
                            };
                            let mut pick = cur;
                            egui::ComboBox::from_id_salt(("plaid_into", i)).width(240.0).height(400.0).selected_text(shown).show_ui(ui, |ui| {
                                ui.selectable_value(&mut pick, Choice::New(b.checken_kind()), format!("New account: {}", b.name));
                                ui.selectable_value(&mut pick, Choice::Skip, "Don't download");
                                ui.separator();
                                for (id, n) in &accts {
                                    ui.selectable_value(&mut pick, Choice::Existing(*id), n);
                                }
                            });
                            match &mut pick {
                                Choice::New(kind) => {
                                    egui::ComboBox::from_id_salt(("plaid_kind", i)).width(120.0).selected_text(kind.label()).show_ui(ui, |ui| {
                                        for k in AccountKind::LIST_ORDER {
                                            ui.selectable_value(kind, k, k.label());
                                        }
                                    });
                                }
                                Choice::Existing(_) => {
                                    ui.label(RichText::new("existing").color(MUTED));
                                }
                                Choice::Skip => {
                                    ui.label("");
                                }
                            }
                            d.choices[i] = pick;
                            ui.end_row();
                        }
                    });
                });
                ui.add_space(10.0);
                ui.label(RichText::new("Checken downloads the transactions the bank has (usually about the last 90 days); pending ones come once they post. New accounts start at the bank's balance.").color(MUTED));
                ui.add_space(14.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let any = d.choices.iter().any(|c| *c != Choice::Skip);
                    let b = egui::Button::new(RichText::new("Connect and Download").size(15.0).color(Color32::WHITE))
                        .fill(if any { ACCENT } else { Color32::from_gray(170) })
                        .stroke(Stroke::NONE)
                        .min_size(egui::vec2(170.0, 30.0))
                        .corner_radius(6.0);
                    if ui.add_enabled(any, b).clicked() {
                        go = true;
                    }
                    if ui.add(egui::Button::new(RichText::new("Cancel").size(15.0)).min_size(egui::vec2(90.0, 30.0)).corner_radius(6.0)).clicked() {
                        close = true;
                    }
                });
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if go && !close {
            self.plaid_finish_connect(ctx, d);
        } else if !close {
            self.plaid.connect = Some(d);
        } else {
            self.status = "The bank wasn't connected.".into();
        }
    }

    fn plaid_finish_connect(&mut self, ctx: &egui::Context, d: ConnectDialog) {
        // The server already keeps this connection's key; Checken keeps its id.
        if self.ledger.plaid.item(&d.item_id).is_none() {
            self.ledger.plaid.items.push(PlaidItem { item_id: d.item_id.clone(), institution: d.institution.clone(), ..Default::default() });
        }
        let mut fresh = Vec::new();
        for (b, c) in d.accounts.iter().zip(&d.choices) {
            let acct = match *c {
                Choice::Skip => {
                    self.ledger.plaid.links.retain(|_, l| !(l.item_id == d.item_id && l.account_id == b.account_id));
                    continue;
                }
                Choice::Existing(id) => {
                    // Moving an account to another connection (say, after
                    // reconnecting the bank): what the old connection downloaded
                    // now counts as the person's own, so the new connection
                    // matches it instead of adding it again.
                    let moved = self.ledger.plaid.links.get(&id).is_some_and(|l| l.item_id != d.item_id);
                    if moved {
                        let in_acct: std::collections::HashSet<u64> =
                            self.ledger.transactions.iter().filter(|t| t.account == id || t.transfer == Some(id)).map(|t| t.id).collect();
                        self.ledger.plaid.txns.retain(|_, tid| !in_acct.contains(tid));
                        self.ledger.plaid.created.retain(|tid| !in_acct.contains(tid));
                    }
                    id
                }
                Choice::New(kind) => {
                    let id = self.ledger.next_account_id();
                    let mut name = b.name.clone();
                    let mut n = 2;
                    while self.ledger.accounts.iter().any(|a| a.name.eq_ignore_ascii_case(&name)) {
                        name = format!("{} {n}", b.name);
                        n += 1;
                    }
                    self.ledger.accounts.push(Account { id, name, kind, opening_balance: 0.0, opening_date: today(), closed: false });
                    fresh.push((id, b.account_id.clone()));
                    id
                }
            };
            // One Checken account follows one bank account.
            self.ledger.plaid.links.retain(|a, l| *a != acct && !(l.item_id == d.item_id && l.account_id == b.account_id));
            self.ledger.plaid.links.insert(acct, PlaidLink { item_id: d.item_id.clone(), account_id: b.account_id.clone(), name: b.name.clone(), mask: b.mask.clone() });
        }
        self.plaid.fresh = fresh;
        // Forget downloads whose transactions no longer exist (deleted accounts and such).
        let ids: std::collections::HashSet<u64> = self.ledger.transactions.iter().map(|t| t.id).collect();
        self.ledger.plaid.txns.retain(|_, tid| ids.contains(tid));
        self.ledger.plaid.created.retain(|tid| ids.contains(tid));
        // A connection none of whose accounts are used any more is let go.
        let old: Vec<String> = self.ledger.plaid.items.iter().map(|i| i.item_id.clone()).filter(|i| *i != d.item_id).collect();
        for item in old {
            self.plaid_forget_if_unused(&item);
        }
        self.dirty = true;
        self.view = View::Register;
        self.acct_filter = None;
        self.scope = None;
        self.date_filter = register::DateFilter::All;
        self.plaid_refresh(ctx, Some(d.item_id));
    }

    /// File > Plaid Settings.
    pub(super) fn plaid_open_settings(&mut self) {
        self.plaid.settings = Some((self.ledger.plaid.server(), String::new()));
    }

    fn plaid_settings_dialog(&mut self, ctx: &egui::Context) {
        let Some((mut server, env)) = self.plaid.settings.take() else { return };
        let mut close = false;
        let mut save = false;
        let mut demo = false;
        let mut manage = false;
        let mut subscribe = false;
        if self.plaid.billing.is_none() {
            self.plaid_check_billing(ctx);
        }
        let billing = self.plaid.billing.clone();
        let width = 620.0;
        let conns: Vec<(String, String, Option<String>, Option<String>, usize)> = self
            .ledger
            .plaid
            .items
            .iter()
            .map(|i| (i.item_id.clone(), i.institution.clone(), i.last_sync.clone(), i.last_error.clone(), self.ledger.plaid.links.values().filter(|l| l.item_id == i.item_id).count()))
            .collect();
        egui::Modal::new(egui::Id::unique("plaid_settings")).frame(dialog_frame()).show(ctx, |ui| {
            ui.set_width(width);
            egui::Frame::new().inner_margin(Margin { left: 26, right: 26, top: 22, bottom: 18 }).show(ui, |ui| {
                ui.set_width(width - 52.0);
                ui.label(RichText::new("Bank Connections (Plaid)").size(22.0).color(NAVY));
                ui.add_space(10.0);
                egui::Grid::new("plaid_settings_grid").num_columns(2).spacing([12.0, 10.0]).show(ui, |ui| {
                    ui.label("Plaid server:");
                    ui.add(egui::TextEdit::singleline(&mut server).desired_width(400.0));
                    ui.end_row();
                    ui.label("");
                    ui.label(
                        RichText::new("The server keeps your Plaid keys and each bank's access key, as Plaid recommends. Checken only knows each connection's id.")
                            .color(MUTED),
                    );
                    ui.end_row();
                });
                ui.add_space(12.0);
                ui.label(semibold("Bank Sync subscription").size(14.0).color(NAVY));
                ui.add_space(4.0);
                match &billing {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Checking…").color(MUTED));
                        });
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(format!("Couldn't check: {e}")).color(RED));
                    }
                    Some(Ok(None)) => {
                        ui.label("Free on this server: it doesn't charge for Bank Sync.");
                    }
                    Some(Ok(Some(b))) => {
                        ui.label(b.summary());
                        ui.horizontal(|ui| {
                            if b.can_manage && ui.button("Manage Subscription…").on_hover_text("Change your card, see receipts or cancel, with Stripe in your browser").clicked() {
                                manage = true;
                            }
                            if b.enabled && !b.active && ui.button(format!("Subscribe ({})…", b.price_text())).clicked() {
                                subscribe = true;
                            }
                        });
                    }
                }
                ui.add_space(12.0);
                ui.label(semibold("Connected banks").size(14.0).color(NAVY));
                ui.add_space(4.0);
                if conns.is_empty() {
                    ui.label(RichText::new("None yet.").color(MUTED));
                }
                for (_, inst, last, err, n) in &conns {
                    let when = last.clone().map(|l| format!("updated {l}")).unwrap_or_else(|| "not updated yet".into());
                    ui.label(format!("{inst}: {n} account{}, {when}", if *n == 1 { "" } else { "s" }));
                    if let Some(e) = err {
                        ui.label(RichText::new(format!("Last update failed: {e}")).color(RED));
                    }
                }
                ui.add_space(10.0);
                if ui.button("Add a bank already connected on the Plaid server (e.g. in the demo at localhost:3000)").clicked() {
                    demo = true;
                }
                ui.add_space(14.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let b = egui::Button::new(RichText::new("Save").size(15.0).color(Color32::WHITE)).fill(ACCENT).stroke(Stroke::NONE).min_size(egui::vec2(100.0, 30.0)).corner_radius(6.0);
                    if ui.add(b).clicked() {
                        save = true;
                    }
                    if ui.add(egui::Button::new(RichText::new("Cancel").size(15.0)).min_size(egui::vec2(90.0, 30.0)).corner_radius(6.0)).clicked() {
                        close = true;
                    }
                });
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if manage {
            self.plaid_manage_subscription(ctx);
        }
        if subscribe {
            if let Some(Ok(Some(b))) = billing {
                self.plaid.subscribe = Some(SubscribeDialog { billing: b, after: After::Nothing, checkout: None });
            }
            close = true;
        }
        if close {
            // Read it again next time the window opens.
            self.plaid.billing = None;
        }
        if (save || demo) && !close {
            let server_t = server.trim().trim_end_matches('/').to_string();
            let new_url = if server_t == "http://localhost:8000" { String::new() } else { server_t };
            if new_url != self.ledger.plaid.server_url {
                // A different server: this Checken registers with it afresh.
                self.ledger.plaid.subscriber_key.clear();
            }
            self.ledger.plaid.server_url = new_url;
            self.plaid.billing = None;
            self.dirty = true;
            if demo {
                self.plaid_connect_from_server(ctx);
            } else {
                self.status = "Saved the Plaid settings.".into();
            }
            return;
        }
        if !close {
            self.plaid.settings = Some((server, env));
        }
    }

    /// The register subtitle's note for a connected account.
    pub(super) fn plaid_note(&self, account: u32) -> Option<String> {
        let link = self.ledger.plaid.links.get(&account)?;
        let item = self.ledger.plaid.item(&link.item_id)?;
        let mask = if link.mask.is_empty() { String::new() } else { format!(" ••{}", link.mask) };
        let when = match (&item.last_error, &item.last_sync) {
            (Some(_), _) => "last update failed".to_string(),
            (None, Some(t)) => format!("updated {t}"),
            (None, None) => "not updated yet".into(),
        };
        Some(format!("Connected to {}{mask}  ·  {when}", item.institution))
    }
}

pub(super) fn dialog_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .corner_radius(10.0)
        .stroke(Stroke::new(1.0, Color32::from_gray(190)))
        .shadow(egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(70) })
}
