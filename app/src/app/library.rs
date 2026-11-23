//! The report library: a toolbar (New, Open, Edit, Duplicate, Export,
//! Delete), folders on the left, and a table of reports. Standard reports
//! come built in; the person's own reports are saved under My Reports.

use super::*;
use egui::{CornerRadius, StrokeKind};
use egui_extras::{Column, TableBuilder};

#[derive(Clone, Copy, PartialEq)]
pub enum ReportKey {
    Saved(u64),
    Builtin(usize),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Folder {
    All,
    Recent,
    Mine,
    Standard(ReportKind),
}

impl Folder {
    pub fn title(self) -> &'static str {
        match self {
            Folder::All => "All Reports",
            Folder::Recent => "Recently Opened",
            Folder::Mine => "My Reports",
            Folder::Standard(k) => folder_name(k),
        }
    }
}

/// The reports that come with the app: name, kind, date range.
const BUILTIN: [(&str, ReportKind, Period); 8] = [
    ("Budget vs. Actual This Month", ReportKind::BudgetVsActual, Period::ThisMonth),
    ("Budget vs. Actual Year to Date", ReportKind::BudgetVsActual, Period::YearToDate),
    ("Spending by Category", ReportKind::Spending, Period::ThisMonth),
    ("Spending, Last 3 Months", ReportKind::Spending, Period::Last3),
    ("Income vs. Expense by Month", ReportKind::IncomeExpense, Period::Last12),
    ("Income vs. Expense Year to Date", ReportKind::IncomeExpense, Period::YearToDate),
    ("Net Worth Over Time", ReportKind::NetWorth, Period::Last12),
    ("Net Worth This Year", ReportKind::NetWorth, Period::YearToDate),
];

fn folder_name(kind: ReportKind) -> &'static str {
    match kind {
        ReportKind::BudgetVsActual => "Budget",
        ReportKind::Spending => "Summary",
        ReportKind::IncomeExpense => "Comparison",
        ReportKind::NetWorth => "Net Worth",
    }
}

fn rows_label(kind: ReportKind) -> &'static str {
    match kind {
        ReportKind::BudgetVsActual | ReportKind::Spending => "Category",
        ReportKind::IncomeExpense | ReportKind::NetWorth => "Month",
    }
}

/// One line of the report table.
#[derive(Clone)]
pub struct Entry {
    pub key: ReportKey,
    pub name: String,
    pub kind: ReportKind,
    pub period: Period,
    pub folder: &'static str,
    pub description: String,
}

pub struct ReportEditor {
    editing: Option<u64>,
    name: String,
    kind: ReportKind,
    period: Period,
    description: String,
    error: Option<String>,
    focus: bool,
}

/// A small green report badge with a rising chart on it.
fn report_icon(p: &egui::Painter, c: egui::Pos2) {
    let r = egui::Rect::from_center_size(c, egui::vec2(20.0, 20.0));
    p.rect_filled(r, 4.0, Color32::from_rgb(0x4c, 0xa8, 0x4a));
    for (i, h) in [5.0, 9.0, 13.0].iter().enumerate() {
        let x = r.left() + 4.0 + i as f32 * 4.5;
        p.rect_filled(egui::Rect::from_min_max(egui::pos2(x, r.bottom() - 3.0 - h), egui::pos2(x + 3.0, r.bottom() - 3.0)), 0.5, Color32::WHITE);
    }
}

fn folder_icon(p: &egui::Painter, c: egui::Pos2) {
    let st = Stroke::new(1.2, Color32::from_gray(95));
    let body = egui::Rect::from_center_size(c + egui::vec2(0.0, 1.0), egui::vec2(18.0, 12.0));
    p.rect_stroke(body, 2.0, st, StrokeKind::Middle);
    let tab = egui::Rect::from_min_size(body.left_top() + egui::vec2(0.0, -3.0), egui::vec2(7.0, 3.0));
    p.rect_stroke(tab, 1.0, st, StrokeKind::Middle);
}

/// A row in the folder list. Returns (row clicked, plus-button clicked).
fn folder_row(ui: &mut egui::Ui, label: &str, indent: f32, selected: bool, bold: bool, folder: bool, plus: bool) -> (bool, bool) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), Sense::click());
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, 0.0, Color32::from_gray(222));
    } else if resp.hovered() {
        p.rect_filled(rect, 0.0, Color32::from_gray(234));
    }
    let mut x = rect.left() + 14.0 + indent;
    if folder {
        folder_icon(p, egui::pos2(x + 9.0, rect.center().y));
        x += 26.0;
    }
    let font = if bold { semibold_font(15.0) } else { FontId::proportional(15.0) };
    p.text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, label, font, NAVY);
    let mut plus_hit = false;
    if plus {
        let pc = egui::pos2(rect.right() - 16.0, rect.center().y);
        p.circle_stroke(pc, 7.0, Stroke::new(1.0, Color32::from_gray(120)));
        p.line_segment([pc + egui::vec2(-3.5, 0.0), pc + egui::vec2(3.5, 0.0)], Stroke::new(1.0, Color32::from_gray(90)));
        p.line_segment([pc + egui::vec2(0.0, -3.5), pc + egui::vec2(0.0, 3.5)], Stroke::new(1.0, Color32::from_gray(90)));
        plus_hit = resp.clicked() && resp.interact_pointer_pos().is_some_and(|pos| pos.distance(pc) < 11.0);
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    (resp.clicked() && !plus_hit, plus_hit)
}

impl App {
    /// Every report, built-in ones first, then the person's own.
    fn all_reports(&self) -> Vec<Entry> {
        let mut v: Vec<Entry> = BUILTIN
            .iter()
            .enumerate()
            .map(|(i, (name, kind, period))| Entry {
                key: ReportKey::Builtin(i),
                name: name.to_string(),
                kind: *kind,
                period: *period,
                folder: folder_name(*kind),
                description: kind.blurb().to_string(),
            })
            .collect();
        v.extend(self.ledger.reports.iter().map(|r| Entry {
            key: ReportKey::Saved(r.id),
            name: r.name.clone(),
            kind: r.kind,
            period: r.period,
            folder: "My Reports",
            description: if r.description.is_empty() { r.kind.blurb().to_string() } else { r.description.clone() },
        }));
        v
    }

    pub(super) fn report_entry(&self, key: ReportKey) -> Option<Entry> {
        self.all_reports().into_iter().find(|e| e.key == key)
    }

    fn folder_reports(&self) -> Vec<Entry> {
        let all = self.all_reports();
        let needle = self.report_search.to_lowercase();
        let mut v: Vec<Entry> = match self.report_folder {
            Folder::All => all,
            Folder::Mine => all.into_iter().filter(|e| matches!(e.key, ReportKey::Saved(_))).collect(),
            Folder::Standard(k) => all.into_iter().filter(|e| matches!(e.key, ReportKey::Builtin(_)) && e.kind == k).collect(),
            Folder::Recent => self.recent_reports.iter().filter_map(|k| all.iter().find(|e| e.key == *k).cloned()).collect(),
        };
        if !needle.is_empty() {
            v.retain(|e| e.name.to_lowercase().contains(&needle) || e.description.to_lowercase().contains(&needle) || e.folder.to_lowercase().contains(&needle));
        }
        if self.report_folder != Folder::Recent {
            v.sort_by_key(|e| e.name.to_lowercase());
        }
        v
    }

    pub(super) fn open_report(&mut self, key: ReportKey) {
        let Some(e) = self.report_entry(key) else { return };
        self.report = e.kind;
        self.period = e.period;
        self.report_open = Some(key);
        self.report_sel = Some(key);
        self.recent_reports.retain(|k| *k != key);
        self.recent_reports.insert(0, key);
        self.recent_reports.truncate(12);
        self.view = View::Reports;
    }

    pub(super) fn new_report(&mut self) {
        self.report_editor = Some(ReportEditor {
            editing: None,
            name: String::new(),
            kind: ReportKind::Spending,
            period: Period::ThisMonth,
            description: String::new(),
            error: None,
            focus: true,
        });
    }

    pub(super) fn edit_report(&mut self, key: ReportKey) {
        let ReportKey::Saved(id) = key else { return };
        let Some(r) = self.ledger.reports.iter().find(|r| r.id == id) else { return };
        self.report_editor = Some(ReportEditor {
            editing: Some(id),
            name: r.name.clone(),
            kind: r.kind,
            period: r.period,
            description: r.description.clone(),
            error: None,
            focus: true,
        });
    }

    fn duplicate_report(&mut self, key: ReportKey) {
        let Some(e) = self.report_entry(key) else { return };
        let name = match key {
            ReportKey::Saved(_) => format!("{} (Copy)", e.name),
            ReportKey::Builtin(_) => e.name.clone(),
        };
        let description = match key {
            ReportKey::Saved(id) => self.ledger.reports.iter().find(|r| r.id == id).map(|r| r.description.clone()).unwrap_or_default(),
            ReportKey::Builtin(_) => String::new(),
        };
        let id = self.ledger.new_id();
        self.ledger.reports.push(SavedReport { id, name: name.clone(), kind: e.kind, period: e.period, description });
        self.dirty = true;
        self.report_folder = Folder::Mine;
        self.report_sel = Some(ReportKey::Saved(id));
        self.status = format!("Saved a copy as \"{name}\" in My Reports.");
    }

    pub(super) fn report_library(&mut self, ui: &mut egui::Ui) {
        page_title(ui, self.report_folder.title(), "", None);
        ui.add_space(8.0);
        let sel = self.report_sel.filter(|k| self.report_entry(*k).is_some());
        let saved_sel = matches!(sel, Some(ReportKey::Saved(_)));
        band(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                if tool_text_button(ui, Icon::New, "New", true, false).on_hover_text("Make a new report").clicked() {
                    self.new_report();
                }
                if tool_text_button(ui, Icon::Open, "Open", sel.is_some(), false).clicked() {
                    if let Some(k) = sel {
                        self.open_report(k);
                    }
                }
                let edit = tool_text_button(ui, Icon::Edit, "Edit", saved_sel, false);
                if edit.on_disabled_hover_text("Standard reports can't be changed. Duplicate one to make your own.").clicked() {
                    if let Some(k) = sel {
                        self.edit_report(k);
                    }
                }
                if tool_text_button(ui, Icon::Duplicate, "Duplicate", sel.is_some(), false).on_hover_text("Save a copy under My Reports").clicked() {
                    if let Some(k) = sel {
                        self.duplicate_report(k);
                    }
                }
                if tool_text_button(ui, Icon::Print, "Print", sel.is_some(), false).on_hover_text("Print the selected report (⌘P)").clicked() {
                    if let Some(e) = sel.and_then(|k| self.report_entry(k)) {
                        self.print_report(&e.name, &e.description, e.kind, e.period);
                    }
                }
                if tool_text_button(ui, Icon::Copy, "Export", sel.is_some(), false).on_hover_text("Copy the report's table for a spreadsheet").clicked() {
                    if let Some(e) = sel.and_then(|k| self.report_entry(k)) {
                        ui.ctx().copy_text(self.report_csv(e.kind, e.period));
                        self.status = format!("Copied {}. Paste it into a spreadsheet.", e.name);
                    }
                }
                let label = if self.report_confirm_delete { "Confirm Delete" } else { "Delete" };
                let del = tool_text_button(ui, Icon::Delete, label, saved_sel, self.report_confirm_delete);
                if del.on_hover_text("Click twice to delete the selected report").clicked() {
                    if let Some(ReportKey::Saved(id)) = sel {
                        if self.report_confirm_delete {
                            let name = self.ledger.reports.iter().find(|r| r.id == id).map(|r| r.name.clone()).unwrap_or_default();
                            self.ledger.reports.retain(|r| r.id != id);
                            self.recent_reports.retain(|k| *k != ReportKey::Saved(id));
                            self.report_sel = None;
                            self.report_confirm_delete = false;
                            self.dirty = true;
                            self.status = format!("Deleted the report \"{name}\".");
                        } else {
                            self.report_confirm_delete = true;
                        }
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.report_search).hint_text("🔍 Search All Reports").min_size(egui::vec2(260.0, 0.0)));
                });
            });
        });

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            // Folder list.
            let height = ui.available_height();
            let (left, _) = ui.allocate_exact_size(egui::vec2(220.0, height), Sense::hover());
            ui.painter().rect_filled(left, 0.0, Color32::from_rgb(0xf6, 0xf6, 0xf6));
            ui.painter().vline(left.right(), left.y_range(), Stroke::new(1.0, LINE));
            let mut pick: Option<Folder> = None;
            let mut add = false;
            let list = egui::UiBuilder::new().max_rect(left.shrink2(egui::vec2(0.0, 6.0))).layout(Layout::top_down(Align::Min));
            ui.scope_builder(list, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let f = self.report_folder;
                if folder_row(ui, "All Reports", 0.0, f == Folder::All, true, false, false).0 {
                    pick = Some(Folder::All);
                }
                if folder_row(ui, "Recently Opened", 0.0, f == Folder::Recent, true, false, false).0 {
                    pick = Some(Folder::Recent);
                }
                let (c, p) = folder_row(ui, "My Reports", 0.0, f == Folder::Mine, true, false, true);
                if c {
                    pick = Some(Folder::Mine);
                }
                add |= p;
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), Sense::click());
                chevron(ui, egui::pos2(rect.left() + 14.0, rect.center().y), self.standard_open);
                ui.painter().text(egui::pos2(rect.left() + 28.0, rect.center().y), Align2::LEFT_CENTER, "Standard Reports", semibold_font(15.0), NAVY);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.standard_open = !self.standard_open;
                }
                if self.standard_open {
                    for k in ReportKind::ALL {
                        if folder_row(ui, folder_name(k), 18.0, f == Folder::Standard(k), false, true, false).0 {
                            pick = Some(Folder::Standard(k));
                        }
                    }
                }
            });
            if let Some(p) = pick {
                self.report_folder = p;
                self.report_confirm_delete = false;
            }
            if add {
                self.report_folder = Folder::Mine;
                self.new_report();
            }

            // Report table.
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                self.report_table_ui(ui);
            });
        });
    }

    fn report_table_ui(&mut self, ui: &mut egui::Ui) {
        let entries = self.folder_reports();
        let sel = self.report_sel;
        let mut clicked: Option<ReportKey> = None;
        let mut double: Option<ReportKey> = None;
        let clip = ui.clip_rect().intersect(ui.max_rect().expand2(egui::vec2(0.0, 2000.0)));
        let body_top = ui.cursor().top() + 30.0 + ui.spacing().item_spacing.y;
        let body_clip = egui::Rect::from_min_max(egui::pos2(clip.left(), body_top.max(clip.top())), clip.max);
        let last_col = 5;
        TableBuilder::new(ui)
            .striped(true)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(34.0))
            .column(Column::initial(220.0).at_least(140.0).clip(true))
            .column(Column::exact(110.0))
            .column(Column::remainder().at_least(100.0).clip(true))
            .column(Column::exact(84.0))
            .column(Column::exact(124.0))
            .header(30.0, |mut h| {
                for (i, name) in ["", "REPORT NAME", "FOLDER", "DESCRIPTION", "ROWS", "DATE"].iter().enumerate() {
                    h.col(|ui| {
                        if i == 0 {
                            ui.painter().circle_filled(ui.max_rect().center(), 3.5, Color32::from_gray(70));
                        } else {
                            let color = if i == 1 { ACCENT } else { Color32::from_gray(90) };
                            ui.label(col_header(name, color));
                        }
                        let r = ui.max_rect();
                        ui.painter().hline(r.x_range(), r.bottom() + 2.0, Stroke::new(1.0, Color32::from_rgb(0xc9, 0xcf, 0xa8)));
                    });
                }
            })
            .body(|body| {
                body.rows(34.0, entries.len(), |mut row| {
                    let e = &entries[row.index()];
                    let selected = sel == Some(e.key);
                    // A solid blue selection, as in the register.
                    let fg = if selected { Color32::WHITE } else { Color32::from_gray(20) };
                    // Each cell paints its piece of one unbroken band across the row,
                    // reaching over the gap to its left; the last runs to the edge.
                    let mut col = 0;
                    let mut cell_bg = |ui: &egui::Ui| {
                        if selected {
                            let cell = ui.max_rect();
                            let left = if col == 0 { body_clip.left() } else { cell.left() - ui.spacing().item_spacing.x };
                            let right = if col == last_col { body_clip.right() } else { cell.right() };
                            let mut p = ui.painter().clone();
                            p.set_clip_rect(body_clip);
                            p.rect_filled(egui::Rect::from_x_y_ranges(left..=right, cell.y_range()), 0.0, entry::EDIT_BLUE);
                        }
                        col += 1;
                    };
                    row.col(|ui| {
                        cell_bg(ui);
                        report_icon(ui.painter(), ui.max_rect().center());
                    });
                    row.col(|ui| {
                        cell_bg(ui);
                        ui.label(RichText::new(&e.name).color(fg));
                    });
                    row.col(|ui| {
                        cell_bg(ui);
                        ui.label(RichText::new(e.folder).color(fg));
                    });
                    row.col(|ui| {
                        cell_bg(ui);
                        ui.label(RichText::new(&e.description).color(fg));
                    });
                    row.col(|ui| {
                        cell_bg(ui);
                        ui.label(RichText::new(rows_label(e.kind)).color(fg));
                    });
                    row.col(|ui| {
                        cell_bg(ui);
                        ui.label(RichText::new(e.period.label()).color(fg));
                    });
                    let resp = row.response();
                    if resp.double_clicked() {
                        double = Some(e.key);
                    } else if resp.clicked() {
                        clicked = Some(e.key);
                    }
                });
            });
        if entries.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                let msg = match self.report_folder {
                    Folder::Mine => "No reports saved yet. Click New, or select a standard report and click Duplicate.",
                    Folder::Recent => "Reports you open will show up here.",
                    _ => "No reports match your search.",
                };
                ui.label(RichText::new(msg).color(MUTED));
            });
        }
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && self.report_editor.is_none() && !ui.ctx().egui_wants_keyboard_input() {
            if let Some(k) = sel {
                double = Some(k);
            }
        }
        if let Some(k) = double {
            self.open_report(k);
        } else if let Some(k) = clicked {
            if self.report_sel == Some(k) {
                self.open_report(k);
            } else {
                self.report_confirm_delete = false;
                self.report_sel = Some(k);
            }
        }
    }

    /// The New Report / Edit Report window.
    pub(super) fn report_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut ed) = self.report_editor.take() else { return };
        let mut close = false;
        let mut save = false;
        let width = 540.0;
        let frame = egui::Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(10.0)
            .stroke(Stroke::new(1.0, Color32::from_gray(190)))
            .shadow(egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(70) });
        egui::Modal::new(egui::Id::unique("report_dialog")).frame(frame).show(ctx, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.0;
            let (bar, _) = ui.allocate_exact_size(egui::vec2(width, 40.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(bar, CornerRadius { nw: 10, ne: 10, sw: 0, se: 0 }, Color32::from_rgb(0xf8, 0xf5, 0xef));
            p.hline(bar.x_range(), bar.bottom(), Stroke::new(1.0, Color32::from_gray(222)));
            let title = if ed.editing.is_some() { "Edit Report" } else { "New Report" };
            p.text(bar.center(), Align2::CENTER_CENTER, title, semibold_font(16.0), Color32::from_gray(60));
            let dot = egui::Rect::from_center_size(egui::pos2(bar.left() + 22.0, bar.center().y), egui::vec2(16.0, 16.0));
            let dot_resp = ui.interact(dot, ui.scope_id().with("close_dot"), Sense::click());
            ui.painter().circle(dot.center(), 7.0, Color32::from_rgb(0xee, 0x5f, 0x57), Stroke::new(0.5, Color32::from_rgb(0xd0, 0x45, 0x3e)));
            if dot_resp.on_hover_text("Close").clicked() {
                close = true;
            }
            egui::Frame::new().inner_margin(Margin { left: 26, right: 26, top: 20, bottom: 22 }).show(ui, |ui| {
                ui.set_width(width - 52.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.horizontal(|ui| {
                    let (icon, _) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), Sense::hover());
                    let p = ui.painter();
                    let big = egui::Rect::from_center_size(icon.center(), egui::vec2(36.0, 36.0));
                    p.rect_filled(big, 7.0, Color32::from_rgb(0x4c, 0xa8, 0x4a));
                    for (i, h) in [9.0, 16.0, 24.0].iter().enumerate() {
                        let x = big.left() + 7.0 + i as f32 * 8.0;
                        p.rect_filled(egui::Rect::from_min_max(egui::pos2(x, big.bottom() - 6.0 - h), egui::pos2(x + 5.0, big.bottom() - 6.0)), 1.0, Color32::WHITE);
                    }
                    ui.add_space(12.0);
                    ui.label(RichText::new(title).size(26.0).color(NAVY));
                });
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(12.0);
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let label = |ui: &mut egui::Ui, text: &str| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(130.0, 30.0), Sense::hover());
                    ui.painter().text(r.right_center(), Align2::RIGHT_CENTER, text, FontId::proportional(15.5), Color32::from_gray(30));
                };
                egui::Grid::new("report_grid").num_columns(2).spacing([12.0, 14.0]).show(ui, |ui| {
                    label(ui, "Report Name:");
                    let name = ui.add(egui::TextEdit::singleline(&mut ed.name).hint_text(ed.kind.title()).desired_width(320.0));
                    if ed.focus {
                        name.request_focus();
                        ed.focus = false;
                    }
                    save |= name.lost_focus() && enter;
                    ui.end_row();
                    label(ui, "Report Type:");
                    egui::ComboBox::from_id_salt("report_kind").selected_text(ed.kind.title()).width(320.0).show_ui(ui, |ui| {
                        for k in ReportKind::ALL {
                            ui.selectable_value(&mut ed.kind, k, k.title());
                        }
                    });
                    ui.end_row();
                    label(ui, "");
                    ui.label(RichText::new(ed.kind.blurb()).small().color(MUTED));
                    ui.end_row();
                    label(ui, "Date Range:");
                    egui::ComboBox::from_id_salt("report_period").selected_text(ed.period.label()).width(200.0).show_ui(ui, |ui| {
                        for p in Period::ALL {
                            ui.selectable_value(&mut ed.period, p, p.label());
                        }
                    });
                    ui.end_row();
                    label(ui, "Description:");
                    let d = ui.add(egui::TextEdit::singleline(&mut ed.description).hint_text("Optional").desired_width(320.0));
                    save |= d.lost_focus() && enter;
                    ui.end_row();
                });
                if let Some(e) = &ed.error {
                    ui.label(RichText::new(e).color(RED));
                }
            });
            let (foot, _) = ui.allocate_exact_size(egui::vec2(width, 62.0), Sense::hover());
            ui.painter().rect_filled(foot, CornerRadius { nw: 0, ne: 0, sw: 10, se: 10 }, Color32::from_rgb(0xec, 0xec, 0xec));
            ui.scope_builder(egui::UiBuilder::new().max_rect(foot.shrink2(egui::vec2(20.0, 0.0))).layout(Layout::right_to_left(Align::Center)), |ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                let label = if ed.editing.is_some() { "Save" } else { "Create" };
                let b = egui::Button::new(RichText::new(label).size(15.5).color(Color32::WHITE))
                    .fill(ACCENT)
                    .stroke(Stroke::NONE)
                    .min_size(egui::vec2(116.0, 30.0))
                    .corner_radius(6.0);
                if ui.add(b).clicked() {
                    save = true;
                }
                if ui.add(egui::Button::new(RichText::new("Cancel").size(15.5)).min_size(egui::vec2(96.0, 30.0)).corner_radius(6.0)).clicked() {
                    close = true;
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if save && !close {
            let name = match ed.name.trim() {
                "" => ed.kind.title().to_string(),
                n => n.to_string(),
            };
            let description = ed.description.trim().to_string();
            match ed.editing {
                Some(id) => {
                    if let Some(r) = self.ledger.reports.iter_mut().find(|r| r.id == id) {
                        r.name = name.clone();
                        r.kind = ed.kind;
                        r.period = ed.period;
                        r.description = description;
                    }
                    if self.report_open == Some(ReportKey::Saved(id)) {
                        self.report = ed.kind;
                        self.period = ed.period;
                    }
                    self.status = format!("Saved the report \"{name}\".");
                }
                None => {
                    let id = self.ledger.new_id();
                    self.ledger.reports.push(SavedReport { id, name: name.clone(), kind: ed.kind, period: ed.period, description });
                    self.report_folder = Folder::Mine;
                    self.open_report(ReportKey::Saved(id));
                    self.status = format!("Created the report \"{name}\" in My Reports.");
                }
            }
            self.dirty = true;
            close = true;
        }
        if !close {
            self.report_editor = Some(ed);
        }
    }
}
