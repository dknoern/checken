//! Entering and editing transactions in place: the row being edited turns
//! into a blue band of fields lined up under the register's columns, with a
//! pale strip below for the memo and the Save / Cancel buttons. A split
//! transaction opens a Splits panel under that, one line per category.

use super::*;
use egui_extras::TableRow;

pub const EDIT_BLUE: Color32 = Color32::from_rgb(0x2a, 0x5c, 0xa6);
const EDIT_STRIP: Color32 = Color32::from_rgb(0xe9, 0xf0, 0xfb);
pub const EDIT_ROW_HEIGHT: f32 = 76.0;
const BAND_HEIGHT: f32 = 38.0;
/// What a split transaction shows in the Category column.
pub const SPLIT_LABEL: &str = "— Split —";

#[derive(Clone, PartialEq)]
pub enum Target {
    None,
    Cat(String),
    Transfer(u32),
}

/// One line of the Splits panel, as typed.
pub struct SplitLine {
    category: String,
    amount: String,
    memo: String,
}

pub struct TxnForm {
    pub editing: Option<u64>,
    pub account: u32,
    date: String,
    number: String,
    payee: String,
    target: Target,
    memo: String,
    amount: String,
    splits: Vec<SplitLine>,
    show_splits: bool,
    error: Option<String>,
    /// Put the cursor in the payee field on the next frame.
    focus: bool,
    /// Scroll the register so the row is in view on the next frame.
    pub scroll: bool,
}

impl TxnForm {
    /// The editing row's height, taller while the Splits panel is open.
    pub fn row_height(&self) -> f32 {
        if self.show_splits { EDIT_ROW_HEIGHT + splits_height(self.splits.len()) } else { EDIT_ROW_HEIGHT }
    }

    /// Opens or closes the Splits panel; opening it the first time starts
    /// with the current category and amount plus an empty line.
    fn toggle_splits(&mut self) -> Result<(), &'static str> {
        if self.show_splits {
            self.show_splits = false;
            return Ok(());
        }
        if matches!(self.target, Target::Transfer(_)) && self.splits.is_empty() {
            return Err("Transfers can't be split. Choose a category first.");
        }
        if self.splits.is_empty() {
            let category = match &self.target {
                Target::Cat(c) => c.clone(),
                _ => String::new(),
            };
            self.splits.push(SplitLine { category, amount: self.amount.clone(), memo: String::new() });
            self.splits.push(SplitLine { category: String::new(), amount: String::new(), memo: String::new() });
        }
        self.show_splits = true;
        Ok(())
    }

    fn split_total(&self) -> f64 {
        self.splits.iter().filter_map(|l| parse_money(&l.amount)).sum()
    }
}

fn splits_height(lines: usize) -> f32 {
    12.0 + 30.0 + lines as f32 * 34.0 + 44.0
}

/// What the person did in the editing row this frame.
#[derive(Default)]
pub struct EditOutcome {
    pub save: bool,
    pub cancel: bool,
    pub autofill: bool,
    pub split_error: Option<&'static str>,
}

impl App {
    pub(super) fn new_transaction(&mut self) {
        // The account being viewed, else the first under the selected heading, else the first open one.
        let account = self
            .acct_filter
            .or_else(|| self.ledger.accounts.iter().find(|a| self.in_scope(a)).map(|a| a.id))
            .or_else(|| self.ledger.accounts.iter().find(|a| !a.closed).map(|a| a.id));
        let Some(account) = account else {
            self.status = "Add an account first.".into();
            self.open_account_form(None);
            return;
        };
        self.form = Some(TxnForm {
            editing: None,
            account,
            date: fmt_date(today()),
            number: String::new(),
            payee: String::new(),
            target: Target::None,
            memo: String::new(),
            amount: String::new(),
            splits: Vec::new(),
            show_splits: false,
            error: None,
            focus: true,
            scroll: true,
        });
        self.selected = None;
        self.confirm_delete = false;
        self.status = "Type the payee and amount (minus for money out), then press Return.".into();
    }

    /// Opens a transaction for editing in place, as seen from the account
    /// being shown (or the account it was entered in, when showing all).
    pub(super) fn edit_transaction(&mut self, id: u64) {
        let Some(t) = self.ledger.transactions.iter().find(|t| t.id == id) else { return };
        let account = match (self.selected_acct, self.acct_filter) {
            (Some(a), _) if self.selected == Some(id) && (t.account == a || t.transfer == Some(a)) => a,
            (_, Some(a)) if t.transfer == Some(a) => a,
            _ => t.account,
        };
        let amount = Ledger::effect_on(t, account);
        let target = match t.transfer {
            Some(x) => Target::Transfer(if t.account == account { x } else { t.account }),
            None if t.category.is_empty() => Target::None,
            None => Target::Cat(t.category.clone()),
        };
        let splits: Vec<SplitLine> = t
            .splits
            .iter()
            .map(|s| SplitLine { category: s.category.clone(), amount: format!("{:.2}", s.amount), memo: s.memo.clone() })
            .collect();
        let show_splits = !splits.is_empty();
        self.form = Some(TxnForm {
            editing: Some(id),
            account,
            date: fmt_date(t.date),
            number: t.number.clone(),
            payee: t.payee.clone(),
            target,
            memo: t.memo.clone(),
            amount: format!("{amount:.2}"),
            splits,
            show_splits,
            error: None,
            focus: true,
            scroll: true,
        });
        self.confirm_delete = false;
    }

    /// The toolbar's Split button: opens the Splits panel for the transaction
    /// being edited, or for the selected one.
    pub(super) fn split_selected(&mut self) {
        if self.form.is_none() {
            let Some(id) = self.selected else { return };
            self.edit_transaction(id);
        }
        if let Some(f) = self.form.as_mut() {
            if let Err(e) = f.toggle_splits() {
                self.status = e.into();
            }
        }
    }

    /// Acts on what happened in the editing row, after the register is drawn.
    pub(super) fn finish_edit(&mut self, mut f: TxnForm, out: EditOutcome) {
        if let Some(e) = out.split_error {
            self.status = e.into();
        }
        if out.autofill {
            self.autofill_from_payee(&mut f);
        }
        if out.cancel {
            self.status = "Changes are saved automatically.".into();
            return;
        }
        if out.save {
            match self.commit_form(&f) {
                Ok(()) => return,
                Err(e) => f.error = Some(e),
            }
        }
        self.form = Some(f);
    }

    /// Typing a payee used before fills in its last category and amount.
    fn autofill_from_payee(&self, f: &mut TxnForm) {
        let typed = f.payee.trim().to_string();
        if typed.is_empty() {
            return;
        }
        // A unique start of a known payee completes to that payee.
        let lower = typed.to_lowercase();
        let starts: Vec<String> = self.ledger.known_payees().into_iter().filter(|p| p.to_lowercase().starts_with(&lower)).collect();
        let name = if starts.len() == 1 { starts[0].clone() } else { typed };
        let Some(prev) = self.ledger.last_for_payee(&name) else { return };
        f.payee = prev.payee.clone();
        if f.target == Target::None && f.splits.is_empty() {
            f.target = match prev.transfer {
                Some(_) if prev.transfer == Some(f.account) => Target::Transfer(prev.account),
                Some(x) => Target::Transfer(x),
                None if prev.category.is_empty() => Target::None,
                None => Target::Cat(prev.category.clone()),
            };
        }
        if f.amount.trim().is_empty() && f.splits.is_empty() {
            let amt = Ledger::effect_on(prev, f.account);
            let amt = if amt == 0.0 { prev.amount } else { amt };
            f.amount = format!("{amt:.2}");
        }
    }

    fn commit_form(&mut self, f: &TxnForm) -> Result<(), String> {
        let date = parse_date(&f.date).ok_or("That date doesn't look right. Try 3/14/2026.")?;
        // Split lines that say something; an untouched empty line is ignored.
        let mut lines: Vec<Split> = Vec::new();
        for l in f.splits.iter().filter(|l| !l.category.is_empty() || !l.amount.trim().is_empty() || !l.memo.trim().is_empty()) {
            let amount = parse_money(&l.amount).ok_or("Each split amount should be a number, like -12.50.")?;
            lines.push(Split { category: l.category.clone(), amount, memo: l.memo.trim().to_string() });
        }
        let (mut category, mut transfer) = match &f.target {
            Target::None => (String::new(), None),
            Target::Cat(c) => (c.clone(), None),
            Target::Transfer(x) => (String::new(), Some(*x)),
        };
        let amount: f64;
        let splits = match lines.len() {
            0 => {
                amount = parse_money(&f.amount).ok_or("The amount should be a number, like -42.50 for money out.")?;
                Vec::new()
            }
            // A single line is just an ordinary transaction.
            1 => {
                category = lines[0].category.clone();
                transfer = None;
                amount = lines[0].amount;
                Vec::new()
            }
            _ => {
                category = String::new();
                transfer = None;
                amount = lines.iter().map(|l| l.amount).sum();
                lines
            }
        };
        if f.payee.trim().is_empty() && amount == 0.0 {
            return Err("Enter at least a payee and an amount.".into());
        }
        if transfer == Some(f.account) {
            return Err("A transfer needs a different account.".into());
        }
        let payee = f.payee.trim().to_string();
        let (number, memo) = (f.number.trim().to_string(), f.memo.trim().to_string());
        let id = match f.editing {
            Some(id) => {
                if let Some(t) = self.ledger.transactions.iter_mut().find(|t| t.id == id) {
                    t.account = f.account;
                    t.date = date;
                    t.number = number;
                    t.payee = payee.clone();
                    t.category = category;
                    t.memo = memo;
                    t.amount = amount;
                    t.transfer = transfer;
                    t.splits = splits;
                }
                self.status = format!("Saved changes to {payee}.");
                id
            }
            None => {
                let id = self.ledger.new_id();
                self.ledger.transactions.push(Transaction {
                    id,
                    account: f.account,
                    date,
                    number,
                    payee: payee.clone(),
                    category,
                    memo,
                    amount,
                    cleared: Cleared::No,
                    transfer,
                    splits,
                });
                self.status = format!("Entered {} for {} in {}.", money(amount), payee, self.ledger.account_name(f.account));
                id
            }
        };
        self.selected = Some(id);
        self.selected_acct = Some(f.account);
        self.dirty = true;
        Ok(())
    }

    /// Finishes the editing row before moving to another line: a blank new
    /// transaction is dropped, anything else is saved. If it can't be saved,
    /// the row stays open with the problem shown, and this returns false.
    pub(super) fn leave_edit(&mut self) -> bool {
        let Some(mut f) = self.form.take() else { return true };
        let blank = f.payee.trim().is_empty() && parse_money(&f.amount).unwrap_or(0.0) == 0.0 && f.splits.is_empty();
        if f.editing.is_none() && blank {
            return true;
        }
        match self.commit_form(&f) {
            Ok(()) => true,
            Err(e) => {
                f.error = Some(e);
                self.form = Some(f);
                false
            }
        }
    }
}

/// The part of a cell where the row's fields sit.
fn band_slot(cell: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(egui::pos2(cell.center().x, cell.top() + BAND_HEIGHT / 2.0), egui::vec2(cell.width() - 2.0, 28.0))
}

/// The part of a cell in the pale strip under the fields.
fn strip_slot(cell: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(
        egui::pos2(cell.center().x, cell.top() + BAND_HEIGHT + (EDIT_ROW_HEIGHT - BAND_HEIGHT) / 2.0),
        egui::vec2(cell.width() - 2.0, 28.0),
    )
}

fn text_field(ui: &mut egui::Ui, slot: egui::Rect, text: &mut String, hint: &str, right: bool) -> egui::Response {
    let align = if right { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER };
    ui.put(slot, egui::TextEdit::singleline(text).hint_text(hint).desired_width(slot.width() - 8.0).align(align))
}

fn in_slot<R>(ui: &mut egui::Ui, slot: egui::Rect, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope_builder(egui::UiBuilder::new().max_rect(slot).layout(Layout::left_to_right(Align::Center)), add).inner
}

/// A white box that looks like a field and acts like a button.
fn field_button(ui: &mut egui::Ui, slot: egui::Rect, text: &str, right: bool) -> egui::Response {
    let resp = ui.allocate_rect(slot, Sense::click());
    let p = ui.painter();
    p.rect(slot, 4.0, Color32::WHITE, Stroke::new(1.0, Color32::from_gray(200)), egui::StrokeKind::Inside);
    let (pos, align) = if right {
        (slot.right_center() - egui::vec2(8.0, 0.0), Align2::RIGHT_CENTER)
    } else {
        (slot.left_center() + egui::vec2(8.0, 0.0), Align2::LEFT_CENTER)
    };
    p.text(pos, align, text, FontId::proportional(14.0), Color32::from_gray(25));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The Splits panel under the editing row: a line per category with its
/// amount and note, − / + to remove or add lines, and the running total.
/// Returns true when Return was pressed in one of its fields.
fn splits_panel(ui: &mut egui::Ui, ledger: &Ledger, f: &mut TxnForm, area: egui::Rect, body_clip: egui::Rect) -> bool {
    let width = 780.0f32.min(area.width() - 40.0);
    let content = egui::Rect::from_min_size(egui::pos2(area.center().x - width / 2.0, area.top() + 12.0), egui::vec2(width, area.height() - 12.0));
    let mut remove: Option<usize> = None;
    let mut add_after: Option<usize> = None;
    let mut clear = false;
    let mut entered = false;
    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
    let total = f.split_total();
    ui.scope_builder(egui::UiBuilder::new().max_rect(content).layout(Layout::top_down(Align::Min)), |ui| {
        ui.set_clip_rect(body_clip);
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.horizontal(|ui| {
            ui.set_height(24.0);
            ui.label(semibold("Splits").size(15.0).color(NAVY));
            ui.label(RichText::new("Divide this transaction between categories. Its amount is the total of the lines.").small().color(MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(semibold(num(total)).size(15.0));
                ui.label(RichText::new("Total").color(MUTED));
            });
        });
        let round = |ui: &mut egui::Ui, t: &str, tip: &str| {
            ui.add(egui::Button::new(RichText::new(t).size(15.0)).corner_radius(11.0).min_size(egui::vec2(24.0, 24.0))).on_hover_text(tip)
        };
        for i in 0..f.splits.len() {
            ui.horizontal(|ui| {
                ui.set_height(28.0);
                if round(ui, "−", "Remove this line").clicked() {
                    remove = Some(i);
                }
                if round(ui, "+", "Add a line below").clicked() {
                    add_after = Some(i);
                }
                let line = &mut f.splits[i];
                let label = if line.category.is_empty() { "Uncategorized".to_string() } else { line.category.clone() };
                egui::ComboBox::from_id_salt(("split_cat", i)).selected_text(label).width(250.0).height(400.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut line.category, String::new(), italic("Uncategorized"));
                    for (kind, title) in [(CatKind::Expense, "Expense Categories:"), (CatKind::Income, "Income Categories:")] {
                        ui.separator();
                        ui.label(RichText::new(title).color(MUTED));
                        for c in ledger.categories.iter().filter(|c| c.kind == kind) {
                            ui.selectable_value(&mut line.category, c.name.clone(), tree_label(&c.name));
                        }
                    }
                });
                let a = ui.add(egui::TextEdit::singleline(&mut line.amount).hint_text("0.00").align(Align2::RIGHT_CENTER).desired_width(110.0));
                let m = ui.add(egui::TextEdit::singleline(&mut line.memo).hint_text("Notes").desired_width(260.0));
                entered |= (a.lost_focus() || m.lost_focus()) && enter;
            });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Clear all splits").clicked() {
                clear = true;
            }
            if ui.button("+ Add Line").clicked() {
                add_after = Some(f.splits.len().saturating_sub(1));
                if f.splits.is_empty() {
                    add_after = None;
                    f.splits.push(SplitLine { category: String::new(), amount: String::new(), memo: String::new() });
                }
            }
        });
    });
    if let Some(i) = remove {
        f.splits.remove(i);
    }
    if let Some(i) = add_after {
        f.splits.insert((i + 1).min(f.splits.len()), SplitLine { category: String::new(), amount: String::new(), memo: String::new() });
    }
    if clear {
        // Back to a single category: the first line's, with the total kept.
        if let Some(first) = f.splits.first().filter(|l| !l.category.is_empty()) {
            f.target = Target::Cat(first.category.clone());
        }
        f.splits.clear();
        f.show_splits = false;
    }
    if !f.splits.is_empty() {
        f.amount = format!("{:.2}", f.split_total());
    }
    entered
}

/// Draws the editing row's cells. `body_clip` is the visible part of the
/// register, so the blue band can run unbroken across the columns.
pub fn edit_row(ledger: &Ledger, f: &mut TxnForm, row: &mut TableRow<'_, '_>, all: bool, body_clip: egui::Rect, out: &mut EditOutcome) {
    let editing = f.editing.is_some();
    let mut enter_in = |r: &egui::Response, ui: &egui::Ui| {
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            out.save = true;
        }
    };
    let mut autofill = false;
    let mut cancel = false;
    let mut save = false;
    let mut toggle = false;
    let mut split_enter = false;
    let split = !f.splits.is_empty();

    // Each cell paints its own piece of the blue band and pale strip (over
    // the row shading), reaching left over the gap so the band is unbroken.
    let last_col = if all { 7 } else { 6 };
    let paint = |ui: &egui::Ui, col: usize| {
        let cell = ui.max_rect();
        let mut p = ui.painter().clone();
        p.set_clip_rect(body_clip);
        let left = if col == 0 { body_clip.left() } else { cell.left() - ui.spacing().item_spacing.x };
        let right = if col == last_col { body_clip.right() } else { cell.right() };
        let band = egui::Rect::from_min_max(egui::pos2(left, cell.top()), egui::pos2(right, cell.top() + BAND_HEIGHT));
        let strip = egui::Rect::from_min_max(band.left_bottom(), egui::pos2(right, cell.bottom()));
        p.rect_filled(band, 0.0, EDIT_BLUE);
        p.rect_filled(strip, 0.0, EDIT_STRIP);
        p.hline(strip.x_range(), strip.bottom() - 0.5, Stroke::new(1.0, Color32::from_rgb(0xb9, 0xcd, 0xee)));
    };

    // Status dot.
    row.col(|ui| {
        paint(ui, 0);
        let cell = ui.max_rect();
        ui.painter().circle_stroke(egui::pos2(cell.center().x, cell.top() + BAND_HEIGHT / 2.0), 4.5, Stroke::new(1.5, Color32::WHITE));
    });
    // Date.
    row.col(|ui| {
        paint(ui, 1);
        let cell = ui.max_rect();
        let r = text_field(ui, band_slot(cell), &mut f.date, "Date", false);
        enter_in(&r, ui);
        let label = if editing { "Edit Details" } else { "New" };
        ui.painter().text(strip_slot(cell).left_center() + egui::vec2(4.0, 0.0), Align2::LEFT_CENTER, label, semibold_font(13.5), EDIT_BLUE);
    });
    // Check number.
    row.col(|ui| {
        paint(ui, 2);
        let r = text_field(ui, band_slot(ui.max_rect()), &mut f.number, "Check #", false);
        enter_in(&r, ui);
    });
    // Payee, with the memo underneath.
    row.col(|ui| {
        paint(ui, 3);
        let cell = ui.max_rect();
        let r = text_field(ui, band_slot(cell), &mut f.payee, "Payee", false);
        if f.focus {
            r.request_focus();
            f.focus = false;
        }
        if r.lost_focus() && !editing {
            autofill = true;
        }
        enter_in(&r, ui);
        let m = text_field(ui, strip_slot(cell), &mut f.memo, "Memo (optional)", false);
        enter_in(&m, ui);
    });
    // Category with the split button beside it, and any error underneath.
    row.col(|ui| {
        paint(ui, 4);
        let cell = ui.max_rect();
        let slot = band_slot(cell);
        let pick = egui::Rect::from_min_max(slot.min, egui::pos2(slot.right() - 30.0, slot.bottom()));
        let fork = egui::Rect::from_center_size(egui::pos2(slot.right() - 13.0, slot.center().y), egui::vec2(26.0, 26.0));
        if split {
            if field_button(ui, pick, SPLIT_LABEL, false).on_hover_text("Show or hide the split lines").clicked() {
                toggle = true;
            }
        } else {
            let label = match &f.target {
                Target::None => "Uncategorized".to_string(),
                Target::Cat(c) => c.clone(),
                Target::Transfer(id) => format!("Transfer: {}", ledger.account_name(*id)),
            };
            let current = f.account;
            in_slot(ui, pick, |ui| {
                egui::ComboBox::from_id_salt("cat_pick").selected_text(label).width(pick.width() - 4.0).height(440.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.target, Target::None, italic("Uncategorized"));
                    for (kind, title) in [(CatKind::Expense, "Expense Categories:"), (CatKind::Income, "Income Categories:")] {
                        ui.separator();
                        ui.label(RichText::new(title).color(MUTED));
                        for c in ledger.categories.iter().filter(|c| c.kind == kind) {
                            ui.selectable_value(&mut f.target, Target::Cat(c.name.clone()), tree_label(&c.name));
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new("Transfers:").color(MUTED));
                    for a in ledger.accounts.iter().filter(|a| a.id != current && !a.closed) {
                        ui.selectable_value(&mut f.target, Target::Transfer(a.id), format!("Transfer: {}", a.name));
                    }
                });
            });
        }
        let fr = ui.allocate_rect(fork, Sense::click());
        if fr.hovered() || f.show_splits {
            ui.painter().rect_filled(fork, 4.0, Color32::from_white_alpha(if f.show_splits { 70 } else { 40 }));
        }
        draw_icon_public(ui.painter(), Icon::Split, fork.center(), 6.5, Color32::WHITE);
        if fr.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Split across categories").clicked() {
            toggle = true;
        }
        if let Some(e) = &f.error {
            let s = strip_slot(cell);
            ui.put(s, egui::Label::new(RichText::new(e).color(RED).small()).truncate()).on_hover_text(e);
        }
    });
    // Amount (the split total when split), with Cancel underneath.
    row.col(|ui| {
        paint(ui, 5);
        let cell = ui.max_rect();
        if split {
            field_button(ui, band_slot(cell), &f.amount, true).on_hover_text("The total of the split lines");
        } else {
            let r = text_field(ui, band_slot(cell), &mut f.amount, "0.00", true);
            enter_in(&r, ui);
        }
        if ui.put(strip_slot(cell), egui::Button::new("Cancel")).clicked() {
            cancel = true;
        }
    });
    // Balance column: Save underneath. The last column also draws the Splits panel.
    let mut panel = |ui: &mut egui::Ui, f: &mut TxnForm| {
        if f.show_splits {
            let cell = ui.max_rect();
            let area = egui::Rect::from_min_max(egui::pos2(body_clip.left(), cell.top() + EDIT_ROW_HEIGHT), egui::pos2(body_clip.right(), cell.bottom()));
            split_enter |= splits_panel(ui, ledger, f, area, body_clip);
        }
    };
    row.col(|ui| {
        paint(ui, 6);
        let cell = ui.max_rect();
        let save_btn = egui::Button::new(RichText::new("Save").color(Color32::WHITE)).fill(ACCENT).stroke(Stroke::NONE);
        if ui.put(strip_slot(cell), save_btn).on_hover_text("Or press Return").clicked() {
            save = true;
        }
        if !all {
            panel(ui, f);
        }
    });
    // Account (only when all accounts are shown).
    if all {
        row.col(|ui| {
            let slot = band_slot(ui.max_rect());
            paint(ui, 7);
            in_slot(ui, slot, |ui| {
                egui::ComboBox::from_id_salt("entry_acct").selected_text(ledger.account_name(f.account)).width(slot.width() - 4.0).show_ui(ui, |ui| {
                    let current = f.account;
                    for a in ledger.accounts.iter().filter(|a| !a.closed || a.id == current) {
                        ui.selectable_value(&mut f.account, a.id, &a.name);
                    }
                });
            });
            panel(ui, f);
        });
    }
    if toggle {
        if let Err(e) = f.toggle_splits() {
            out.split_error = Some(e);
        }
    }
    out.autofill |= autofill;
    out.cancel |= cancel;
    out.save |= save || split_enter;
}
