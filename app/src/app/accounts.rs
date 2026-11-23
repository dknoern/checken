//! The Add Account / Edit Account window. Adding is two steps: pick the kind
//! of account, then fill in its name, type, starting date and balance.

use super::*;
use chrono::Datelike;
use egui::{CornerRadius, StrokeKind};

const WIDTH: f32 = 560.0;
const TITLE_BAR: Color32 = Color32::from_rgb(0xf8, 0xf5, 0xef);
const FOOTER: Color32 = Color32::from_rgb(0xec, 0xec, 0xec);

/// The kinds offered on the first step: group, short title, description, default type.
const CHOICES: [(&str, &str, &str, AccountKind); 4] = [
    ("Banking", "Banking", "Checking, savings and cash", AccountKind::Checking),
    ("Credit Cards", "Credit Card", "Cards you pay off over time", AccountKind::CreditCard),
    ("Investments", "Investment", "Brokerage and retirement", AccountKind::Investment),
    ("Property & Debt", "Property & Debt", "Homes, cars and loans", AccountKind::Asset),
];

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Choose,
    Details,
}

/// Index of the Banking tile, for opening straight onto its details.
pub const CHOICE_BANKING: usize = 0;

pub struct AccountForm {
    editing: Option<u32>,
    step: Step,
    group: &'static str,
    name: String,
    kind: AccountKind,
    date: jiff::civil::Date,
    balance: String,
    closed: bool,
    confirm_delete: bool,
    error: Option<String>,
    focus: bool,
}

impl AccountForm {
    /// Moves to the details step for one of the kinds on the first step.
    pub fn pick(&mut self, choice: usize) {
        let (group, _, _, kind) = CHOICES[choice];
        self.group = group;
        self.kind = kind;
        self.step = Step::Details;
        self.focus = true;
        self.error = None;
    }
}

/// Cards and loans are entered as the amount owed and kept as a negative balance.
fn is_owed(kind: AccountKind) -> bool {
    matches!(kind, AccountKind::CreditCard | AccountKind::Loan)
}

fn to_jiff(d: NaiveDate) -> jiff::civil::Date {
    jiff::civil::date(d.year() as i16, d.month() as i8, d.day() as i8)
}

fn from_jiff(d: jiff::civil::Date) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year() as i32, d.month() as u32, d.day() as u32).unwrap_or_else(today)
}

fn choice_title(group: &str) -> &'static str {
    CHOICES.iter().find(|c| c.0 == group).map(|c| c.1).unwrap_or("Banking")
}

/// A small picture for each kind of account.
fn group_icon(p: &egui::Painter, r: egui::Rect, group: &str) {
    match group {
        "Banking" => {
            let green = Color32::from_rgb(0x3f, 0x8f, 0x2f);
            p.rect_stroke(r, 1.0, Stroke::new(2.5, green), StrokeKind::Inside);
            p.rect_stroke(r.shrink(5.0), 0.0, Stroke::new(1.2, Color32::from_rgb(0x9c, 0xcf, 0x8c)), StrokeKind::Inside);
            let oval = egui::Rect::from_center_size(r.center(), egui::vec2(r.height() * 0.45, r.height() * 0.62));
            p.add(egui::Shape::ellipse_stroke(oval.center(), oval.size() / 2.0, Stroke::new(1.2, Color32::from_rgb(0x9c, 0xcf, 0x8c))));
        }
        "Credit Cards" => {
            p.rect_filled(r, 4.0, Color32::from_rgb(0x3a, 0x6f, 0xd8));
            let stripe = egui::Rect::from_min_size(r.min + egui::vec2(0.0, r.height() * 0.22), egui::vec2(r.width(), r.height() * 0.2));
            p.rect_filled(stripe, 0.0, Color32::from_rgb(0x1f, 0x3f, 0x8a));
            let chip = egui::Rect::from_min_size(r.min + egui::vec2(r.width() * 0.12, r.height() * 0.55), egui::vec2(r.width() * 0.2, r.height() * 0.25));
            p.rect_filled(chip, 1.5, Color32::from_rgb(0xf2, 0xc9, 0x4c));
        }
        "Investments" => {
            let purple = Color32::from_rgb(0x8e, 0x5c, 0xd8);
            for (i, h) in [0.35, 0.55, 0.8].iter().enumerate() {
                let w = r.width() / 4.0;
                let x = r.left() + w * 0.3 + i as f32 * w * 1.25;
                let bar = egui::Rect::from_min_max(egui::pos2(x, r.bottom() - r.height() * h), egui::pos2(x + w, r.bottom()));
                p.rect_filled(bar, 1.5, purple);
            }
            p.hline(r.x_range(), r.bottom(), Stroke::new(1.5, Color32::from_gray(120)));
        }
        _ => {
            let orange = Color32::from_rgb(0xe8, 0x80, 0x24);
            let st = Stroke::new(2.2, orange);
            let (l, rr, t, b) = (r.left() + 4.0, r.right() - 4.0, r.top(), r.bottom());
            let mid = r.center().x;
            let eave = t + r.height() * 0.42;
            p.line_segment([egui::pos2(l - 3.0, eave), egui::pos2(mid, t)], st);
            p.line_segment([egui::pos2(mid, t), egui::pos2(rr + 3.0, eave)], st);
            p.rect_stroke(egui::Rect::from_min_max(egui::pos2(l, eave), egui::pos2(rr, b)), 0.0, st, StrokeKind::Inside);
            let door = egui::Rect::from_min_max(egui::pos2(mid - 4.0, b - r.height() * 0.32), egui::pos2(mid + 4.0, b));
            p.rect_filled(door, 0.0, orange);
        }
    }
}

/// A right-aligned field label of fixed width, as in a Mac form.
fn form_label(ui: &mut egui::Ui, text: &str) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(150.0, 30.0), Sense::hover());
    ui.painter().text(r.right_center(), Align2::RIGHT_CENTER, text, FontId::proportional(15.5), Color32::from_gray(30));
}

impl App {
    pub(super) fn open_account_form(&mut self, id: Option<u32>) {
        let form = match id.and_then(|id| self.ledger.account(id)) {
            Some(a) => AccountForm {
                editing: Some(a.id),
                step: Step::Details,
                group: a.kind.group(),
                name: a.name.clone(),
                kind: a.kind,
                date: to_jiff(a.opening_date),
                balance: format!("{:.2}", if is_owed(a.kind) { -a.opening_balance } else { a.opening_balance }),
                closed: a.closed,
                confirm_delete: false,
                error: None,
                focus: false,
            },
            None => AccountForm {
                editing: None,
                step: Step::Choose,
                group: "Banking",
                name: String::new(),
                kind: AccountKind::Checking,
                date: to_jiff(today()),
                balance: String::new(),
                closed: false,
                confirm_delete: false,
                error: None,
                focus: false,
            },
        };
        self.account_form = Some(form);
    }

    pub(super) fn account_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.account_form.take() else { return };
        let mut close = false;
        let mut finish = false;
        let mut delete = false;
        let n_txns = f
            .editing
            .map(|id| self.ledger.transactions.iter().filter(|t| t.account == id || t.transfer == Some(id)).count())
            .unwrap_or(0);
        let frame = egui::Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(10.0)
            .stroke(Stroke::new(1.0, Color32::from_gray(190)))
            .shadow(egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(70) });
        egui::Modal::new(egui::Id::unique("account_dialog")).frame(frame).show(ctx, |ui| {
            ui.set_width(WIDTH);
            ui.spacing_mut().item_spacing.y = 0.0;
            // Title bar with the red close button.
            let (bar, _) = ui.allocate_exact_size(egui::vec2(WIDTH, 40.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(bar, CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 }, TITLE_BAR);
            p.hline(bar.x_range(), bar.bottom(), Stroke::new(1.0, Color32::from_gray(222)));
            let title = if f.editing.is_some() { "Edit Account" } else { "Add Account" };
            p.text(bar.center(), Align2::CENTER_CENTER, title, semibold_font(16.0), Color32::from_gray(60));
            let dot = egui::Rect::from_center_size(egui::pos2(bar.left() + 22.0, bar.center().y), egui::vec2(16.0, 16.0));
            let dot_resp = ui.interact(dot, ui.scope_id().with("close_dot"), Sense::click());
            ui.painter().circle(dot.center(), 7.0, Color32::from_rgb(0xee, 0x5f, 0x57), Stroke::new(0.5, Color32::from_rgb(0xd0, 0x45, 0x3e)));
            if dot_resp.hovered() {
                ui.painter().text(dot.center(), Align2::CENTER_CENTER, "×", FontId::proportional(13.0), Color32::from_rgb(0x7a, 0x14, 0x10));
            }
            if dot_resp.on_hover_text("Close").clicked() {
                close = true;
            }

            // Body.
            egui::Frame::new().inner_margin(Margin { left: 26, right: 26, top: 20, bottom: 22 }).show(ui, |ui| {
                ui.set_width(WIDTH - 52.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                match f.step {
                    Step::Choose => self.choose_step(ui, &mut f),
                    Step::Details => self.details_step(ui, &mut f, &mut finish),
                }
            });

            // Footer.
            let (foot, _) = ui.allocate_exact_size(egui::vec2(WIDTH, 62.0), Sense::hover());
            ui.painter().rect_filled(foot, CornerRadius { nw: 0, ne: 0, sw: 10, se: 10 }, FOOTER);
            let inner = foot.shrink2(egui::vec2(20.0, 0.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(inner).layout(Layout::left_to_right(Align::Center)), |ui| {
                let big = |t: &str| egui::Button::new(RichText::new(t).size(15.5)).min_size(egui::vec2(96.0, 30.0)).corner_radius(6.0);
                match (f.editing.is_some(), f.step) {
                    (true, _) => {
                        let txt = if f.confirm_delete { format!("Delete it and {n_txns} transactions?") } else { "Delete Account".into() };
                        let b = egui::Button::new(RichText::new(txt).size(15.5).color(RED)).min_size(egui::vec2(96.0, 30.0)).corner_radius(6.0);
                        if ui.add(b).clicked() {
                            delete = true;
                        }
                    }
                    (false, Step::Details) => {
                        if ui.add(big("Start Over")).clicked() {
                            f.step = Step::Choose;
                            f.error = None;
                        }
                    }
                    (false, Step::Choose) => {}
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    if f.step == Step::Details {
                        let label = if f.editing.is_some() { "Save" } else { "Finish" };
                        let b = egui::Button::new(RichText::new(label).size(15.5).color(Color32::WHITE))
                            .fill(ACCENT)
                            .stroke(Stroke::NONE)
                            .min_size(egui::vec2(116.0, 30.0))
                            .corner_radius(6.0);
                        if ui.add(b).clicked() {
                            finish = true;
                        }
                    }
                    if ui.add(big("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if finish && !close {
            self.save_account_form(&mut f, &mut close);
        }
        if delete {
            if f.confirm_delete {
                if let Some(id) = f.editing {
                    self.delete_account(id);
                }
                close = true;
            } else {
                f.confirm_delete = true;
            }
        }
        if !close {
            self.account_form = Some(f);
        }
    }

    /// Step one: a tile for each kind of account.
    fn choose_step(&self, ui: &mut egui::Ui, f: &mut AccountForm) {
        ui.label(RichText::new("What kind of account would you like to add?").size(20.0).color(NAVY));
        ui.add_space(8.0);
        let tile_w = (ui.available_width() - 14.0) / 2.0;
        for pair in CHOICES.chunks(2) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                for (group, title, blurb, _) in pair {
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(tile_w, 86.0), Sense::click());
                    let p = ui.painter();
                    let hover = resp.hovered();
                    p.rect(
                        r,
                        8.0,
                        if hover { Color32::from_rgb(0xee, 0xf4, 0xfd) } else { Color32::WHITE },
                        Stroke::new(1.0, if hover { ACCENT } else { Color32::from_gray(210) }),
                        StrokeKind::Inside,
                    );
                    let icon = egui::Rect::from_min_size(egui::pos2(r.left() + 18.0, r.center().y - 17.0), egui::vec2(52.0, 34.0));
                    group_icon(p, icon, group);
                    let x = icon.right() + 18.0;
                    p.text(egui::pos2(x, r.center().y - 11.0), Align2::LEFT_CENTER, *title, semibold_font(17.0), NAVY);
                    p.text(egui::pos2(x, r.center().y + 13.0), Align2::LEFT_CENTER, *blurb, FontId::proportional(13.5), MUTED);
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        f.pick(CHOICES.iter().position(|c| c.0 == *group).unwrap_or(0));
                    }
                }
            });
            ui.add_space(6.0);
        }
    }

    /// Step two (and editing): name, type, starting date and balance.
    fn details_step(&self, ui: &mut egui::Ui, f: &mut AccountForm, finish: &mut bool) {
        ui.horizontal(|ui| {
            let (icon, _) = ui.allocate_exact_size(egui::vec2(64.0, 40.0), Sense::hover());
            group_icon(ui.painter(), icon.shrink2(egui::vec2(4.0, 2.0)), f.group);
            ui.add_space(12.0);
            let verb = if f.editing.is_some() { "Edit" } else { "Add" };
            ui.label(RichText::new(format!("{verb} {} Account", choice_title(f.group))).size(28.0).color(NAVY));
        });
        ui.add_space(12.0);
        ui.separator();
        ui.add_space(14.0);
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        // Adding: the kinds of the tile picked. Editing: any kind, so an account
        // can move (say a line of credit from Checking to Credit Card).
        let kinds: Vec<AccountKind> = if f.editing.is_some() {
            AccountKind::LIST_ORDER.to_vec()
        } else {
            AccountKind::ALL.iter().copied().filter(|k| k.group() == f.group).collect()
        };
        egui::Grid::new("acct_grid").num_columns(2).spacing([12.0, 14.0]).show(ui, |ui| {
            form_label(ui, "Account Name:");
            let name = ui.add(egui::TextEdit::singleline(&mut f.name).hint_text(f.kind.label()).desired_width(300.0));
            if f.focus {
                name.request_focus();
                f.focus = false;
            }
            *finish |= name.lost_focus() && enter;
            ui.end_row();

            form_label(ui, "Account Type:");
            egui::ComboBox::from_id_salt("acct_kind").selected_text(f.kind.label()).width(300.0).show_ui(ui, |ui| {
                for k in &kinds {
                    ui.selectable_value(&mut f.kind, *k, k.label());
                }
            });
            ui.end_row();

            form_label(ui, "On This Date:");
            ui.add(egui_extras::DatePickerButton::new(&mut f.date).id_salt("acct_date").format("%m/%d/%Y").calendar_week(false));
            ui.end_row();

            form_label(ui, if is_owed(f.kind) { "Amount Owed:" } else { "Balance Equals:" });
            ui.horizontal(|ui| {
                let bal = ui.add(egui::TextEdit::singleline(&mut f.balance).hint_text("0.00").align(Align2::RIGHT_CENTER).desired_width(150.0));
                *finish |= bal.lost_focus() && enter;
                let tip = if is_owed(f.kind) { "What you owe on this date" } else { "What the account held on this date" };
                ui.label(RichText::new(tip).small().color(MUTED));
            });
            ui.end_row();

            if f.editing.is_some() {
                form_label(ui, "Closed:");
                ui.checkbox(&mut f.closed, "Move to Hidden Accounts");
                ui.end_row();
            }
        });
        if let Some(e) = &f.error {
            ui.add_space(6.0);
            ui.label(RichText::new(e).color(RED));
        }
    }

    fn save_account_form(&mut self, f: &mut AccountForm, close: &mut bool) {
        let name = match f.name.trim() {
            "" => f.kind.label().to_string(),
            n => n.to_string(),
        };
        let Some(entered) = parse_money(&f.balance) else {
            f.error = Some("The balance should be a number, like 1250.00.".into());
            return;
        };
        let open = if is_owed(f.kind) { -entered.abs() } else { entered };
        let date = from_jiff(f.date);
        match f.editing {
            Some(id) => {
                if let Some(a) = self.ledger.accounts.iter_mut().find(|a| a.id == id) {
                    a.name = name.clone();
                    a.kind = f.kind;
                    a.opening_balance = open;
                    a.opening_date = date;
                    a.closed = f.closed;
                }
                self.status = format!("Updated {name}.");
            }
            None => {
                let id = self.ledger.next_account_id();
                self.ledger.accounts.push(Account { id, name: name.clone(), kind: f.kind, opening_balance: open, opening_date: date, closed: false });
                self.status = format!("Added {name}. Click New to enter its first transaction.");
                self.open_register(Some(id));
            }
        }
        self.dirty = true;
        *close = true;
    }

    /// Deletes an account and every transaction entered in it, after keeping a
    /// safety copy. A transfer from another account stays in that account as
    /// a plain "Transfer" line (kept out of spending and income), so its
    /// balance doesn't change.
    pub(super) fn delete_account(&mut self, id: u32) {
        let name = self.ledger.account_name(id);
        if self.shot.is_none() {
            let stamp = chrono::Local::now().format("%Y-%m-%d %H%M%S");
            let safety = data_path().with_file_name(format!("ledger before deleting an account {stamp}.json"));
            if let Err(e) = self.ledger.save_to(&safety) {
                self.status = format!("Didn't delete {name}: couldn't keep a safety copy first ({e}).");
                return;
            }
        }
        self.ledger.accounts.retain(|a| a.id != id);
        self.ledger.transactions.retain(|t| t.account != id);
        // A deleted account no longer downloads from the bank; a connection
        // with no accounts left is forgotten, key and all.
        if let Some(link) = self.ledger.plaid.links.remove(&id) {
            self.plaid_forget_if_unused(&link.item_id);
        }
        for t in &mut self.ledger.transactions {
            if t.transfer == Some(id) {
                t.transfer = None;
                if t.category.is_empty() {
                    t.category = "Transfer".into();
                }
            }
        }
        self.status = format!("Deleted {name}.");
        self.dirty = true;
        self.selected = None;
        self.form = None;
        if self.acct_filter == Some(id) || self.view == View::Register {
            self.open_register(None);
        }
    }

    /// The "Delete account?" confirmation, as Quicken's sidebar Delete… asks.
    pub(super) fn delete_account_dialog(&mut self, ctx: &egui::Context) {
        let Some(id) = self.pending_delete_account.take() else { return };
        let Some(acct) = self.ledger.account(id).cloned() else { return };
        let own = self.ledger.transactions.iter().filter(|t| t.account == id).count();
        let linked = self.ledger.transactions.iter().filter(|t| t.transfer == Some(id)).count();
        let mut close = false;
        let mut confirm = false;
        let width = 500.0;
        let frame = egui::Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(10.0)
            .stroke(Stroke::new(1.0, Color32::from_gray(190)))
            .shadow(egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(70) });
        egui::Modal::new(egui::Id::unique("delete_account_dialog")).frame(frame).show(ctx, |ui| {
            ui.set_width(width);
            egui::Frame::new().inner_margin(Margin { left: 26, right: 26, top: 22, bottom: 18 }).show(ui, |ui| {
                ui.set_width(width - 52.0);
                ui.label(RichText::new(format!("Delete “{}”?", acct.name)).size(22.0).color(NAVY));
                ui.add_space(10.0);
                let what = match own {
                    0 => "The account will be deleted. It has no transactions of its own.".to_string(),
                    1 => "The account and its 1 transaction will be deleted.".to_string(),
                    n => format!("The account and its {n} transactions will be deleted."),
                };
                ui.label(what);
                if linked > 0 {
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!(
                        "{linked} transfer{} from other accounts stay in those accounts as “Transfer” lines, so their balances don't change.",
                        if linked == 1 { "" } else { "s" }
                    ))
                    .color(MUTED));
                }
                ui.add_space(4.0);
                ui.label(RichText::new("To keep it but take it out of the list, use Hide instead. Your current data is kept as a safety copy first.").color(MUTED));
                ui.add_space(16.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let b = egui::Button::new(RichText::new("Delete").size(15.0).color(Color32::WHITE))
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
            self.delete_account(id);
            return;
        }
        if !close {
            self.pending_delete_account = Some(id);
        }
    }
}
