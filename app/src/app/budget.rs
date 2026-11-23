//! The monthly budget and the category list.

use super::*;
use chrono::Months;

impl App {
    pub(super) fn budget(&mut self, ui: &mut egui::Ui) {
        let m0 = self.budget_month;
        let m1 = last_of_month(m0);
        if self.budget_buf_month != Some(m0) {
            self.refill_budget_buf();
        }
        page_title(ui, "Budgets", "Plan each category for the month, then watch how you're doing.", None);
        ui.add_space(10.0);
        band(ui, |ui| ui.horizontal(|ui| {
            if ui.button("◀").on_hover_text("Previous month").clicked() {
                self.budget_month = m0 - Months::new(1);
            }
            ui.label(semibold(m0.format("%B %Y").to_string()).size(17.0));
            if ui.button("▶").on_hover_text("Next month").clicked() {
                self.budget_month = m0 + Months::new(1);
            }
            if first_of_month(today()) != m0 && ui.button("This month").clicked() {
                self.budget_month = first_of_month(today());
            }
            right_or_after(ui, 400.0, |ui| {
                if ui.button("Copy last month's budget").clicked() {
                    let prev = month_key(m0 - Months::new(1));
                    let plan = self.ledger.budgets.get(&prev).cloned().unwrap_or_default();
                    self.ledger.budgets.insert(month_key(m0), plan);
                    self.dirty = true;
                    self.budget_buf_month = None;
                    self.status = "Copied last month's budget.".into();
                }
                if ui.button("Use last month's actuals").clicked() {
                    let p0 = m0 - Months::new(1);
                    for (c, v) in self.ledger.actuals(p0, last_of_month(p0)) {
                        if v > 0.0 && !c.is_empty() {
                            self.ledger.set_budget(m0, &c, v.round());
                        }
                    }
                    self.dirty = true;
                    self.budget_buf_month = None;
                    self.status = "Filled the budget from last month's actual amounts.".into();
                }
            });
        }));
        ui.add_space(10.0);

        let actual = self.ledger.actuals(m0, m1);
        let sum = |kind: CatKind, planned: bool| -> f64 {
            self.ledger
                .categories
                .iter()
                .filter(|c| c.kind == kind)
                .map(|c| if planned { self.ledger.budgeted(&c.name, m0, m1) } else { actual.get(&c.name).copied().unwrap_or(0.0) })
                .sum()
        };
        let (pi, ai) = (sum(CatKind::Income, true), sum(CatKind::Income, false));
        let (pe, ae) = (sum(CatKind::Expense, true), sum(CatKind::Expense, false));
        let left = pe - ae;
        let net = ai - ae;
        card_row(
            ui,
            &[
                ("Income", money(ai), GREEN, format!("of {} planned", money(pi))),
                ("Spending", money(ae), RED, format!("of {} budgeted", money(pe))),
                (
                    "Left to spend",
                    money(left),
                    if left < 0.0 { RED } else { ACCENT },
                    if left < 0.0 { "over budget".into() } else { "this month".into() },
                ),
                ("Income minus spending", money(net), amount_color(net), format!("planned {}", money(pi - pe))),
            ],
        );
        let uncategorized = actual.get("").copied().unwrap_or(0.0);
        if uncategorized.abs() > 0.004 {
            let msg = format!("{} of this month's transactions have no category.", money(uncategorized));
            ui.label(RichText::new(msg).color(ORANGE));
        }
        ui.add_space(10.0);

        // Parents show their subcategories added in (Quicken's roll-up).
        let rolled_actual = roll_up(&actual);
        let mut changes: Vec<(String, f64)> = Vec::new();
        let mut fold: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (kind, title) in [(CatKind::Expense, "Expenses"), (CatKind::Income, "Income")] {
                panel(ui, title, |ui| {
                    let grid = egui::Grid::new(("budget_grid", title)).num_columns(5).spacing([28.0, 6.0]).striped(true);
                    grid.min_col_width(100.0).show(ui, |ui| {
                        let rem_title = if kind == CatKind::Income { "Still to come" } else { "Remaining" };
                        for h in ["Category", "Budget", "Actual", rem_title, "Progress"] {
                            ui.label(RichText::new(h).strong().color(Color32::from_gray(70)));
                        }
                        ui.end_row();
                        let (mut tb, mut ta) = (0.0, 0.0);
                        for c in self.ledger.categories.iter().filter(|c| c.kind == kind) {
                            let b = self.ledger.budgeted_rolled(&c.name, m0, m1);
                            let a = rolled_actual.get(&c.name).copied().unwrap_or(0.0);
                            // Totals count each top category once (it already holds its subcategories).
                            if cat_depth(&c.name) == 0 {
                                tb += b;
                                ta += a;
                            }
                            if self.cat_folded_away(&c.name) {
                                continue;
                            }
                            let kids = self.ledger.has_subcategories(&c.name);
                            if tree_name(ui, &c.name, kids, !self.cat_collapsed.contains(&c.name)) {
                                fold = Some(c.name.clone());
                            }
                            let buf = self.budget_buf.entry(c.name.clone()).or_default();
                            let edit = egui::TextEdit::singleline(buf).hint_text("0.00").align(Align2::RIGHT_CENTER).min_size(egui::vec2(90.0, 0.0));
                            let tip = if kids { "This category's budget, including its subcategories" } else { "This category's budget for the month" };
                            if ui.add(edit).on_hover_text(tip).lost_focus() {
                                if let Some(v) = parse_money(buf) {
                                    if (v - b).abs() > 0.004 {
                                        changes.push((c.name.clone(), v));
                                    }
                                }
                            }
                            let strong = |t: String| if kids { RichText::new(t).strong() } else { RichText::new(t) };
                            ui.label(strong(money(a)));
                            let rem = b - a;
                            let rem_col = if kind == CatKind::Expense && rem < 0.0 { RED } else { Color32::from_gray(50) };
                            ui.label(strong(money(rem)).color(rem_col));
                            progress(ui, a, b, kind);
                            ui.end_row();
                        }
                        ui.label(RichText::new("Total").strong());
                        ui.label(RichText::new(money(tb)).strong());
                        ui.label(RichText::new(money(ta)).strong());
                        ui.label(RichText::new(money(tb - ta)).strong());
                        progress(ui, ta, tb, kind);
                        ui.end_row();
                    });
                });
                ui.add_space(10.0);
            }
        });
        if let Some(name) = fold {
            if !self.cat_collapsed.remove(&name) {
                self.cat_collapsed.insert(name);
            }
        }
        if !changes.is_empty() {
            for (c, v) in changes {
                // A parent's figure includes its subcategories, so what's
                // typed there sets the parent's own share to make up the rest.
                let subs = self.ledger.budgeted_rolled(&c, m0, m1) - self.ledger.budgeted(&c, m0, m1);
                let own = v - subs;
                if own < -0.004 {
                    self.status = format!("{}'s subcategories are already budgeted {}; enter at least that.", cat_leaf(&c), money(subs));
                    continue;
                }
                self.ledger.set_budget(m0, &c, own.max(0.0));
                self.status = "Budget updated.".into();
            }
            self.dirty = true;
            self.budget_buf_month = None;
        }
    }

    /// Whether a category is hidden because one of its parents is folded.
    pub(super) fn cat_folded_away(&self, name: &str) -> bool {
        let mut p = cat_parent(name);
        while let Some(parent) = p {
            if self.cat_collapsed.contains(parent) {
                return true;
            }
            p = cat_parent(parent);
        }
        false
    }

    fn refill_budget_buf(&mut self) {
        let m0 = self.budget_month;
        self.budget_buf = self
            .ledger
            .categories
            .iter()
            .map(|c| {
                let v = self.ledger.budgeted_rolled(&c.name, m0, m0);
                (c.name.clone(), if v == 0.0 { String::new() } else { format!("{v:.2}") })
            })
            .collect();
        self.budget_buf_month = Some(m0);
    }


    /// The Categories list, as Quicken shows it: one list of categories with
    /// their subcategories indented under them (fold arrows on parents), each
    /// with its type, how many transactions use it and this month's amount.
    pub(super) fn categories(&mut self, ui: &mut egui::Ui) {
        page_title(ui, "Categories", "Categories and their subcategories group your income and spending for budgets and reports.", None);
        ui.add_space(10.0);
        let mut add_now = false;
        let parents: Vec<String> = self.ledger.categories.iter().map(|c| c.name.clone()).collect();
        band(ui, |ui| {
            ui.horizontal(|ui| {
                let hint = if self.new_cat_parent.is_some() { "New subcategory name" } else { "New category name" };
                let r = ui.add(egui::TextEdit::singleline(&mut self.new_cat).hint_text(hint).desired_width(220.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    add_now = true;
                }
                ui.label(RichText::new("under").color(MUTED));
                let under = self.new_cat_parent.clone().unwrap_or_else(|| "(top level)".into());
                egui::ComboBox::from_id_salt("newcatparent").width(240.0).height(420.0).selected_text(under).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.new_cat_parent, None, "(top level: a new category)");
                    ui.separator();
                    for p in &parents {
                        let label = format!("{}{}", "    ".repeat(cat_depth(p)), cat_leaf(p));
                        ui.selectable_value(&mut self.new_cat_parent, Some(p.clone()), label);
                    }
                });
                // A subcategory has its parent's type, as in Quicken.
                let parent_kind = self.new_cat_parent.as_deref().and_then(|p| self.ledger.category_kind(p));
                if let Some(k) = parent_kind {
                    self.new_cat_kind = k;
                }
                ui.add_enabled_ui(parent_kind.is_none(), |ui| {
                    egui::ComboBox::from_id_salt("newcatkind")
                        .width(100.0)
                        .selected_text(if self.new_cat_kind == CatKind::Income { "Income" } else { "Expense" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.new_cat_kind, CatKind::Expense, "Expense");
                            ui.selectable_value(&mut self.new_cat_kind, CatKind::Income, "Income");
                        });
                });
                let label = if self.new_cat_parent.is_some() { "Add Subcategory" } else { "Add Category" };
                if primary_button(ui, label).clicked() {
                    add_now = true;
                }
            });
        });
        if add_now {
            let leaf = self.new_cat.trim().replace(':', " ");
            let name = match &self.new_cat_parent {
                Some(p) => format!("{p}:{leaf}"),
                None => leaf.clone(),
            };
            if leaf.is_empty() {
                self.status = "Type a name for the new category first.".into();
            } else if self.ledger.category_kind(&name).is_some() {
                self.status = format!("There's already a category called {name}.");
            } else {
                self.ledger.categories.push(Category { name: name.clone(), kind: self.new_cat_kind });
                self.ledger.normalize_categories();
                if let Some(p) = &self.new_cat_parent {
                    self.cat_collapsed.remove(p);
                }
                self.status = match &self.new_cat_parent {
                    Some(p) => format!("Added the subcategory {leaf} under {p}."),
                    None => format!("Added the category {leaf}."),
                };
                self.new_cat.clear();
                self.dirty = true;
                self.budget_buf_month = None;
            }
        }
        ui.add_space(10.0);
        let t = today();
        let actual = roll_up(&self.ledger.actuals(first_of_month(t), last_of_month(t)));
        let mut uses: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for tx in &self.ledger.transactions {
            for (c, _) in tx.parts() {
                *uses.entry(c).or_default() += 1;
            }
        }
        let uses: std::collections::HashMap<String, usize> = uses.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        let mut rename: Option<(String, String)> = None;
        let mut delete: Option<String> = None;
        let mut start_rename: Option<String> = None;
        let mut add_sub: Option<String> = None;
        let mut cancel_rename = false;
        let mut fold: Option<String> = None;
        let shown = self.ledger.categories.len();
        let tops = self.ledger.categories.iter().filter(|c| cat_depth(&c.name) == 0).count();
        ui.label(RichText::new(format!("{shown} categories ({tops} top-level, {} subcategories)", shown - tops)).color(MUTED));
        ui.add_space(4.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let grid = egui::Grid::new("cats_tree").num_columns(5).spacing([28.0, 6.0]).striped(true);
            grid.min_col_width(90.0).show(ui, |ui| {
                for h in ["Name", "Type", "Uses", "This month", ""] {
                    ui.label(RichText::new(h).strong().color(Color32::from_gray(70)));
                }
                ui.end_row();
                for c in &self.ledger.categories {
                    if self.cat_folded_away(&c.name) {
                        continue;
                    }
                    let kids = self.ledger.has_subcategories(&c.name);
                    let n = uses.get(&c.name).copied().unwrap_or(0);
                    let renaming = matches!(&self.cat_rename, Some((old, _)) if *old == c.name);
                    match (&mut self.cat_rename, renaming) {
                        (Some((_, buf)), true) => {
                            ui.horizontal(|ui| {
                                ui.add_space(cat_depth(&c.name) as f32 * 22.0 + 18.0);
                                ui.add(egui::TextEdit::singleline(buf).desired_width(200.0));
                            });
                        }
                        _ => {
                            if tree_name(ui, &c.name, kids, !self.cat_collapsed.contains(&c.name)) {
                                fold = Some(c.name.clone());
                            }
                        }
                    }
                    ui.label(if c.kind == CatKind::Income { "Income" } else { "Expense" });
                    ui.label(if n == 0 { RichText::new("Unused").color(MUTED) } else { RichText::new(n.to_string()).color(ACCENT) });
                    ui.label(money(actual.get(&c.name).copied().unwrap_or(0.0)));
                    ui.horizontal(|ui| {
                        if renaming {
                            if ui.button("Save").clicked() {
                                if let Some((old, buf)) = &self.cat_rename {
                                    rename = Some((old.clone(), buf.trim().replace(':', " ")));
                                }
                            }
                            if ui.button("Cancel").clicked() {
                                cancel_rename = true;
                            }
                        } else {
                            if ui.small_button("Rename").clicked() {
                                start_rename = Some(c.name.clone());
                            }
                            if ui.small_button("+ Subcategory").on_hover_text("Add a subcategory under this one").clicked() {
                                add_sub = Some(c.name.clone());
                            }
                            let can_delete = n == 0 && !kids;
                            let del = ui.add_enabled(can_delete, egui::Button::new("Delete").small());
                            let why = if kids { "Delete its subcategories first" } else { "Only unused categories can be deleted" };
                            if del.on_disabled_hover_text(why).clicked() {
                                delete = Some(c.name.clone());
                            }
                        }
                    });
                    ui.end_row();
                }
            });
        });
        if let Some(name) = fold {
            if !self.cat_collapsed.remove(&name) {
                self.cat_collapsed.insert(name);
            }
        }
        if let Some(p) = add_sub {
            self.new_cat_parent = Some(p.clone());
            self.status = format!("Type the new subcategory's name at the top; it goes under {p}.");
        }
        // Renaming changes a category's own name; its parent stays the same.
        if let Some(n) = start_rename {
            self.cat_rename = Some((n.clone(), cat_leaf(&n).to_string()));
        }
        if cancel_rename {
            self.cat_rename = None;
        }
        if let Some((old, leaf)) = rename {
            let new = match cat_parent(&old) {
                Some(p) => format!("{p}:{leaf}"),
                None => leaf.clone(),
            };
            if leaf.is_empty() || (new != old && self.ledger.category_kind(&new).is_some()) {
                self.status = "Pick a new, unused name.".into();
            } else {
                self.ledger.rename_category(&old, &new);
                if self.cat_collapsed.remove(&old) {
                    self.cat_collapsed.insert(new.clone());
                }
                self.status = format!("Renamed {} to {leaf}.", cat_leaf(&old));
                self.cat_rename = None;
                self.dirty = true;
                self.budget_buf_month = None;
            }
        }
        if let Some(name) = delete {
            self.ledger.categories.retain(|c| c.name != name);
            for m in self.ledger.budgets.values_mut() {
                m.remove(&name);
            }
            self.status = format!("Deleted the category {}.", cat_leaf(&name));
            self.dirty = true;
            self.budget_buf_month = None;
        }
    }
}

/// A category's name cell: indented by how deep it sits, with a fold arrow on
/// a category that has subcategories (bold, as a parent). Returns true when
/// the arrow (or the parent's name) was clicked to fold or unfold it.
pub(super) fn tree_name(ui: &mut egui::Ui, name: &str, has_kids: bool, open: bool) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add_space(cat_depth(name) as f32 * 22.0);
        let (r, resp) = ui.allocate_exact_size(egui::vec2(14.0, 18.0), if has_kids { Sense::click() } else { Sense::hover() });
        if has_kids {
            chevron(ui, r.center(), open);
            clicked |= resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        }
        let text = if has_kids { RichText::new(cat_leaf(name)).strong() } else { RichText::new(cat_leaf(name)) };
        let l = ui.add(egui::Label::new(text).sense(if has_kids { Sense::click() } else { Sense::hover() }));
        let l = if cat_depth(name) > 0 { l.on_hover_text(name) } else { l };
        clicked |= has_kids && l.clicked();
    });
    clicked
}
