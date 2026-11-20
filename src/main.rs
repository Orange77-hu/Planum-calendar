#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Planum · 计划日历 —— 纯 Rust / egui 原生界面（v2 重设计）
//! 设计目标：流畅（不卡顿）、丝滑（过渡与微交互）、正确（零裁切/零挤压/零错位）
use eframe::egui;
use egui::{Color32, Vec2, FontId, RichText, Stroke, Rounding, Margin, Align2, Pos2};
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use std::time::{Duration as StdDuration, Instant};

// Win32：DWM 把原生标题栏染成主面板同色，视觉上像内嵌（拖拽/双击/缩放仍全原生）
#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: isize, attr: u32, pv: *const core::ffi::c_void, cb: u32) -> i32;
}
#[cfg(target_os = "windows")]
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
#[cfg(target_os = "windows")]
const DWMWA_CAPTION_COLOR: u32 = 35;

/// 从 eframe Frame 取 Win32 HWND
#[cfg(target_os = "windows")]
fn frame_hwnd(frame: &eframe::Frame) -> Option<isize> {
    use raw_window_handle::{RawWindowHandle, HasWindowHandle};
    match frame.window_handle().ok()?.as_ref() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}
/// 原生标题栏染成主面板底色（一次性）
#[cfg(target_os = "windows")]
fn tint_titlebar(hwnd: isize, bg: Color32) {
    unsafe {
        let dark: i32 = 1;
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as *const _, 4);
        // DWM 要 COLORREF: 0x00BBGGRR
        let cref: u32 = ((bg.b() as u32) << 16) | ((bg.g() as u32) << 8) | (bg.r() as u32);
        DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &cref as *const _ as *const _, 4);
    }
}

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
    // 原生标题栏 DWM 染色（一次性）
    dwm_applied: bool,
    db: Db,
    today: NaiveDate,
    view: NaiveDate,
    selected: NaiveDate,

    // 表单状态
    add_prio: u8,
    add_text: String,
    add_open: bool,
    // 打开表单后自动聚焦输入框（首帧 request_focus 一次）
    focus_add: bool,
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
    // 阶段计划编辑
    edit_plan_id: Option<u64>,
    focus_plan: bool,
    // 日期跳转
    jump_open: bool,
    focus_jump: bool,
    jump_text: String,
    // 导入导出
    io_open: bool,
    io_scope: IoScope,
    io_format: IoFormat,
    io_date: String,
    io_msg: String,

    // 周/月视图切换（false=月视图, true=周视图）
    week_view: bool,
    // 待办添加栏时间段输入
    add_start: String,
    add_end: String,
    // 课程表单
    course_open: bool,
    course_edit_id: Option<u64>,
    course_name: String,
    course_wd: u8,
    course_start: String,
    course_end: String,
    course_place: String,
    course_start_date: String,
    course_end_date: String,
    course_color: [f32; 3],
    course_slot_idx: usize,
    focus_course: bool,
    // 时间段设置面板
    slot_open: bool,
    slot_draft: Vec<(String, String)>,

    // 右键菜单
    show_ctx: bool,
    ctx_pos: Pos2,
    ctx_kind: CtxKind,

    // 持久化
    pending_save: bool,
    last_save: Instant,
    // 窗口几何记忆：每帧记录当前窗口状态，save 时写盘，new 时读取并应用
    win_pos: Pos2,
    win_size: Vec2,
    win_max: bool,
    win_applied: bool,

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
enum CtxKind { None, PlanZone, Calendar, Course(u64) }

/// 导入/导出范围
#[derive(Clone, Copy, PartialEq)]
enum IoScope { Day, Week, Month, Year }
impl IoScope {
    fn label(self) -> &'static str {
        match self { IoScope::Day => "当日", IoScope::Week => "本周", IoScope::Month => "本月", IoScope::Year => "本年" }
    }
}

/// 导出格式
#[derive(Clone, Copy, PartialEq)]
enum IoFormat { Json, Markdown }

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        setup_style(&cc.egui_ctx);
        let today = Local::now().date_naive();
        // 读取上次窗口几何（x,y,w,h,max），默认给一组合理值
        let (mut wp, mut ws, mut wm) = (Pos2::new(80.0, 60.0), Vec2::new(1420.0, 900.0), false);
        if let Some(st) = cc.storage {
            if let Some(raw) = st.get_string("win_geom") {
                let p: Vec<&str> = raw.split(',').collect();
                if p.len() == 5 {
                    if let (Ok(x), Ok(y), Ok(w), Ok(h)) = (
                        p[0].parse::<f32>(), p[1].parse::<f32>(), p[2].parse::<f32>(), p[3].parse::<f32>()) {
                        wp = Pos2::new(x, y); ws = Vec2::new(w, h); wm = p[4] == "1";
                    }
                }
            }
        }
        let db = data::load();
        let start_week = db.default_week_view;
        // 周视图启动定位到"今天所在周"，月视图定位到月初
        let start_view = if start_week { today }
                         else { NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap() };
        Self {
            dwm_applied: false,
            db,
            view: start_view,
            selected: today,
            today,
            add_prio: 2,
            add_text: String::new(),
            add_open: false,
            focus_add: false,
            sel_todo: None,
            edit_todo: None,
            edit_text: String::new(),
            edit_rect: None,
            plan_name: String::new(),
            plan_start: String::new(),
            plan_end: String::new(),
            plan_color: 0,
            plan_open: false,
            edit_plan_id: None,
            focus_plan: false,
            jump_open: false,
            focus_jump: false,
            jump_text: String::new(),
            io_open: false,
            io_scope: IoScope::Day,
            io_format: IoFormat::Markdown,
            io_date: dkey(&today),
            io_msg: String::new(),
            week_view: start_week,
            add_start: String::new(),
            add_end: String::new(),
            course_open: false,
            course_edit_id: None,
            course_name: String::new(),
            course_wd: 1,
            course_start: "08:00".into(),
            course_end: "09:40".into(),
            course_place: String::new(),
            course_start_date: String::new(),
            course_end_date: String::new(),
            course_color: data::default_course_color(),
            course_slot_idx: 0,
            focus_course: false,
            slot_open: false,
            slot_draft: Vec::new(),
            show_ctx: false,
            ctx_pos: egui::pos2(0.0, 0.0),
            ctx_kind: CtxKind::None,
            pending_save: false,
            last_save: Instant::now(),
            win_pos: wp,
            win_size: ws,
            win_max: wm,
            win_applied: false,
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
            start: self.add_start.trim().to_string(), end: self.add_end.trim().to_string(),
        });
        self.add_text.clear();
        self.save_later();
    }

    fn open_plan_form(&mut self) {
        self.plan_open = true;
        self.focus_plan = true;
        self.edit_plan_id = None;
        self.plan_name.clear();
        self.form_anim = 0.0;
        self.plan_start = dkey(&self.selected);
        self.plan_end = dkey(&(self.selected + Duration::days(3)));
        self.plan_color = self.db.plans.len() % PLAN_COLORS.len();
    }
    /// 编辑已有阶段计划：加载数据进表单
    fn edit_plan(&mut self, id: u64) {
        if let Some(pl) = self.db.plans.iter().find(|x| x.id == id).cloned() {
            self.plan_open = true;
            self.focus_plan = true;
            self.form_anim = 0.0;
            self.edit_plan_id = Some(id);
            self.plan_name = pl.name.clone();
            self.plan_start = pl.start.clone();
            self.plan_end = pl.end.clone();
            // 反查颜色索引
            self.plan_color = PLAN_COLORS.iter().position(|x| {
                (x[0]-pl.color[0]).abs() < 0.01 && (x[1]-pl.color[1]).abs() < 0.01 && (x[2]-pl.color[2]).abs() < 0.01
            }).unwrap_or(0);
        }
    }
    fn open_add(&mut self) {
        if !self.add_open { self.form_anim = 0.0; }
        self.add_open = true;
        self.focus_add = true;
        self.add_start.clear(); self.add_end.clear();
    }
    /// 切换周/月视图：周视图按周翻页，月视图按月翻页
    fn shift_page(&mut self, delta: i32) {
        if self.week_view {
            self.view = self.view + Duration::days(7 * delta as i64);
            self.selected = self.view;
        } else { self.shift_month(delta); }
        self.month_anim = 0.0;
    }
    // ── 课程表单 ──
    fn open_course_form(&mut self, id: Option<u64>) {
        self.course_open = true; self.focus_course = true; self.course_edit_id = id;
        if let Some(id) = id {
            if let Some(c) = self.db.courses.iter().find(|c| c.id == id).cloned() {
                self.course_color = c.color;
                // 匹配已有时间段索引（先算再移动 c.start/c.end）
                let slots = self.db.slots();
                self.course_slot_idx = slots.iter().position(|sl| sl.start == c.start && sl.end == c.end)
                    .unwrap_or(0);
                self.course_name = c.name; self.course_wd = c.weekday;
                self.course_start = c.start; self.course_end = c.end; self.course_place = c.place;
                self.course_start_date = c.start_date.clone(); self.course_end_date = c.end_date.clone();
            }
        } else {
            self.course_name.clear(); self.course_wd = (self.selected.weekday().num_days_from_sunday() as u8);
            let slots = self.db.slots();
            self.course_slot_idx = 0;
            self.course_start = slots.get(0).map(|x| x.start.clone()).unwrap_or_else(|| "08:00".into());
            self.course_end = slots.get(0).map(|x| x.end.clone()).unwrap_or_else(|| "09:40".into());
            self.course_place.clear();
            self.course_start_date.clear(); self.course_end_date.clear();
            self.course_color = data::default_course_color();
        }
    }
    fn save_course(&mut self) {
        let name = self.course_name.trim();
        if name.is_empty() { return; }
        let st = self.course_start.trim().to_string();
        let en = self.course_end.trim().to_string();
        if st.is_empty() || en.is_empty() { return; }
        if let Some(id) = self.course_edit_id.take() {
            if let Some(c) = self.db.courses.iter_mut().find(|c| c.id == id) {
                c.name = name.to_string(); c.weekday = self.course_wd;
                c.start = st; c.end = en; c.place = self.course_place.trim().to_string();
                c.start_date = self.course_start_date.trim().to_string();
                c.end_date = self.course_end_date.trim().to_string();
                c.color = self.course_color;
            }
        } else {
            // 同名日程沿用已存在的颜色
            let col = self.db.courses.iter().find(|c| c.name == name)
                .map(|c| c.color).unwrap_or(self.course_color);
            self.db.courses.push(data::Course {
                id: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
                name: name.to_string(), weekday: self.course_wd, start: st, end: en,
                place: self.course_place.trim().to_string(),
                start_date: self.course_start_date.trim().to_string(),
                end_date: self.course_end_date.trim().to_string(), color: col,
            });
        }
        self.course_open = false; self.save_later();
    }
    fn import_courses(&mut self) {
        if let Some(path) = rfd::FileDialog::new().set_title("选择课表 JSON 文件").add_filter("JSON", &["json"]).pick_file() {
            match std::fs::read_to_string(&path) {
                Ok(txt) => match data::import_courses(&txt) {
                    Ok(mut cs) => {
                        let n = cs.len();
                        self.db.courses.append(&mut cs);
                        self.io_msg = format!("课表导入完成: {} 节课", n);
                        self.save_later();
                    }
                    Err(e) => self.io_msg = format!("课表解析失败: {}", e),
                },
                Err(e) => self.io_msg = format!("读取失败: {}", e),
            }
        }
    }

    fn save_plan(&mut self) {
        let name = if self.plan_name.trim().is_empty() { "未命名计划".into() } else { self.plan_name.trim().into() };
        let mut s = self.plan_start.clone();
        let mut e = self.plan_end.clone();
        if s.is_empty() || e.is_empty() { s = dkey(&self.selected); e = s.clone(); }
        if e < s { std::mem::swap(&mut s, &mut e); }
        let color = PLAN_COLORS[self.plan_color % PLAN_COLORS.len()];
        if let Some(id) = self.edit_plan_id.take() {
            // 更新已有计划
            if let Some(pl) = self.db.plans.iter_mut().find(|x| x.id == id) {
                pl.name = name; pl.color = color; pl.start = s; pl.end = e;
            }
        } else {
            // 新建计划
            self.db.plans.push(Plan {
                id: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64,
                name, color, start: s, end: e,
            });
        }
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
        // 月视图 view=月初；周视图 view=今天（draw_week_view 会算出所在周的周日）
        if self.week_view { self.view = self.today; }
        else { self.view = NaiveDate::from_ymd_opt(self.today.year(), self.today.month(), 1).unwrap(); }
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
    // Segoe UI Symbol：唯一正确渲染竖三点 ⋮ (U+22EE) 的系统字体
    if let Ok(bytes) = std::fs::read("C:/Windows/Fonts/seguisym.ttf") {
        fonts.font_data.insert("sym".into(), egui::FontData::from_owned(bytes).into());
        fonts.families.entry(egui::FontFamily::Name("sym".into())).or_default().push("sym".into());
    }
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    // 强制深色：否则 egui 跟随 Windows 系统主题（白天=浅色），按钮白底 + 白字 → 看不见
    ctx.set_theme(egui::ThemePreference::Dark);
    let mut style = (*ctx.style()).clone();
    // 排版层级：display / title / body / label（严格 4 级）
    style.text_styles.insert(egui::TextStyle::Small,     FontId::new(11.0, egui::FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Body,      FontId::new(13.0, egui::FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Button,    FontId::new(12.5,egui::FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Heading,   FontId::new(20.0, egui::FontFamily::Proportional));
    style.text_styles.insert(egui::TextStyle::Monospace, FontId::new(11.0, egui::FontFamily::Monospace));
    style.spacing.item_spacing = Vec2::new(6.0, 4.0);
    style.spacing.button_padding = Vec2::new(9.0, 4.0);
    // 滚动条缩细、贴左栏右缘，不碍事
    style.spacing.scroll.bar_width = 4.0;
    style.spacing.scroll.bar_outer_margin = -9.0;
    style.spacing.scroll.bar_inner_margin = 1.0;
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
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // 每帧锁定深色主题：persistence 会记住 System，白天系统浅色会把按钮变白底白字
        if ctx.theme() != egui::Theme::Dark { ctx.set_theme(egui::ThemePreference::Dark); }
        // ── 原生标题栏染成主面板底色（一次性，视觉上像内嵌）──
        #[cfg(target_os = "windows")]
        if !self.dwm_applied {
            if let Some(h) = frame_hwnd(frame) {
                tint_titlebar(h, c(theme::BG));
                self.dwm_applied = true;
            }
        }

        // ── 窗口几何记忆：首帧应用上次的尺寸/位置/最大化状态 ──
        if !self.win_applied {
            if self.win_max {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(self.win_pos));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(self.win_size));
            }
            self.win_applied = true;
        }
        // 每帧记录当前窗口几何（save 时写盘；最大化时不记位置/尺寸，避免下次恢复成最大化前的怪尺寸）
        let vp = ctx.input(|i| i.viewport().clone());
        if let (Some(or), Some(ir)) = (vp.outer_rect, vp.inner_rect) {
            let is_max = vp.maximized.unwrap_or(false);
            self.win_max = is_max;
            if !is_max {
                self.win_pos = or.min;
                self.win_size = ir.size();
            }
        }

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
                // 左右栏布局：左栏定宽 → 中间留舒服间隙 → 右栏占据剩余并适度收窄
                // 布局：左 margin(16) + 左栏(left_w) + 中缝(gap) + 右栏(剩余) + 右 margin(16)，全部固定。
                // egui 的 Frame 背景 = content_ui.min_rect()（内容宽度），长文字换行后 min_rect
                // 不超出 left_rect，背景停在左栏内，gap 露出 BG，两栏自然分开。
                let gap = 28.0f32;
                // 左栏加宽到 312，把「阶段计划」行的＋新建按钮完整包进面板内；
                // right_left 由 left_w 算出，中缝与右栏左缘随之同步右移，右栏相应变窄。
                let left_w = 352.0f32.min(area.width() * 0.40).max(220.0);
                // 左栏起点回到 area.min.x（比上次右移 20px），宽度减 20 → 左栏右缘不变、栏体变窄
                let left_origin = Pos2::new(area.min.x, area.min.y);
                let left_rect = egui::Rect::from_min_size(left_origin, egui::vec2(left_w, area.height()));
                // 右栏左边界 = 左栏右缘 + 间隙；右边界贴到 area.max.x（CentralPanel 16px 边距即右边距）
                let right_left = area.min.x + left_w + gap;
                let right_r = area.max.x;
                let right_rect = egui::Rect::from_min_max(
                    Pos2::new(right_left, area.min.y),
                    Pos2::new(right_r, area.max.y),
                );
                // 左栏：绝对矩形 + 独立滚动
                ui.allocate_ui_at_rect(left_rect, |u| {
                    // 背景手动画在固定 left_rect：egui 的 Frame 背景 = content_ui.min_rect()，
                    // 内容一溢出背景就铺出左栏、吃掉中缝；rect_filled 画固定矩形则永不串色。
                    u.painter().rect_filled(left_rect, Rounding::same(radius::LG), c(theme::PANEL));
                    u.painter().rect_stroke(left_rect, Rounding::same(radius::LG),
                        Stroke::new(1.0, c(theme::STROKE)));
                    egui::Frame::none()
                        .inner_margin(Margin::symmetric(18.0, 16.0)).show(u, |ui| {
                            // 顶部（日期/时钟/统计）固定不滚动
                            self.left_header(ui, &now);
                            // 只有阶段计划 + 未来七天这一块独立滚动，滚动条贴左栏面板右缘
                            let lists_top = ui.cursor().top();
                            let sb_rect = egui::Rect::from_min_max(
                                Pos2::new(left_rect.right() - 20.0, lists_top + 4.0),
                                Pos2::new(left_rect.right() - 10.0, left_rect.bottom() - 6.0),
                            );
                            egui::ScrollArea::vertical().id_source("left_lists_scroll")
                                .auto_shrink([false, false])
                                .scroll_bar_rect(sb_rect)
                                .show(ui, |ui| { self.left_lists(ui); });
                        });
                });
                // 右栏：绝对矩形，内部三段式固定分区
                ui.allocate_ui_at_rect(right_rect, |u| {
                    u.painter().rect_filled(right_rect, Rounding::same(radius::LG), c(theme::PANEL));
                    u.painter().rect_stroke(right_rect, Rounding::same(radius::LG),
                        Stroke::new(1.0, c(theme::STROKE)));
                    egui::Frame::none()
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

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let raw = format!("{},{},{},{},{}",
            self.win_pos.x, self.win_pos.y, self.win_size.x, self.win_size.y,
            if self.win_max { 1 } else { 0 });
        storage.set_string("win_geom", raw);
    }

}

impl App {
    // ═══════════ 自绘标题栏 ═══════════
    // ═══════════ 左栏 ═══════════
    fn left_header(&mut self, ui: &mut egui::Ui, now: &chrono::DateTime<Local>) {
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
    }

    // ═══════════ 左栏下半：阶段计划 + 未来七天（独立滚动区）═══════════
    fn left_lists(&mut self, ui: &mut egui::Ui) {
        // ── 阶段计划 ──
        ui.horizontal(|ui| {
            section_title(ui, "阶段计划");
            // 按钮贴内容区右缘：right_to_left 不会溢出左栏（add_space(available_width) 会顶出界）
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("＋ 新建").clicked() { self.open_plan_form(); }
            });
        });
        let zone = ui.allocate_response(Vec2::new(ui.available_width(), 4.0), egui::Sense::click());
        if zone.secondary_clicked() { self.ctx(CtxKind::PlanZone, ui_pos(ui)); }
        for p in self.db.plans.clone() {
            let rg = format!("{}→{}·{}天", short(&p.start), short(&p.end),
                             (parse_key(&p.end) - parse_key(&p.start)).num_days() + 1);
            // 整行 hover 高亮：在 horizontal 内用 rect_contains_pointer（坐标一致）
            // hover 高亮矩形：在 horizontal 外用 cursor 位置构造整行矩形，
            // 不用 ui.max_rect()（嵌套 right_to_left 后它只剩按钮区宽，高亮会错位到最右）
            let row_left = ui.cursor().left();
            let row_top = ui.cursor().top();
            let row_w = ui.available_width();
            let row_hl = egui::Rect::from_min_size(Pos2::new(row_left, row_top), Vec2::new(row_w, 20.0));
            let row_hover = ui.rect_contains_pointer(row_hl);
            ui.horizontal(|ui| {
                // 色块 + 名称：左对齐，名称占据剩余宽度
                ui.add(egui::Label::new(RichText::new("■").color(rgb(p.color)).size(11.0)));
                // 给日期(≈120px)+⋯按钮(≈30px)留足宽度，防止日期溢出被按钮挤压
                let name_w = ui.available_width() - 166.0;
                // hover 时名称文字提亮+加粗（不用背景色块）；无背景高亮，避免切割日期/被按钮遮
                let name_style = if row_hover {
                    RichText::new(&p.name).size(12.5).color(c(theme::ACCENT2)).strong()
                } else {
                    RichText::new(&p.name).size(12.0).color(c(theme::TEXT2))
                };
                let name_resp = ui.allocate_ui_with_layout(
                    Vec2::new(name_w.max(40.0), 20.0), egui::Layout::left_to_right(egui::Align::Center),
                    // truncate 自带完整文本 tooltip，不再手动 on_hover_text（否则悬停出现两行重复全名）
                    |ui| ui.add(egui::Label::new(name_style).truncate()),
                ).inner;
                if name_resp.clicked() {
                    let d = parse_key(&p.start);
                    self.select(d);
                    self.view = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
                }
                if name_resp.secondary_clicked() { self.ctx(CtxKind::PlanZone, ui_pos(ui)); }
                // 日期 + ⋯ 菜单：right_to_left 固定在行右缘，与名称垂直居中对齐，每行同一列
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.push_id(p.id, |ui| {
                        egui::menu::menu_custom_button(ui,
                            egui::Button::new(RichText::new("⋮").font(FontId::new(15.0, egui::FontFamily::Name("sym".into()))))
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::new(1.0, c(theme::STROKE_HI)))
                                .rounding(Rounding::same(4.0))
                                .min_size(Vec2::new(13.0, 22.0)), |ui| {
                            if ui.button("✎  编辑").clicked() { self.edit_plan(p.id); ui.close_menu(); }
                            if ui.button("×  删除").clicked() {
                                self.db.plans.retain(|x| x.id != p.id);
                                if self.edit_plan_id == Some(p.id) { self.edit_plan_id = None; self.plan_open = false; }
                                self.save_later();
                                ui.close_menu();
                            }
                        });
                    });
                    ui.label(RichText::new(rg).size(10.0).color(c(theme::TEXT3)));
                });
            });
        }
        // 表单展开：淡入背景（form_anim 驱动）
        if self.plan_open { self.plan_form(ui); }
        section_rule(ui);

        // ── 未来七天（每日计划 + 当天阶段计划 合并显示）──
        section_title(ui, "未来七天");
        let mut any = false;
        for i in 0..7 {
            let d = self.today + Duration::days(i);
            let dk = dkey(&d);
            let todos: Vec<(String, u8)> = self.db.todos_of(&dk).into_iter()
                .filter(|t| !t.done).map(|t| (t.text.clone(), t.prio)).collect();
            let plans: Vec<(String, [f32;3])> = self.db.plans_on(&dk).into_iter()
                .map(|p| (p.name.clone(), p.color)).collect();
            if todos.is_empty() && plans.is_empty() { continue; }
            any = true;
            let day_label = if d == self.today { "今天".to_string() } else { d.format("%m/%d").to_string() };
            // 每天的日期标题
            ui.add(egui::Label::new(RichText::new(&day_label).size(11.0).strong().color(c(theme::ACCENT2))));
            // 当天的每日计划（缩进）
            for (text, prio) in todos {
                ui.horizontal(|ui| {
                    let row_rect = ui.max_rect();
                    ui.add_space(10.0);
                    let r = ui.allocate_response(Vec2::new(3.0, 14.0), egui::Sense::hover()).rect;
                    ui.painter().rect_filled(r, Rounding::same(1.5), rgb(prio_color(prio)));
                    // Frame 包住整段 wrap 文字：背景跟随内容 min_rect（含所有换行行），
                    // hover 时一个计划一个圆角框圈住全部文字，不再只亮第一行。
                    let hovered = ui.rect_contains_pointer(row_rect);
                    let base = rgb(prio_color(prio));
                    let fill = if hovered {
                        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 40)
                    } else { Color32::TRANSPARENT };
                    let text_w = (row_rect.width() - 40.0).max(40.0);
                    egui::Frame::none().fill(fill).rounding(Rounding::same(radius::SM))
                        .inner_margin(Margin::symmetric(5.0, 2.0)).show(ui, |ui| {
                            ui.set_width(text_w - 10.0);
                            let resp = ui.add(egui::Label::new(
                                RichText::new(&text).size(12.0).color(c(theme::TEXT2))).wrap().sense(egui::Sense::click()));
                            if resp.clicked() { self.select(d); }
                            if resp.secondary_clicked() { self.select(d); self.ctx(CtxKind::Calendar, ui_pos(ui)); }
                        });
                });
            }
            // 当天关联的阶段计划（缩进，紧随每日计划之后）
            for (pl_name, pl_color) in plans {
                let prow = ui.max_rect();
                let pw = (prow.width() - 24.0).max(40.0);
                let resp = ui.allocate_ui_with_layout(
                    Vec2::new(pw, 14.0), egui::Layout::top_down(egui::Align::LEFT),
                    |ui| ui.add(egui::Label::new(RichText::new(format!("■ {}", pl_name)).size(10.5).color(rgb(pl_color))).wrap()),
                ).inner;
                if resp.clicked() { self.select(d); }
                if resp.secondary_clicked() { self.select(d); self.ctx(CtxKind::Calendar, ui_pos(ui)); }
            }
        }
        if !any {
            ui.label(RichText::new("未来七天暂无安排").size(12.0).color(c(theme::TEXT3)));
        }
    }

    fn plan_form(&mut self, ui: &mut egui::Ui) {
        let t = Self::ease(self.form_anim);
        let bg = mix(c(theme::CARD), c(theme::ACCENT), (1.0 - t) * 0.28);
        egui::Frame::none().fill(bg).rounding(Rounding::same(radius::SM))
            .inner_margin(Margin::same(10.0)).show(ui, |ui| {
                let name_resp = ui.add(egui::TextEdit::singleline(&mut self.plan_name)
                    .hint_text("计划名称，如「项目冲刺」").desired_width(ui.available_width()));
                if self.focus_plan { name_resp.request_focus(); self.focus_plan = false; }
                if name_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { self.save_plan(); }
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
                let save_label = if self.edit_plan_id.is_some() { "保存修改" } else { "创建计划" };
                if ui.button(save_label).clicked() { self.save_plan(); }
            });
    }

    // ═══════════ 右栏 ═══════════
    fn right_panel(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();

        // ① 头部（固定高度，不参与滚动）
        ui.horizontal(|ui| {
            let title_txt = if self.week_view {
                let back = if self.db.week_start_sunday { self.view.weekday().num_days_from_sunday() as i64 }
                           else { self.view.weekday().num_days_from_monday() as i64 };
                let w0 = self.view - Duration::days(back);
                let w6 = w0 + Duration::days(6);
                format!("{}月{}日 – {}月{}日", w0.month(), w0.day(), w6.month(), w6.day())
            } else { format!("{}年{}月", self.view.year(), self.view.month()) };
            ui.label(RichText::new(title_txt).font(FontId::proportional(26.0)).color(c(theme::TEXT)));
            let ly = solar2lunar(&NaiveDate::from_ymd_opt(self.view.year(), self.view.month(), 15).unwrap());
            ui.label(RichText::new(format!("{}年·{}", year_gz(&ly), animal(&ly))).size(10.5).color(c(theme::TEXT3)));
            // 翻页按钮贴内容区右缘 = 日历周六列右缘（right_to_left 保证不被裁）
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_sized([28.0, 26.0], egui::Button::new("›")).clicked() { self.shift_page(1); }
                if self.jump_open {
                    let resp = ui.add_sized([126.0, 26.0], egui::TextEdit::singleline(&mut self.jump_text)
                        .id_source("jump_inline_edit").hint_text("YYYY-MM-DD 回车"));
                    if self.focus_jump { resp.request_focus(); self.focus_jump = false; }
                    let enter = resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if enter || resp.lost_focus() {
                        if let Ok(d) = NaiveDate::parse_from_str(self.jump_text.trim(), "%Y-%m-%d") {
                            self.selected = d;
                            self.view = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
                            self.month_anim = 0.0; self.select_pulse = 0.0;
                        }
                        self.jump_open = false;
                    }
                } else if ui.add_sized([52.0, 26.0], egui::Button::new("跳转")).on_hover_text("输入日期跳转").clicked() {
                    self.jump_open = true; self.jump_text = String::new(); self.focus_jump = true;
                }
                if ui.add_sized([28.0, 26.0], egui::Button::new("‹")).clicked() { self.shift_page(-1); }
                if ui.add_sized([52.0, 26.0], egui::Button::new("设置")).on_hover_text("排序规则 / 周起点 / 导入导出 / 课表").clicked() {
                    self.io_open = !self.io_open;
                }
                // right_to_left：后写的显示在更左侧
                if ui.add_sized([44.0, 26.0], egui::Button::new(if self.week_view { "周" } else { "月" })
                    .frame(true)).on_hover_text("切换月视图 / 周视图").clicked() {
                    self.week_view = !self.week_view;
                    if self.week_view { self.view = self.selected; } // 切到周视图时定位到选中日期所在周
                }
                if ui.add_sized([44.0, 26.0], egui::Button::new("日程")).on_hover_text("管理每周重复日程").clicked() {
                    self.open_course_form(None);
                }
                if ui.add_sized([44.0, 26.0], egui::Button::new("时段")).on_hover_text("自定义周视图时间段").clicked() {
                    self.slot_draft = self.db.slots().into_iter().map(|s| (s.start, s.end)).collect();
                    self.slot_open = !self.slot_open;
                }
            });
        });
        // ◀ 单独绑定（上面占位，这里补事件不便，统一在下方处理）
        // ── 设置面板：排序 / 周起点 / 颜色 / 导入导出 ──
        if self.io_open {
            egui::Frame::none().fill(c(theme::CARD)).rounding(Rounding::same(radius::SM))
                .inner_margin(Margin::symmetric(8.0, 6.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("设 置").size(12.0).strong().color(c(theme::ACCENT2)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("×").clicked() { self.io_open = false; }
                        });
                    });
                    // 排序规则：每日计划 / 每日日程表各自独立
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("排序:").size(11.0).color(c(theme::TEXT2)));
                        ui.label(RichText::new("计划").size(11.0).color(c(theme::TEXT3)));
                        for (i, (nm, v)) in [("按重要度", false), ("按时间", true)].iter().enumerate() {
                            let _ = i;
                            if ui.add(egui::Button::new(RichText::new(*nm).size(10.5))
                                .frame(self.db.by_time == *v)).clicked() { self.db.by_time = *v; self.save_later(); }
                        }
                        ui.add_space(8.0);
                        ui.label(RichText::new("日程").size(11.0).color(c(theme::TEXT3)));
                        for (nm, v) in [("按时间", true), ("按重要度", false)].iter() {
                            if ui.add(egui::Button::new(RichText::new(*nm).size(10.5))
                                .frame(self.db.schedule_by_time == *v)).clicked() { self.db.schedule_by_time = *v; self.save_later(); }
                        }
                    });
                    // 周视图：一周起点 + 方块颜色跟随日程
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("周视图:").size(11.0).color(c(theme::TEXT2)));
                        for (nm, v) in [("周一起", false), ("周日起", true)].iter() {
                            if ui.add(egui::Button::new(RichText::new(*nm).size(10.5))
                                .frame(self.db.week_start_sunday == *v)).clicked() { self.db.week_start_sunday = *v; self.save_later(); }
                        }
                        ui.add_space(8.0);
                        ui.label(RichText::new("默认视图:").size(11.0).color(c(theme::TEXT3)));
                        for (nm, v) in [("月", false), ("周", true)].iter() {
                            if ui.add(egui::Button::new(RichText::new(*nm).size(10.5))
                                .frame(self.db.default_week_view == *v)).clicked() {
                                self.db.default_week_view = *v; self.week_view = *v;
                                if *v { self.view = self.today; } else {
                                    self.view = NaiveDate::from_ymd_opt(self.today.year(), self.today.month(), 1).unwrap();
                                }
                                self.save_later();
                            }
                        }
                        ui.label(RichText::new("卡片文字:").size(11.0).color(c(theme::TEXT3)));
                        for (nm, v) in [("居中", true), ("左对齐", false)].iter() {
                            if ui.add(egui::Button::new(RichText::new(*nm).size(10.5))
                                .frame(self.db.card_center == *v)).clicked() { self.db.card_center = *v; self.save_later(); }
                        }
                        if ui.add(egui::Checkbox::new(&mut self.db.color_by_schedule,
                            RichText::new("方块颜色跟随日程").size(11.0))).changed() { self.save_later(); }
                    });
                    section_rule(ui);
                    // ── 导入导出（原有功能全部保留）──
                    // 第一行：范围 + 日期
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("范围:").size(11.0).color(c(theme::TEXT2)));
                        for sc in [IoScope::Day, IoScope::Week, IoScope::Month, IoScope::Year] {
                            let on = self.io_scope == sc;
                            if ui.add(egui::Button::new(RichText::new(sc.label()).size(11.0))
                                .frame(on)).clicked() { self.io_scope = sc; }
                        }
                        ui.label(RichText::new("日期:").size(11.0).color(c(theme::TEXT2)));
                        ui.add(egui::TextEdit::singleline(&mut self.io_date)
                            .id_source("io_date_edit").desired_width(104.0).hint_text("YYYY-MM-DD"));
                        ui.label(RichText::new("格式:").size(11.0).color(c(theme::TEXT2)));
                        if ui.add(egui::Button::new("JSON").frame(self.io_format == IoFormat::Json)).clicked() { self.io_format = IoFormat::Json; }
                        if ui.add(egui::Button::new("Markdown").frame(self.io_format == IoFormat::Markdown)).clicked() { self.io_format = IoFormat::Markdown; }
                    });
                    // 第二行：导出 / 导入 / 关闭（点击弹原生文件管理器选位置）
                    ui.horizontal(|ui| {
                        if ui.button("导出…").clicked() {
                            let scope = match self.io_scope { IoScope::Day=>"day", IoScope::Week=>"week", IoScope::Month=>"month", IoScope::Year=>"year" };
                            let center = NaiveDate::parse_from_str(self.io_date.trim(), "%Y-%m-%d")
                                .unwrap_or(self.selected);
                            let (content, ext, filter) = match self.io_format {
                                IoFormat::Json => (data::export_json(&self.db, scope, center), "json", "JSON"),
                                IoFormat::Markdown => (data::export_markdown(&self.db, scope, center), "md", "Markdown"),
                            };
                            let def_name = format!("planum_{}_{}.{}", scope, dkey(&center), ext);
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("选择导出位置").set_file_name(&def_name)
                                .add_filter(filter, &[ext]).save_file()
                            {
                                match std::fs::write(&path, content) {
                                    Ok(_) => self.io_msg = format!("已导出: {}", path.file_name().unwrap_or_default().to_string_lossy()),
                                    Err(e) => self.io_msg = format!("导出失败: {}", e),
                                }
                            } else {
                                self.io_msg = "已取消导出".into();
                            }
                        }
                        if ui.button("导入…").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_title("选择要导入的 JSON 文件")
                                .add_filter("JSON", &["json"]).pick_file()
                            {
                                match std::fs::read_to_string(&path) {
                                    Ok(txt) => match data::import_json(&mut self.db, &txt) {
                                        Ok((nt, np)) => { self.io_msg = format!("导入完成: {}待办 {}计划", nt, np); self.save_later(); }
                                        Err(e) => self.io_msg = format!("解析失败: {}", e),
                                    },
                                    Err(e) => self.io_msg = format!("读取失败: {}", e),
                                }
                            } else {
                                self.io_msg = "已取消导入".into();
                            }
                        }
                        if ui.button("导入日程…").on_hover_text("从 JSON 导入每周重复日程").clicked() { self.import_courses(); }
                        if ui.button("课表模板…").on_hover_text("导出一份带示例的课表 JSON").clicked() {
                            let tpl = r#"{
  "courses": [
    {
      "name": "高等数学",
      "weekday": 1,
      "start": "08:00",
      "end": "09:40",
      "place": "A301",
      "start_date": "2026-09-01",
      "end_date": "2027-01-15"
    },
    {
      "name": "英语二",
      "weekday": 2,
      "start": "10:00",
      "end": "11:40",
      "place": "B205",
      "start_date": "",
      "end_date": ""
    },
    {
      "name": "模电",
      "day": 3,
      "start": "14:00",
      "end": "15:40",
      "place": "实验楼 201"
    }
  ]
}
"#;
                            if let Some(path) = rfd::FileDialog::new().set_title("保存课表模板").set_file_name("planum_courses.json")
                                .add_filter("JSON", &["json"]).save_file() {
                                match std::fs::write(&path, tpl) {
                                    Ok(_) => self.io_msg = "模板已保存：weekday 0=周日..6=周六（或用 day 1=周一..7=周日）；start/end 是每天时段；start_date/end_date 是生效日期范围，留空=永久".into(),
                                    Err(e) => self.io_msg = format!("保存失败: {}", e),
                                }
                            }
                        }
                        if ui.button("×").clicked() { self.io_open = false; }
                    });
                    if !self.io_msg.is_empty() {
                        ui.label(RichText::new(&self.io_msg).size(10.5).color(c(theme::MINT)));
                    }
                });
        }
        // ── 课程表单 ──
        if self.course_open {
            egui::Frame::none().fill(c(theme::CARD)).rounding(Rounding::same(radius::SM))
                .inner_margin(Margin::symmetric(8.0, 6.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(if self.course_edit_id.is_some() { "编辑日程" } else { "添加日程" })
                            .size(12.0).strong().color(c(theme::ACCENT2)));
                        ui.label(RichText::new("星期:").size(11.0).color(c(theme::TEXT2)));
                        for wd in 0u8..7 {
                            let on = self.course_wd == wd;
                            if ui.add(egui::Button::new(RichText::new(["日","一","二","三","四","五","六"][wd as usize]).size(11.0))
                                .frame(on)).clicked() { self.course_wd = wd; }
                        }
                    });
                    ui.horizontal(|ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut self.course_name)
                            .hint_text("课程名，如「高等数学」").desired_width((ui.available_width()-240.0).max(80.0)));
                        if self.focus_course { r.request_focus(); self.focus_course = false; }
                        // 选已有时间段（选中后自动填起止）
                        let slots = self.db.slots();
                        let cur_label = slots.get(self.course_slot_idx)
                            .map(|s| format!("{}-{}", s.start, s.end)).unwrap_or_else(|| "自定义".into());
                        egui::ComboBox::from_id_source("course_slot_combo").selected_text(cur_label).show_ui(ui, |ui| {
                            for (i, sl) in slots.iter().enumerate() {
                                if ui.selectable_value(&mut self.course_slot_idx, i,
                                    format!("{}-{}", sl.start, sl.end)).clicked() {
                                    self.course_start = sl.start.clone(); self.course_end = sl.end.clone();
                                }
                            }
                        });
                        ui.add(egui::TextEdit::singleline(&mut self.course_place).hint_text("地点").desired_width(70.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("生效日期:").size(11.0).color(c(theme::TEXT3)));
                        ui.add(egui::TextEdit::singleline(&mut self.course_start_date).hint_text("起 YYYY-MM-DD").desired_width(110.0));
                        ui.label(RichText::new("–").size(11.0).color(c(theme::TEXT3)));
                        ui.add(egui::TextEdit::singleline(&mut self.course_end_date).hint_text("止 YYYY-MM-DD").desired_width(110.0));
                        ui.label(RichText::new("留空=永久").size(9.5).color(c(theme::TEXT3)));
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("颜色:").size(11.0).color(c(theme::TEXT3)));
                        for (i, col) in data::PLAN_COLORS.iter().enumerate() {
                            let size = 18.0;
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(size, size), egui::Sense::click());
                            let filled = (self.course_color[0]-col[0]).abs()<0.01
                                      && (self.course_color[1]-col[1]).abs()<0.01
                                      && (self.course_color[2]-col[2]).abs()<0.01;
                            ui.painter().rect_filled(rect, Rounding::same(3.0), rgb(*col));
                            if filled { ui.painter().rect_stroke(rect.expand(2.0), Rounding::same(4.0), Stroke::new(2.0, c(theme::TEXT))); }
                            if resp.clicked() { self.course_color = *col; let _ = i; }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("保存").clicked() { self.save_course(); }
                        if self.course_edit_id.is_some() && ui.button("删除").clicked() {
                            if let Some(id) = self.course_edit_id.take() { self.db.courses.retain(|c| c.id != id); self.save_later(); }
                            self.course_open = false;
                        }
                        if ui.button("取消").clicked() { self.course_open = false; self.course_edit_id = None; }
                    });
                });
        }
        // ── 时间段设置 ──
        if self.slot_open {
            egui::Frame::none().fill(c(theme::CARD)).rounding(Rounding::same(radius::SM))
                .inner_margin(Margin::symmetric(8.0, 6.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("周视图时间段（每天卡片数量一致）").size(12.0).strong().color(c(theme::ACCENT2)));
                        if ui.button("+ 添加").clicked() { self.slot_draft.push(("19:00".into(), "21:00".into())); }
                    });
                    let mut rm = None;
                    for (i, (st, en)) in self.slot_draft.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{}.", i+1)).size(11.0).color(c(theme::TEXT3)));
                            ui.add(egui::TextEdit::singleline(st).desired_width(62.0));
                            ui.label(RichText::new("–").size(11.0).color(c(theme::TEXT3)));
                            ui.add(egui::TextEdit::singleline(en).desired_width(62.0));
                            if ui.button("×").clicked() { rm = Some(i); }
                        });
                    }
                    if let Some(i) = rm { self.slot_draft.remove(i); }
                    ui.horizontal(|ui| {
                        if ui.button("保存时间段").clicked() {
                            self.db.slots = self.slot_draft.iter().map(|(a,b)| data::Slot { start: a.clone(), end: b.clone() }).collect();
                            self.save_later(); self.slot_open = false;
                        }
                        if ui.button("恢复默认").clicked() { self.slot_draft = data::default_slots().into_iter().map(|s|(s.start,s.end)).collect(); }
                        if ui.button("取消").clicked() { self.slot_open = false; }
                    });
                });
        }
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
        if self.week_view { self.draw_week_view(ui, width, cal_h); } else { self.calendar(ui, width, cal_h); }
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

    // ═══════════ 周视图：7 列（周日→周六），每列按自定义时间段切卡片 ═══════════
    fn draw_week_view(&mut self, ui: &mut egui::Ui, width: f32, cal_h: f32) {
        let slots = self.db.slots();
        let nslot = slots.len().max(1);
        // 一周起点：随开关（周日起 / 周一起）
        let sun_first = self.db.week_start_sunday;
        let back = if sun_first { self.view.weekday().num_days_from_sunday() as i64 }
                   else { self.view.weekday().num_days_from_monday() as i64 };
        let week_start = self.view - Duration::days(back);
        let days: Vec<NaiveDate> = (0..7).map(|i| week_start + Duration::days(i)).collect();
        let gap = 3.0;
        let lab_w = 46.0f32; // 最左侧独立时段列，不再压周日列
        let col_w = (width - lab_w - gap * 7.0) / 7.0;
        let head_h = 24.0;
        let all_day_h = 30.0;
        let slot_h = ((cal_h - head_h - all_day_h) / nslot as f32).max(18.0);

        // ── 星期表头 ──
        // 关键：时段标签/日期列都以表头「时段」格子的实际 left 为基准，
        // 不能用 area_x(ui)（那是 clip 左缘≈窗口最左，会把 8:00 画到软件最左侧）
        let mut base_x = ui.cursor().left();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            // 第一格：时段列表头
            let (lab_rect, _) = ui.allocate_exact_size(Vec2::new(lab_w, head_h), egui::Sense::hover());
            base_x = lab_rect.left();
            ui.painter().text(lab_rect.center(), Align2::CENTER_CENTER, "时段",
                FontId::proportional(11.0), c(theme::TEXT3));
            for (i, d) in days.iter().enumerate() {
                let wd = d.weekday().num_days_from_sunday() as u8; // 真实星期：0=周日
                let (rect, _) = ui.allocate_exact_size(Vec2::new(col_w, head_h), egui::Sense::hover());
                let is_today = *d == self.today;
                let is_sel = *d == self.selected;
                let fill = if is_today { c(theme::ACCENT) }
                           else if is_sel { ca(theme::ACCENT2, 60) }
                           else { c(theme::CARD) };
                ui.painter().rect_filled(rect, Rounding::same(radius::SM), fill);
                let col = if is_today { c("#10131a") } else { c(theme::TEXT) };
                ui.painter().text(rect.center(), Align2::CENTER_CENTER,
                    format!("{} {}", WEEK[wd as usize], d.day()), FontId::proportional(12.0), col);
            }
        });

        // ── 时间段网格 ──
        let grid_top = ui.cursor().top();
        for (si, slot) in slots.iter().enumerate() {
            let yt = grid_top + si as f32 * slot_h;
            let cell_rect = egui::Rect::from_min_size(Pos2::new(base_x, yt), Vec2::new(lab_w, slot_h - gap));
            ui.painter().rect_filled(cell_rect, Rounding::same(2.0), ca(theme::BG, 120));
            // 时段标签：起止时间在方块内上下居中、拉开距离（约 1/3 与 2/3 高度处）
            let cx = cell_rect.center().x;
            let y1 = cell_rect.top() + cell_rect.height() / 3.0;
            let y2 = cell_rect.top() + cell_rect.height() * 2.0 / 3.0;
            ui.painter().text(Pos2::new(cx, y1), Align2::CENTER_CENTER, &slot.start,
                FontId::proportional(10.5), c(theme::TEXT));
            ui.painter().text(Pos2::new(cx, y2), Align2::CENTER_CENTER, &slot.end,
                FontId::proportional(10.5), c(theme::TEXT2));
        }
        // 每天的内容：课程 + 时段待办，按格子落位（用 allocate_ui_at_rect 保证坐标精确）
        for (di, d) in days.iter().enumerate() {
            let dk = dkey(d);
            let wd = d.weekday().num_days_from_sunday() as u8; // 真实星期：0=周日
            let x = base_x + lab_w + gap + di as f32 * (col_w + gap);
            // 该天所有有时段的项：课程 + 待办
            struct Item { id: u64, start: String, end: String, text: String, color: Color32, is_course: bool }
            let mut items: Vec<Item> = Vec::new();
            for co in self.db.courses_on(wd, &dk) {
                items.push(Item { id: co.id, start: co.start.clone(), end: co.end.clone(),
                    text: co.name.clone(), color: rgb(co.color), is_course: true });
            }
            for t in self.db.todos_of(&dk) {
                if !t.start.is_empty() {
                    items.push(Item { id: t.id, start: t.start.clone(), end: t.end.clone(), text: t.text.clone(),
                        color: if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) }, is_course: false });
                }
            }
            for (si, slot) in slots.iter().enumerate() {
                let cell_top = grid_top + si as f32 * slot_h + 1.0;
                let cr = egui::Rect::from_min_size(Pos2::new(x, cell_top), Vec2::new(col_w, slot_h - gap - 2.0));
                let in_slot: Vec<&Item> = items.iter().filter(|it| {
                    // 项的开始时间落在本段 [start,end) 内
                    it.start.as_str() >= slot.start.as_str() && it.start.as_str() < slot.end.as_str()
                }).collect();
                // 方块增强：有内容的格子画圆角卡片；颜色可跟随日程/优先级
                if !in_slot.is_empty() {
                    let first_col = in_slot[0].color;
                    let (fill, stroke_col) = if self.db.color_by_schedule {
                        (Color32::from_rgba_unmultiplied(first_col.r(), first_col.g(), first_col.b(), 46),
                         Color32::from_rgba_unmultiplied(first_col.r(), first_col.g(), first_col.b(), 150))
                    } else {
                        (ca(theme::CARD, 210), c(theme::STROKE))
                    };
                    ui.painter().rect_filled(cr, Rounding::same(4.0), fill);
                    ui.painter().rect_stroke(cr, Rounding::same(4.0), Stroke::new(1.0, stroke_col));
                }
                // 日程按数量平分卡片高度：单个占满卡片（字号≈卡片高度80%）居中，每多一个平分一次
                let show_n = in_slot.len().min(3);
                if show_n > 0 {
                    let band = cr.height() / show_n as f32;
                    for (i, it) in in_slot.iter().take(3).enumerate() {
                        let band_top = cr.top() + i as f32 * band;
                        let band_mid = band_top + band / 2.0;
                        // 右键课程方块 → 弹出日程编辑菜单（待办仍用日历右键菜单）
                        if it.is_course {
                            let item_rect = egui::Rect::from_min_size(Pos2::new(cr.left(), band_top),
                                Vec2::new(cr.width(), band));
                            if ui.rect_contains_pointer(item_rect)
                               && ui.ctx().input(|i| i.pointer.button_clicked(egui::PointerButton::Secondary)) {
                                self.ctx(CtxKind::Course(it.id), ui_pos(ui));
                            }
                        }
                        // 字号随 band 缩放：单项≈卡片80%，多项平分后自动缩小
                        let name_fs = (band * 0.62).clamp(12.0, 40.0);
                        let time_fs = (name_fs * 0.42).clamp(8.0, 14.0);
                        let name = trunc_to_width(ui, &it.text, FontId::proportional(name_fs), col_w - 12.0);
                        let has_time = !it.start.is_empty();
                        // 名字 + 时间整体在 band 内垂直居中
                        let (name_y, time_y) = if has_time {
                            let total = name_fs + time_fs;
                            (band_mid - total / 2.0 + name_fs / 2.0,
                             band_mid - total / 2.0 + name_fs + time_fs / 2.0)
                        } else { (band_mid, band_mid) };
                        // 卡片文字对齐：设置里可切换 居中 / 左对齐
                        let center = self.db.card_center;
                        let tx = if center { cr.center().x } else { cr.left() + 8.0 };
                        let align = if center { Align2::CENTER_CENTER } else { Align2::LEFT_CENTER };
                        ui.painter().text(Pos2::new(tx, name_y),
                            align, &name, FontId::proportional(name_fs), it.color);
                        if has_time {
                            ui.painter().text(Pos2::new(tx, time_y), align,
                                format!("{}-{}", it.start, it.end), FontId::proportional(time_fs), c(theme::TEXT3));
                        }
                    }
                    if in_slot.len() > 3 {
                        ui.painter().text(Pos2::new(cr.right() - 4.0, cr.bottom() - 2.0),
                            Align2::RIGHT_BOTTOM, format!("+{}", in_slot.len()-3),
                            FontId::proportional(9.0), c(theme::TEXT3));
                    }
                }
            }
            // 全天待办区：该天未设时段的待办（最多 2 条 + N）
            let all_todos: Vec<_> = self.db.todos_of(&dk).into_iter().filter(|t| t.start.is_empty()).collect();
            let ar = egui::Rect::from_min_size(Pos2::new(x, grid_top + nslot as f32 * slot_h + 1.0), Vec2::new(col_w, all_day_h - 4.0));
            ui.allocate_ui_at_rect(ar, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("全天").size(8.5).color(c(theme::TEXT3)));
                for t in all_todos.iter().take(2) {
                    let label = trunc_to_width(ui, &t.text, FontId::proportional(9.0), col_w - 5.0);
                    ui.label(RichText::new(label).size(9.0).color(if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) }));
                }
                if all_todos.len() > 2 { ui.label(RichText::new(format!("+{}", all_todos.len()-2)).size(8.0).color(c(theme::TEXT3))); }
            });
            // 点击整列选中那天（用最底部一个透明响应区覆盖整列）
            let col_rect = egui::Rect::from_min_size(Pos2::new(x, grid_top - head_h), Vec2::new(col_w, head_h + nslot as f32 * slot_h + all_day_h));
            let resp = ui.interact(col_rect, ui.id().with(("week_col", di)), egui::Sense::click());
            if resp.clicked() { self.select(*d); }
        }
        // 推进 cursor 到网格底部，供下方详情布局
        let bottom = grid_top + nslot as f32 * slot_h + all_day_h;
        ui.allocate_space(Vec2::new(0.0, (bottom - ui.cursor().top()).max(0.0)));
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
                    // ── 当日日程关键字：彩色小方块 + 名字关键字，跟随日程色 ──
                    // 日程 = 带时间的每日计划（课程 + 带 start 的待办）
                    let mut agenda_kw: Vec<(String, Color32)> = Vec::new();
                    for co in self.db.courses_on(d.weekday().num_days_from_sunday() as u8, &dkey(&d)) {
                        agenda_kw.push((co.name.chars().take(2).collect(), rgb(co.color)));
                    }
                    for t in todos.iter().filter(|t| !t.start.is_empty()) {
                        agenda_kw.push((t.text.chars().take(2).collect(),
                            if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) }));
                    }
                    let ak = agenda_kw.len().min(2);
                    if ak > 0 {
                        let kw_fs = 8.5f32;
                        let pad = 3.0f32;
                        // 先量每个关键字宽度，整体居中
                        let kw_widths: Vec<f32> = agenda_kw.iter().take(ak).map(|(txt,_)| {
                            ui.fonts(|f| f.layout_no_wrap(txt.clone(), FontId::proportional(kw_fs), Color32::WHITE)).rect.width() + pad*2.0
                        }).collect();
                        let total: f32 = kw_widths.iter().sum::<f32>() + (ak as f32 - 1.0) * 2.0;
                        let start_x = rect.center().x - total / 2.0;
                        let by = rect.bottom() - 22.0;
                        let mut cur_x = start_x;
                        for (i, (txt, col)) in agenda_kw.iter().take(ak).enumerate() {
                            let w = kw_widths[i];
                            let kr = egui::Rect::from_min_size(Pos2::new(cur_x, by), Vec2::new(w, 13.0));
                            ui.painter().rect_filled(kr, Rounding::same(3.0),
                                Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 50));
                            ui.painter().rect_stroke(kr, Rounding::same(3.0),
                                Stroke::new(0.8, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 160)));
                            ui.painter().text(kr.center(), Align2::CENTER_CENTER, txt,
                                FontId::proportional(kw_fs), *col);
                            cur_x += w + 2.0;
                        }
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
                    if is_today {
                        // 当日高亮：占满左上角的圆角方块（数字画在其上）
                        let hb = egui::Rect::from_min_size(
                            Pos2::new(rect.left() + 4.0, rect.top() + 2.0), Vec2::new(25.0, 23.0));
                        ui.painter().rect_filled(hb, Rounding::same(radius::SM), c(theme::ACCENT));
                    }
                    let day_pos = if is_today { Pos2::new(x + 3.0, ty + 1.0) } else { Pos2::new(x, ty) };
                    ui.painter().text(day_pos, Align2::LEFT_TOP, d.day().to_string(),
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
                            let font = FontId::proportional(11.0);
                            // 按任务条实际宽度截断，窗口缩小时也不溢出格子边界
                            let label = trunc_to_width(ui, &t.text, font.clone(), tr.width() - 8.0);
                            ui.painter().text(Pos2::new(rect.left() + 10.0, ypos + 2.0), Align2::LEFT_TOP,
                                label, font, col);
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

        // 当日关联的阶段计划（紧凑色块 + 名称）
        let day_plans = self.db.plans_on(&dkey(&self.selected));
        if !day_plans.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("阶段计划:").size(10.5).color(c(theme::TEXT3)));
                for pl in day_plans {
                    ui.label(RichText::new(format!("■ {}", pl.name)).size(10.5).color(rgb(pl.color)));
                }
            });
        }

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
                        // 时间段输入（可留空=全天）
                        ui.add(egui::TextEdit::singleline(&mut self.add_start).hint_text("起").desired_width(38.0));
                        ui.label(RichText::new("–").size(10.0).color(c(theme::TEXT3)));
                        ui.add(egui::TextEdit::singleline(&mut self.add_end).hint_text("止").desired_width(38.0));
                        let resp = ui.add(egui::TextEdit::singleline(&mut self.add_text)
                            .hint_text("写下计划事项，回车添加…").desired_width((ui.available_width() - 46.0).max(40.0)));
                        if self.focus_add { resp.request_focus(); self.focus_add = false; }
                        // 单行 TextEdit 收到 Enter 会先 surrender_focus，必须用 lost_focus 判定；
                        // 内容为空时把焦点要回来，让用户能继续输入
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if self.add_text.trim().is_empty() { resp.request_focus(); } else { self.add_todo(); }
                        }
                        if ui.button("添加").clicked() { self.add_todo(); }
                    });
                });
        }

        let k = dkey(&self.selected);
        // ════════ 每日详情左右分栏：左=每日计划(按重要度)，右=每日日程表(按时间) ════════
        // 用 egui 原生 ui.columns 分栏，自动处理 ScrollArea 内坐标，最可靠
        let todos: Vec<Todo> = self.db.todos_of(&k).into_iter().cloned().collect();
        let mut pending_toggle: Vec<(u64, bool)> = Vec::new();
        let mut pending_del: Vec<u64> = Vec::new();
        let mut pending_edit: Vec<(u64, String)> = Vec::new();
        let mut pending_prio: Vec<(u64, u8)> = Vec::new();
        let mut pending_edit_open: Vec<u64> = Vec::new();
        let mut pending_ctx: Option<Pos2> = None;
        // ── 左右分栏（egui columns）──
        ui.columns(2, |cols| {
        // 左栏：每日计划
        let ui = &mut cols[0];
        ui.label(RichText::new("每日计划").size(12.0).strong().color(c(theme::ACCENT2)));
        if todos.is_empty() {
            ui.label(RichText::new("这一天还没有计划。右键面板打开添加栏，选好优先级写入安排；连续多日的安排请用左侧「阶段计划」创建。")
                .size(12.0).color(c(theme::TEXT3)));
        } else {
            for t in &todos {
                let mut done = t.done;
                let is_sel = self.sel_todo == Some(t.id);
                let is_editing = self.edit_todo == Some(t.id);
                ui.horizontal(|ui| {
                    let row_rect = ui.max_rect();
                    if ui.checkbox(&mut done, "").clicked() { pending_toggle.push((t.id, done)); }
                    let r = ui.allocate_response(Vec2::new(3.0, 17.0), egui::Sense::hover()).rect;
                    ui.painter().rect_filled(r, Rounding::same(1.0),
                        if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) });
                    // 高亮色跟随优先级/完成状态：高红 中橙 低蓝 已完成灰
                    let hl_base = if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) };
                    // 右侧要给 ≡菜单 + 低/中/高 留位置，再多留 12px 间隔，避免高亮框盖住优先级字
                    let content_w = (row_rect.width() - 92.0).max(80.0);
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
                        // 文字块用 Frame 包住：Frame 背景跟随内容 min_rect（含多行 wrap），
                        // 一个计划的多行文字自然被一个圆角框圈住；选中加亮+字重。
                        let hovered = ui.rect_contains_pointer(row_rect);
                        let alpha = if is_sel { 70 } else if hovered { 34 } else { 0 };
                        let fill_col = if alpha > 0 {
                            Color32::from_rgba_unmultiplied(hl_base.r(), hl_base.g(), hl_base.b(), alpha)
                        } else { Color32::TRANSPARENT };
                        let frame = egui::Frame::none()
                            .fill(fill_col)
                            .rounding(Rounding::same(radius::SM))
                            .inner_margin(Margin::symmetric(6.0, 3.0));
                        let disp = if t.start.is_empty() { t.text.clone() }
                                   else if t.end.is_empty() { format!("[{}] {}", t.start, t.text) }
                                   else { format!("[{}-{}] {}", t.start, t.end, t.text) };
                        let style = if t.done {
                            RichText::new(&disp).size(13.0).color(c(theme::TEXT3)).strikethrough()
                        } else if is_sel {
                            // 选中：提亮到近白 + 加粗字重
                            RichText::new(&disp).size(13.5).color(c(theme::TEXT)).strong()
                        } else {
                            RichText::new(&disp).size(13.0).color(c(theme::TEXT2))
                        };
                        frame.show(ui, |ui| {
                            ui.set_width(content_w - 12.0);
                            let resp = ui.add(egui::Label::new(style).wrap().sense(egui::Sense::click()));
                            if resp.clicked() { self.sel_todo = Some(t.id); }
                            if resp.secondary_clicked() { self.sel_todo = Some(t.id); pending_ctx = Some(ui_pos(ui)); }
                            if resp.double_clicked() {
                                self.edit_todo = Some(t.id);
                                self.edit_text = t.text.clone();
                                self.sel_todo = Some(t.id);
                                self.edit_rect = None;
                            }
                        });
                        // 文字框与右侧低/中/高之间留 ~10px 间隔
                        ui.add_space(10.0);
                    }
                    // 菜单按钮 + 优先级文字固定在行右缘（right_to_left 保证每行同一垂直列对齐）
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // ≡ 方块菜单：编辑文字 / 删除（push_id 防每行串台）
                        ui.push_id(("todo_menu", t.id), |ui| {
                            egui::menu::menu_custom_button(ui,
                            egui::Button::new(RichText::new("⋮").font(FontId::new(15.0, egui::FontFamily::Name("sym".into()))))
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::new(1.0, c(theme::STROKE_HI)))
                                .rounding(Rounding::same(4.0))
                                .min_size(Vec2::new(13.0, 22.0)), |ui| {
                                if ui.button("✎  编辑文字").clicked() { pending_edit_open.push(t.id); ui.close_menu(); }
                                if ui.button("×  删除").clicked() { pending_del.push(t.id); ui.close_menu(); }
                            });
                        });
                        // 优先级文字：点击循环切换 高→中→低→高
                        let prio_label = ["", "高", "中", "低"][t.prio as usize];
                        let pr = ui.add(egui::Button::new(RichText::new(prio_label).size(9.5)
                            .color(if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) })).frame(false));
                        if pr.clicked() {
                            let next = (t.prio % 3) + 1; // 1→2→3→1
                            pending_prio.push((t.id, next));
                        }
                        pr.on_hover_text("点击切换优先级");
                    });
                });
            }
        }
        // 右栏：每日日程表（课程 + 带时段待办，按时间排序）
        let ui = &mut cols[1];
        ui.label(RichText::new("每日日程表").size(12.0).strong().color(c(theme::MINT)));
            // 课程 + 带时段待办，按开始时间排序
            #[derive(Clone)] struct Row { start: String, end: String, text: String, col: Color32, course: bool, id: u64 }
            let mut rows: Vec<Row> = Vec::new();
            for c0 in self.db.courses_on(self.selected.weekday().num_days_from_sunday() as u8, &k) {
                rows.push(Row { start:c0.start.clone(), end:c0.end.clone(), text:c0.name.clone(),
                    col:rgb(c0.color), course:true, id:c0.id });
            }
            for t in self.db.todos_of(&k) {
                if !t.start.is_empty() {
                    rows.push(Row { start:t.start.clone(), end:t.end.clone(), text:t.text.clone(),
                        col:if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) }, course:false, id:t.id });
                }
            }
            if self.db.schedule_by_time { rows.sort_by(|a,b| a.start.cmp(&b.start).then(a.id.cmp(&b.id))); }
            else { rows.sort_by(|a,b| b.course.cmp(&a.course).then(a.id.cmp(&b.id))); }
            if rows.is_empty() {
                ui.label(RichText::new("今天没有时段安排。课程按周重复，带时段的待办会自动出现在这里。")
                    .size(11.0).color(c(theme::TEXT3)));
            }
            let mut edit_course_click: Option<u64> = None;
            for r0 in &rows {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}-{}", r0.start, r0.end)).size(10.5).color(c(theme::TEXT3)));
                    // 手绘小色条代替 ▮/▪ 字符（字符会被渲染成实心方块像乱码）
                    let bar = ui.allocate_response(Vec2::new(3.0, 14.0), egui::Sense::hover()).rect;
                    ui.painter().rect_filled(bar, Rounding::same(1.5), r0.col);
                    let txt = trunc_to_width(ui, &r0.text, FontId::proportional(11.0), (ui.available_width() - 90.0).max(40.0));
                    ui.label(RichText::new(txt).size(11.5).color(r0.col).strong());
                    if r0.course && ui.button("编辑").clicked() { edit_course_click = Some(r0.id); }
                });
            }
            if let Some(id) = edit_course_click { self.open_course_form(Some(id)); }
        }); // 关闭分栏 columns
        for (id, val) in pending_toggle {
            if let Some(item) = self.db.todos.iter_mut().find(|x| x.id == id) { item.done = val; }
            self.save_later();
        }
        for id in pending_del { self.db.todos.retain(|x| x.id != id); self.save_later(); }
        for (id, prio) in pending_prio {
            if let Some(item) = self.db.todos.iter_mut().find(|x| x.id == id) { item.prio = prio; }
            self.save_later();
        }
        for id in pending_edit_open {
            self.edit_todo = Some(id);
            self.edit_text = self.db.todos.iter().find(|x| x.id == id).map(|x| x.text.clone()).unwrap_or_default();
            self.sel_todo = Some(id);
            self.edit_rect = None;
        }
        for (id, txt) in pending_edit {
            if let Some(item) = self.db.todos.iter_mut().find(|x| x.id == id) { item.text = txt; }
            self.edit_todo = None;
            self.save_later();
        }
        if let Some(p) = pending_ctx { self.ctx(CtxKind::Calendar, p); }
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
                self.edit_plan_id = None; self.jump_open = false; self.io_open = false;
                self.course_open = false; self.course_edit_id = None; self.slot_open = false;
            }
            // 编辑/新建/弹窗态：屏蔽 N/P/T 与左右翻页快捷键
            // 注意必须短路（!editing && consume_key），否则编辑框里输入 n/p/t 会被拦截去触发新建
            let editing = self.edit_todo.is_some() || self.edit_plan_id.is_some()
                          || self.jump_open || self.io_open || self.plan_open || self.add_open
                          || self.course_open || self.slot_open;
            if !editing && i.consume_key(egui::Modifiers::NONE, egui::Key::N) { self.open_add(); }
            if !editing && i.consume_key(egui::Modifiers::NONE, egui::Key::P) { self.open_plan_form(); }
            if !editing && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) { self.shift_page(-1); }
            if !editing && i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) { self.shift_page(1); }
            if !editing && i.consume_key(egui::Modifiers::NONE, egui::Key::T) { self.goto_today(); }
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
                    CtxKind::Course(id) => {
                        ui.label(RichText::new("日程").size(10.0).color(c(theme::TEXT3)));
                        if ctx_btn(ui, "✎  编辑这个日程") { self.open_course_form(Some(id)); close = true; }
                        if ctx_btn(ui, "×  删除这个日程") {
                            self.db.courses.retain(|c| c.id != id); self.save_later(); close = true;
                        }
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
/// 按可用像素宽度截断文字（含省略号），用于窄格子里的任务条，防止溢出边界
fn trunc_to_width(ui: &egui::Ui, s: &str, font: FontId, max_w: f32) -> String {
    if s.is_empty() { return String::new(); }
    let full_w = ui.fonts(|f| f.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE))
        .rect.width();
    if full_w <= max_w { return s.to_string(); }
    let ell_w = ui.fonts(|f| f.layout_no_wrap("…".to_string(), font.clone(), Color32::WHITE))
        .rect.width();
    let avail = (max_w - ell_w).max(0.0);
    let mut out = String::new();
    let mut w = 0.0f32;
    for ch in s.chars() {
        let cw = ui.fonts(|f| f.layout_no_wrap(ch.to_string(), font.clone(), Color32::WHITE))
            .rect.width();
        if w + cw > avail { break; }
        w += cw;
        out.push(ch);
    }
    if out.is_empty() { "…".to_string() } else { format!("{}…", out) }
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
            .with_title("Planum · 计划日历")
            .with_icon(std::sync::Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native("Planum · 计划日历", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
        .expect("failed to launch Planum");
}
