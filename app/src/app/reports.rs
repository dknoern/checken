//! Reports: budget vs. actual, spending by category, income vs. expense by
//! month, and net worth over time. Each has a chart and a table that can be
//! copied as CSV.

use super::*;
use chrono::Months;

impl ReportKind {
    pub(super) const ALL: [ReportKind; 4] = [ReportKind::BudgetVsActual, ReportKind::Spending, ReportKind::IncomeExpense, ReportKind::NetWorth];
    pub(super) fn title(self) -> &'static str {
        match self {
            ReportKind::BudgetVsActual => "Budget vs. Actual",
            ReportKind::Spending => "Spending by Category",
            ReportKind::IncomeExpense => "Income vs. Expense",
            ReportKind::NetWorth => "Net Worth",
        }
    }
    pub(super) fn blurb(self) -> &'static str {
        match self {
            ReportKind::BudgetVsActual => "How each category did against its budget.",
            ReportKind::Spending => "Where your money went, largest first.",
            ReportKind::IncomeExpense => "Money in and out, month by month.",
            ReportKind::NetWorth => "Everything you own minus everything you owe, at each month's end.",
        }
    }
}

impl Period {
    pub(super) const ALL: [Period; 5] = [Period::ThisMonth, Period::LastMonth, Period::Last3, Period::YearToDate, Period::Last12];
    pub(super) fn label(self) -> &'static str {
        match self {
            Period::ThisMonth => "This Month",
            Period::LastMonth => "Last Month",
            Period::Last3 => "Last 3 Months",
            Period::YearToDate => "Year to Date",
            Period::Last12 => "Last 12 Months",
        }
    }
    pub(super) fn range(self) -> (NaiveDate, NaiveDate) {
        let t = today();
        let m = first_of_month(t);
        match self {
            Period::ThisMonth => (m, last_of_month(t)),
            Period::LastMonth => (m - Months::new(1), m - chrono::Days::new(1)),
            Period::Last3 => (m - Months::new(2), last_of_month(t)),
            Period::YearToDate => (NaiveDate::from_ymd_opt(chrono::Datelike::year(&t), 1, 1).unwrap(), last_of_month(t)),
            Period::Last12 => (m - Months::new(11), last_of_month(t)),
        }
    }
}

/// A report's table: header row plus body rows, all as text.
pub(super) struct Table {
    pub(super) header: Vec<String>,
    pub(super) rows: Vec<Vec<String>>,
    /// Rows drawn in bold (totals).
    pub(super) bold: Vec<usize>,
}

impl Table {
    fn new(header: &[&str]) -> Table {
        Table { header: header.iter().map(|s| s.to_string()).collect(), rows: Vec::new(), bold: Vec::new() }
    }
    fn push(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }
    fn push_bold(&mut self, row: Vec<String>) {
        self.bold.push(self.rows.len());
        self.rows.push(row);
    }
    fn csv(&self) -> String {
        let esc = |s: &String| {
            if s.contains([',', '"']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.clone() }
        };
        std::iter::once(&self.header)
            .chain(self.rows.iter())
            .map(|r| r.iter().map(esc).collect::<Vec<_>>().join(","))
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn show(&self, ui: &mut egui::Ui, id: &str) {
        let grid = egui::Grid::new(id).num_columns(self.header.len()).spacing([32.0, 6.0]).striped(true);
        grid.min_col_width(90.0).show(ui, |ui| {
            for h in &self.header {
                ui.label(RichText::new(h).strong().color(Color32::from_gray(70)));
            }
            ui.end_row();
            for (i, row) in self.rows.iter().enumerate() {
                let bold = self.bold.contains(&i);
                for (j, cell) in row.iter().enumerate() {
                    let mut text = RichText::new(cell);
                    if bold {
                        text = text.strong();
                    }
                    if j > 0 && cell.starts_with('-') {
                        text = text.color(RED);
                    }
                    ui.label(text);
                }
                ui.end_row();
            }
        });
    }
}

fn months_in(from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let mut v = Vec::new();
    let mut m = first_of_month(from);
    while m <= to {
        v.push(m);
        m = m + Months::new(1);
    }
    v
}

impl App {
    pub(super) fn reports(&mut self, ui: &mut egui::Ui) {
        match self.report_open {
            Some(key) => self.report_view(ui, key),
            None => self.report_library(ui),
        }
    }

    /// The table of a report as CSV text, for Export.
    pub(super) fn report_csv(&self, kind: ReportKind, period: Period) -> String {
        let (from, to) = period.range();
        self.report_table(kind, from, to).csv()
    }

    pub(super) fn report_table(&self, kind: ReportKind, from: NaiveDate, to: NaiveDate) -> Table {
        match kind {
            ReportKind::BudgetVsActual => self.budget_vs_actual(from, to),
            ReportKind::Spending => self.spending(from, to),
            ReportKind::IncomeExpense => self.income_expense(from, to),
            ReportKind::NetWorth => self.net_worth(from, to),
        }
    }

    fn report_view(&mut self, ui: &mut egui::Ui, key: library::ReportKey) {
        let Some(entry) = self.report_entry(key) else {
            self.report_open = None;
            return;
        };
        self.report = entry.kind;
        let mut back = false;
        ui.horizontal(|ui| {
            let link = egui::Label::new(semibold(format!("‹ {}", self.report_folder.title())).size(15.0).color(ACCENT)).sense(Sense::click());
            if ui.add(link).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                back = true;
            }
        });
        let subtitle = if entry.description.is_empty() { entry.kind.blurb().to_string() } else { entry.description.clone() };
        page_title(ui, &entry.name, &subtitle, None);
        ui.add_space(10.0);
        let (from, to) = self.period.range();
        let table = self.report_table(self.report, from, to);
        let mut copy = false;
        let mut edit = false;
        let mut print = false;
        let before = self.period;
        band(ui, |ui| ui.horizontal(|ui| {
            ui.label(RichText::new("Date range:").color(Color32::from_gray(60)));
            egui::ComboBox::from_id_salt("period").width(180.0).selected_text(self.period.label()).show_ui(ui, |ui| {
                for p in Period::ALL {
                    ui.selectable_value(&mut self.period, p, p.label());
                }
            });
            ui.label(RichText::new(format!("{} – {}", fmt_date(from), fmt_date(to))).color(MUTED));
            right_or_after(ui, 260.0, |ui| {
                if tool_text_button(ui, Icon::Copy, "Export", true, false).on_hover_text("Copy the table to paste into a spreadsheet").clicked() {
                    copy = true;
                }
                if tool_text_button(ui, Icon::Print, "Print", true, false).on_hover_text("Print this report (⌘P)").clicked() {
                    print = true;
                }
                if matches!(key, library::ReportKey::Saved(_)) && tool_text_button(ui, Icon::Edit, "Edit", true, false).clicked() {
                    edit = true;
                }
            });
        }));
        if self.period != before {
            if let library::ReportKey::Saved(id) = key {
                if let Some(r) = self.ledger.reports.iter_mut().find(|r| r.id == id) {
                    r.period = self.period;
                    self.dirty = true;
                }
            }
        }
        if copy {
            ui.ctx().copy_text(table.csv());
            self.status = format!("Copied {}. Paste it into a spreadsheet.", entry.name);
        }
        if edit {
            self.edit_report(key);
        }
        if print {
            let (kind, period) = (self.report, self.period);
            self.print_report(&entry.name, &subtitle, kind, period);
        }
        if back {
            self.report_open = None;
        }
        ui.add_space(8.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            panel(ui, entry.kind.title(), |ui| self.report_chart(ui, from, to));
            ui.add_space(12.0);
            panel(ui, "Details", |ui| table.show(ui, "report_table"));
        });
    }

    fn report_chart(&self, ui: &mut egui::Ui, from: NaiveDate, to: NaiveDate) {
        match self.report {
            ReportKind::BudgetVsActual => {
                // One bar pair per top category, subcategories included.
                let actual = roll_up(&self.ledger.actuals(from, to));
                let rows: Vec<(String, f64, f64)> = self
                    .ledger
                    .categories
                    .iter()
                    .filter(|c| c.kind == CatKind::Expense && cat_depth(&c.name) == 0)
                    .map(|c| (c.name.clone(), self.ledger.budgeted_rolled(&c.name, from, to), actual.get(&c.name).copied().unwrap_or(0.0)))
                    .filter(|r| r.1 > 0.0 || r.2 > 0.0)
                    .collect();
                legend(ui, &[(Color32::from_gray(190), "Budget"), (GREEN, "Actual (within budget)"), (RED, "Actual (over budget)")]);
                paired_bars(ui, &rows);
            }
            ReportKind::Spending => {
                let mut rows: Vec<(String, f64)> = top_totals(&self.ledger.actuals(from, to))
                    .into_iter()
                    .filter(|(c, v)| self.ledger.category_kind(c) != Some(CatKind::Income) && *v > 0.0)
                    .map(|(c, v)| (if c.is_empty() { "Uncategorized".into() } else { c }, v))
                    .collect();
                rows.sort_by(|a, b| b.1.total_cmp(&a.1));
                if rows.is_empty() {
                    ui.label(RichText::new("No spending in this period.").color(MUTED));
                }
                hbars(ui, &rows, ACCENT);
            }
            ReportKind::IncomeExpense => {
                let months = months_in(from, to);
                let (inc, exp): (Vec<f64>, Vec<f64>) = months.iter().map(|m| self.month_totals(*m)).unzip();
                let labels: Vec<String> = months.iter().map(|m| m.format("%b %y").to_string()).collect();
                legend(ui, &[(GREEN, "Income"), (RED, "Expenses")]);
                column_chart(ui, &labels, &[(GREEN, inc), (RED, exp)]);
            }
            ReportKind::NetWorth => {
                let months = months_in(from, to);
                let vals: Vec<f64> = months.iter().map(|m| self.ledger.net_worth_at(Some(last_of_month(*m).min(today())))).collect();
                let labels: Vec<String> = months.iter().map(|m| m.format("%b %y").to_string()).collect();
                column_chart(ui, &labels, &[(ACCENT, vals)]);
            }
        }
    }

    fn month_totals(&self, m: NaiveDate) -> (f64, f64) {
        let a = self.ledger.actuals(m, last_of_month(m));
        let inc = a.iter().filter(|(c, _)| self.ledger.category_kind(c) == Some(CatKind::Income)).map(|(_, v)| v).sum();
        let exp = a.iter().filter(|(c, _)| self.ledger.category_kind(c) != Some(CatKind::Income)).map(|(_, v)| v).sum();
        (inc, exp)
    }

    /// Budget vs actual in category tree order: each category's figures
    /// include its subcategories, which are listed indented under it.
    fn budget_vs_actual(&self, from: NaiveDate, to: NaiveDate) -> Table {
        let actual = roll_up(&self.ledger.actuals(from, to));
        let mut t = Table::new(&["Category", "Budget", "Actual", "Difference", "% used"]);
        for (kind, title) in [(CatKind::Income, "Total income"), (CatKind::Expense, "Total expenses")] {
            let (mut tb, mut ta) = (0.0, 0.0);
            for c in self.ledger.categories.iter().filter(|c| c.kind == kind) {
                let b = self.ledger.budgeted_rolled(&c.name, from, to);
                let a = actual.get(&c.name).copied().unwrap_or(0.0);
                if b == 0.0 && a == 0.0 {
                    continue;
                }
                if cat_depth(&c.name) == 0 {
                    tb += b;
                    ta += a;
                }
                let row = vec![tree_label(&c.name), money(b), money(a), money(b - a), pct(a, b)];
                if self.ledger.has_subcategories(&c.name) {
                    t.push_bold(row);
                } else {
                    t.push(row);
                }
            }
            t.push_bold(vec![title.into(), money(tb), money(ta), money(tb - ta), pct(ta, tb)]);
        }
        t
    }

    /// Spending by category: top categories largest first, each followed by
    /// its subcategories (largest first), indented.
    fn spending(&self, from: NaiveDate, to: NaiveDate) -> Table {
        let own = self.ledger.actuals(from, to);
        let rolled = roll_up(&own);
        let is_spend = |c: &str| self.ledger.category_kind(c) != Some(CatKind::Income);
        let total: f64 = own.iter().filter(|(c, _)| is_spend(c)).map(|(_, v)| v).sum();
        let months = months_in(from, to).len().max(1) as f64;
        let mut t = Table::new(&["Category", "Spent", "Share", "Per month"]);
        // Children of a category (or the top level for ""), largest first.
        let children = |parent: Option<&str>| -> Vec<(String, f64)> {
            let mut v: Vec<(String, f64)> = rolled
                .iter()
                .filter(|(c, _)| is_spend(c) && (if c.is_empty() { parent.is_none() } else { cat_parent(c) == parent }))
                .map(|(c, v)| (c.clone(), *v))
                .collect();
            v.sort_by(|a, b| b.1.total_cmp(&a.1));
            v
        };
        fn walk(t: &mut Table, children: &dyn Fn(Option<&str>) -> Vec<(String, f64)>, parent: Option<&str>, total: f64, months: f64) {
            for (c, v) in children(parent) {
                let name = if c.is_empty() { "Uncategorized".to_string() } else { tree_label(&c) };
                let row = vec![name, money(v), pct(v, total), money(v / months)];
                let kids = if c.is_empty() { Vec::new() } else { children(Some(&c)) };
                if kids.is_empty() {
                    t.push(row);
                } else {
                    t.push_bold(row);
                    walk(t, children, Some(&c), total, months);
                }
            }
        }
        walk(&mut t, &children, None, total, months);
        t.push_bold(vec!["Total".into(), money(total), "100%".into(), money(total / months)]);
        t
    }

    fn income_expense(&self, from: NaiveDate, to: NaiveDate) -> Table {
        let mut t = Table::new(&["Month", "Income", "Expenses", "Net", "Saved"]);
        let (mut ti, mut te) = (0.0, 0.0);
        for m in months_in(from, to) {
            let (i, e) = self.month_totals(m);
            ti += i;
            te += e;
            t.push(vec![m.format("%B %Y").to_string(), money(i), money(e), money(i - e), pct(i - e, i)]);
        }
        t.push_bold(vec!["Total".into(), money(ti), money(te), money(ti - te), pct(ti - te, ti)]);
        t
    }

    fn net_worth(&self, from: NaiveDate, to: NaiveDate) -> Table {
        let mut t = Table::new(&["Month end", "Assets", "Debts", "Net worth", "Change"]);
        let mut prev: Option<f64> = None;
        for m in months_in(from, to) {
            let d = last_of_month(m).min(today());
            let (mut assets, mut debts) = (0.0, 0.0);
            for a in &self.ledger.accounts {
                let b = self.ledger.balance_at(a.id, Some(d));
                if b >= 0.0 { assets += b } else { debts += b }
            }
            let nw = assets + debts;
            let change = prev.map(|p| signed_money(nw - p)).unwrap_or_default();
            t.push(vec![fmt_date(d), money(assets), money(debts), money(nw), change]);
            prev = Some(nw);
        }
        t
    }
}

/// Amounts added up by top category ("Food & Dining" includes Groceries...).
pub(super) fn top_totals(own: &std::collections::BTreeMap<String, f64>) -> std::collections::BTreeMap<String, f64> {
    let mut out = std::collections::BTreeMap::new();
    for (c, v) in own {
        *out.entry(cat_top(c).to_string()).or_insert(0.0) += v;
    }
    out
}

fn pct(a: f64, b: f64) -> String {
    if b.abs() < 0.005 { "—".into() } else { format!("{:.0}%", a / b * 100.0) }
}

fn legend(ui: &mut egui::Ui, items: &[(Color32, &str)]) {
    ui.horizontal(|ui| {
        for (c, label) in items {
            let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
            ui.painter().rect_filled(r, 2.0, *c);
            ui.label(RichText::new(*label).small().color(MUTED));
            ui.add_space(8.0);
        }
    });
    ui.add_space(4.0);
}

/// Budget (grey) and actual (green / red) bars side by side per category.
fn paired_bars(ui: &mut egui::Ui, rows: &[(String, f64, f64)]) {
    let max = rows.iter().map(|r| r.1.max(r.2)).fold(1.0, f64::max);
    let label_w = 150.0;
    let value_w = 190.0;
    for (name, b, a) in rows {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), Sense::hover());
        let p = ui.painter();
        p.text(egui::pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, name, FontId::proportional(13.5), Color32::from_gray(40));
        let room = (rect.width() - label_w - value_w).max(40.0);
        let x0 = rect.left() + label_w;
        let wb = (*b / max) as f32 * room;
        let wa = (*a / max) as f32 * room;
        p.rect_filled(egui::Rect::from_min_size(egui::pos2(x0, rect.top() + 3.0), egui::vec2(wb.max(1.0), 11.0)), 2.0, Color32::from_gray(190));
        let col = if *a > *b + 0.004 { RED } else { GREEN };
        p.rect_filled(egui::Rect::from_min_size(egui::pos2(x0, rect.top() + 16.0), egui::vec2(wa.max(1.0), 11.0)), 2.0, col);
        p.text(
            egui::pos2(rect.right(), rect.center().y),
            Align2::RIGHT_CENTER,
            format!("{} of {}", money(*a), money(*b)),
            FontId::proportional(13.0),
            Color32::from_gray(50),
        );
        resp.on_hover_text(format!("{name}\nBudget {}\nActual {}", money(*b), money(*a)));
    }
}

/// Vertical columns per label, one bar per series, with a zero line.
fn column_chart(ui: &mut egui::Ui, labels: &[String], series: &[(Color32, Vec<f64>)]) {
    let height = 230.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    let vals = series.iter().flat_map(|s| s.1.iter().copied());
    let (lo, hi) = vals.fold((0.0f64, 0.0f64), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let span = (hi - lo).max(1.0);
    let plot = egui::Rect::from_min_max(egui::pos2(rect.left() + 80.0, rect.top() + 8.0), egui::pos2(rect.right() - 8.0, rect.bottom() - 24.0));
    let y_of = |v: f64| plot.bottom() - ((v - lo) / span) as f32 * plot.height();
    for k in 0..=4 {
        let v = lo + span * k as f64 / 4.0;
        let y = y_of(v);
        p.line_segment([egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)], Stroke::new(1.0, Color32::from_gray(235)));
        p.text(egui::pos2(plot.left() - 6.0, y), Align2::RIGHT_CENTER, money(v.round()), FontId::proportional(11.0), MUTED);
    }
    let zero = y_of(0.0);
    p.line_segment([egui::pos2(plot.left(), zero), egui::pos2(plot.right(), zero)], Stroke::new(1.0, Color32::from_gray(150)));
    let n = labels.len().max(1) as f32;
    let slot = plot.width() / n;
    let group_w = (slot * 0.7).min(90.0);
    let bar_w = group_w / series.len().max(1) as f32;
    for (i, label) in labels.iter().enumerate() {
        let cx = plot.left() + slot * (i as f32 + 0.5);
        let x0 = cx - group_w / 2.0;
        for (s, (col, vals)) in series.iter().enumerate() {
            let v = vals.get(i).copied().unwrap_or(0.0);
            let (y1, y2) = (y_of(v), zero);
            let r = egui::Rect::from_min_max(
                egui::pos2(x0 + bar_w * s as f32 + 1.0, y1.min(y2)),
                egui::pos2(x0 + bar_w * (s as f32 + 1.0) - 1.0, y1.max(y2)),
            );
            p.rect_filled(r, 2.0, *col);
        }
        p.text(egui::pos2(cx, plot.bottom() + 6.0), Align2::CENTER_TOP, label, FontId::proportional(11.5), Color32::from_gray(60));
    }
}
