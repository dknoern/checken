//! File > Import > Quicken CSV File (and Mint.com File): pick a CSV, see what
//! it holds and where each account's lines will go, then import.

use super::*;
use crate::import::{self, Parsed, Plan, Target};

/// A file that was read and is waiting for the person to confirm the import.
pub struct PendingImport {
    file: String,
    parsed: Parsed,
    targets: BTreeMap<String, Target>,
    plan: Plan,
    title: &'static str,
}

#[derive(Clone, Copy)]
pub enum Source {
    Quicken,
    Mint,
    /// A bank download file: Quicken's QFX (Web Connect) or OFX.
    BankFile,
}

/// 12794 → "12,794".
fn count(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", count(n), if n == 1 { one } else { many })
}

impl App {
    /// Asks for a CSV or bank file and reads it.
    pub(super) fn choose_import(&mut self, source: Source) {
        let title = match source {
            Source::Quicken => "Import Quicken CSV File",
            Source::Mint => "Import Mint.com File",
            Source::BankFile => "Import Bank or Brokerage File (QFX, OFX)",
        };
        let dialog = rfd::FileDialog::new().set_title(title).set_directory(super::filemenu::documents_dir());
        let dialog = match source {
            Source::BankFile => dialog.add_filter("Bank file (QFX, OFX)", &["qfx", "QFX", "ofx", "OFX", "qbo", "QBO"]),
            _ => dialog.add_filter("CSV file", &["csv", "CSV", "txt"]),
        };
        let picked = dialog.pick_file();
        if let Some(path) = picked {
            self.load_import(&path, source);
        }
    }

    /// Reads a CSV file and opens the import preview (or says why it can't).
    pub(super) fn load_import(&mut self, path: &std::path::Path, source: Source) {
        let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        let text = match std::fs::read(path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) => {
                self.status = format!("Couldn't open {file}: {e}");
                return;
            }
        };
        let fallback = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Imported".into());
        // A bank file is read as one whatever menu item opened it.
        let bank_file = matches!(source, Source::BankFile) || crate::qfx::looks_like_ofx(&text);
        let source = if bank_file { Source::BankFile } else { source };
        let parsed = if bank_file { crate::qfx::parse(&text) } else { import::parse(&text, &fallback) };
        match parsed {
            Ok(mut parsed) => {
                // Bank files carry no categories: a payee the person has
                // categorised before gets that category when the money goes the
                // same way, as Quicken remembers payees.
                for r in parsed.rows.iter_mut().filter(|r| r.category.is_empty()) {
                    let prev = self.ledger.last_for_payee(&r.payee).filter(|t| t.transfer.is_none() && t.splits.is_empty() && (t.amount >= 0.0) == (r.amount >= 0.0));
                    if let Some(t) = prev {
                        r.category = t.category.clone();
                    }
                }
                let targets = import::default_targets(&self.ledger, &parsed);
                let plan = Plan::build(&self.ledger, &parsed, &targets);
                let title = match source {
                    Source::Quicken => "Import Quicken CSV File",
                    Source::Mint => "Import Mint.com File",
                    Source::BankFile => "Import Bank File (QFX, OFX)",
                };
                self.pending_import = Some(PendingImport { file, parsed, targets, plan, title });
            }
            Err(why) => self.status = format!("Couldn't import {file}. {why}"),
        }
    }

    /// Adds the previewed file's transactions, keeping a safety copy first.
    pub(super) fn finish_import(&mut self, p: PendingImport) {
        if self.shot.is_none() {
            let stamp = chrono::Local::now().format("%Y-%m-%d %H%M%S");
            let safety = data_path().with_file_name(format!("ledger before import {stamp}.json"));
            if let Err(e) = self.ledger.save_to(&safety) {
                self.status = format!("Didn't import: couldn't keep a safety copy first ({e}).");
                return;
            }
        }
        let out = import::apply(&mut self.ledger, &p.parsed, &p.targets, &p.plan);
        self.acct_filter = None;
        self.selected = None;
        self.form = None;
        self.cat_filter = None;
        self.date_filter = register::DateFilter::All;
        self.budget_buf_month = None;
        self.view = View::Register;
        self.dirty = true;
        let mut extra: Vec<String> = Vec::new();
        if out.accounts_made > 0 {
            extra.push(plural(out.accounts_made, "new account", "new accounts"));
        }
        if out.reopened > 0 {
            extra.push(format!("{} shown again", plural(out.reopened, "hidden account", "hidden accounts")));
        }
        if out.categories_made > 0 {
            extra.push(plural(out.categories_made, "new category", "new categories"));
        }
        if p.plan.duplicates > 0 {
            extra.push(format!("{} skipped as already here", count(p.plan.duplicates)));
        }
        let mut msg = format!("Imported {}", plural(out.added, "transaction", "transactions"));
        if !extra.is_empty() {
            msg.push_str(&format!(" ({})", extra.join(", ")));
        }
        msg.push('.');
        self.status = msg;
    }

    /// The import preview: what's in the file, where each account goes, and Import.
    pub(super) fn import_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut p) = self.pending_import.take() else { return };
        let mut close = false;
        let mut go = false;
        let mut changed = false;
        let width = 790.0;
        let frame = egui::Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(10.0)
            .stroke(Stroke::new(1.0, Color32::from_gray(190)))
            .shadow(egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(70) });
        // Hidden (closed) accounts are offered too, after the open ones.
        let mut open_accounts: Vec<(u32, String)> = self.ledger.accounts.iter().filter(|a| !a.closed).map(|a| (a.id, a.name.clone())).collect();
        open_accounts.extend(self.ledger.accounts.iter().filter(|a| a.closed).map(|a| (a.id, format!("{} (hidden)", a.name))));
        let hidden = import::hidden_targets(&self.ledger, &p.parsed, &p.targets);
        egui::Modal::new(egui::Id::unique("import_dialog")).frame(frame).show(ctx, |ui| {
            ui.set_width(width);
            egui::Frame::new().inner_margin(Margin { left: 26, right: 26, top: 22, bottom: 18 }).show(ui, |ui| {
                ui.set_width(width - 52.0);
                ui.label(RichText::new(p.title).size(22.0).color(NAVY));
                ui.add_space(8.0);
                let span = match (p.plan.first, p.plan.last) {
                    (Some(a), Some(b)) => format!(" from {} to {}", fmt_date(a), fmt_date(b)),
                    _ => String::new(),
                };
                ui.label(format!("“{}” has {}{}.", p.file, plural(p.parsed.rows.len() - p.parsed.synthetic_openings.len(), "transaction", "transactions"), span));
                ui.add_space(12.0);
                ui.label(semibold("Accounts in the file").size(14.0).color(NAVY));
                ui.add_space(4.0);
                egui::ScrollArea::vertical().max_height(240.0).auto_shrink([false, true]).show(ui, |ui| {
                    egui::Grid::new("import_accounts").num_columns(5).spacing([16.0, 8.0]).striped(true).show(ui, |ui| {
                        ui.label(RichText::new("Account").color(MUTED));
                        ui.label(RichText::new("Lines").color(MUTED));
                        ui.label(RichText::new("Already here").color(MUTED));
                        ui.label(RichText::new("Import into").color(MUTED));
                        ui.label(RichText::new("Kind").color(MUTED));
                        ui.end_row();
                        for name in p.parsed.accounts.clone() {
                            let n = p.plan.per_account.get(&name).copied().unwrap_or(0);
                            ui.allocate_ui_with_layout(egui::vec2(200.0, 22.0), Layout::left_to_right(Align::Center), |ui| {
                                ui.set_min_width(200.0);
                                ui.add(egui::Label::new(&name).truncate()).on_hover_text(&name);
                            });
                            ui.label(count(n));
                            let here = p.plan.dup_per_account.get(&name).copied().unwrap_or(0);
                            ui.label(match here {
                                0 => RichText::new("none").color(MUTED),
                                h if h == n => RichText::new("all").color(MUTED),
                                h => RichText::new(count(h)).color(MUTED),
                            });
                            let cur = p.targets.get(&name).cloned().unwrap_or(Target::New(AccountKind::Checking));
                            let shown = match &cur {
                                Target::New(_) => "New account".to_string(),
                                Target::Existing(id) => match self.ledger.account(*id) {
                                    Some(a) if a.closed => format!("{} (hidden)", a.name),
                                    _ => self.ledger.account_name(*id),
                                },
                            };
                            let mut pick = cur.clone();
                            egui::ComboBox::from_id_salt(("import_into", &name)).width(200.0).selected_text(shown).show_ui(ui, |ui| {
                                let kind = match cur {
                                    Target::New(k) => k,
                                    Target::Existing(_) => import::guess_kind(&name, p.parsed.kind_hint),
                                };
                                ui.selectable_value(&mut pick, Target::New(kind), "New account");
                                ui.separator();
                                for (id, an) in &open_accounts {
                                    ui.selectable_value(&mut pick, Target::Existing(*id), an);
                                }
                            });
                            match &mut pick {
                                Target::New(kind) => {
                                    egui::ComboBox::from_id_salt(("import_kind", &name)).width(120.0).selected_text(kind.label()).show_ui(ui, |ui| {
                                        for k in AccountKind::ALL {
                                            ui.selectable_value(kind, k, k.label());
                                        }
                                    });
                                }
                                Target::Existing(id) => {
                                    let closed = self.ledger.account(*id).is_some_and(|a| a.closed);
                                    ui.label(RichText::new(if closed { "shown again" } else { "existing" }).color(MUTED));
                                }
                            }
                            if pick != cur {
                                p.targets.insert(name.clone(), pick);
                                changed = true;
                            }
                            ui.end_row();
                        }
                    });
                });
                ui.add_space(12.0);
                let plan = &p.plan;
                let new_accounts = p.targets.values().filter(|t| matches!(t, Target::New(_))).count();
                let mut lines: Vec<String> = vec![format!("{} will be added.", plural(plan.adds.len(), "transaction", "transactions"))];
                if new_accounts > 0 || !plan.new_categories.is_empty() {
                    lines.push(format!(
                        "{} and {} will be made.",
                        plural(new_accounts, "new account", "new accounts"),
                        plural(plan.new_categories.len(), "new category", "new categories")
                    ));
                }
                if !plan.openings.is_empty() {
                    if p.parsed.synthetic_openings.is_empty() {
                        lines.push(format!("{} become the new accounts' starting balances.", plural(plan.openings.len(), "opening balance line", "opening balance lines")));
                    } else {
                        lines.push("New accounts start at the balance the bank reports in the file.".to_string());
                    }
                }
                if plan.transfers_matched > 0 {
                    lines.push(format!("{} between these accounts will be linked.", plural(plan.transfers_matched, "transfer", "transfers")));
                }
                if plan.duplicates > 0 {
                    lines.push(format!(
                        "{} already in Checken (same account, date and amount) will be skipped.",
                        plural(plan.duplicates, "transaction", "transactions")
                    ));
                }
                if !hidden.is_empty() {
                    lines.push(format!("Hidden {} {} will be shown again.", if hidden.len() == 1 { "account" } else { "accounts" }, hidden.join(", ")));
                }
                if p.parsed.scheduled > 0 {
                    lines.push(format!("{} that haven't happened yet are left out.", plural(p.parsed.scheduled, "scheduled transaction", "scheduled transactions")));
                }
                for l in lines {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("•").color(ACCENT));
                        ui.label(l);
                    });
                }
                ui.add_space(4.0);
                ui.label(RichText::new("Your current data is kept as a safety copy next to your data file first.").color(MUTED));
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let nothing = p.plan.adds.is_empty() && p.plan.openings.is_empty() && hidden.is_empty();
                    let b = egui::Button::new(RichText::new("Import").size(15.0).color(Color32::WHITE))
                        .fill(if nothing { Color32::from_gray(170) } else { ACCENT })
                        .stroke(Stroke::NONE)
                        .min_size(egui::vec2(110.0, 30.0))
                        .corner_radius(6.0);
                    if ui.add_enabled(!nothing, b).clicked() {
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
        if changed {
            p.plan = Plan::build(&self.ledger, &p.parsed, &p.targets);
        }
        if go && !close {
            self.finish_import(p);
            return;
        }
        if !close {
            self.pending_import = Some(p);
        }
    }

    /// File > Remove Duplicate Imports: finds copies an earlier import added
    /// without a payee and asks before taking them out.
    pub(super) fn find_import_duplicates(&mut self) {
        let ids = import::import_duplicates(&self.ledger);
        if ids.is_empty() {
            self.status = "No duplicate imported transactions were found.".into();
        } else {
            self.pending_dedupe = Some(ids);
        }
    }

    /// The "Remove Duplicate Imports?" confirmation.
    pub(super) fn dedupe_dialog(&mut self, ctx: &egui::Context) {
        let Some(ids) = self.pending_dedupe.take() else { return };
        let mut close = false;
        let mut confirm = false;
        let width = 520.0;
        let frame = egui::Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(10.0)
            .stroke(Stroke::new(1.0, Color32::from_gray(190)))
            .shadow(egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(70) });
        // Which accounts they're in, most first.
        let mut per: BTreeMap<String, usize> = BTreeMap::new();
        for t in self.ledger.transactions.iter().filter(|t| ids.binary_search(&t.id).is_ok()) {
            *per.entry(self.ledger.account_name(t.account)).or_default() += 1;
        }
        let mut per: Vec<(String, usize)> = per.into_iter().collect();
        per.sort_by(|a, b| b.1.cmp(&a.1));
        egui::Modal::new(egui::Id::unique("dedupe_dialog")).frame(frame).show(ctx, |ui| {
            ui.set_width(width);
            egui::Frame::new().inner_margin(Margin { left: 26, right: 26, top: 22, bottom: 18 }).show(ui, |ui| {
                ui.set_width(width - 52.0);
                ui.label(RichText::new("Remove Duplicate Imports?").size(22.0).color(NAVY));
                ui.add_space(10.0);
                ui.label(format!(
                    "{} look like copies an earlier import added twice: each has no payee and matches another transaction with a payee on the same day, account, amount and category.",
                    plural(ids.len(), "transaction", "transactions")
                ));
                ui.add_space(8.0);
                let shown: Vec<String> = per.iter().take(6).map(|(a, n)| format!("{a} ({})", count(*n))).collect();
                let more = if per.len() > 6 { format!(" and {} more", per.len() - 6) } else { String::new() };
                ui.label(RichText::new(format!("In {}{}.", shown.join(", "), more)).color(MUTED));
                ui.add_space(4.0);
                ui.label(RichText::new("The copies without a payee are taken out; the ones with a payee stay. Your current data is kept as a safety copy first.").color(MUTED));
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let b = egui::Button::new(RichText::new("Remove").size(15.0).color(Color32::WHITE))
                        .fill(RED)
                        .stroke(Stroke::NONE)
                        .min_size(egui::vec2(110.0, 30.0))
                        .corner_radius(6.0);
                    if ui.add(b).clicked() {
                        confirm = true;
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
        if confirm && !close {
            if self.shot.is_none() {
                let stamp = chrono::Local::now().format("%Y-%m-%d %H%M%S");
                let safety = data_path().with_file_name(format!("ledger before removing duplicates {stamp}.json"));
                if let Err(e) = self.ledger.save_to(&safety) {
                    self.status = format!("Didn't change anything: couldn't keep a safety copy first ({e}).");
                    return;
                }
            }
            let before = self.ledger.transactions.len();
            self.ledger.transactions.retain(|t| ids.binary_search(&t.id).is_err());
            let taken = before - self.ledger.transactions.len();
            self.selected = None;
            self.form = None;
            self.dirty = true;
            self.status = format!("Removed {}.", plural(taken, "duplicate transaction", "duplicate transactions"));
            return;
        }
        if !close {
            self.pending_dedupe = Some(ids);
        }
    }
}
