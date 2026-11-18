#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Planum · 计划日历 —— 纯 Rust / egui 原生界面（v2 重设计）
//! 设计目标：流畅（不卡顿）、丝滑（过渡与微交互）、正确（零裁切/零挤压/零错位）
use eframe::egui;
use egui::{Color32, Vec2, FontId, RichText, Stroke, Rounding, Margin, Align2, Pos2};
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use std::time::{Duration as StdDuration, Instant};

mod data;
mod lunar;
use data::{Db, Todo, Plan, dkey, parse_key, prio_color, PLAN_COLORS};
use lunar::{solar2lunar, festival_of, lunar_text, lunar_short, lunar_day, year_gz, animal, day_gz, term_around};

// ═══════════════════════════════════════════════════════════════
// 设计 Token —— 统一色板/圆角/间距，所有组件只从这里取色
// ═══════════════════════════════════════════════════════════════
fn c(hex: &str) -> Color32 { Color32::from_hex(hex).unwrap() }
/// 带透明度的颜色
fn ca(hex: &str, a: u8) -> Color32 { let col = c(hex); Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), a) }
/// 颜色混合（用于 hover / 过渡）
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (a.r() as f32 * (1.0 - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1.0 - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1.0 - t) + b.b() as f32 * t) as u8,
        255,
    )
}

mod theme {
    // 背景层次：底 → 面板 → 卡片
    pub const BG: &str     = "#0a0d14";
    pub const PANEL: &str  = "#111622";
    pub const CARD: &str   = "#171e2c";
    // 描边
    pub const STROKE: &str = "#283247";
    pub const STROKE_HI: &str = "#3b4863";
    // 文字层级（WCAG AA 对比度）
    pub const TEXT: &str   = "#f2f5fa";
    pub const TEXT2: &str  = "#d6dfea";
    pub const TEXT3: &str  = "#aab8cb";
    // 强调色（琥珀 + 青蓝 + 鼠尾草绿）
    pub const ACCENT: &str = "#f2a14b";
    pub const ACCENT2: &str = "#ffd9a8";
    pub const ACCENT_INK: &str = "#241503";
    pub const COOL: &str   = "#8fbfe4";
    pub const MINT: &str   = "#7fd0b4";
    pub const DONE: &str   = "#8593a8";
}
use theme::ACCENT_INK;

/// 圆角 token
mod radius {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 11.0;
    pub const LG: f32 = 15.0;
}

const WEEK: [&str;7] = ["日","一","二","三","四","五","六"];
const WEEK_FULL: [&str;7] = ["星期日","星期一","星期二","星期三","星期四","星期五","星期六"];

struct App {
    db: Db,
    today: NaiveDate,
    view: NaiveDate,
    selected: NaiveDate,

    // 表单状态
    add_prio: u8,
    add_text: String,
    add_open: bool,
    // 待办选中/双击编辑
    sel_todo: Option<u64>,
    edit_todo: Option<u64>,
    edit_text: String,
    edit_rect: Option<egui::Rect>,
    plan_name: String,
    plan_start: String,
    plan_end: String,
    plan_color: usize,
    plan_open: bool,

    // 右键菜单
    show_ctx: bool,
    ctx_pos: Pos2,
    ctx_kind: CtxKind,

    // 持久化
    pending_save: bool,
    last_save: Instant,

    // ── 动画状态（丝滑的核心）──
    /// 月份切换淡入进度 0→1
    month_anim: f32,
    /// 选中格脉冲 0→1
    select_pulse: f32,
    /// 表单展开进度 0→1
    form_anim: f32,
    /// 上次绘制时间，用于按帧推进动画
    last_frame: Instant,
}

#[derive(Clone, Copy, PartialEq)]
enum CtxKind { None, PlanZone, Calendar }

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);
        let today = Local::now().date_naive();
        Self {
            db: data::load(),
            view: NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap(),
            selected: today,
            today,
            add_prio: 2,
            add_text: String::new(),
            add_open: false,
            sel_todo: None,
            edit_todo: None,
            edit_text: String::new(),
            edit_rect: None,
            plan_name: String::new(),
            plan_start: String::new(),
            plan_end: String::new(),
            plan_color: 0,
            plan_open: false,
            show_ctx: false,
            ctx_pos: egui::pos2(0.0, 0.0),
            ctx_kind: CtxKind::None,
            pending_save: false,
            last_save: Instant::now(),
            month_anim: 1.0,
            select_pulse: 1.0,
            form_anim: 1.0,
            last_frame: Instant::now(),
        }
    }

    /// 统一推进所有动画，返回是否仍有动画在跑
    fn tick_anim(&mut self, dt: f32) -> bool {
        let mut active = false;
        if self.month_anim < 1.0 {
            self.month_anim = (self.month_anim + dt / 0.26).min(1.0);
            active = true;
        }
        if self.select_pulse < 1.0 {
            self.select_pulse = (self.select_pulse + dt / 0.32).min(1.0);
            active = true;
        }
        if self.form_anim < 1.0 {
            self.form_anim = (self.form_anim + dt / 0.22).min(1.0);
            active = true;
        }
        active
    }
    fn ease(t: f32) -> f32 { let t = t.clamp(0.0,1.0); 1.0 - (1.0-t)*(1.0-t)*(1.0-t) } // easeOutCubic

    fn save_later(&mut self) { self.pending_save = true; }

    fn add_todo(&mut self) {
        let text = self.add_text.trim().to_string();
        if text.is_empty() { return; }
        self.db.todos.push(Todo {
            id: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
            date: dkey(&self.selected), text, prio: self.add_prio, done: false,
        });
        self.add_text.clear();
        self.save_later();
    }

    fn open_plan_form(&mut self) {
        self.plan_open = true;
        self.form_anim = 0.0;
        self.plan_start = dkey(&self.selected);
        self.plan_end = dkey(&(self.selected + Duration::days(3)));
        self.plan_color = self.db.plans.len() % PLAN_COLORS.len();
    }
    fn open_add(&mut self) {
        if !self.add_open { self.form_anim = 0.0; }
        self.add_open = true;
    }

    fn save_plan(&mut self) {
        let name = if self.plan_name.trim().is_empty() { "未命名计划".into() } else { self.plan_name.trim().into() };
        let mut s = self.plan_start.clone();
        let mut e = self.plan_end.clone();
        if s.is_empty() || e.is_empty() { s = dkey(&self.selected); e = s.clone(); }
        if e < s { std::mem::swap(&mut s, &mut e); }
        self.db.plans.push(Plan {
            id: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
            name, color: PLAN_COLORS[self.plan_color % PLAN_COLORS.len()], start: s, end: e,
        });
        self.plan_name.clear();
        self.plan_open = false;
        self.save_later();
    }

    fn shift_month(&mut self, delta: i32) {
        let y = self.view.year();
        let m = self.view.month() as i32 + delta;
        let (ny, nm) = if m < 1 {(y-1,12)} else if m > 12 {(y+1,1)} else {(y,m)};
        self.view = NaiveDate::from_ymd_opt(ny, nm as u32, 1).unwrap();
        self.month_anim = 0.0;
    }
    fn goto_today(&mut self) {
        self.today = Local::now().date_naive();
        self.selected = self.today;
        self.view = NaiveDate::from_ymd_opt(self.today.year(), self.today.month(), 1).unwrap();
        self.month_anim = 0.0;
        self.select_pulse = 0.0;
    }
    fn select(&mut self, d: NaiveDate) {
        if d != self.selected { self.select_pulse = 0.0; }
        self.selected = d;
    }
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let candidates = [
        "C:/Windows/Fonts/NotoSansSC-VF.ttf",
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/Deng.ttf",
        "C:/Windows/Fonts/simhei.ttf",
    ];
    for p in candidates {
        if let Ok(bytes) = std::fs::read(p) {
            fonts.font_data.insert("cjk".into(), egui::FontData::from_owned(bytes).into());
            fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "cjk".into());
            fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "cjk".into());
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    // 排版层级：display / title / body / label（严格 4 级）
    style.text_styles.insert(egui::TextStyle::Small,     FontId::new(11.0, egui::FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Body,      FontId::new(13.0, egui::FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Button,    FontId::new(12.5,egui::FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Heading,   FontId::new(20.0, egui::FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Monospace, FontId::new(11.0, egui::FontFamily::Monospace));
    style.spacing.item_spacing = Vec2::new(6.0, 4.0);
    style.spacing.button_padding = Vec2::new(9.0, 4.0);
    let mut v = style.visuals.clone();
    v.dark_mode = true;
    v.window_fill = c(theme::PANEL);
    v.panel_fill = c(theme::BG);
    v.extreme_bg_color = c(theme::CARD);
    v.override_text_color = Some(c(theme::TEXT));
    // 按钮三态完整
    v.widgets.inactive.bg_fill = c(theme::CARD);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, c(theme::TEXT2));
    v.widgets.hovered.bg_fill = c(theme::STROKE);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, c(theme::ACCENT2));
    v.widgets.active.bg_fill = c(theme::ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.0, c(ACCENT_INK));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, c(theme::TEXT3));
    v.selection.bg_fill = ca(theme::ACCENT, 64);
    v.selection.stroke = Stroke::new(1.0, c(theme::ACCENT));
    v.window_rounding = Rounding::same(radius::MD);
    style.visuals = v;
    ctx.set_style(style);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 单一时间源：每帧只取一次，跨天自动更新
        let now = Local::now();
        let today = now.date_naive();
        if today != self.today {
            self.today = today;
            self.selected = today;
            self.view = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
            self.month_anim = 0.0;
        }

        // 按真实帧间隔推进动画（丝滑关键：与帧率解耦）
        let t = self.last_frame.elapsed().as_secs_f32();
        self.last_frame = Instant::now();
        let animating = self.tick_anim(t.min(0.1));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(c(theme::BG)).inner_margin(Margin::symmetric(16.0, 12.0)))
            .show(ctx, |ui| {
                let area = ui.available_rect_before_wrap();
                // 显式对称间距：左边距 = 中缝 = 右边距（由 CentralPanel margin 提供）
                let gap = 16.0f32;
                let left_w = 292.0f32.min(area.width() * 0.32).max(220.0);
                let left_rect = egui::Rect::from_min_size(area.min, egui::vec2(left_w, area.height()));
                // 右栏基准左边界（含之前的 +20 右移）
                let base_left = area.min.x + left_w + gap + 20.0;
                let base_full_w = area.max.x - base_left;
                // 右栏整体右移 50px（左右边界一起平移，宽度不变；右边界 clamp 防超出窗口被裁）
                let shift = 50.0f32;
                let right_left = base_left + shift;
                let right_r = (area.max.x - base_full_w / 14.0 + shift).min(area.max.x);
                let right_rect = egui::Rect::from_min_max(
                    Pos2::new(right_left, area.min.y),
                    Pos2::new(right_r, area.max.y),
                );
                // 左栏：绝对矩形 + 独立滚动
                ui.allocate_ui_at_rect(left_rect, |u| {
                    egui::Frame::none().fill(c(theme::PANEL)).rounding(Rounding::same(radius::LG))
                        .stroke(Stroke::new(1.0, c(theme::STROKE)))
                        .inner_margin(Margin::symmetric(18.0, 16.0)).show(u, |ui| {
                            egui::ScrollArea::vertical().id_source("left_sidebar_scroll")
                                .auto_shrink([false, false])
                                .show(ui, |ui| { ui.vertical(|ui| self.left_panel(ui, &now)); });
                        });
                });
                // 右栏：绝对矩形，内部三段式固定分区
                ui.allocate_ui_at_rect(right_rect, |u| {
                    egui::Frame::none().fill(c(theme::PANEL)).rounding(Rounding::same(radius::LG))
                        .stroke(Stroke::new(1.0, c(theme::STROKE)))
                        .inner_margin(Margin::symmetric(18.0, 16.0)).show(u, |ui| {
                            self.right_panel(ui);
                        });
                });
            });

        self.handle_ctx(ctx);
        self.handle_keys(ctx);

        // 编辑中左键点击编辑框外 -> 退出编辑（失焦已在渲染处保存）
        if self.edit_todo.is_some() {
            let clicked = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Primary));
            if clicked {
                let pos = ctx.input(|i| i.pointer.latest_pos());
                let outside = match (pos, self.edit_rect) {
                    (Some(p), Some(r)) => !r.expand(2.0).contains(p),
                    // edit_rect 为 None = 刚双击进入编辑、编辑框尚未绘制的那一帧，
                    // 此时绝不能把进入编辑的那次点击误判为"点击外部"，否则编辑状态会被立刻取消
                    _ => false,
                };
                if outside {
                    if let Some(id) = self.edit_todo {
                        let txt = self.edit_text.trim().to_string();
                        if !txt.is_empty() && !self.db.todos.iter().any(|x| x.id == id && x.text == txt) {
                            if let Some(item) = self.db.todos.iter_mut().find(|x| x.id == id) { item.text = txt; }
                            self.save_later();
                        }
                    }
                    self.edit_todo = None; self.edit_rect = None;
                }
            }
        }

        if self.pending_save && self.last_save.elapsed() > StdDuration::from_millis(250) {
            data::save(&self.db);
            self.pending_save = false;
            self.last_save = Instant::now();
        }

        // 重绘调度：动画期间立即重绘；否则精确等到下一秒（时钟丝滑且不空转）
        if animating {
            ctx.request_repaint();
        } else {
            let ms_until_sec = 1000u64.saturating_sub(now.timestamp_subsec_millis() as u64) + 16;
            ctx.request_repaint_after(StdDuration::from_millis(ms_until_sec));
        }
    }
}

impl App {
    // ═══════════ 左栏 ═══════════
    fn left_panel(&mut self, ui: &mut egui::Ui, now: &chrono::DateTime<Local>) {
        // ── 大日期（视觉焦点）──
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.today.day().to_string())
                .font(FontId::proportional(84.0)).color(c(theme::TEXT)));
            ui.vertical(|ui| {
                ui.add_space(10.0);
                ui.label(RichText::new(format!("{}月", self.today.month())).size(25.0).color(c(theme::ACCENT2)));
                ui.label(RichText::new(self.today.year().to_string()).size(11.0).color(c(theme::TEXT3)));
                ui.add_space(2.0);
                ui.label(RichText::new(WEEK_FULL[self.today.weekday().num_days_from_sunday() as usize])
                    .size(11.5).strong().color(c(ACCENT_INK)).background_color(c(theme::ACCENT)));
            });
        });
        let l = solar2lunar(&self.today);
        ui.add_space(2.0);
        ui.label(RichText::new(lunar_text(&l)).size(13.0).color(c(theme::TEXT2)));
        ui.horizontal_wrapped(|ui| {
            if let Some(f) = festival_of(&self.today, &l) { tag(ui, &f, TagKind::Warm); }
            let ta = term_around(&self.today);
            if let Some((name, _)) = &ta.last { tag(ui, &format!("节气·{}", name), TagKind::Cool); }
            if let Some((name, d)) = &ta.next {
                tag(ui, &format!("{}·还有{}天", name, (*d - self.today).num_days()), TagKind::Cool);
            }
        });
        section_rule(ui);

        // ── 时钟 + 今日进度 ──
        ui.label(RichText::new(format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second()))
            .font(FontId::monospace(38.0)).color(c(theme::TEXT)));
        let pct = (now.hour() * 3600 + now.minute() * 60 + now.second()) as f32 / 86400.0;
        ui.add(egui::ProgressBar::new(pct).desired_height(4.0).fill(c(theme::ACCENT)));
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("时间进度 {:.1}%", pct * 100.0)).size(11.5).color(c(theme::TEXT3)));
            let dow = now.weekday().num_days_from_sunday();
            let wl = if dow == 0 || dow == 6 { "周末 · 好好休息".into() }
                     else { format!("距周末 {} 天", 6 - dow) };
            ui.label(RichText::new(wl).size(11.5).color(c(theme::TEXT3)));
        });
        section_rule(ui);

        // ── 本月统计（宽度一次性算好，三卡等宽）──
        ui.label(RichText::new("MONTH · 本月计划").size(11.0).color(c(theme::TEXT3)));
        let ym = self.view.format("%Y-%m").to_string();
        let total = self.db.todos.iter().filter(|t| t.date.starts_with(&ym)).count();
        let done = self.db.todos.iter().filter(|t| t.date.starts_with(&ym) && t.done).count();
        let open = total - done;
        let card_w = (ui.available_width() - 12.0) / 3.0;
        ui.horizontal(|ui| {
            stat_card(ui, &total.to_string(), "总任务", c(theme::TEXT), card_w);
            stat_card(ui, &done.to_string(), "已完成", c(theme::ACCENT), card_w);
            stat_card(ui, &open.to_string(), "进行中", c(theme::COOL), card_w);
        });
        let rate = if total > 0 { done as f32 / total as f32 } else { 0.0 };
        ui.add(egui::ProgressBar::new(rate).desired_height(5.0).fill(c(theme::MINT)));
        ui.label(RichText::new(format!("完成率 {:.0}%", rate * 100.0)).size(11.0).color(c(theme::TEXT3)));
        section_rule(ui);

        // ── 阶段计划 ──
        ui.horizontal(|ui| {
            section_title(ui, "阶段计划");
            ui.add_space(ui.available_width());
            if ui.small_button("＋ 新建").clicked() { self.open_plan_form(); }
        });
        let zone = ui.allocate_response(Vec2::new(ui.available_width(), 4.0), egui::Sense::click());
        if zone.secondary_clicked() { self.ctx(CtxKind::PlanZone, ui_pos(ui)); }
        for p in self.db.plans.clone() {
            let rg = format!("{}→{}·{}天", short(&p.start), short(&p.end),
                             (parse_key(&p.end) - parse_key(&p.start)).num_days() + 1);
            // 整行 hover 高亮：在 horizontal 内用 rect_contains_pointer（坐标一致）
            ui.horizontal(|ui| {
                let row_rect = ui.max_rect();
                if ui.rect_contains_pointer(row_rect) {
                    ui.painter().rect_filled(row_rect, Rounding::same(radius::SM), ca(theme::ACCENT, 42));
                }
                ui.add(egui::Label::new(RichText::new("■").color(rgb(p.color)).size(11.0)));
                if ui.add(egui::Label::new(RichText::new(&p.name).size(12.0).color(c(theme::TEXT2)))).clicked() {
                    let d = parse_key(&p.start);
                    self.select(d);
                    self.view = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
                }
                ui.label(RichText::new(rg).size(10.0).color(c(theme::TEXT3)));
                if ui.small_button("×").clicked() { self.db.plans.retain(|x| x.id != p.id); self.save_later(); }
            });
        }
        // 表单展开：淡入背景（form_anim 驱动）
        if self.plan_open { self.plan_form(ui); }
        section_rule(ui);

        // ── 未来七天 ──
        section_title(ui, "未来七天");
        let mut items: Vec<(NaiveDate, String, u8)> = Vec::new();
        for i in 0..7 {
            let d = self.today + Duration::days(i);
            for t in self.db.todos_of(&dkey(&d)) { if !t.done { items.push((d, t.text.clone(), t.prio)); } }
        }
        if items.is_empty() {
            ui.label(RichText::new("未来七天暂无未完成安排").size(12.0).color(c(theme::TEXT3)));
        }
        for (d, text, prio) in items.into_iter() {
            ui.horizontal(|ui| {
                // 整行 hover 高亮（坐标一致）
                let row_rect = ui.max_rect();
                if ui.rect_contains_pointer(row_rect) {
                    ui.painter().rect_filled(row_rect, Rounding::same(radius::SM), ca(theme::ACCENT, 42));
                }
                let day_label = if d == self.today { "今天".to_string() } else { d.format("%m/%d").to_string() };
                // 用粗体亮色强调日期，去掉橙色背景块（避免与行 hover 高亮撞色）
                ui.add(egui::Label::new(RichText::new(format!("{}  ", day_label)).size(11.0)
                    .strong().color(c(theme::ACCENT2))));
                let r = ui.allocate_response(Vec2::new(3.0, 14.0), egui::Sense::hover()).rect;
                ui.painter().rect_filled(r, Rounding::same(1.5), rgb(prio_color(prio)));
                if ui.add(egui::Label::new(RichText::new(&text).size(12.0).color(c(theme::TEXT2)))).clicked() {
                    self.select(d);
                }
            });
        }
    }

    fn plan_form(&mut self, ui: &mut egui::Ui) {
        let t = Self::ease(self.form_anim);
        let bg = mix(c(theme::CARD), c(theme::ACCENT), (1.0 - t) * 0.28);
        egui::Frame::none().fill(bg).rounding(Rounding::same(radius::SM))
            .inner_margin(Margin::same(10.0)).show(ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.plan_name)
                    .hint_text("计划名称，如「项目冲刺」").desired_width(ui.available_width()));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.plan_start).hint_text("起始 YYYY-MM-DD").desired_width(104.0));
                    ui.label(RichText::new("→").color(c(theme::TEXT3)));
                    ui.add(egui::TextEdit::singleline(&mut self.plan_end).hint_text("结束 YYYY-MM-DD").desired_width(104.0));
                });
                ui.horizontal_wrapped(|ui| {
                    for (i, col) in PLAN_COLORS.iter().enumerate() {
                        let on = i == self.plan_color;
                        let label = RichText::new(if on { "◉" } else { "●" }).color(rgb(*col)).size(if on {18.0} else {15.0});
                        if ui.add(egui::Button::new(label).frame(false)).clicked() { self.plan_color = i; }
                    }
                });
                if ui.button("创建计划").clicked() { self.save_plan(); }
            });
    }

    // ═══════════ 右栏 ═══════════
    fn right_panel(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();

        // ① 头部（固定高度，不参与滚动）
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}年{}月", self.view.year(), self.view.month()))
                .font(FontId::proportional(28.0)).color(c(theme::TEXT)));
            let ly = solar2lunar(&NaiveDate::from_ymd_opt(self.view.year(), self.view.month(), 15).unwrap());
            ui.label(RichText::new(format!("{}年·{}", year_gz(&ly), animal(&ly))).size(10.5).color(c(theme::TEXT3)));
            // 翻页按钮贴内容区右缘 = 日历周六列右缘（right_to_left 保证不被裁）
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_sized([28.0, 26.0], egui::Button::new("›")).clicked() { self.shift_month(1); }
                if ui.add_sized([58.0, 26.0], egui::Button::new("今天")).clicked() { self.goto_today(); }
                if ui.add_sized([28.0, 26.0], egui::Button::new("‹")).clicked() { self.shift_month(-1); }
            });
        });
        // ◀ 单独绑定（上面占位，这里补事件不便，统一在下方处理）
        // 图例
        ui.horizontal_wrapped(|ui| {
            legend(ui, rgb(prio_color(1)), "高"); legend(ui, rgb(prio_color(2)), "中");
            legend(ui, rgb(prio_color(3)), "低"); legend(ui, c(theme::DONE), "已完成");
            ui.label(RichText::new("★ 重点日").size(11.0).color(c(theme::ACCENT2)));
        });
        section_rule(ui);

        // ② 月历区：高度 = 可用高度的 58%，留足底部详情
        let avail_h = ui.available_height();
        let cal_h = (avail_h * 0.64).clamp(200.0, (avail_h - 170.0).max(200.0));
        let cal_top = ui.cursor().top();
        self.calendar(ui, width, cal_h);
        let cal_bottom = ui.cursor().top();

        // 月份切换淡入遮罩（不拦截事件，只画在 painter 层）
        if self.month_anim < 1.0 {
            let a = ((1.0 - Self::ease(self.month_anim)) * 90.0) as u8;
            let r = egui::Rect::from_min_max(Pos2::new(area_x(ui), cal_top), Pos2::new(area_right(ui), cal_bottom));
            ui.painter().rect_filled(r, Rounding::same(radius::MD), Color32::from_rgba_unmultiplied(10, 13, 20, a));
        }

        section_rule(ui);

        // ③ 当日详情：独立滚动，任务再多也能看
        let remain = ui.available_height().max(110.0);
        egui::ScrollArea::vertical().id_source("day_agenda_scroll")
            .max_height(remain).auto_shrink([false, false])
            .show(ui, |ui| self.agenda_panel(ui));
    }

    fn calendar(&mut self, ui: &mut egui::Ui, width: f32, cal_h: f32) {
        let days = WEEK;
        let gap = 6.0f32;
        let cell_w = ((width - gap * 6.0) / 7.0).max(38.0);
        // 星期表头
        ui.horizontal(|ui| {
            for (i, d) in days.iter().enumerate() {
                let col = if i == 0 || i == 6 { c(theme::ACCENT) } else { c(theme::TEXT3) };
                ui.allocate_ui(Vec2::new(cell_w, 16.0), |ui| {
                    ui.centered_and_justified(|ui| { ui.label(RichText::new(*d).size(11.5).color(col)); });
                });
            }
        });

        let y = self.view.year();
        let m = self.view.month();
        let first_wd = NaiveDate::from_ymd_opt(y, m, 1).unwrap().weekday().num_days_from_sunday() as i32;
        let cell_h = (cal_h / 6.0).clamp(38.0, 92.0);
        let mut ctx_cell: Option<NaiveDate> = None;

        for row in 0..6 {
            ui.horizontal_top(|ui| {
                for _col in 0..7 {
                    let idx = row * 7 + _col;
                    let d = NaiveDate::from_ymd_opt(y, m, 1).unwrap() + Duration::days(idx - first_wd as i64);
                    let k = dkey(&d);
                    let l = solar2lunar(&d);
                    let fest = festival_of(&d, &l);
                    let todos = self.db.todos_of(&k);
                    let open = todos.iter().filter(|t| !t.done).count();
                    let plans = self.db.plans_on(&k);
                    let marked = self.db.marks.get(&k).copied().unwrap_or(false);
                    let is_today = d == self.today;
                    let sel = d == self.selected;
                    let out = d.month() != m;

                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(cell_w, cell_h), egui::Sense::click());
                    let hovered = resp.hovered();
                    // 背景层次：选中 > hover > 今天 > 普通
                    let bg = if sel {
                        let pulse = 1.0 - Self::ease(self.select_pulse);
                        mix(c(theme::CARD), c(theme::ACCENT), 0.18 + pulse * 0.22)
                    } else if hovered { c(theme::STROKE) }
                      else if is_today { mix(c(theme::CARD), c(theme::ACCENT), 0.10) }
                      else { c(theme::CARD) };
                    ui.painter().rect_filled(rect, Rounding::same(radius::SM), bg);
                    // 描边层次
                    let border = if sel { c(theme::ACCENT) }
                                 else if marked { ca(theme::ACCENT, 150) }
                                 else if hovered { c(theme::STROKE_HI) }
                                 else { c(theme::STROKE) };
                    ui.painter().rect_stroke(rect, Rounding::same(radius::SM),
                        Stroke::new(if sel || marked { 1.5 } else { 0.6 }, border));
                    // hover 顶部高光条（微交互）
                    if hovered && !sel {
                        ui.painter().line_segment(
                            [rect.left_top() + Vec2::new(9.0, 0.0), rect.right_top() - Vec2::new(9.0, 0.0)],
                            Stroke::new(1.5, ca(theme::ACCENT, 130)),
                        );
                    }
                    // 阶段计划标记：小型彩色方块，水平并排在格子底部居中（不遮挡两侧信息）
                    let n = plans.len().min(5);
                    if n > 0 {
                        let sz = 6.0f32;
                        let gap = 3.0f32;
                        let total_w = n as f32 * sz + (n as f32 - 1.0) * gap;
                        let start_x = rect.center().x - total_w / 2.0;
                        let by = rect.bottom() - sz - 3.0;
                        for (i, p) in plans.iter().take(5).enumerate() {
                            let rp = egui::Rect::from_min_size(
                                Pos2::new(start_x + i as f32 * (sz + gap), by),
                                Vec2::new(sz, sz));
                            ui.painter().rect_filled(rp, Rounding::same(1.5), rgb(p.color));
                        }
                    }
                    let alpha = if out { 0.42 } else { 1.0 };
                    let x = rect.left() + 6.0;
                    let ty = rect.top() + 4.0;
                    let day_col = if is_today { c(ACCENT_INK) } else if out { ca(theme::TEXT, 110) } else { c(theme::TEXT) };
                    if is_today { ui.painter().circle_filled(Pos2::new(x + 10.0, ty + 10.0), 10.0, c(theme::ACCENT)); }
                    ui.painter().text(Pos2::new(x, ty), Align2::LEFT_TOP, d.day().to_string(),
                        FontId::proportional(13.0), day_col);
                    let mini = fest.clone().unwrap_or_else(|| if l.day == 1 { lunar_short(&l) } else { String::new() });
                    if !mini.is_empty() {
                        let mc = if fest.is_some() { c(theme::ACCENT2) } else { c(theme::TEXT3) };
                        ui.painter().text(Pos2::new(x + 24.0, ty + 1.0), Align2::LEFT_TOP, &mini,
                            FontId::proportional(10.0), Color32::from_rgba_unmultiplied(mc.r(), mc.g(), mc.b(), (255.0 * alpha) as u8));
                    }
                    if marked {
                        let star_pos = Pos2::new(rect.right() - 16.0, ty);
                        ui.painter().text(star_pos, Align2::LEFT_TOP, "★",
                            FontId::proportional(11.0), c(theme::ACCENT));
                        // 悬停星标显示备注
                        let star_rect = egui::Rect::from_min_size(star_pos, Vec2::new(13.0, 14.0));
                        if ui.rect_contains_pointer(star_rect) {
                            let note = self.db.mark_notes.get(&k).map(|x| x.as_str()).unwrap_or("");
                            let tip = if note.is_empty() { "重点日（无备注）".to_string() }
                                      else { note.to_string() };
                            egui::show_tooltip_text(ui.ctx(), ui.layer_id(), egui::Id::new(("mark_note", &k)), tip);
                        }
                    }
                    // 任务条：更高、左侧色条 + 右缩进，避免文字压线
                    for (i, t) in todos.iter().take(2).enumerate() {
                        let col = if t.done { c(theme::DONE) } else { c(theme::TEXT2) };
                        let ypos = ty + 24.0 + i as f32 * 18.0;
                        if ypos + 15.0 < rect.bottom() - 7.0 {
                            let tr = egui::Rect::from_min_size(
                                Pos2::new(rect.left() + 5.0, ypos), Vec2::new(rect.width() - 10.0, 15.0));
                            ui.painter().rect_filled(tr, Rounding::same(4.0),
                                ca(theme::BG, if out { 90 } else { 175 }));
                            ui.painter().text(Pos2::new(rect.left() + 10.0, ypos + 2.0), Align2::LEFT_TOP,
                                trunc(&t.text, 9), FontId::proportional(11.0), col);
                            ui.painter().rect_filled(
                                egui::Rect::from_min_size(Pos2::new(rect.left() + 5.0, ypos), Vec2::new(2.5, 15.0)),
                                Rounding::same(1.0),
                                if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) });
                        }
                    }
                    if todos.len() > 2 {
                        ui.painter().text(Pos2::new(rect.left() + 7.0, rect.bottom() - 15.0), Align2::LEFT_TOP,
                            format!("+{} 项", todos.len() - 2), FontId::proportional(9.5), c(theme::TEXT3));
                    } else if open > 0 {
                        ui.painter().text(Pos2::new(rect.right() - 6.0, rect.bottom() - 4.0), Align2::RIGHT_BOTTOM,
                            format!("{}项", open), FontId::proportional(9.5), c(theme::TEXT3));
                    }
                    if resp.clicked() { self.select(d); }
                    if resp.secondary_clicked() { ctx_cell = Some(d); }
                }
            });
        }
        if let Some(d) = ctx_cell { self.select(d); self.ctx(CtxKind::Calendar, ui_pos(ui)); }
    }

    fn agenda_panel(&mut self, ui: &mut egui::Ui) {
        let l = solar2lunar(&self.selected);
        let fest = festival_of(&self.selected, &l);
        let title = format!("{}月{}日 · {}", self.selected.month(), self.selected.day(),
                            WEEK_FULL[self.selected.weekday().num_days_from_sunday() as usize]);
        let meta = format!("农历 {}{}{} · {}", if l.is_leap { "闰" } else { "" },
                           lunar_short(&l), lunar_day(&l), day_gz(&self.selected));
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).font(FontId::proportional(19.0)).color(c(theme::TEXT)));
            let meta_line = match &fest { Some(f) => format!("{} · {}", meta, f), None => meta.clone() };
            ui.label(RichText::new(meta_line).size(11.0).color(c(theme::TEXT3)));
            // 详情中显示重点日备注（恒定一个 label 占位，避免备注从空变非空时控件数量变化导致输入框失焦）
            let dk_sel = dkey(&self.selected);
            let is_marked = self.db.marks.get(&dk_sel).copied().unwrap_or(false);
            let note_disp = if is_marked {
                match self.db.mark_notes.get(&dk_sel) {
                    Some(n) if !n.is_empty() => format!("★ {}", n),
                    _ => String::new(),
                }
            } else { String::new() };
            ui.label(RichText::new(note_disp).size(11.0).color(c(theme::ACCENT2)));
            // 标记按钮贴内容区右缘 = 日历周六列右缘（right_to_left 保证不被裁）
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let k = dkey(&self.selected);
                let mk = self.db.marks.get(&k).copied().unwrap_or(false);
                if ui.add(egui::Button::new(RichText::new(if mk { "★ 已标记" } else { "☆ 标记重点日" }).size(11.5))).clicked() {
                    if mk {
                        self.db.marks.remove(&k);
                        self.db.mark_notes.remove(&k);
                    } else {
                        self.db.marks.insert(k.clone(), true);
                        self.db.mark_notes.entry(k.clone()).or_default();
                    }
                    self.save_later();
                }
                // 已标记时显示备注输入框
                if mk {
                    let note = self.db.mark_notes.entry(k.clone()).or_default();
                    if ui.add(egui::TextEdit::singleline(note)
                        .id_source(("mark_note_edit", k.clone()))
                        .hint_text("重点日备注…").desired_width(180.0)).changed() {
                        self.save_later();
                    }
                }
            });
        });

        // 添加栏（淡入）
        if self.add_open {
            let t = Self::ease(self.form_anim);
            let bg = mix(c(theme::CARD), c(theme::ACCENT), (1.0 - t) * 0.22);
            egui::Frame::none().fill(bg).rounding(Rounding::same(radius::SM))
                .inner_margin(Margin::symmetric(6.0, 4.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for p in [1u8, 2, 3] {
                            let label = RichText::new(format!("{} {}", if self.add_prio == p { "◉" } else { "●" },
                                ["", "高", "中", "低"][p as usize])).color(rgb(prio_color(p))).size(11.0);
                            if ui.add(egui::Button::new(label).frame(false)).clicked() { self.add_prio = p; }
                        }
                        ui.add(egui::TextEdit::singleline(&mut self.add_text)
                            .hint_text("写下计划事项，回车添加…").desired_width((ui.available_width() - 56.0).max(40.0)));
                        if ui.button("添加").clicked() { self.add_todo(); }
                    });
                });
        }

        let k = dkey(&self.selected);
        let todos = self.db.todos_of(&k);
        let mut pending_toggle: Vec<(u64, bool)> = Vec::new();
        let mut pending_del: Vec<u64> = Vec::new();
        let mut pending_edit: Vec<(u64, String)> = Vec::new();
        if todos.is_empty() {
            ui.label(RichText::new("这一天还没有计划。右键面板打开添加栏，选好优先级写入安排；连续多日的安排请用左侧「阶段计划」创建。")
                .size(12.0).color(c(theme::TEXT3)));
        } else {
            for t in todos {
                let mut done = t.done;
                let is_sel = self.sel_todo == Some(t.id);
                let is_editing = self.edit_todo == Some(t.id);
                ui.horizontal(|ui| {
                    let row_rect = ui.max_rect();
                    // 高亮色跟随优先级/完成状态：高红 中橙 低蓝 已完成灰
                    let hl_base = if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) };
                    if is_sel {
                        ui.painter().rect_filled(row_rect, Rounding::same(radius::SM),
                            Color32::from_rgba_unmultiplied(hl_base.r(), hl_base.g(), hl_base.b(), 60));
                    } else if ui.rect_contains_pointer(row_rect) {
                        ui.painter().rect_filled(row_rect, Rounding::same(radius::SM),
                            Color32::from_rgba_unmultiplied(hl_base.r(), hl_base.g(), hl_base.b(), 30));
                    }
                    if ui.checkbox(&mut done, "").clicked() { pending_toggle.push((t.id, done)); }
                    let r = ui.allocate_response(Vec2::new(3.0, 17.0), egui::Sense::hover()).rect;
                    ui.painter().rect_filled(r, Rounding::same(1.0),
                        if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) });
                    if is_editing {
                        // 双击编辑：单行输入框，回车或失焦保存
                        let resp = ui.add(egui::TextEdit::singleline(&mut self.edit_text)
                            .desired_width((row_rect.width() - 130.0).max(80.0))
                            .hint_text("编辑事项…"));
                        self.edit_rect = Some(resp.rect);
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if enter {
                            let txt = self.edit_text.trim().to_string();
                            if !txt.is_empty() && txt != t.text { pending_edit.push((t.id, txt)); }
                            self.edit_todo = None;
                            self.edit_rect = None;
                        }
                    } else {
                        let style = if t.done {
                            RichText::new(&t.text).size(13.0).color(c(theme::TEXT3)).strikethrough()
                        } else {
                            RichText::new(&t.text).size(13.0).color(c(theme::TEXT2))
                        };
                        // 双击编辑：allocate_ui_with_layout 撑满内容宽度，左对齐 + 双击命中
                        // （不用 add_sized，它内部是 centered_and_justified 会让文字居中）
                        let content_w = (row_rect.width() - 70.0).max(60.0);
                        let resp = ui.allocate_ui_with_layout(
                            Vec2::new(content_w, 20.0),
                            egui::Layout::left_to_right(egui::Align::LEFT),
                            |ui| ui.add(egui::Label::new(style).sense(egui::Sense::click())),
                        ).inner;
                        if resp.clicked() { self.sel_todo = Some(t.id); }
                        if resp.double_clicked() {
                            self.edit_todo = Some(t.id);
                            self.edit_text = t.text.clone();
                            self.sel_todo = Some(t.id);
                            self.edit_rect = None;
                        }
                    }
                    // 优先级 + 删除键固定在行右缘（right_to_left 保证每行同一垂直列对齐）
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").clicked() { pending_del.push(t.id); }
                        ui.label(RichText::new(["", "高", "中", "低"][t.prio as usize]).size(9.5)
                            .color(if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) }));
                    });
                });
            }
        }
        for (id, val) in pending_toggle {
            if let Some(item) = self.db.todos.iter_mut().find(|x| x.id == id) { item.done = val; }
            self.save_later();
        }
        for id in pending_del { self.db.todos.retain(|x| x.id != id); self.save_later(); }
        for (id, txt) in pending_edit {
            if let Some(item) = self.db.todos.iter_mut().find(|x| x.id == id) { item.text = txt; }
            self.edit_todo = None;
            self.save_later();
        }
        ui.add_space(2.0);
        ui.label(RichText::new("右键打开编辑 · N 添加待办 · P 新建阶段计划 · Esc 收起").size(10.5).color(c(theme::TEXT3)));
    }

    fn ctx(&mut self, kind: CtxKind, pos: Pos2) {
        self.ctx_kind = kind;
        self.ctx_pos = pos;
        self.show_ctx = true;
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                self.show_ctx = false; self.add_open = false; self.plan_open = false;
                self.edit_todo = None; self.sel_todo = None; self.edit_rect = None;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::N) { self.open_add(); }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::P) { self.open_plan_form(); }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) { self.shift_month(-1); }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) { self.shift_month(1); }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::T) { self.goto_today(); }
        });
    }

    fn handle_ctx(&mut self, ctx: &egui::Context) {
        if !self.show_ctx { return; }
        // 记录本帧左键点击位置：点菜单外即关闭
        let click_pos = ctx.input(|i| {
            if i.pointer.button_clicked(egui::PointerButton::Primary) {
                i.pointer.latest_pos()
            } else {
                None
            }
        });
        let kind = self.ctx_kind;
        let pos = self.ctx_pos;
        let mut close = false;
        let resp = egui::Window::new("ctx").id(egui::Id::new("ctx_menu")).fixed_pos(pos)
            .collapsible(false).resizable(false).title_bar(false)
            .frame(egui::Frame::none().fill(c("#1a2130")).rounding(Rounding::same(radius::SM))
                .stroke(Stroke::new(1.0_f32, c(theme::STROKE_HI))).inner_margin(Margin::same(5.0)))
            .show(ctx, |ui| {
                match kind {
                    CtxKind::PlanZone => {
                        ui.label(RichText::new("阶段计划").size(10.0).color(c(theme::TEXT3)));
                        if ctx_btn(ui, "＋ 新建阶段计划") { self.open_plan_form(); close = true; }
                        if ctx_btn(ui, "回到今天") { self.goto_today(); close = true; }
                    }
                    CtxKind::Calendar => {
                        ui.label(RichText::new("计划日历").size(10.0).color(c(theme::TEXT3)));
                        if ctx_btn(ui, "＋ 添加待办事项") { self.open_add(); close = true; }
                        if ctx_btn(ui, "★ 标记 / 取消重点日") {
                            let kk = dkey(&self.selected);
                            if self.db.marks.remove(&kk).is_none() {
                                self.db.marks.insert(kk.clone(), true);
                                self.db.mark_notes.entry(kk).or_default();
                            } else {
                                self.db.mark_notes.remove(&kk);
                            }
                            self.save_later(); close = true;
                        }
                        if ctx_btn(ui, "回到今天") { self.goto_today(); close = true; }
                    }
                    CtxKind::None => {}
                }
            });
        // 左键点击菜单以外区域 -> 关闭
        if let Some(cp) = click_pos {
            match &resp {
                Some(r) => { if !r.response.rect.contains(cp) { close = true; } }
                None => { close = true; }
            }
        }
        if close { self.show_ctx = false; }
    }
}

// ═══════════════════════════════════════════════════════════════
// 组件助手
// ═══════════════════════════════════════════════════════════════
fn section_rule(ui: &mut egui::Ui) {
    ui.add_space(7.0);
    ui.separator();
    ui.add_space(7.0);
}
fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).font(FontId::proportional(17.0)).color(c(theme::TEXT)));
}
enum TagKind { Warm, Cool }
fn tag(ui: &mut egui::Ui, text: &str, kind: TagKind) {
    let (fg, bg) = match kind {
        TagKind::Warm => (c(theme::ACCENT2), ca(theme::ACCENT, 26)),
        TagKind::Cool => (c(theme::COOL), ca(theme::COOL, 24)),
    };
    ui.add(egui::Label::new(RichText::new(format!(" {} ", text)).size(11.0).color(fg).background_color(bg)));
}
fn stat_card(ui: &mut egui::Ui, n: &str, l: &str, col: Color32, w: f32) {
    ui.allocate_ui(Vec2::new(w, 54.0), |ui| {
        egui::Frame::none().fill(c(theme::CARD)).rounding(Rounding::same(radius::SM))
            .stroke(Stroke::new(0.6_f32, c(theme::STROKE)))
            .inner_margin(Margin::symmetric(9.0, 8.0)).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(n).font(FontId::proportional(26.0)).color(col));
                    ui.label(RichText::new(l).size(10.5).color(c(theme::TEXT3)));
                });
            });
    });
}
fn legend(ui: &mut egui::Ui, col: Color32, t: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(RichText::new("■").color(col).size(10.0)));
        ui.label(RichText::new(t).size(11.0).color(c(theme::TEXT2)));
    });
}
fn ctx_btn(ui: &mut egui::Ui, label: &str) -> bool {
    // 保留按钮边框：egui 默认 hover 态用 hovered.bg_fill 高亮
    ui.add(egui::Button::new(RichText::new(label).size(12.5).color(c(theme::TEXT2)))
        .min_size(Vec2::new(170.0, 0.0))).clicked()
}
fn ui_pos(ui: &egui::Ui) -> Pos2 { ui.ctx().pointer_latest_pos().unwrap_or(egui::pos2(120.0, 120.0)) }
fn area_x(ui: &egui::Ui) -> f32 { ui.clip_rect().left() }
fn area_right(ui: &egui::Ui) -> f32 { ui.clip_rect().right() }
fn rgb(col: [f32; 3]) -> Color32 {
    Color32::from_rgb((col[0] * 255.0) as u8, (col[1] * 255.0) as u8, (col[2] * 255.0) as u8)
}
fn short(k: &str) -> String {
    let p: Vec<&str> = k.split('-').collect();
    format!("{}/{}", p[1].parse::<u32>().unwrap_or(0), p[2].parse::<u32>().unwrap_or(0))
}
fn trunc(s: &str, n: usize) -> String {
    let ch: Vec<char> = s.chars().collect();
    if ch.len() > n { format!("{}…", ch[..n].iter().collect::<String>()) } else { s.to_string() }
}

fn main() {
    // 嵌入应用图标（128x128 RGBA，非预乘 alpha）
    let icon = egui::IconData {
        rgba: include_bytes!("icon.rgba").to_vec(),
        width: 128,
        height: 128,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1420.0, 900.0])
            .with_min_inner_size([1080.0, 680.0])
            .with_maximized(true)
            .with_title("Planum · 计划日历")
            .with_icon(std::sync::Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native("Planum · 计划日历", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
        .expect("failed to launch Planum");
}
