//! The Home screen: net worth, this month at a glance, recent activity.

use super::*;

impl App {
    pub(super) fn home(&mut self, ui: &mut egui::Ui) {
        let t = today();
        let (m0, m1) = (first_of_month(t), last_of_month(t));
        let nw = self.ledger.net_worth_at(None);
        page_title(ui, "Home", &t.format("%A, %B %-d, %Y").to_string(), Some(("Net Worth", money(nw), amount_color(nw))));
        ui.add_space(12.0);
        let actual = self.ledger.actuals(m0, m1);
        let is_income = |c: &str| self.ledger.category_kind(c) == Some(CatKind::Income);
        let income: f64 = actual.iter().filter(|(c, _)| is_income(c)).map(|(_, v)| v).sum();
        let spent: f64 = actual.iter().filter(|(c, _)| !is_income(c)).map(|(_, v)| v).sum();
        let planned: f64 = self
            .ledger
            .categories
            .iter()
            .filter(|c| c.kind == CatKind::Expense)
            .map(|c| self.ledger.budgeted(&c.name, m0, m1))
            .sum();
        let nw = self.ledger.net_worth_at(None);
        let nw_prev = self.ledger.net_worth_at(Some(m0 - chrono::Days::new(1)));
        let left = planned - spent;
        card_row(
            ui,
            &[
                ("Net worth", money(nw), amount_color(nw), format!("{} since last month", signed_money(nw - nw_prev))),
                ("Income this month", money(income), GREEN, format!("{} so far", t.format("%B"))),
                ("Spending this month", money(spent), RED, format!("of {} budgeted", money(planned))),
                (
                    "Left to spend",
                    money(left),
                    if left < 0.0 { RED } else { ACCENT },
                    if left < 0.0 { "over budget".into() } else { "remaining in budget".into() },
                ),
            ],
        );
        ui.add_space(14.0);
        let mut goto: Option<View> = None;
        let mut goto_acct: Option<u32> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.columns(2, |cols| {
                panel(&mut cols[0], "Where the money went this month", |ui| {
                    // By top category, subcategories included.
                    let mut rows: Vec<(String, f64)> = reports::top_totals(&actual)
                        .iter()
                        .filter(|(c, v)| !is_income(c) && **v > 0.0)
                        .map(|(c, v)| (if c.is_empty() { "Uncategorized".into() } else { c.clone() }, *v))
                        .collect();
                    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
                    rows.truncate(8);
                    if rows.is_empty() {
                        ui.label(RichText::new("No spending yet this month.").color(MUTED));
                    }
                    hbars(ui, &rows, RED);
                    ui.add_space(6.0);
                    if ui.link("Open the budget").clicked() {
                        goto = Some(View::Budget);
                    }
                });
                panel(&mut cols[1], "Account balances", |ui| {
                    egui::Grid::new("home_bal").num_columns(3).spacing([16.0, 6.0]).striped(true).show(ui, |ui| {
                        for a in self.ledger.accounts.iter().filter(|a| !a.closed) {
                            if ui.link(&a.name).clicked() {
                                goto_acct = Some(a.id);
                            }
                            ui.label(RichText::new(a.kind.label()).color(MUTED));
                            let b = self.ledger.balance(a.id);
                            ui.label(RichText::new(money(b)).color(amount_color(b)));
                            ui.end_row();
                        }
                    });
                });
            });
            ui.add_space(14.0);
            panel(ui, "Recent transactions", |ui| {
                let mut idx: Vec<usize> = (0..self.ledger.transactions.len()).collect();
                idx.sort_by_key(|&i| std::cmp::Reverse((self.ledger.transactions[i].date, self.ledger.transactions[i].id)));
                egui::Grid::new("recent").num_columns(5).spacing([28.0, 5.0]).striped(true).min_col_width(60.0).show(ui, |ui| {
                    for i in idx.into_iter().take(10) {
                        let t = &self.ledger.transactions[i];
                        ui.label(fmt_date(t.date));
                        ui.label(&t.payee);
                        ui.label(RichText::new(self.category_text(t, t.account)).color(MUTED));
                        ui.label(RichText::new(self.ledger.account_name(t.account)).color(MUTED));
                        let c = if t.amount >= 0.0 { GREEN } else { Color32::from_gray(35) };
                        ui.label(RichText::new(money(t.amount)).color(c));
                        ui.end_row();
                    }
                });
            });
        });
        if let Some(v) = goto {
            self.go(v);
        }
        if let Some(a) = goto_acct {
            self.open_register(Some(a));
        }
    }
}
