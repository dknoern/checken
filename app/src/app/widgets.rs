//! Small drawing helpers shared by the screens, in a light desktop
//! finance-app style: grey sidebar, white pages, blue pills, green filters.

use super::{ACCENT, BAND, CLEAR_GREEN, GREEN, LINE, NAVY, ORANGE, PAPER, RED};
use crate::model::{CatKind, money};
use eframe::egui::{self, Align, Align2, Color32, FontFamily, FontId, Layout, Margin, RichText, Sense, Stroke, StrokeKind};
use std::sync::Arc;

pub const SEMIBOLD: &str = "semibold";
pub const ITALIC: &str = "italic";

pub fn amount_color(v: f64) -> Color32 {
    if v < -0.004 { RED } else { Color32::from_gray(30) }
}

pub fn signed_money(v: f64) -> String {
    if v >= 0.0 { format!("+{}", money(v)) } else { money(v) }
}

/// Whole dollars, as in the sidebar: "$10,453", "-$18,890".
pub fn money_whole(v: f64) -> String {
    let s = money(v.round());
    s.strip_suffix(".00").map(str::to_string).unwrap_or(s)
}

/// A plain number with thousands separators and no currency sign: "-1,068.53".
pub fn num(v: f64) -> String {
    money(v).replacen('$', "", 1)
}

pub fn semibold(text: impl Into<String>) -> RichText {
    RichText::new(text).family(FontFamily::Name(SEMIBOLD.into()))
}

pub fn italic(text: impl Into<String>) -> RichText {
    RichText::new(text).family(FontFamily::Name(ITALIC.into()))
}

/// A register column heading: small letter-spaced capitals.
pub fn col_header(text: &str, color: Color32) -> RichText {
    RichText::new(text).size(10.5).extra_letter_spacing(0.9).color(color)
}

pub fn semibold_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Big page title with a smaller line under it, and an optional figure on the right.
/// A page's title and subtitle on the left and, optionally, a labelled
/// figure (a balance or total) on the right. The figure keeps its full width;
/// a long subtitle wraps onto more lines in the space left of it.
pub fn page_title(ui: &mut egui::Ui, title: &str, subtitle: &str, right: Option<(&str, String, Color32)>) {
    let gap = 24.0;
    // How wide the figure on the right needs to be.
    let right_w = right
        .as_ref()
        .map(|(label, value, color)| {
            let p = ui.painter();
            let l = p.layout_no_wrap(label.to_string(), FontId::proportional(13.0), *color).size().x;
            let v = p.layout_no_wrap(value.clone(), FontId::proportional(26.0), *color).size().x;
            l.max(v) + 2.0
        })
        .unwrap_or(0.0);
    ui.horizontal_top(|ui| {
        let left_w = (ui.available_width() - right_w - if right.is_some() { gap } else { 0.0 }).max(120.0);
        ui.allocate_ui_with_layout(egui::vec2(left_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_max_width(left_w);
            ui.add(egui::Label::new(RichText::new(title).size(24.0).color(NAVY)).truncate());
            if !subtitle.is_empty() {
                ui.add(egui::Label::new(RichText::new(subtitle).size(14.0).color(Color32::from_gray(40))).wrap());
            }
        });
        if let Some((label, value, color)) = right {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.allocate_ui_with_layout(egui::vec2(right_w, 0.0), egui::Layout::top_down(egui::Align::Max), |ui| {
                    ui.add(egui::Label::new(RichText::new(label).size(13.0).color(Color32::from_gray(70))).extend());
                    ui.add(egui::Label::new(RichText::new(value).size(26.0).color(color)).extend());
                });
            });
        }
    });
}

/// The light grey strip that holds tabs and filters.
pub fn band<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(BAND)
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Like Quicken, controls that don't fit are cut off at the band's
            // right edge rather than drawn over each other or wrapped.
            let clip = ui.max_rect().expand2(egui::vec2(10.0, 8.0)).intersect(ui.clip_rect());
            ui.set_clip_rect(clip);
            add(ui)
        })
        .inner
}

/// A category as a pick-list entry: its own name, indented under its parent
/// ("Auto & Transport" then "    Auto Insurance").
pub fn tree_label(name: &str) -> String {
    format!("{}{}", "      ".repeat(crate::model::cat_depth(name)), crate::model::cat_leaf(name))
}

/// Adds a widget at the right end of a horizontal row when it fits there, and
/// otherwise right after what's already in the row, where the band cuts it
/// off: the way Quicken's filter bar behaves when the window gets narrow.
pub fn right_or_after<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    if ui.available_width() >= width + 12.0 {
        ui.with_layout(Layout::right_to_left(Align::Center), add).inner
    } else {
        ui.add_space(12.0);
        add(ui)
    }
}

/// A tab in a band: solid blue when selected, plain text otherwise.
pub fn tab_pill(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let text = RichText::new(label).size(14.0).color(if selected { Color32::WHITE } else { Color32::from_gray(25) });
    ui.add(
        egui::Button::new(text)
            .fill(if selected { ACCENT } else { Color32::TRANSPARENT })
            .stroke(Stroke::NONE)
            .corner_radius(4.0)
            .min_size(egui::vec2(0.0, 28.0)),
    )
}

pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(ACCENT).stroke(Stroke::NONE))
}

pub fn green_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(Color32::WHITE).size(15.0))
            .fill(CLEAR_GREEN)
            .stroke(Stroke::new(1.0, Color32::from_rgb(0x4a, 0x9a, 0x38)))
            .corner_radius(3.0)
            .min_size(egui::vec2(0.0, 26.0)),
    )
}

/// A round outlined icon button, like the ones at the top of the sidebar.
pub fn circle_button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    circle_button_badge(ui, icon, tip, false)
}

/// A round button with an optional orange dot at its top left, meaning
/// something there wants attention.
pub fn circle_button_badge(ui: &mut egui::Ui, icon: Icon, tip: &str, badge: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), Sense::click());
    let p = ui.painter();
    let fill = if resp.hovered() { Color32::from_gray(232) } else { Color32::WHITE };
    p.circle(rect.center(), 15.0, fill, Stroke::new(1.0, Color32::from_gray(205)));
    draw_icon(p, icon, rect.center(), 7.0, ACCENT);
    if badge {
        p.circle_filled(rect.center() + egui::vec2(-11.0, -11.0), 4.5, ORANGE);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip)
}

/// A borderless grey icon button, like the sidebar toggle.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(34.0, 30.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, 5.0, Color32::from_gray(222));
    }
    draw_icon(p, icon, rect.center(), 8.0, Color32::from_gray(95));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip)
}

#[derive(Clone, Copy)]
pub enum Icon {
    Plus,
    More,
    New,
    Edit,
    Delete,
    Reconcile,
    Copy,
    Dollar,
    Sidebar,
    Open,
    Duplicate,
    Print,
    Split,
    Refresh,
}

/// Draws one of the app's line icons, for screens that place their own.
pub fn draw_icon_public(p: &egui::Painter, icon: Icon, c: egui::Pos2, s: f32, col: Color32) {
    draw_icon(p, icon, c, s, col);
}

fn draw_icon(p: &egui::Painter, icon: Icon, c: egui::Pos2, s: f32, col: Color32) {
    let st = Stroke::new(1.6, col);
    let pt = |x: f32, y: f32| egui::pos2(c.x + x * s, c.y + y * s);
    match icon {
        Icon::Plus => {
            p.line_segment([pt(-1.0, 0.0), pt(1.0, 0.0)], st);
            p.line_segment([pt(0.0, -1.0), pt(0.0, 1.0)], st);
        }
        Icon::More => {
            for x in [-1.0, 0.0, 1.0] {
                p.circle_filled(pt(x, 0.0), 2.0, col);
            }
        }
        Icon::New => {
            p.circle_stroke(c, s * 1.3, st);
            p.line_segment([pt(-0.6, 0.0), pt(0.6, 0.0)], st);
            p.line_segment([pt(0.0, -0.6), pt(0.0, 0.6)], st);
        }
        Icon::Edit => {
            p.line_segment([pt(-1.0, 1.0), pt(0.9, -0.9)], Stroke::new(3.0, col));
            p.line_segment([pt(-1.2, 1.2), pt(-0.8, 1.2)], st);
        }
        Icon::Delete => {
            let r = egui::Rect::from_min_max(pt(-0.8, -0.7), pt(0.8, 1.2));
            p.rect_stroke(r, 1.5, st, StrokeKind::Middle);
            p.line_segment([pt(-1.2, -0.8), pt(1.2, -0.8)], st);
            p.line_segment([pt(-0.3, -1.2), pt(0.3, -1.2)], st);
            for x in [-0.3, 0.3] {
                p.line_segment([pt(x, -0.3), pt(x, 0.9)], Stroke::new(1.0, col));
            }
        }
        Icon::Reconcile => {
            p.circle_stroke(c, s * 1.3, st);
            p.line_segment([pt(-0.6, 0.05), pt(-0.15, 0.5)], st);
            p.line_segment([pt(-0.15, 0.5), pt(0.65, -0.45)], st);
        }
        Icon::Copy => {
            p.rect_stroke(egui::Rect::from_min_max(pt(-1.0, -0.6), pt(0.5, 1.2)), 1.5, st, StrokeKind::Middle);
            p.line_segment([pt(-0.5, -1.1), pt(1.0, -1.1)], st);
            p.line_segment([pt(1.0, -1.1), pt(1.0, 0.7)], st);
        }
        Icon::Open => {
            // A window with a title strip.
            let r = egui::Rect::from_min_max(pt(-1.2, -0.9), pt(1.2, 1.0));
            p.rect_stroke(r, 2.0, st, StrokeKind::Middle);
            p.line_segment([pt(-1.2, -0.4), pt(1.2, -0.4)], st);
        }
        Icon::Split => {
            // A fork: one stem dividing into two.
            p.line_segment([pt(0.0, 1.2), pt(0.0, 0.1)], st);
            p.line_segment([pt(0.0, 0.1), pt(-0.9, -0.9)], st);
            p.line_segment([pt(0.0, 0.1), pt(0.9, -0.9)], st);
            p.line_segment([pt(-1.2, -0.9), pt(-0.6, -0.9)], st);
            p.line_segment([pt(0.6, -0.9), pt(1.2, -0.9)], st);
        }
        Icon::Print => {
            // Paper feeding into a printer body.
            p.rect_stroke(egui::Rect::from_min_max(pt(-0.7, -1.2), pt(0.7, -0.35)), 1.0, st, StrokeKind::Middle);
            p.rect_stroke(egui::Rect::from_min_max(pt(-1.3, -0.35), pt(1.3, 0.6)), 2.0, st, StrokeKind::Middle);
            p.rect_stroke(egui::Rect::from_min_max(pt(-0.7, 0.25), pt(0.7, 1.2)), 1.0, st, StrokeKind::Middle);
            p.line_segment([pt(-0.4, 0.6), pt(0.4, 0.6)], Stroke::new(1.0, col));
            p.line_segment([pt(-0.4, 0.9), pt(0.4, 0.9)], Stroke::new(1.0, col));
        }
        Icon::Duplicate => {
            p.rect_stroke(egui::Rect::from_min_max(pt(-1.1, -0.5), pt(0.6, 1.2)), 1.5, st, StrokeKind::Middle);
            p.rect_stroke(egui::Rect::from_min_max(pt(-0.6, -1.1), pt(1.1, 0.6)), 1.5, st, StrokeKind::Middle);
            p.line_segment([pt(-0.25, 0.35), pt(0.25, 0.35)], Stroke::new(1.2, col));
            p.line_segment([pt(0.0, 0.1), pt(0.0, 0.6)], Stroke::new(1.2, col));
        }
        Icon::Dollar => {
            p.text(c, Align2::CENTER_CENTER, "$", FontId::proportional(s * 2.7), col);
        }
        Icon::Refresh => {
            // Two arcs chasing each other, with arrowheads, as Quicken's update button.
            for (start, tip_dir) in [(-0.35f32, 1.0f32), (std::f32::consts::PI - 0.35, -1.0)] {
                let n = 12;
                let pts: Vec<egui::Pos2> = (0..=n)
                    .map(|k| {
                        let a = start + k as f32 / n as f32 * 2.3;
                        pt(a.cos() * 1.05, a.sin() * 1.05)
                    })
                    .collect();
                p.add(egui::Shape::line(pts.clone(), st));
                let end = *pts.last().unwrap();
                let a = start + 2.3;
                // Arrowhead along the arc's direction at its end.
                let dir = egui::vec2(-a.sin(), a.cos()) * s * 0.55;
                let normal = egui::vec2(a.cos(), a.sin()) * s * 0.45;
                let _ = tip_dir;
                p.line_segment([end, end - dir + normal], st);
                p.line_segment([end, end - dir - normal], st);
            }
        }
        Icon::Sidebar => {
            // A window outline with a narrow pane on its left.
            p.rect_stroke(egui::Rect::from_min_max(pt(-1.3, -1.0), pt(1.3, 1.0)), 2.5, st, StrokeKind::Middle);
            p.line_segment([pt(-0.45, -1.0), pt(-0.45, 1.0)], st);
            for y in [-0.55, -0.2, 0.15] {
                p.line_segment([pt(-1.05, y), pt(-0.7, y)], Stroke::new(1.2, col));
            }
        }
    }
}

/// An icon beside its label, as in the report library's toolbar.
pub fn tool_text_button(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool, danger: bool) -> egui::Response {
    let font = FontId::proportional(15.5);
    let text_w = ui.painter().layout_no_wrap(label.to_string(), font.clone(), Color32::BLACK).size().x;
    let size = egui::vec2(text_w + 34.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    if enabled && resp.hovered() {
        p.rect_filled(rect, 5.0, Color32::from_gray(228));
    }
    let col = match (enabled, danger) {
        (false, _) => Color32::from_gray(175),
        (true, true) => RED,
        (true, false) => Color32::from_gray(45),
    };
    draw_icon(p, icon, egui::pos2(rect.left() + 13.0, rect.center().y), 6.5, col);
    p.text(egui::pos2(rect.left() + 27.0, rect.center().y), Align2::LEFT_CENTER, label, font, col);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// An icon above a small label, as in the register's bottom toolbar.
pub fn toolbar_button(ui: &mut egui::Ui, icon: Icon, label: &str, enabled: bool, danger: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(68.0, 48.0), if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    if enabled && resp.hovered() {
        p.rect_filled(rect.shrink(2.0), 6.0, Color32::from_gray(238));
    }
    let col = match (enabled, danger) {
        (false, _) => Color32::from_gray(185),
        (true, true) => RED,
        (true, false) => Color32::from_gray(90),
    };
    draw_icon(p, icon, egui::pos2(rect.center().x, rect.top() + 16.0), 7.0, col);
    p.text(egui::pos2(rect.center().x, rect.bottom() - 6.0), Align2::CENTER_BOTTOM, label, FontId::proportional(12.5), col);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// A sidebar row: optional coloured dot, the name and a whole-dollar balance.
pub fn account_row(ui: &mut egui::Ui, name: &str, bal: Option<f64>, dot: Option<Color32>, selected: bool, bold: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), if bold { 25.0 } else { 21.0 }), Sense::click());
    let p = ui.painter().clone();
    if selected {
        p.rect_filled(rect, 5.0, Color32::from_gray(218));
    } else if resp.hovered() {
        p.rect_filled(rect, 5.0, Color32::from_gray(228));
    }
    if let Some(d) = dot {
        p.circle_filled(egui::pos2(rect.left() + 15.0, rect.center().y), 4.0, d);
    }
    let font = if bold { semibold_font(13.5) } else { FontId::proportional(13.5) };
    let fg = Color32::from_gray(25);
    let mut right = rect.right() - 6.0;
    if let Some(b) = bal {
        let bal_galley = p.layout_no_wrap(money_whole(b), font.clone(), amount_color(b));
        right -= bal_galley.size().x;
        p.galley(egui::pos2(right, rect.center().y - bal_galley.size().y / 2.0), bal_galley, fg);
    }
    let left = rect.left() + if bold { 14.0 } else { 25.0 };
    let mut job = egui::text::LayoutJob::simple_singleline(name.to_string(), font, fg);
    job.wrap = egui::text::TextWrapping::truncate_at_width((right - left - 10.0).max(20.0));
    let name_galley = p.layout_job(job);
    p.galley(egui::pos2(left, rect.center().y - name_galley.size().y / 2.0), name_galley, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn card(ui: &mut egui::Ui, width: f32, title: &str, value: &str, color: Color32, note: &str) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(6.0)
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(width - 30.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).color(Color32::from_gray(80)));
                ui.label(RichText::new(value).size(24.0).color(color));
                ui.label(RichText::new(note).small().color(Color32::from_gray(110)));
            });
        });
}

/// A row of equal-width summary cards filling the available width.
pub fn card_row(ui: &mut egui::Ui, cards: &[(&str, String, Color32, String)]) {
    ui.horizontal(|ui| {
        let n = cards.len() as f32;
        let w = ((ui.available_width() - (n - 1.0) * 12.0) / n).floor().max(150.0);
        ui.spacing_mut().item_spacing.x = 12.0;
        for (title, value, color, note) in cards {
            card(ui, w, title, value, *color, note);
        }
    });
}

pub fn panel(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0, LINE))
        .corner_radius(6.0)
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(semibold(title).size(16.0).color(NAVY));
            ui.add_space(6.0);
            add(ui);
        });
}

pub fn progress(ui: &mut egui::Ui, actual: f64, budget: f64, kind: CatKind) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(200.0, 16.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 8.0, Color32::from_gray(232));
    if budget <= 0.0 {
        if actual > 0.0 {
            let col = if kind == CatKind::Expense { RED } else { GREEN };
            p.rect_filled(rect, 8.0, col.gamma_multiply(0.75));
            p.text(rect.center(), Align2::CENTER_CENTER, "no budget", FontId::proportional(11.0), Color32::WHITE);
        }
        return;
    }
    let frac = (actual / budget).max(0.0);
    let col = match kind {
        CatKind::Income => GREEN,
        CatKind::Expense if frac > 1.0 => RED,
        CatKind::Expense if frac > 0.9 => ORANGE,
        CatKind::Expense => CLEAR_GREEN,
    };
    let mut fill = rect;
    fill.set_width(rect.width() * frac.min(1.0) as f32);
    if fill.width() > 1.0 {
        p.rect_filled(fill, 8.0, col);
    }
    p.rect_stroke(rect, 8.0, Stroke::new(1.0, Color32::from_gray(215)), StrokeKind::Inside);
    let txt_col = if frac > 0.55 { Color32::WHITE } else { Color32::from_gray(60) };
    p.text(rect.center(), Align2::CENTER_CENTER, format!("{:.0}%", frac * 100.0), FontId::proportional(11.0), txt_col);
}

/// Horizontal bars, one per (label, value), scaled to the largest value.
pub fn hbars(ui: &mut egui::Ui, rows: &[(String, f64)], color: Color32) {
    let max = rows.iter().map(|r| r.1.abs()).fold(0.0, f64::max).max(1.0);
    let label_w = 150.0;
    let value_w = 96.0;
    for (i, (label, v)) in rows.iter().enumerate() {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.0), Sense::hover());
        let p = ui.painter();
        p.text(egui::pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), Color32::from_gray(30));
        let bar_room = (rect.width() - label_w - value_w - 8.0).max(20.0);
        let w = (v.abs() / max) as f32 * bar_room;
        let bar = egui::Rect::from_min_size(egui::pos2(rect.left() + label_w, rect.top() + 4.0), egui::vec2(w.max(2.0), 14.0));
        let c = if color == Color32::TRANSPARENT { slice_color(i) } else { color };
        p.rect_filled(bar, 3.0, c);
        p.text(egui::pos2(rect.right(), rect.center().y), Align2::RIGHT_CENTER, money(*v), FontId::proportional(14.0), Color32::from_gray(30));
        resp.on_hover_text(format!("{label}: {}", money(*v)));
    }
}

/// Chart colours for categories, in order.
pub fn slice_color(i: usize) -> Color32 {
    const C: [Color32; 12] = [
        Color32::from_rgb(0x66, 0xbb, 0x6a),
        Color32::from_rgb(0x7b, 0x83, 0xeb),
        Color32::from_rgb(0xc5, 0x8a, 0xf9),
        Color32::from_rgb(0xf4, 0xa2, 0x59),
        Color32::from_rgb(0x4a, 0xa3, 0xdf),
        Color32::from_rgb(0xee, 0x7f, 0xa0),
        Color32::from_rgb(0x7e, 0xdc, 0xe2),
        Color32::from_rgb(0xf2, 0xf2, 0x7a),
        Color32::from_rgb(0x5c, 0x6b, 0xc0),
        Color32::from_rgb(0xb5, 0xc9, 0x5a),
        Color32::from_rgb(0xe5, 0x73, 0x73),
        Color32::from_rgb(0x9e, 0x9e, 0x9e),
    ];
    C[i % C.len()]
}

/// A ring chart: one slice per value, starting at twelve o'clock.
pub fn donut(ui: &egui::Ui, center: egui::Pos2, outer: f32, inner: f32, values: &[f64]) {
    let total: f64 = values.iter().filter(|v| **v > 0.0).sum();
    let p = ui.painter();
    if total <= 0.0 {
        p.circle_stroke(center, (outer + inner) / 2.0, Stroke::new(outer - inner, Color32::from_gray(230)));
        return;
    }
    let mut mesh = egui::Mesh::default();
    let mut a0 = -std::f32::consts::FRAC_PI_2;
    for (i, v) in values.iter().enumerate() {
        if *v <= 0.0 {
            continue;
        }
        let sweep = (*v / total) as f32 * std::f32::consts::TAU;
        let steps = ((sweep / 0.04).ceil() as usize).max(2);
        let col = slice_color(i);
        let base = mesh.vertices.len() as u32;
        for k in 0..=steps {
            let a = a0 + sweep * k as f32 / steps as f32;
            let dir = egui::vec2(a.cos(), a.sin());
            mesh.colored_vertex(center + dir * outer, col);
            mesh.colored_vertex(center + dir * inner, col);
        }
        for k in 0..steps as u32 {
            let o = base + k * 2;
            mesh.add_triangle(o, o + 1, o + 2);
            mesh.add_triangle(o + 1, o + 3, o + 2);
        }
        // A thin white gap between slices.
        let dir = egui::vec2(a0.cos(), a0.sin());
        p.line_segment([center + dir * inner, center + dir * outer], Stroke::new(1.0, Color32::WHITE));
        a0 += sweep;
    }
    p.add(egui::Shape::mesh(mesh));
    let mut a = -std::f32::consts::FRAC_PI_2;
    for v in values.iter().filter(|v| **v > 0.0) {
        let dir = egui::vec2(a.cos(), a.sin());
        p.line_segment([center + dir * (inner - 1.0), center + dir * (outer + 1.0)], Stroke::new(1.5, Color32::WHITE));
        a += (*v / total) as f32 * std::f32::consts::TAU;
    }
}

/// Use the Mac system font (regular, semibold and italic) when it is there,
/// keeping egui's own fonts as fallbacks for symbols.
fn setup_fonts(ctx: &egui::Context) {
    use egui::epaint::text::{FontData, FontDefinitions, FontTweak, VariationCoords};
    let mut fonts = FontDefinitions::default();
    let load = |path: &str| std::fs::read(path).ok().map(Arc::new);
    let mut primary: Vec<(String, String)> = Vec::new();
    if let Some(sf) = load("/System/Library/Fonts/SFNS.ttf") {
        let weighted = |w: f32| {
            let tweak = FontTweak { coords: VariationCoords::new([("wght", w)]), ..Default::default() };
            Arc::new(FontData::from_blob(sf.clone(), 0).tweak(tweak))
        };
        fonts.font_data.insert("sf-regular".into(), weighted(400.0));
        fonts.font_data.insert("sf-semibold".into(), weighted(590.0));
        primary.push(("sf-regular".into(), "sf-semibold".into()));
    }
    let italic = load("/System/Library/Fonts/SFNSItalic.ttf").map(|b| {
        let tweak = FontTweak { coords: VariationCoords::new([("wght", 400.0)]), ..Default::default() };
        Arc::new(FontData::from_blob(b, 0).tweak(tweak))
    });
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let (regular, bold) = primary.pop().unwrap_or_else(|| (fallback[0].clone(), fallback[0].clone()));
    let with = |first: &str| std::iter::once(first.to_string()).chain(fallback.iter().cloned()).collect::<Vec<_>>();
    let mut italic_name = regular.clone();
    if let Some(it) = italic {
        fonts.font_data.insert("sf-italic".into(), it);
        italic_name = "sf-italic".into();
    }
    fonts.families.insert(FontFamily::Proportional, with(&regular));
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), with(&bold));
    fonts.families.insert(FontFamily::Name(ITALIC.into()), with(&italic_name));
    ctx.set_fonts(fonts);
}

pub fn setup_style(ctx: &egui::Context) {
    setup_fonts(ctx);
    ctx.set_visuals(egui::Visuals::light());
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.interact_size.y = 26.0;
        for (style, font) in s.text_styles.iter_mut() {
            font.size = match style {
                egui::TextStyle::Small => 11.5,
                egui::TextStyle::Body | egui::TextStyle::Button | egui::TextStyle::Monospace => 14.0,
                egui::TextStyle::Heading => 30.0,
                _ => font.size,
            };
        }
        s.visuals.override_text_color = None;
        s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_gray(25));
        s.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_gray(25));
        s.visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
        s.visuals.widgets.inactive.bg_fill = Color32::WHITE;
        s.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_gray(200));
        s.visuals.widgets.hovered.weak_bg_fill = Color32::from_gray(245);
        s.visuals.widgets.inactive.corner_radius = 4.0.into();
        s.visuals.widgets.hovered.corner_radius = 4.0.into();
        s.visuals.widgets.active.corner_radius = 4.0.into();
        s.visuals.selection.bg_fill = Color32::from_rgb(0xd3, 0xe3, 0xfa);
        s.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        s.visuals.faint_bg_color = Color32::from_gray(246);
        s.visuals.panel_fill = PAPER;
        s.visuals.extreme_bg_color = Color32::WHITE;
        s.visuals.window_fill = Color32::WHITE;
        s.interaction.selectable_labels = false;
    });
}
