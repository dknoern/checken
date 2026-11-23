//! The transactions screen: one account or all of them, with Transactions /
//! Spending / Income tabs, filters, a category ring chart and the register.

use super::*;
use chrono::Datelike;
use egui_extras::{Column, TableBuilder};

#[derive(Clone, Copy, PartialEq)]
pub enum RegTab {
    Transactions,
    Spending,
    Income,
}

#[derive(Clone, Copy, PartialEq)]
pub enum DateFilter {
    All,
    ThisMonth,
    LastMonth,
    Last30,
    Last90,
    ThisYear,
}

impl DateFilter {
    const ALL: [DateFilter; 6] =
        [DateFilter::ThisYear, DateFilter::ThisMonth, DateFilter::LastMonth, DateFilter::Last30, DateFilter::Last90, DateFilter::All];
    fn label(self) -> &'static str {
        match self {
            DateFilter::All => "All Dates",
            DateFilter::ThisMonth => "This Month",
            DateFilter::LastMonth => "Last Month",
            DateFilter::Last30 => "Last 30 Days",
            DateFilter::Last90 => "Last 90 Days",
            DateFilter::ThisYear => "This Year",
        }
    }
    fn keeps(self, d: NaiveDate) -> bool {
        let t = today();
        let m = first_of_month(t);
        match self {
            DateFilter::All => true,
            DateFilter::ThisMonth => d >= m,
            DateFilter::LastMonth => d >= m - chrono::Months::new(1) && d < m,
            DateFilter::Last30 => d >= t - chrono::Days::new(30),
            DateFilter::Last90 => d >= t - chrono::Days::new(90),
            DateFilter::ThisYear => d.year() == t.year(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum StatusFilter {
    Any,
    Uncleared,
    Cleared,
    Reconciled,
}

impl StatusFilter {
    const ALL: [StatusFilter; 4] = [StatusFilter::Any, StatusFilter::Uncleared, StatusFilter::Cleared, StatusFilter::Reconciled];
    fn label(self) -> &'static str {
        match self {
            StatusFilter::Any => "Any Status",
            StatusFilter::Uncleared => "Uncleared",
            StatusFilter::Cleared => "Cleared",
            StatusFilter::Reconciled => "Reconciled",
        }
    }
    fn keeps(self, c: Cleared) -> bool {
        match self {
            StatusFilter::Any => true,
            StatusFilter::Uncleared => c == Cleared::No,
            StatusFilter::Cleared => c == Cleared::Cleared,
            StatusFilter::Reconciled => c == Cleared::Reconciled,
        }
    }
}

/// One line of the register: a transaction as seen from one account.
pub struct Row {
    pub txn: usize,
    pub account: u32,
    pub amount: f64,
    pub balance: f64,
}

/// One category's share in the Spending / Income chart.
struct Slice {
    name: String,
    count: usize,
    value: f64,
}

impl App {
    /// Every register line in date order with running balances. With all
    /// accounts shown, both sides of a transfer appear.
    fn all_rows(&self) -> Vec<Row> {
        if let Some(a) = self.acct_filter {
            return self
                .ledger
                .register(a)
                .into_iter()
                .map(|r| Row { txn: r.txn, account: a, amount: r.amount, balance: r.balance })
                .collect();
        }
        // All accounts, or the ones under the selected heading ("Savings", "Banking").
        let mut rows: Vec<Row> = Vec::new();
        for a in self.ledger.accounts.iter().filter(|a| self.in_scope(a)) {
            rows.extend(self.ledger.register(a.id).into_iter().map(|r| Row { txn: r.txn, account: a.id, amount: r.amount, balance: 0.0 }));
        }
        let txns = &self.ledger.transactions;
        rows.sort_by_key(|r| (txns[r.txn].date, txns[r.txn].id, r.account));
        let mut bal: f64 = self.ledger.accounts.iter().filter(|a| self.in_scope(a)).map(|a| a.opening_balance).sum();
        for r in &mut rows {
            bal += r.amount;
            r.balance = bal;
        }
        rows
    }

    fn is_income(&self, t: &Transaction) -> bool {
        t.transfer.is_none() && t.parts().iter().any(|(c, _)| !is_transfer_category(c) && self.ledger.category_kind(c) == Some(CatKind::Income))
    }

    fn is_spending(&self, t: &Transaction) -> bool {
        t.transfer.is_none() && t.parts().iter().any(|(c, _)| !is_transfer_category(c) && self.ledger.category_kind(c) != Some(CatKind::Income))
    }

    /// The category name a split line shows (empty means Uncategorized).
    fn part_name(c: &str) -> String {
        if c.is_empty() { "Uncategorized".into() } else { c.to_string() }
    }

    /// The Spending / Income chart groups by top category, as Quicken's does
    /// ("Food & Dining" holds Groceries, Restaurants...). Transfers and
    /// splits keep their own labels.
    fn chart_name(&self, t: &Transaction, account: u32) -> String {
        if t.transfer.is_some() || !t.splits.is_empty() || t.category.is_empty() {
            self.category_text(t, account)
        } else {
            cat_top(&t.category).to_string()
        }
    }

    /// The register lines that pass the filters, newest first.
    pub(super) fn visible_rows(&self, use_cat_filter: bool) -> Vec<Row> {
        let needle = self.search.to_lowercase();
        let mut rows = self.all_rows();
        rows.retain(|r| {
            let t = &self.ledger.transactions[r.txn];
            let tab_ok = match self.reg_tab {
                RegTab::Transactions => true,
                RegTab::Spending => self.is_spending(t),
                RegTab::Income => self.is_income(t),
            };
            let cat_ok = !use_cat_filter
                || self.cat_filter.as_ref().is_none_or(|c| {
                    *c == self.chart_name(t, r.account) || t.splits.iter().any(|s| cat_top(&Self::part_name(&s.category)) == c.as_str())
                });
            tab_ok
                && cat_ok
                && self.date_filter.keeps(t.date)
                && self.status_filter.keeps(t.cleared)
                && (needle.is_empty()
                    || t.payee.to_lowercase().contains(&needle)
                    || t.memo.to_lowercase().contains(&needle)
                    || t.splits.iter().any(|s| s.category.to_lowercase().contains(&needle) || s.memo.to_lowercase().contains(&needle))
                    || t.number.contains(&needle)
                    || self.category_text(t, r.account).to_lowercase().contains(&needle)
                    || self.ledger.account_name(r.account).to_lowercase().contains(&needle)
                    || num(r.amount).contains(&needle))
        });
        rows.reverse();
        rows
    }

    pub(super) fn register(&mut self, ui: &mut egui::Ui) {
        let (title, subtitle, label, total) = match self.acct_filter.and_then(|id| self.ledger.account(id)) {
            Some(a) => (
                a.name.clone(),
                {
                    let base = format!("{} account  ·  cleared balance {}", a.kind.label(), money(self.ledger.cleared_balance(a.id)));
                    match self.plaid_note(a.id) {
                        Some(n) => format!("{base}  ·  {n}"),
                        None => base,
                    }
                },
                "Balance",
                self.ledger.balance(a.id),
            ),
            None => {
                let ids: Vec<u32> = self.ledger.accounts.iter().filter(|a| self.in_scope(a)).map(|a| a.id).collect();
                let n = ids.len();
                let title = self.scope.map_or("All Transactions", |s| s.title()).to_string();
                let total: f64 = ids.iter().map(|id| self.ledger.balance(*id)).sum();
                let plural = if n == 1 { "account" } else { "accounts" };
                (title, format!("Account Group ({n} {plural})"), "Total", total)
            }
        };
        page_title(ui, &title, &subtitle, Some((label, money(total), amount_color(total))));
        ui.add_space(10.0);

        let rows = self.visible_rows(true);
        let shown_total: f64 = rows.iter().map(|r| r.amount).sum();
        let mut clear = false;
        band(ui, |ui| {
            ui.horizontal(|ui| {
                for (tab, label) in [(RegTab::Transactions, "Transactions"), (RegTab::Spending, "Spending"), (RegTab::Income, "Income")] {
                    if tab_pill(ui, label, self.reg_tab == tab).clicked() && self.reg_tab != tab {
                        self.reg_tab = tab;
                        self.cat_filter = None;
                    }
                }
                right_or_after(ui, 320.0, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("🔍 Search All Visible Columns").desired_width(320.0));
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("date_filter").width(220.0).selected_text(self.date_filter.label()).show_ui(ui, |ui| {
                    for f in DateFilter::ALL {
                        ui.selectable_value(&mut self.date_filter, f, f.label());
                    }
                });
                let acct_label = match (self.acct_filter, self.scope) {
                    (Some(id), _) => self.ledger.account_name(id),
                    (None, Some(s)) => format!("All {} Accounts", s.title()),
                    (None, None) => "All Accounts".into(),
                };
                let mut pick: Option<Option<u32>> = None;
                let mut pick_scope: Option<Scope> = None;
                egui::ComboBox::from_id_salt("acct_filter").width(240.0).height(500.0).selected_text(acct_label).show_ui(ui, |ui| {
                    if ui.selectable_label(self.acct_filter.is_none() && self.scope.is_none(), "All Accounts").clicked() {
                        pick = Some(None);
                    }
                    // Each heading, then its kinds, then their accounts, as in the sidebar.
                    for group in LIST_GROUPS {
                        if !self.ledger.accounts.iter().any(|a| !a.closed && a.kind.list_group() == group) {
                            continue;
                        }
                        ui.separator();
                        let s = Scope::Group(group);
                        if ui.selectable_label(self.acct_filter.is_none() && self.scope == Some(s), format!("All {group} Accounts")).clicked() {
                            pick_scope = Some(s);
                        }
                        for kind in AccountKind::LIST_ORDER.iter().copied().filter(|k| k.list_group() == group) {
                            let accts = self.list_accounts(kind);
                            if accts.is_empty() {
                                continue;
                            }
                            let s = Scope::Kind(kind);
                            if ui.selectable_label(self.acct_filter.is_none() && self.scope == Some(s), format!("  All {} Accounts", kind.label())).clicked() {
                                pick_scope = Some(s);
                            }
                            for a in accts {
                                if ui.selectable_label(self.acct_filter == Some(a.id), format!("      {}", a.name)).clicked() {
                                    pick = Some(Some(a.id));
                                }
                            }
                        }
                    }
                });
                if let Some(p) = pick {
                    self.open_register(p);
                }
                if let Some(s) = pick_scope {
                    self.open_scope(s);
                }
                egui::ComboBox::from_id_salt("status_filter").width(160.0).selected_text(self.status_filter.label()).show_ui(ui, |ui| {
                    for f in StatusFilter::ALL {
                        ui.selectable_value(&mut self.status_filter, f, f.label());
                    }
                });
                if green_button(ui, "Clear Filters").clicked() {
                    clear = true;
                }
                let count = RichText::new(format!("{} items ({})", rows.len(), num(shown_total))).color(Color32::from_gray(80));
                let w = ui.painter().layout_no_wrap(count.text().to_string(), FontId::proportional(14.0), Color32::BLACK).size().x;
                right_or_after(ui, w, |ui| {
                    ui.add(egui::Label::new(count).extend());
                });
            });
        });
        if clear {
            self.date_filter = DateFilter::All;
            self.status_filter = StatusFilter::Any;
            self.search.clear();
            self.cat_filter = None;
        }

        if self.reg_tab != RegTab::Transactions {
            self.category_chart(ui);
        }
        ui.add_space(4.0);
        self.register_table(ui, &rows);
    }

    fn category_chart(&mut self, ui: &mut egui::Ui) {
        let spending = self.reg_tab == RegTab::Spending;
        let mut slices: Vec<Slice> = Vec::new();
        for r in self.visible_rows(false) {
            let t = &self.ledger.transactions[r.txn];
            let parts: Vec<(String, f64)> = if t.splits.is_empty() {
                vec![(self.chart_name(t, r.account), r.amount)]
            } else {
                t.splits
                    .iter()
                    .filter(|s| (self.ledger.category_kind(&s.category) == Some(CatKind::Income)) != spending)
                    .map(|s| (cat_top(&Self::part_name(&s.category)).to_string(), s.amount))
                    .collect()
            };
            for (name, amount) in parts {
                let v = if spending { -amount } else { amount };
                match slices.iter_mut().find(|s| s.name == name) {
                    Some(s) => {
                        s.count += 1;
                        s.value += v;
                    }
                    None => slices.push(Slice { name, count: 1, value: v }),
                }
            }
        }
        slices.sort_by(|a, b| b.value.total_cmp(&a.value));
        let charted: f64 = slices.iter().filter(|s| s.value > 0.0).map(|s| s.value).sum();
        let mut pick: Option<Option<String>> = None;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.set_height(300.0);
            // The ring and its total.
            let (rect, _) = ui.allocate_exact_size(egui::vec2(330.0, 300.0), Sense::hover());
            let title = if spending { "Spending" } else { "Income" };
            ui.painter().text(rect.left_top() + egui::vec2(16.0, 4.0), egui::Align2::LEFT_TOP, title, FontId::proportional(15.0), MUTED);
            let center = egui::pos2(rect.center().x, rect.top() + 140.0);
            let values: Vec<f64> = slices.iter().map(|s| s.value).collect();
            donut(ui, center, 112.0, 56.0, &values);
            ui.painter().text(egui::pos2(center.x, rect.top() + 262.0), egui::Align2::CENTER_CENTER, money(charted), semibold_font(24.0), NAVY);
            if slices.iter().any(|s| s.value < 0.0) {
                ui.painter().text(egui::pos2(center.x, rect.top() + 288.0), egui::Align2::CENTER_CENTER, "Negative values not charted", FontId::proportional(14.0), MUTED);
            }
            ui.add_space(16.0);
            // The category table.
            ui.vertical(|ui| {
                let width = ui.available_width().min(780.0);
                let cols = |r: egui::Rect| (r.left() + 34.0, r.left() + width * 0.62, r.left() + width * 0.82, r.right() - 6.0);
                let (hr, _) = ui.allocate_exact_size(egui::vec2(width, 26.0), Sense::hover());
                let (cn, cc, cv, cp) = cols(hr);
                let hf = FontId::proportional(11.5);
                let hc = Color32::from_gray(90);
                let p = ui.painter();
                p.text(egui::pos2(cn, hr.center().y), egui::Align2::LEFT_CENTER, "CATEGORY", hf.clone(), hc);
                p.text(egui::pos2(cc, hr.center().y), egui::Align2::RIGHT_CENTER, "TRANSACTIONS", hf.clone(), hc);
                p.text(egui::pos2(cv, hr.center().y), egui::Align2::RIGHT_CENTER, "VALUE", hf.clone(), hc);
                p.text(egui::pos2(cp, hr.center().y), egui::Align2::RIGHT_CENTER, "PERCENT", hf, hc);
                p.hline(hr.x_range(), hr.bottom(), Stroke::new(1.0, LINE));
                egui::ScrollArea::vertical().id_salt("cat_table").max_height(268.0).show(ui, |ui| {
                    if slices.is_empty() {
                        ui.label(RichText::new("Nothing in this view yet.").color(MUTED));
                    }
                    for (i, s) in slices.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(width, 32.0), Sense::click());
                        let (cn, cc, cv, cp) = cols(r);
                        let p = ui.painter();
                        let selected = self.cat_filter.as_deref() == Some(s.name.as_str());
                        if selected {
                            p.rect_filled(r, 3.0, Color32::from_rgb(0xd3, 0xe3, 0xfa));
                        } else if resp.hovered() {
                            p.rect_filled(r, 3.0, Color32::from_gray(245));
                        }
                        let col = slice_color(i);
                        p.circle_filled(egui::pos2(r.left() + 16.0, r.center().y), 7.0, col);
                        let f = FontId::proportional(15.5);
                        let dark = Color32::from_gray(20);
                        p.text(egui::pos2(cn, r.center().y), egui::Align2::LEFT_CENTER, &s.name, f.clone(), dark);
                        p.text(egui::pos2(cc, r.center().y), egui::Align2::RIGHT_CENTER, s.count.to_string(), f.clone(), dark);
                        p.text(egui::pos2(cv, r.center().y), egui::Align2::RIGHT_CENTER, money(s.value), f.clone(), amount_color(s.value));
                        let pct = if charted > 0.0 && s.value > 0.0 { format!("{:.2}%", s.value / charted * 100.0) } else { "—".into() };
                        let g = p.layout_no_wrap(pct, f, dark);
                        let badge = egui::Rect::from_min_max(
                            egui::pos2(cp - g.size().x - 10.0, r.center().y - 12.0),
                            egui::pos2(cp + 2.0, r.center().y + 12.0),
                        );
                        if s.value > 0.0 {
                            p.rect_filled(badge, 3.0, col.gamma_multiply(0.75));
                        }
                        p.galley(egui::pos2(badge.left() + 5.0, r.center().y - g.size().y / 2.0), g, dark);
                        p.hline(r.x_range(), r.bottom(), Stroke::new(1.0, Color32::from_gray(238)));
                        if resp.on_hover_text("Click to show only this category").clicked() {
                            pick = Some(if selected { None } else { Some(s.name.clone()) });
                        }
                    }
                });
            });
        });
        if let Some(p) = pick {
            self.cat_filter = p;
        }
    }

    fn register_table(&mut self, ui: &mut egui::Ui, rows: &[Row]) {
        let all = self.acct_filter.is_none();
        let mut clicked: Option<(u64, u32)> = None;
        let mut double: Option<(u64, u32)> = None;
        let mut toggle: Option<usize> = None;
        let selected = self.selected;
        let selected_acct = self.selected_acct;
        // The row being edited replaces its line; a new one goes on top.
        let mut form = self.form.take();
        let mut items: Vec<Option<usize>> = (0..rows.len()).map(Some).collect();
        let mut edit_at: Option<usize> = None;
        if let Some(f) = &form {
            let txns = &self.ledger.transactions;
            let pos = f.editing.and_then(|id| {
                rows.iter()
                    .position(|r| txns[r.txn].id == id && r.account == f.account)
                    .or_else(|| rows.iter().position(|r| txns[r.txn].id == id))
            });
            match pos {
                Some(p) => items[p] = None,
                None => items.insert(0, None),
            }
            edit_at = Some(pos.unwrap_or(0));
        }
        let scroll_to = edit_at.filter(|_| form.as_ref().is_some_and(|f| f.scroll));
        if let Some(f) = form.as_mut() {
            f.scroll = false;
        }
        let clip = ui.clip_rect();
        let body_top = ui.cursor().top() + 26.0 + ui.spacing().item_spacing.y;
        let body_clip = egui::Rect::from_min_max(egui::pos2(clip.left(), body_top.max(clip.top())), clip.max);
        // The selection band stays within the table's own width.
        let sel_clip = body_clip.intersect(ui.max_rect().expand2(egui::vec2(0.0, 4000.0)));
        let mut out = entry::EditOutcome::default();
        let mut table = TableBuilder::new(ui)
            .striped(true)
            .resizable(false)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(26.0))
            .column(Column::exact(90.0))
            .column(Column::exact(70.0))
            .column(Column::remainder().at_least(120.0).clip(true))
            .column(Column::exact(if all { 180.0 } else { 220.0 }).clip(true))
            .column(Column::exact(100.0))
            .column(Column::exact(if all { 116.0 } else { 124.0 }));
        if all {
            table = table.column(Column::exact(160.0).clip(true));
        }
        if let Some(i) = scroll_to {
            table = table.scroll_to_row(i, Some(Align::Center));
        }
        let mut names = vec!["", "DATE", "CHECK #", "PAYEE", "CATEGORY", "AMOUNT", "BALANCE"];
        if all {
            names.push("ACCOUNT");
        }
        table
            .header(26.0, |mut h| {
                for (i, name) in names.iter().enumerate() {
                    h.col(|ui| {
                        let color = if i == 1 { ACCENT } else { Color32::from_gray(80) };
                        let text = col_header(name, color);
                        if i == 5 || i == 6 {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(text));
                        } else if i == 0 {
                            ui.painter().circle_filled(ui.max_rect().center(), 3.5, Color32::from_gray(70));
                        } else {
                            ui.label(text);
                        }
                        let r = ui.max_rect();
                        ui.painter().hline(r.x_range(), r.bottom() + 2.0, Stroke::new(1.0, Color32::from_rgb(0xc9, 0xcf, 0xa8)));
                    });
                }
            })
            .body(|body| {
                let edit_h = form.as_ref().map(|f| f.row_height()).unwrap_or(entry::EDIT_ROW_HEIGHT);
                let heights = items.iter().map(|i| if i.is_none() { edit_h } else { 28.0 });
                body.heterogeneous_rows(heights, |mut row| {
                    let Some(ri) = items[row.index()] else {
                        if let Some(f) = form.as_mut() {
                            entry::edit_row(&self.ledger, f, &mut row, all, body_clip, &mut out);
                        }
                        return;
                    };
                    let r = &rows[ri];
                    let t = &self.ledger.transactions[r.txn];
                    // The other side of a transfer being edited stays unhighlighted.
                    let being_edited = form.as_ref().and_then(|f| f.editing) == Some(t.id);
                    // The selected line is a solid dark-blue band with white text,
                    // the same as a selected report.
                    let hl = selected == Some(t.id) && selected_acct.is_none_or(|a| a == r.account) && !being_edited;
                    let last_col = if all { 7 } else { 6 };
                    let mut col = 0;
                    let mut band = |ui: &egui::Ui| {
                        if hl {
                            let cell = ui.max_rect();
                            let left = if col == 0 { sel_clip.left() } else { cell.left() - ui.spacing().item_spacing.x };
                            let right = if col == last_col { sel_clip.right() } else { cell.right() };
                            let mut p = ui.painter().clone();
                            p.set_clip_rect(sel_clip);
                            p.rect_filled(egui::Rect::from_x_y_ranges(left..=right, cell.y_range()), 0.0, entry::EDIT_BLUE);
                        }
                        col += 1;
                    };
                    let text = if hl { Color32::WHITE } else { Color32::from_gray(20) };
                    let soft = if hl { Color32::from_rgb(0xc9, 0xd7, 0xef) } else { MUTED };
                    let plus = if hl { Color32::from_rgb(0xa8, 0xf0, 0xb4) } else { GREEN };
                    row.col(|ui| {
                        band(ui);
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::click());
                        let c = rect.center();
                        match (t.cleared, hl) {
                            (Cleared::No, false) => ui.painter().circle_filled(c, 4.5, DOT_BLUE),
                            (Cleared::No, true) => ui.painter().circle_filled(c, 4.5, Color32::WHITE),
                            (Cleared::Cleared, _) => ui.painter().circle_stroke(c, 4.0, Stroke::new(1.5, if hl { Color32::WHITE } else { Color32::from_gray(150) })),
                            (Cleared::Reconciled, _) => ui.painter().circle_filled(c, 4.5, plus),
                        };
                        let tip = match t.cleared {
                            Cleared::No => "New, not cleared yet. Click to mark cleared.",
                            Cleared::Cleared => "Cleared. Click to mark reconciled.",
                            Cleared::Reconciled => "Reconciled. Click to mark not cleared.",
                        };
                        if resp.on_hover_text(tip).clicked() {
                            toggle = Some(r.txn);
                        }
                    });
                    row.col(|ui| {
                        band(ui);
                        ui.label(RichText::new(t.date.format("%m/%d/%y").to_string()).color(text));
                    });
                    row.col(|ui| {
                        band(ui);
                        ui.label(RichText::new(&t.number).color(text));
                    });
                    row.col(|ui| {
                        band(ui);
                        ui.label(RichText::new(&t.payee).color(text));
                        if !t.memo.is_empty() {
                            ui.label(RichText::new(&t.memo).color(soft));
                        }
                    });
                    row.col(|ui| {
                        band(ui);
                        if t.category.is_empty() && t.transfer.is_none() {
                            ui.label(italic("Uncategorized").color(soft));
                        } else {
                            // A subcategory shows its own name, as in Quicken's
                            // register; the full "Parent:Sub" path is on hover.
                            let plain = t.transfer.is_none() && t.splits.is_empty();
                            let shown = if plain { cat_leaf(&t.category).to_string() } else { self.category_text(t, r.account) };
                            let l = ui.label(RichText::new(shown).color(text));
                            let l = if plain && cat_depth(&t.category) > 0 { l.on_hover_text(&t.category) } else { l };
                            if !t.splits.is_empty() {
                                let lines: Vec<String> =
                                    t.splits.iter().map(|s| format!("{}  {}", Self::part_name(&s.category), num(s.amount))).collect();
                                l.on_hover_text(lines.join("\n"));
                            }
                        }
                    });
                    row.col(|ui| {
                        band(ui);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let c = if r.amount > 0.0 { plus } else { text };
                            ui.label(RichText::new(num(r.amount)).color(c));
                        });
                    });
                    row.col(|ui| {
                        band(ui);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let c = if hl { soft } else { Color32::from_gray(120) };
                            ui.label(italic(num(r.balance)).color(c));
                        });
                    });
                    if all {
                        row.col(|ui| {
                            band(ui);
                            ui.label(RichText::new(self.ledger.account_name(r.account)).color(text));
                        });
                    }
                    let resp = row.response();
                    if resp.double_clicked() {
                        double = Some((t.id, r.account));
                    } else if resp.clicked() {
                        clicked = Some((t.id, r.account));
                    }
                });
            });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            out.cancel = true;
        }
        if let Some(f) = form {
            self.finish_edit(f, out);
        }
        if rows.is_empty() && self.form.is_none() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No transactions match. Try Clear Filters, or click New below to add one.").color(MUTED));
            });
        }
        if let Some(i) = toggle {
            let t = &mut self.ledger.transactions[i];
            t.cleared = t.cleared.next();
            self.dirty = true;
        } else if let Some((id, acct)) = double {
            if self.leave_edit() {
                self.selected = Some(id);
                self.selected_acct = Some(acct);
                self.edit_transaction(id);
                if let Some(f) = self.form.as_mut() {
                    f.scroll = false;
                }
            }
        } else if let Some((id, acct)) = clicked {
            if self.leave_edit() {
                self.selected = Some(id);
                self.selected_acct = Some(acct);
                self.confirm_delete = false;
            }
        }
    }

    pub(super) fn register_toolbar_left(&mut self, ui: &mut egui::Ui) {
        if toolbar_button(ui, Icon::New, "New", true, false).on_hover_text("Add a transaction (⌘N)").clicked() {
            self.new_transaction();
        }
        let sel = self.selected.filter(|id| self.ledger.transactions.iter().any(|t| t.id == *id));
        if toolbar_button(ui, Icon::Edit, "Edit", sel.is_some(), false).clicked() {
            if let Some(id) = sel {
                self.edit_transaction(id);
            }
        }
        let can_split = sel.is_some() || self.form.is_some();
        if toolbar_button(ui, Icon::Split, "Split", can_split, false).on_hover_text("Split a transaction across categories").clicked() {
            self.split_selected();
        }
        let label = if self.confirm_delete { "Confirm" } else { "Delete" };
        let del = toolbar_button(ui, Icon::Delete, label, sel.is_some(), self.confirm_delete);
        if del.on_hover_text("Click twice to delete the selected transaction").clicked() {
            if let Some(id) = sel {
                if self.confirm_delete {
                    self.ledger.transactions.retain(|t| t.id != id);
                    self.selected = None;
                    self.form = None;
                    self.confirm_delete = false;
                    self.dirty = true;
                    self.status = "Transaction deleted.".into();
                } else {
                    self.confirm_delete = true;
                }
            }
        }
    }

    pub(super) fn register_toolbar_right(&mut self, ui: &mut egui::Ui) {
        let rec = toolbar_button(ui, Icon::Reconcile, "Reconcile", self.acct_filter.is_some(), false)
            .on_hover_text("Lock in every cleared transaction in this account");
        if rec.clicked() {
            if let Some(a) = self.acct_filter {
                let mut n = 0;
                for t in self.ledger.transactions.iter_mut().filter(|t| t.account == a || t.transfer == Some(a)) {
                    if t.cleared == Cleared::Cleared {
                        t.cleared = Cleared::Reconciled;
                        n += 1;
                    }
                }
                self.status = if n == 0 {
                    "Nothing to reconcile: mark transactions cleared first by clicking their dots.".into()
                } else {
                    self.dirty = true;
                    format!("Reconciled {n} transactions. Cleared balance {}.", money(self.ledger.cleared_balance(a)))
                };
            }
        }
        if toolbar_button(ui, Icon::Copy, "Copy", true, false).on_hover_text("Copy the shown transactions for a spreadsheet").clicked() {
            let mut out = String::from("Date,Check,Payee,Category,Memo,Amount,Balance,Account\n");
            for r in self.visible_rows(true) {
                let t = &self.ledger.transactions[r.txn];
                let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
                out += &format!(
                    "{},{},{},{},{},{:.2},{:.2},{}\n",
                    fmt_date(t.date),
                    q(&t.number),
                    q(&t.payee),
                    q(&self.category_text(t, r.account)),
                    q(&t.memo),
                    r.amount,
                    r.balance,
                    q(&self.ledger.account_name(r.account))
                );
            }
            ui.ctx().copy_text(out);
            self.status = "Copied the shown transactions. Paste them into a spreadsheet.".into();
        }
    }
}
