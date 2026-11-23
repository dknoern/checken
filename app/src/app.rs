//! The window: account sidebar, top navigation, bottom toolbar, and the view switch.

mod accounts;
mod budget;
mod entry;
mod filemenu;
mod home;
mod importer;
mod link_window;
mod plaid_ui;
mod library;
mod print;
mod register;
mod reports;
mod widgets;

use crate::model::*;
use chrono::NaiveDate;
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, Margin, RichText, Sense, Stroke};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
pub use widgets::*;
use accounts::CHOICE_BANKING;

/// Near-black text for titles.
pub const NAVY: Color32 = Color32::from_rgb(0x1d, 0x1d, 0x1f);
pub const ACCENT: Color32 = Color32::from_rgb(0x2b, 0x62, 0xc4);
pub const RED: Color32 = Color32::from_rgb(0xd0, 0x2c, 0x2c);
pub const GREEN: Color32 = Color32::from_rgb(0x2f, 0x9a, 0x3a);
pub const CLEAR_GREEN: Color32 = Color32::from_rgb(0x5c, 0xb5, 0x47);
pub const ORANGE: Color32 = Color32::from_rgb(0xf0, 0x8c, 0x2e);
pub const DOT_BLUE: Color32 = Color32::from_rgb(0x3d, 0x9b, 0xe9);
pub const SIDEBAR: Color32 = Color32::from_rgb(0xee, 0xee, 0xee);
pub const BAND: Color32 = Color32::from_rgb(0xf3, 0xf3, 0xf3);
pub const MUTED: Color32 = Color32::from_rgb(0x70, 0x70, 0x74);
pub const LINE: Color32 = Color32::from_rgb(0xdd, 0xdd, 0xdd);
pub const PAPER: Color32 = Color32::WHITE;

#[derive(Clone, Copy, PartialEq)]
enum View {
    Home,
    Register,
    Budget,
    Reports,
    Categories,
}

struct App {
    ledger: Ledger,
    view: View,
    /// Which account the register shows; None means all accounts.
    acct_filter: Option<u32>,
    /// With no single account: a heading's accounts ("Savings", "Banking"); None means all.
    scope: Option<Scope>,
    reg_tab: register::RegTab,
    date_filter: register::DateFilter,
    status_filter: register::StatusFilter,
    cat_filter: Option<String>,
    search: String,
    selected: Option<u64>,
    /// The account of the selected line, so only one side of a transfer lights up.
    selected_acct: Option<u32>,
    form: Option<entry::TxnForm>,
    confirm_delete: bool,
    collapsed: BTreeSet<String>,
    show_hidden: bool,
    /// The left pane folded down to a narrow strip of buttons.
    sidebar_collapsed: bool,
    /// Height of the top bar last frame, so the sidebar's divider lines up with it.
    top_h: f32,
    account_form: Option<accounts::AccountForm>,
    budget_month: NaiveDate,
    budget_buf: BTreeMap<String, String>,
    budget_buf_month: Option<NaiveDate>,
    new_cat: String,
    new_cat_kind: CatKind,
    /// The category a new one is added under (a subcategory), if any.
    new_cat_parent: Option<String>,
    /// Categories whose subcategories are folded away (Categories and Budgets).
    cat_collapsed: BTreeSet<String>,
    cat_rename: Option<(String, String)>,
    /// The kind and date range of the report being viewed.
    report: ReportKind,
    period: Period,
    report_folder: library::Folder,
    report_sel: Option<library::ReportKey>,
    report_open: Option<library::ReportKey>,
    report_search: String,
    report_editor: Option<library::ReportEditor>,
    report_confirm_delete: bool,
    standard_open: bool,
    recent_reports: Vec<library::ReportKey>,
    menu_bar: Option<filemenu::MenuBar>,
    pending_restore: Option<filemenu::PendingRestore>,
    /// File > Purge All Data's confirmation, while it's open.
    pending_purge: Option<filemenu::PurgeDialog>,
    pending_import: Option<importer::PendingImport>,
    /// Duplicate imported transactions waiting for the person to confirm.
    pending_dedupe: Option<Vec<u64>>,
    /// An account waiting for the person to confirm Delete….
    pending_delete_account: Option<u32>,
    /// Bank connections through Plaid: background work and its windows.
    plaid: plaid_ui::PlaidUi,
    /// A Plaid action to start on the first frame (CHECKEN_PLAID, for checks).
    plaid_autostart: Option<String>,
    /// Frames a screenshot run has waited for bank work.
    plaid_wait: u32,
    /// A check run accepts the connect window's suggestions by itself.
    plaid_auto_accept: bool,
    plaid_ctx: Option<egui::Context>,
    dirty: bool,
    status: String,
    shot: Option<PathBuf>,
    frames: u32,
    shot_requested: bool,
    shot_img: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,
}

impl App {
    fn new(shot: Option<PathBuf>) -> App {
        if shot.is_some() {
            // A screenshot run never writes the person's files, bank keys included.
            crate::plaid::practice_run();
        }
        // A screenshot run reads the data but never changes files.
        let mut ledger = Ledger::load_with(shot.is_none());
        // CHECKEN_PLAID_SERVER points a check run at another Plaid server.
        if let Ok(server) = std::env::var("CHECKEN_PLAID_SERVER") {
            ledger.plaid.server_url = server;
        }
        // POCKET_LEDGER_VIEW picks the opening screen (handy for screenshots).
        let env = std::env::var("POCKET_LEDGER_VIEW").unwrap_or_default();
        let view = match env.as_str() {
            "home" => View::Home,
            "budget" => View::Budget,
            "reports" => View::Reports,
            "categories" => View::Categories,
            _ => View::Register,
        };
        // CHECKEN_ACCOUNT=<name> opens that account's register (for checks).
        let named = std::env::var("CHECKEN_ACCOUNT").ok().and_then(|n| ledger.accounts.iter().find(|a| a.name == n).map(|a| a.id));
        let acct_filter = if named.is_some() {
            named
        } else if env == "account" {
            ledger.accounts.iter().find(|a| !a.closed).map(|a| a.id)
        } else {
            None
        };
        // Start on My Reports when there are some saved, otherwise on all of them.
        let report_folder = if ledger.reports.is_empty() { library::Folder::All } else { library::Folder::Mine };
        let mut app = App {
            ledger,
            view,
            acct_filter,
            scope: None,
            reg_tab: if env == "spending" { register::RegTab::Spending } else { register::RegTab::Transactions },
            date_filter: register::DateFilter::ThisYear,
            status_filter: register::StatusFilter::Any,
            cat_filter: None,
            search: String::new(),
            selected: None,
            selected_acct: None,
            form: None,
            confirm_delete: false,
            collapsed: BTreeSet::new(),
            show_hidden: false,
            sidebar_collapsed: std::env::var("POCKET_LEDGER_SIDEBAR").as_deref() == Ok("collapsed"),
            top_h: 61.0,
            account_form: None,
            budget_month: first_of_month(today()),
            budget_buf: BTreeMap::new(),
            budget_buf_month: None,
            new_cat: String::new(),
            new_cat_kind: CatKind::Expense,
            new_cat_parent: None,
            cat_collapsed: BTreeSet::new(),
            cat_rename: None,
            report: ReportKind::BudgetVsActual,
            period: Period::ThisMonth,
            report_folder,
            report_sel: None,
            report_open: None,
            report_search: String::new(),
            report_editor: None,
            report_confirm_delete: false,
            standard_open: true,
            recent_reports: Vec::new(),
            menu_bar: None,
            pending_restore: None,
            pending_purge: None,
            pending_import: None,
            pending_dedupe: None,
            pending_delete_account: None,
            plaid: Default::default(),
            plaid_autostart: std::env::var("CHECKEN_PLAID").ok(),
            plaid_wait: 0,
            plaid_auto_accept: false,
            plaid_ctx: None,
            dirty: false,
            status: "Changes are saved automatically.".into(),
            shot,
            frames: 0,
            shot_requested: false,
            shot_img: Arc::new(Mutex::new(None)),
        };
        // CHECKEN_IMPORT=<file.csv> opens the import preview for that file at
        // start (handy for screenshots); CHECKEN_IMPORT_APPLY=1 also imports it.
        if let Some(path) = std::env::var_os("CHECKEN_IMPORT") {
            app.load_import(std::path::Path::new(&path), importer::Source::Quicken);
            if std::env::var("CHECKEN_IMPORT_APPLY").as_deref() == Ok("1") {
                if let Some(p) = app.pending_import.take() {
                    app.finish_import(p);
                }
            }
        }
        // POCKET_LEDGER_VIEW=savings / banking opens that heading's register.
        match env.as_str() {
            "savings" => app.scope = Some(Scope::Kind(AccountKind::Savings)),
            "banking" => app.scope = Some(Scope::Group("Banking")),
            _ => {}
        }
        // CHECKEN_DELETE_ACCOUNT=<name> opens the Delete… confirmation for that account at start.
        if let Ok(name) = std::env::var("CHECKEN_DELETE_ACCOUNT") {
            app.pending_delete_account = app.ledger.accounts.iter().find(|a| a.name == name).map(|a| a.id);
        }
        // CHECKEN_PURGE=1 opens the Purge All Data confirmation at start (for checks).
        if std::env::var("CHECKEN_PURGE").as_deref() == Ok("1") {
            app.pending_purge = Some(filemenu::PurgeDialog::default());
        }
        // CHECKEN_DEDUPE=1 opens the Remove Duplicate Imports check at start.
        if std::env::var("CHECKEN_DEDUPE").as_deref() == Ok("1") {
            app.find_import_duplicates();
        }
        app
    }

    fn go(&mut self, view: View) {
        if self.view != view {
            self.view = view;
            self.form = None;
            self.confirm_delete = false;
        }
    }

    /// Show the register for one account (or all of them).
    fn open_register(&mut self, account: Option<u32>) {
        self.view = View::Register;
        if self.acct_filter != account || self.scope.is_some() {
            self.acct_filter = account;
            self.scope = None;
            self.selected = None;
            self.form = None;
            self.cat_filter = None;
            self.confirm_delete = false;
        }
    }

    /// Show the register for every account under a heading, as Quicken does
    /// when "Savings" or "Banking" is selected.
    fn open_scope(&mut self, scope: Scope) {
        self.view = View::Register;
        if self.scope != Some(scope) || self.acct_filter.is_some() {
            self.acct_filter = None;
            self.scope = Some(scope);
            self.selected = None;
            self.form = None;
            self.cat_filter = None;
            self.confirm_delete = false;
        }
    }

    /// Whether the register (with no single account chosen) includes this account.
    fn in_scope(&self, a: &Account) -> bool {
        match self.scope {
            Some(s) => s.includes(a),
            None => !a.closed,
        }
    }

    /// Open accounts of one kind in list order (by name, as Quicken lists them).
    fn list_accounts(&self, kind: AccountKind) -> Vec<&Account> {
        let mut v: Vec<&Account> = self.ledger.accounts.iter().filter(|a| !a.closed && a.kind == kind).collect();
        v.sort_by_key(|a| a.name.to_lowercase());
        v
    }

    fn category_text(&self, t: &Transaction, viewing: u32) -> String {
        if !t.splits.is_empty() {
            return entry::SPLIT_LABEL.to_string();
        }
        if let Some(x) = t.transfer {
            let other = if t.account == viewing { x } else { t.account };
            format!("Transfer: {}", self.ledger.account_name(other))
        } else if t.category.is_empty() {
            "Uncategorized".into()
        } else {
            t.category.clone()
        }
    }

    fn has_uncleared(&self, account: u32) -> bool {
        self.ledger
            .transactions
            .iter()
            .any(|t| t.cleared == Cleared::No && (t.account == account || t.transfer == Some(account)))
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let tabs: [(&str, View); 4] = [("Home", View::Home), ("Reports", View::Reports), ("Budgets", View::Budget), ("Categories", View::Categories)];
        let gap = 30.0;
        let widths: Vec<f32> = tabs
            .iter()
            .map(|(l, _)| ui.painter().layout_no_wrap(l.to_string(), semibold_font(14.5), NAVY).size().x)
            .collect();
        let total: f32 = widths.iter().sum::<f32>() + gap * (tabs.len() as f32 - 1.0);
        ui.horizontal(|ui| {
            ui.set_height(32.0);
            ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
            ui.spacing_mut().item_spacing.x = gap;
            for (label, v) in tabs {
                let sel = self.view == v;
                let text = semibold(label).size(14.5).color(if sel { ACCENT } else { NAVY });
                let r = ui.add(egui::Label::new(text).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand);
                if sel {
                    let rr = r.rect;
                    ui.painter().hline(rr.x_range(), rr.bottom() + 4.0, Stroke::new(2.0, ACCENT));
                }
                if r.clicked() {
                    self.go(v);
                }
            }
        });
    }

    /// The strip at the top of the left pane, as tall as the top bar, with a
    /// divider under it that lines up with the top bar's.
    fn sidebar_header(&mut self, ui: &mut egui::Ui, add: impl FnOnce(&mut Self, &mut egui::Ui)) {
        let panel = ui.max_rect().expand2(egui::vec2(8.0, 10.0));
        let line_y = panel.top() + self.top_h;
        ui.horizontal(|ui| {
            ui.set_height((line_y - ui.cursor().top() - 2.0).max(30.0));
            add(self, ui);
        });
        ui.painter().hline(panel.x_range(), line_y, Stroke::new(1.0, Color32::from_gray(212)));
        let gap = line_y + 10.0 - ui.cursor().top();
        if gap > 0.0 {
            ui.add_space(gap);
        }
    }

    fn more_menu(&mut self, more: &egui::Response) {
        egui::Popup::menu(more).show(|ui| {
            if ui.button("Add account…").clicked() {
                self.open_account_form(None);
            }
            if ui.button("Save a Backup…").clicked() {
                ui.close();
                self.save_backup();
            }
            if ui.button("Restore from Backup…").clicked() {
                ui.close();
                self.choose_restore();
            }
            if ui.button("Import Quicken CSV File…").clicked() {
                ui.close();
                self.choose_import(importer::Source::Quicken);
            }
            if ui.button("Import Mint.com File (CSV)…").clicked() {
                ui.close();
                self.choose_import(importer::Source::Mint);
            }
            if ui.button("Import Bank File (QFX, OFX)…").clicked() {
                ui.close();
                self.choose_import(importer::Source::BankFile);
            }
            if ui.button("Remove Duplicate Imports…").clicked() {
                ui.close();
                self.find_import_duplicates();
            }
            ui.separator();
            if ui.button("Connect a Bank Account (Plaid)…").clicked() {
                ui.close();
                let ctx = ui.ctx().clone();
                self.plaid_connect(&ctx, None);
            }
            if ui.button("Update All Bank Accounts").clicked() {
                ui.close();
                let ctx = ui.ctx().clone();
                self.plaid_refresh(&ctx, None);
            }
            if ui.button("Bank Connections (Plaid)…").clicked() {
                ui.close();
                self.plaid_open_settings();
            }
            ui.separator();
            if ui.button(RichText::new("Purge All Data…").color(RED)).clicked() {
                ui.close();
                self.pending_purge = Some(filemenu::PurgeDialog::default());
            }
            ui.separator();
            if let Some(id) = self.acct_filter {
                if ui.button("Edit this account…").clicked() {
                    self.open_account_form(Some(id));
                }
                if ui.button("Delete this account…").clicked() {
                    ui.close();
                    self.pending_delete_account = Some(id);
                }
            }
            let label = if self.show_hidden { "Hide closed accounts" } else { "Show closed accounts" };
            if ui.button(label).clicked() {
                self.show_hidden = !self.show_hidden;
                self.sidebar_collapsed = false;
            }
            let label = if self.sidebar_collapsed { "Show sidebar" } else { "Hide sidebar" };
            if ui.button(format!("{label}   ⌃⌘S")).clicked() {
                self.sidebar_collapsed = !self.sidebar_collapsed;
            }
        });
    }

    /// The left pane folded down: the toggle, then round buttons for the
    /// account list, adding an account, and the More menu.
    fn sidebar_rail(&mut self, ui: &mut egui::Ui) {
        self.sidebar_header(ui, |app, ui| {
            ui.add_space(((ui.available_width() - 34.0) / 2.0).max(0.0));
            if icon_button(ui, Icon::Sidebar, "Show the sidebar (⌃⌘S)").clicked() {
                app.sidebar_collapsed = false;
            }
        });
        let any_new = self.ledger.accounts.iter().any(|a| !a.closed && self.has_uncleared(a.id));
        let mut goto: Option<Option<u32>> = None;
        let mut goscope: Option<Scope> = None;
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 12.0;
            let accts = circle_button_badge(ui, Icon::Dollar, "Accounts", any_new);
            egui::Popup::menu(&accts).show(|ui| {
                ui.set_min_width(300.0);
                ui.spacing_mut().item_spacing.y = 1.0;
                let on_register = self.view == View::Register;
                if account_row(ui, "All Transactions", None, None, on_register && self.acct_filter.is_none() && self.scope.is_none(), true).clicked() {
                    goto = Some(None);
                }
                for group in LIST_GROUPS {
                    let mut accts: Vec<&Account> = self.ledger.accounts.iter().filter(|a| !a.closed && a.kind.list_group() == group).collect();
                    if accts.is_empty() {
                        continue;
                    }
                    accts.sort_by_key(|a| (AccountKind::LIST_ORDER.iter().position(|k| *k == a.kind), a.name.to_lowercase()));
                    ui.add_space(4.0);
                    let total: f64 = accts.iter().map(|a| self.ledger.balance(a.id)).sum();
                    let sel = on_register && self.acct_filter.is_none() && self.scope == Some(Scope::Group(group));
                    if account_row(ui, group, Some(total), None, sel, true).clicked() {
                        goscope = Some(Scope::Group(group));
                    }
                    for a in accts {
                        let selected = on_register && self.acct_filter == Some(a.id);
                        let dot = self.has_uncleared(a.id).then_some(DOT_BLUE);
                        if account_row(ui, &a.name, Some(self.ledger.balance(a.id)), dot, selected, false).clicked() {
                            goto = Some(Some(a.id));
                        }
                    }
                }
            });
            if circle_button(ui, Icon::Plus, "Add an account").clicked() {
                self.open_account_form(None);
            }
            let more = circle_button(ui, Icon::More, "More");
            self.more_menu(&more);
        });
        if let Some(a) = goto {
            self.open_register(a);
        }
        if let Some(s) = goscope {
            self.open_scope(s);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        self.sidebar_header(ui, |app, ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.add_space(24.0);
            // Update every bank-connected account, as Quicken's ⟳ button does.
            let tip = if app.plaid.busy.is_some() { "Updating from the bank…" } else { "Update bank accounts" };
            if circle_button(ui, Icon::Refresh, tip).clicked() {
                let ctx = ui.ctx().clone();
                app.plaid_refresh(&ctx, None);
            }
            if circle_button(ui, Icon::Plus, "Add an account").clicked() {
                app.open_account_form(None);
            }
            let more = circle_button(ui, Icon::More, "More");
            app.more_menu(&more);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, Icon::Sidebar, "Hide the sidebar (⌃⌘S)").clicked() {
                    app.sidebar_collapsed = true;
                }
            });
        });

        let mut goto: Option<Option<u32>> = None;
        let mut open_edit: Option<u32> = None;
        let mut toggle_close: Option<u32> = None;
        let mut ask_delete: Option<u32> = None;
        let mut bank: Option<BankAct> = None;
        let mut toggle_group: Option<String> = None;
        let mut goscope: Option<Scope> = None;
        let on_register = self.view == View::Register;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let all_selected = on_register && self.acct_filter.is_none() && self.scope.is_none();
            if account_row(ui, "All Transactions", None, None, all_selected, true).clicked() {
                goto = Some(None);
            }
            ui.add_space(6.0);
            // Quicken's hierarchy: Banking > Checking, Savings, Credit Card, Cash;
            // Investing > Brokerage; Property & Debt > Asset, Loan. Every heading
            // can be selected to see its accounts' transactions together.
            for group in LIST_GROUPS {
                let accts: Vec<&Account> = self.ledger.accounts.iter().filter(|a| !a.closed && a.kind.list_group() == group).collect();
                if accts.is_empty() {
                    continue;
                }
                let total: f64 = accts.iter().map(|a| self.ledger.balance(a.id)).sum();
                let open = !self.collapsed.contains(group);
                let sel = on_register && self.acct_filter.is_none() && self.scope == Some(Scope::Group(group));
                let (resp, fold) = heading_row(ui, group, Some(total), Some(open), sel);
                if fold {
                    toggle_group = Some(group.to_string());
                } else if resp.clicked() {
                    goscope = Some(Scope::Group(group));
                }
                if !open {
                    ui.add_space(6.0);
                    continue;
                }
                for kind in AccountKind::LIST_ORDER.iter().copied().filter(|k| k.list_group() == group) {
                    let of_kind = self.list_accounts(kind);
                    if of_kind.is_empty() {
                        continue;
                    }
                    ui.add_space(2.0);
                    let sel = on_register && self.acct_filter.is_none() && self.scope == Some(Scope::Kind(kind));
                    if heading_row(ui, kind.label(), None, None, sel).0.clicked() {
                        goscope = Some(Scope::Kind(kind));
                    }
                    for a in of_kind {
                        let selected = on_register && self.acct_filter == Some(a.id);
                        let dot = self.has_uncleared(a.id).then_some(DOT_BLUE);
                        let resp = account_row(ui, &a.name, Some(self.ledger.balance(a.id)), dot, selected, false);
                        if resp.clicked() {
                            goto = Some(Some(a.id));
                        }
                        // Right-click, as in Quicken's sidebar.
                        let linked = self.ledger.plaid.links.get(&a.id).map(|l| l.item_id.clone());
                        resp.context_menu(|ui| {
                            if ui.button("Account Settings…").clicked() {
                                open_edit = Some(a.id);
                                ui.close();
                            }
                            if ui.button("Rename…").clicked() {
                                open_edit = Some(a.id);
                                ui.close();
                            }
                            ui.separator();
                            match &linked {
                                Some(item) => {
                                    if ui.button("Update from Bank").clicked() {
                                        bank = Some(BankAct::Update(item.clone()));
                                        ui.close();
                                    }
                                    if ui.button("Stop Updating from Bank").on_hover_text("Keep the account and its transactions; stop downloading").clicked() {
                                        bank = Some(BankAct::Unlink(a.id));
                                        ui.close();
                                    }
                                }
                                None => {
                                    if ui.button("Connect to Bank (Plaid)…").on_hover_text("Download this account's transactions from the bank").clicked() {
                                        bank = Some(BankAct::Connect(a.id));
                                        ui.close();
                                    }
                                }
                            }
                            ui.separator();
                            if ui.button("Hide…").on_hover_text("Keep the account and its transactions, but take it out of the list").clicked() {
                                toggle_close = Some(a.id);
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Delete…").on_hover_text("Delete the account and its transactions").clicked() {
                                ask_delete = Some(a.id);
                                ui.close();
                            }
                        });
                    }
                }
                ui.add_space(8.0);
            }
            ui.add_space(2.0);
            ui.separator();
            let nw = self.ledger.net_worth_at(None);
            ui.horizontal(|ui| {
                ui.add_space(14.0);
                ui.label(semibold("Total").size(14.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(6.0);
                    ui.label(semibold(money_whole(nw)).size(14.0).color(amount_color(nw)));
                });
            });
            ui.separator();
            let closed: Vec<&Account> = self.ledger.accounts.iter().filter(|a| a.closed).collect();
            if !closed.is_empty() {
                let r = ui.horizontal(|ui| {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(16.0, 22.0), Sense::click());
                    chevron(ui, rect.center(), self.show_hidden);
                    let l = ui.add(egui::Label::new(semibold("Hidden Accounts").size(14.0)).sense(Sense::click()));
                    resp.clicked() || l.clicked()
                });
                if r.inner {
                    self.show_hidden = !self.show_hidden;
                }
                if self.show_hidden {
                    for a in closed {
                        let selected = on_register && self.acct_filter == Some(a.id);
                        let resp = account_row(ui, &a.name, Some(self.ledger.balance(a.id)), None, selected, false);
                        if resp.clicked() {
                            goto = Some(Some(a.id));
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Account Settings…").clicked() {
                                open_edit = Some(a.id);
                                ui.close();
                            }
                            if ui.button("Show in Account List").clicked() {
                                toggle_close = Some(a.id);
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Delete…").on_hover_text("Delete the account and its transactions").clicked() {
                                ask_delete = Some(a.id);
                                ui.close();
                            }
                        });
                    }
                }
            }
        });
        if let Some(g) = toggle_group {
            if !self.collapsed.remove(&g) {
                self.collapsed.insert(g);
            }
        }
        if let Some(a) = goto {
            self.open_register(a);
        }
        if let Some(id) = open_edit {
            self.open_account_form(Some(id));
        }
        if ask_delete.is_some() {
            self.pending_delete_account = ask_delete;
        }
        match bank {
            Some(BankAct::Update(item)) => {
                let ctx = ui.ctx().clone();
                self.plaid_refresh(&ctx, Some(item));
            }
            Some(BankAct::Unlink(id)) => self.plaid_unlink(id),
            Some(BankAct::Connect(id)) => {
                let ctx = ui.ctx().clone();
                self.plaid_connect(&ctx, Some(id));
            }
            None => {}
        }
        if let Some(s) = goscope {
            self.open_scope(s);
        }
        if let Some(id) = toggle_close {
            if let Some(a) = self.ledger.accounts.iter_mut().find(|a| a.id == id) {
                a.closed = !a.closed;
                self.status = format!("{} is now {}.", a.name, if a.closed { "hidden" } else { "open" });
                self.dirty = true;
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if self.view == View::Register {
                self.register_toolbar_left(ui);
            }
            ui.add_space(16.0);
            match &self.plaid.busy {
                Some(b) => {
                    ui.add(egui::Spinner::new().size(14.0));
                    ui.label(RichText::new(b).color(ACCENT));
                }
                None => {
                    ui.label(RichText::new(&self.status).color(MUTED));
                }
            }
            if self.view == View::Register {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| self.register_toolbar_right(ui));
            }
        });
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(path) = self.shot.clone() else { return };
        // Bank work started for a check gets up to a minute to finish first.
        if self.plaid.busy.is_some() && self.frames < 3600 {
            self.frames = self.frames.max(1);
            self.plaid_wait += 1;
            if self.plaid_wait < 3600 {
                ctx.request_repaint();
                return;
            }
        }
        self.frames += 1;
        ctx.request_repaint();
        if self.frames >= 120 && !self.shot_requested {
            self.shot_requested = true;
            let slot = self.shot_img.clone();
            ctx.request_screenshot(move |img| {
                *slot.lock().unwrap() = Some(img);
            });
        }
        let taken = self.shot_img.lock().unwrap().take();
        if let Some(img) = taken {
            let [w, h] = img.size;
            let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            let ok = image::RgbaImage::from_raw(w as u32, h as u32, bytes)
                .map(|buf| buf.save(&path).map_err(|e| eprintln!("could not save screenshot: {e}")).is_ok())
                .unwrap_or(false);
            std::process::exit(if ok { 0 } else { 1 });
        }
    }
}

fn chevron(ui: &egui::Ui, c: egui::Pos2, open: bool) {
    let s = 4.0;
    let st = Stroke::new(1.5, Color32::from_gray(90));
    let pts = if open {
        [egui::pos2(c.x - s, c.y - s / 2.0), egui::pos2(c.x, c.y + s / 2.0), egui::pos2(c.x + s, c.y - s / 2.0)]
    } else {
        [egui::pos2(c.x - s / 2.0, c.y - s), egui::pos2(c.x + s / 2.0, c.y), egui::pos2(c.x - s / 2.0, c.y + s)]
    };
    ui.painter().line_segment([pts[0], pts[1]], st);
    ui.painter().line_segment([pts[1], pts[2]], st);
}

/// A heading in the account list. A top heading ("Banking") has a chevron
/// that folds it and a total; a sub-heading ("Savings") has neither. Clicking
/// the name selects the heading (its accounts' transactions together);
/// clicking the chevron folds it. Returns the row and whether to fold.
/// A bank action picked from an account's right-click menu.
enum BankAct {
    Update(String),
    Unlink(u32),
    Connect(u32),
}

fn heading_row(ui: &mut egui::Ui, name: &str, total: Option<f64>, open: Option<bool>, selected: bool) -> (egui::Response, bool) {
    let top = open.is_some();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), if top { 25.0 } else { 22.0 }), Sense::click());
    let chev = open.map(|_| {
        let r = egui::Rect::from_min_size(rect.min, egui::vec2(18.0, rect.height()));
        ui.interact(r, resp.id.with("fold"), Sense::click())
    });
    let on_chev = chev.as_ref().is_some_and(|c| c.hovered());
    let p = ui.painter().clone();
    if selected {
        p.rect_filled(rect, 5.0, Color32::from_gray(218));
    } else if resp.hovered() && !on_chev {
        p.rect_filled(rect, 5.0, Color32::from_gray(228));
    }
    if let Some(o) = open {
        chevron(ui, egui::pos2(rect.left() + 7.0, rect.center().y), o);
    }
    let (x, font, color) = if top {
        (rect.left() + 18.0, semibold_font(14.0), NAVY)
    } else {
        (rect.left() + 14.0, semibold_font(13.5), if selected { NAVY } else { Color32::from_gray(110) })
    };
    p.text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, name, font.clone(), color);
    if let Some(t) = total {
        p.text(egui::pos2(rect.right() - 6.0, rect.center().y), egui::Align2::RIGHT_CENTER, money_whole(t), font, amount_color(t));
    }
    let fold = chev.is_some_and(|c| c.clicked());
    (resp.on_hover_cursor(egui::CursorIcon::PointingHand), fold)
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.shot.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_menu(&ctx);
        self.plaid_poll();
        if let Some(what) = self.plaid_autostart.take() {
            match what.as_str() {
                "demo" => self.plaid_connect_from_server(&ctx),
                "demo-sync" => {
                    self.plaid_auto_accept = true;
                    self.plaid_ctx = Some(ctx.clone());
                    self.plaid_connect_from_server(&ctx);
                }
                "settings" => self.plaid_open_settings(),
                "refresh" => self.plaid_refresh(&ctx, None),
                // Opens Plaid's window and closes it after 3 s, as a person would (a check of the window).
                "connect-close" => {
                    self.plaid.test_close_after = Some(std::time::Instant::now());
                    self.plaid_connect(&ctx, None);
                }
                _ => {}
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::P)) {
            self.print_current();
        }
        if self.view == View::Register && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::N)) {
            self.new_transaction();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::CTRL, egui::Key::S)) {
            self.sidebar_collapsed = !self.sidebar_collapsed;
        }
        let side_frame =
            egui::Frame::new().fill(SIDEBAR).inner_margin(Margin::symmetric(8, 10)).stroke(Stroke::new(1.0, Color32::from_gray(212)));
        if self.sidebar_collapsed {
            egui::Panel::left("sidebar_rail")
                .resizable(false)
                .exact_size(68.0)
                .frame(side_frame)
                .show(ui, |ui| self.sidebar_rail(ui));
        } else {
            egui::Panel::left("sidebar").resizable(true).default_size(290.0).frame(side_frame).show(ui, |ui| self.sidebar(ui));
        }
        let top = egui::Panel::top("top")
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(Margin::symmetric(14, 8)))
            .show(ui, |ui| self.top_bar(ui));
        self.top_h = top.response.rect.height();
        ui.painter().hline(top.response.rect.x_range(), top.response.rect.bottom(), Stroke::new(1.0, LINE));
        let bottom = egui::Panel::bottom("toolbar")
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(Margin::symmetric(12, 4)))
            .show(ui, |ui| self.toolbar(ui));
        ui.painter().hline(bottom.response.rect.x_range(), bottom.response.rect.top(), Stroke::new(1.0, LINE));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(PAPER).inner_margin(Margin { left: 18, right: 18, top: 14, bottom: 0 }))
            .show(ui, |ui| match self.view {
                View::Home => self.home(ui),
                View::Register => self.register(ui),
                View::Budget => self.budget(ui),
                View::Reports => self.reports(ui),
                View::Categories => self.categories(ui),
            });
        self.account_dialog(&ctx);
        self.report_dialog(&ctx);
        self.restore_dialog(&ctx);
        self.purge_dialog(&ctx);
        self.plaid_dialogs(&ctx);
        self.import_dialog(&ctx);
        self.dedupe_dialog(&ctx);
        self.delete_account_dialog(&ctx);

        if self.dirty {
            self.dirty = false;
            // A screenshot run never touches the person's saved data.
            if self.shot.is_none() {
                if let Err(e) = self.ledger.save() {
                    self.status = format!("Couldn't save your data: {e}");
                }
            }
        }
        self.screenshot_step(&ctx);
    }
}

pub fn run() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let shot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(PathBuf::from);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Checken")
        // CHECKEN_WIDTH picks the starting width (for checking a narrow window).
        .with_inner_size([std::env::var("CHECKEN_WIDTH").ok().and_then(|w| w.parse::<f32>().ok()).unwrap_or(1280.0).max(960.0), 800.0])
        .with_min_inner_size([960.0, 560.0])
        .with_icon(Arc::new(crate::icon::icon_data()));
    if shot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    if shot.is_some() {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    eframe::run_native(
        "Checken",
        options,
        Box::new(move |cc| {
            setup_style(&cc.egui_ctx);
            let screenshot = shot.is_some();
            let mut app = App::new(shot);
            // The menu bar is left alone during a screenshot run.
            if !screenshot || std::env::var_os("POCKET_LEDGER_MENU").is_some() {
                app.menu_bar = filemenu::install(&cc.egui_ctx);
            }
            // POCKET_LEDGER_EDIT=new|edit opens the editing row (handy for screenshots).
            match std::env::var("POCKET_LEDGER_EDIT").as_deref() {
                Ok("new") => app.new_transaction(),
                Ok("select") => {
                    let rows = app.visible_rows(true);
                    if let Some(r) = rows.get(2) {
                        app.selected = Some(app.ledger.transactions[r.txn].id);
                        app.selected_acct = Some(r.account);
                    }
                }
                Ok("split") => {
                    // Opens the Splits panel on the first ordinary (non-transfer) transaction.
                    let rows = app.visible_rows(true);
                    if let Some(r) = rows.iter().find(|r| app.ledger.transactions[r.txn].transfer.is_none() && !app.ledger.transactions[r.txn].payee.is_empty()) {
                        app.selected = Some(app.ledger.transactions[r.txn].id);
                        app.selected_acct = Some(r.account);
                        app.split_selected();
                    }
                }
                Ok("select_transfer") => {
                    let rows = app.visible_rows(true);
                    if let Some(r) = rows.iter().find(|r| app.ledger.transactions[r.txn].transfer.is_some()) {
                        app.selected = Some(app.ledger.transactions[r.txn].id);
                        app.selected_acct = Some(r.account);
                    }
                }
                Ok("edit") => {
                    let rows = app.visible_rows(true);
                    if let Some(r) = rows.get(3) {
                        let id = app.ledger.transactions[r.txn].id;
                        app.selected = Some(id);
                        app.edit_transaction(id);
                    }
                }
                _ => {}
            }
            // POCKET_LEDGER_ADD=choose|banking|edit opens the account window (for screenshots).
            match std::env::var("POCKET_LEDGER_ADD").as_deref() {
                Ok("choose") => app.open_account_form(None),
                Ok("banking") => {
                    app.open_account_form(None);
                    if let Some(f) = app.account_form.as_mut() {
                        f.pick(CHOICE_BANKING);
                    }
                }
                Ok("edit") => {
                    let id = app.ledger.accounts.first().map(|a| a.id);
                    app.open_account_form(id);
                }
                _ => {}
            }
            // POCKET_LEDGER_REPORT=open|new|standard shows a report screen state (for screenshots).
            match std::env::var("POCKET_LEDGER_REPORT").as_deref() {
                Ok("open") => {
                    let key = app.ledger.reports.first().map(|r| library::ReportKey::Saved(r.id)).unwrap_or(library::ReportKey::Builtin(2));
                    app.open_report(key);
                }
                Ok("new") => app.new_report(),
                Ok("standard") => app.report_folder = library::Folder::All,
                Ok("print") => {
                    // Writes the print page for a standard report (not opened in a screenshot run).
                    app.open_report(library::ReportKey::Builtin(0));
                    app.print_current();
                }
                Ok("select") => {
                    app.report_folder = library::Folder::All;
                    app.report_sel = Some(library::ReportKey::Builtin(2));
                }
                _ => {}
            }
            Ok(Box::new(app))
        }),
    )
}
