//! The menu bar (on the Mac: Checken, File, View, Window) and the
//! File menu's backup actions: Save a Backup and Restore from Backup.
//! (Import lives in importer.rs.)

use super::*;
use std::sync::mpsc::Receiver;

/// Keeps the native menu alive and brings its clicks into the app.
pub struct MenuBar {
    #[cfg(target_os = "macos")]
    _menu: muda::Menu,
    rx: Receiver<String>,
}

/// A backup that was opened and is waiting for the person to confirm.
pub struct PendingRestore {
    ledger: Ledger,
    file: String,
}

/// Builds the Mac menu bar. Other systems use the sidebar's More menu.
#[cfg(target_os = "macos")]
pub fn install(ctx: &egui::Context) -> Option<MenuBar> {
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    let cmd = Modifiers::META;
    let key = |m: Modifiers, c: Code| Some(Accelerator::new(m, c));
    let item = |id: &str, text: &str, acc: Option<Accelerator>| MenuItem::with_id(id, text, true, acc);

    let about = AboutMetadata {
        name: Some("Checken".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        icon: muda::Icon::from_rgba(crate::icon::pixels(256), 256, 256).ok(),
        ..Default::default()
    };
    let app_menu = Submenu::with_items(
        "Checken",
        true,
        &[
            &PredefinedMenuItem::about(Some("About Checken"), Some(about)),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(Some("Hide Checken")),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(Some("Quit Checken")),
        ],
    )
    .ok()?;
    let import = Submenu::with_items(
        "Import",
        true,
        &[
            &item("import_bank_file", "Bank or Brokerage File (QFX, OFX)…", None),
            &item("import_quicken_csv", "Quicken CSV File…", key(cmd | Modifiers::SHIFT, Code::KeyI)),
            &item("import_mint_csv", "Mint.com File (CSV)…", None),
            &PredefinedMenuItem::separator(),
            &item("dedupe_imports", "Remove Duplicate Imports…", None),
        ],
    )
    .ok()?;
    let file = Submenu::with_items(
        "File",
        true,
        &[
            &item("new_txn", "New Transaction", key(cmd, Code::KeyN)),
            &item("new_account", "New Account…", key(cmd | Modifiers::SHIFT, Code::KeyA)),
            &item("new_report", "New Report…", key(cmd | Modifiers::ALT, Code::KeyR)),
            &PredefinedMenuItem::separator(),
            &item("backup", "Save a Backup…", key(cmd | Modifiers::SHIFT, Code::KeyB)),
            &item("restore", "Restore from Backup…", None),
            &PredefinedMenuItem::separator(),
            &import,
            &PredefinedMenuItem::separator(),
            &item("plaid_connect", "Connect a Bank Account (Plaid)…", None),
            &item("plaid_refresh", "Update All Bank Accounts", key(cmd, Code::KeyR)),
            &item("plaid_settings", "Bank Connections (Plaid)…", None),
            &PredefinedMenuItem::separator(),
            &item("print", "Print Report…", key(cmd, Code::KeyP)),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::close_window(None),
        ],
    )
    .ok()?;
    let view = Submenu::with_items(
        "View",
        true,
        &[
            &item("sidebar", "Show or Hide Sidebar", key(cmd | Modifiers::CONTROL, Code::KeyS)),
            &PredefinedMenuItem::separator(),
            &item("go_home", "Home", key(cmd, Code::Digit1)),
            &item("go_register", "All Transactions", key(cmd, Code::Digit2)),
            &item("go_reports", "Reports", key(cmd, Code::Digit3)),
            &item("go_budgets", "Budgets", key(cmd, Code::Digit4)),
            &item("go_categories", "Categories", key(cmd, Code::Digit5)),
        ],
    )
    .ok()?;
    let window = Submenu::with_items(
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(Some("Zoom")),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::fullscreen(None),
        ],
    )
    .ok()?;
    let menu = Menu::with_items(&[&app_menu, &file, &view, &window]).ok()?;
    menu.init_for_nsapp();
    window.set_as_windows_menu_for_nsapp();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let wake = ctx.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let _ = tx.send(e.id.0.clone());
        wake.request_repaint();
    }));
    Some(MenuBar { _menu: menu, rx })
}

#[cfg(not(target_os = "macos"))]
pub fn install(_ctx: &egui::Context) -> Option<MenuBar> {
    None
}

pub(super) fn documents_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).map(|h| h.join("Documents")).unwrap_or_else(|| PathBuf::from("."))
}

impl App {
    /// Acts on menu bar clicks since the last frame.
    pub(super) fn handle_menu(&mut self, ctx: &egui::Context) {
        let Some(bar) = &self.menu_bar else { return };
        let ids: Vec<String> = bar.rx.try_iter().collect();
        for id in ids {
            match id.as_str() {
                "new_txn" => {
                    self.view = View::Register;
                    self.new_transaction();
                }
                "new_account" => self.open_account_form(None),
                "new_report" => {
                    self.view = View::Reports;
                    self.new_report();
                }
                "backup" => self.save_backup(),
                "restore" => self.choose_restore(),
                "import_quicken_csv" => self.choose_import(super::importer::Source::Quicken),
                "import_mint_csv" => self.choose_import(super::importer::Source::Mint),
                "import_bank_file" => self.choose_import(super::importer::Source::BankFile),
                "dedupe_imports" => self.find_import_duplicates(),
                "plaid_connect" => self.plaid_connect(ctx, None),
                "plaid_refresh" => self.plaid_refresh(ctx, None),
                "plaid_settings" => self.plaid_open_settings(),
                "print" => self.print_current(),
                "sidebar" => self.sidebar_collapsed = !self.sidebar_collapsed,
                "go_home" => self.go(View::Home),
                "go_register" => self.open_register(None),
                "go_reports" => self.go(View::Reports),
                "go_budgets" => self.go(View::Budget),
                "go_categories" => self.go(View::Categories),
                _ => {}
            }
        }
    }

    /// File > Save a Backup: writes all the data to a file the person picks.
    pub(super) fn save_backup(&mut self) {
        let name = format!("Checken Backup {}.json", today().format("%Y-%m-%d"));
        let picked = rfd::FileDialog::new()
            .set_title("Save a Backup")
            .set_directory(documents_dir())
            .set_file_name(&name)
            .add_filter("Checken backup", &["json"])
            .save_file();
        let Some(path) = picked else { return };
        self.status = match self.ledger.save_to(&path) {
            Ok(()) => format!("Backup saved to {}.", path.display()),
            Err(e) => format!("Couldn't save the backup: {e}"),
        };
    }

    /// File > Restore from Backup: opens a backup and asks before replacing anything.
    pub(super) fn choose_restore(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Restore from Backup")
            .set_directory(documents_dir())
            .add_filter("Checken backup", &["json"])
            .pick_file();
        let Some(path) = picked else { return };
        let parsed = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str::<Ledger>(&s).ok());
        match parsed {
            Some(ledger) => {
                let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                self.pending_restore = Some(PendingRestore { ledger, file });
            }
            None => self.status = format!("{} isn't a Checken backup.", path.display()),
        }
    }

    /// The "Restore from Backup?" confirmation.
    pub(super) fn restore_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_restore.take() else { return };
        let mut close = false;
        let mut restore = false;
        let width = 500.0;
        let frame = egui::Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(10.0)
            .stroke(Stroke::new(1.0, Color32::from_gray(190)))
            .shadow(egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(70) });
        let l = &pending.ledger;
        let summary = format!(
            "It has {} accounts, {} transactions, {} categories and {} saved reports.",
            l.accounts.len(),
            l.transactions.len(),
            l.categories.len(),
            l.reports.len()
        );
        egui::Modal::new(egui::Id::unique("restore_dialog")).frame(frame).show(ctx, |ui| {
            ui.set_width(width);
            egui::Frame::new().inner_margin(Margin { left: 26, right: 26, top: 22, bottom: 18 }).show(ui, |ui| {
                ui.set_width(width - 52.0);
                ui.label(RichText::new("Restore from Backup?").size(22.0).color(NAVY));
                ui.add_space(10.0);
                ui.label(format!("Everything in Checken will be replaced with the backup “{}”.", pending.file));
                ui.add_space(4.0);
                ui.label(RichText::new(summary).color(MUTED));
                ui.add_space(4.0);
                ui.label(RichText::new("Your current data is kept as a safety copy next to your data file first.").color(MUTED));
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let b = egui::Button::new(RichText::new("Restore").size(15.0).color(Color32::WHITE))
                        .fill(ACCENT)
                        .stroke(Stroke::NONE)
                        .min_size(egui::vec2(110.0, 30.0))
                        .corner_radius(6.0);
                    if ui.add(b).clicked() {
                        restore = true;
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
        if restore && !close {
            let stamp = chrono::Local::now().format("%Y-%m-%d %H%M%S");
            let safety = data_path().with_file_name(format!("ledger before restore {stamp}.json"));
            if let Err(e) = self.ledger.save_to(&safety) {
                self.status = format!("Didn't restore: couldn't keep a safety copy first ({e}).");
                return;
            }
            self.ledger = pending.ledger;
            self.acct_filter = None;
            self.selected = None;
            self.form = None;
            self.budget_buf_month = None;
            self.report_open = None;
            self.report_sel = None;
            self.recent_reports.clear();
            self.view = View::Register;
            self.dirty = true;
            self.status = format!("Restored from {}. Your previous data is in {}.", pending.file, safety.display());
            return;
        }
        if !close {
            self.pending_restore = Some(pending);
        }
    }
}
