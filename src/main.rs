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
fn tint_titlebar(hwnd: isize, bg: Color32, dark_mode: bool) {
    unsafe {
        let dark: i32 = if dark_mode { 1 } else { 0 };
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
use std::sync::{Mutex, OnceLock};

/// 颜色混合（用于 hover / 过渡 / 皮肤交叉淡入）
fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (a.r() as f32 * (1.0 - t) + b.r() as f32 * t) as u8,
        (a.g() as f32 * (1.0 - t) + b.g() as f32 * t) as u8,
        (a.b() as f32 * (1.0 - t) + b.b() as f32 * t) as u8,
        255,
    )
}

// ── 皮肤系统 ──────────────────────────────────────────────────
// 一套皮肤 = 一组完整配色。界面上所有取色都走 c(theme::X)，
// 于是换皮肤时整个窗口（含自绘网格）会一起平滑过渡。
/// 一套皮肤的调色板
#[derive(Clone, Copy)]
struct Palette {
    name: &'static str,
    dark: bool,
    bg: Color32,
    panel: Color32,
    card: Color32,
    stroke: Color32,
    stroke_hi: Color32,
    text: Color32,
    text2: Color32,
    text3: Color32,
    accent: Color32,
    accent2: Color32,
    ink: Color32,
    cool: Color32,
    mint: Color32,
    done: Color32,
}

/// 按语义名取一套皮肤里的某个颜色
fn pal_color(p: &Palette, key: &str) -> Color32 {
    match key {
        "bg" => p.bg, "panel" => p.panel, "card" => p.card,
        "stroke" => p.stroke, "stroke_hi" => p.stroke_hi,
        "text" => p.text, "text2" => p.text2, "text3" => p.text3,
        "accent" => p.accent, "accent2" => p.accent2, "ink" => p.ink,
        "cool" => p.cool, "mint" => p.mint, "done" => p.done,
        _ => Color32::from_rgb(255, 0, 255),
    }
}

macro_rules! pal {
    ($name:expr, $dark:expr, $bg:expr, $panel:expr, $card:expr, $stroke:expr, $stroke_hi:expr,
     $text:expr, $text2:expr, $text3:expr, $accent:expr, $accent2:expr, $ink:expr,
     $cool:expr, $mint:expr, $done:expr) => {
        Palette {
            name: $name, dark: $dark,
            bg: c($bg), panel: c($panel), card: c($card),
            stroke: c($stroke), stroke_hi: c($stroke_hi),
            text: c($text), text2: c($text2), text3: c($text3),
            accent: c($accent), accent2: c($accent2), ink: c($ink),
            cool: c($cool), mint: c($mint), done: c($done),
        }
    };
}

static SKINS_CACHE: OnceLock<Vec<Palette>> = OnceLock::new();
/// 内置的整套皮肤（0 = 原来的曜夜深色，保证旧观感不变）
fn skins() -> &'static [Palette] {
    SKINS_CACHE.get_or_init(|| vec![
        // 曜夜 · 琥珀（原始配色）
        pal!("曜夜", true, "#0a0d14", "#111622", "#171e2c", "#283247", "#3b4863",
             "#f2f5fa", "#d6dfea", "#aab8cb", "#f2a14b", "#ffd9a8", "#241503",
             "#8fbfe4", "#7fd0b4", "#8593a8"),
        // 晨雾 · 明亮清爽（浅色）
        pal!("晨雾", false, "#eef2f8", "#ffffff", "#f6f9fd", "#d8e0ec", "#b9c6d9",
             "#18232f", "#3d4d63", "#64758e", "#2563eb", "#1746c4", "#ffffff",
             "#0e7490", "#0f9b83", "#94a3b8"),
        // 森屿 · 墨绿治愈
        pal!("森屿", true, "#09120f", "#10201a", "#152a22", "#25483b", "#356550",
             "#eafaf2", "#c2e6d5", "#92bca9", "#59d0a0", "#b9f2d6", "#062319",
             "#7cc9dd", "#9fe8b0", "#82998d"),
        // 霞光 · 玫瑰暖调
        pal!("霞光", true, "#130c13", "#1e1320", "#291b2b", "#472941", "#67395f",
             "#fdf1fa", "#efd0e6", "#cda3bf", "#f08fba", "#ffd2e6", "#2c0b1e",
             "#c79cd8", "#8fd9c0", "#9a8296"),
        // 深海 · 静蓝沉稳
        pal!("深海", true, "#080f1d", "#0e1931", "#15223d", "#22365d", "#304a79",
             "#eaf2ff", "#c9d9f2", "#9db2d6", "#6aa8ff", "#c3dbff", "#071a36",
             "#67d2ea", "#7fd9cd", "#8597b5"),
        // 暖阳 · 奶油暖调（第二套浅色，午后阳光感）
        pal!("暖阳", false, "#f7f3ec", "#fffdf8", "#f0e9dd", "#ded3c1", "#c8b99f",
             "#2e241a", "#4f4132", "#7d6d59", "#c2622f", "#9c4518", "#fff7ee",
             "#0e7490", "#0f8a6a", "#8a7a66"),
        // 星黛 · 暮紫（新增整套：深靛底 + 薰衣草紫强调，冷色但柔和）
        pal!("星黛", true, "#0d0b16", "#171328", "#201a35", "#36304f", "#4d4470",
             "#f4f0ff", "#d5cced", "#9e93bd", "#b18cff", "#d9c6ff", "#1a1030",
             "#8ecae6", "#86e0c0", "#9a90b5"),
        // 青瓷 · 淡碧（第三套浅色：瓷白底 + 青瓷绿强调，清爽安静）
        pal!("青瓷", false, "#eef6f3", "#ffffff", "#f5fbf8", "#cfe2dc", "#a6c9be",
             "#16302a", "#34564d", "#5e7a71", "#0f8f7a", "#0a6b5b", "#ffffff",
             "#0e7490", "#0f9b83", "#8aa79f"),
        // 樱雾 · 浅粉（第四套浅色：樱粉白底 + 玫瑰粉强调，柔和清新）
        pal!("樱雾", false, "#fdf5f8", "#ffffff", "#fbeef4", "#eddbe4", "#dcbfd0",
             "#392634", "#64495c", "#91788a", "#d4548a", "#a83266", "#ffffff",
             "#0e7490", "#0f9b83", "#9a8296"),
    ])
}

/// 皮肤切换状态：from → to 的交叉淡入进度
#[derive(Clone, Copy)]
struct SkinState { from: usize, to: usize, t: f32 }
static SKIN_STATE: OnceLock<Mutex<SkinState>> = OnceLock::new();
fn skin_state() -> &'static Mutex<SkinState> {
    SKIN_STATE.get_or_init(|| Mutex::new(SkinState { from: 0, to: 0, t: 1.0 }))
}
/// 当前（目标）皮肤序号
fn active_skin() -> usize { skin_state().lock().unwrap().to }
/// 直接取“目标皮肤”的颜色（不受淡入进度影响，给标题栏染色用）
fn target_color(key: &str) -> Color32 {
    let list = skins();
    pal_color(&list[active_skin().min(list.len() - 1)], key)
}
/// 当前皮肤是否为深色（决定 egui 主题与标题栏文字明暗）
fn skin_dark() -> bool { skins()[active_skin().min(skins().len() - 1)].dark }
/// 立即切换且不带淡入（启动时恢复已保存的皮肤，避免开场闪一下旧色）
fn set_skin_now(i: usize) {
    let i = i.min(skins().len() - 1);
    let mut st = skin_state().lock().unwrap();
    st.from = i; st.to = i; st.t = 1.0;
}
fn set_skin(i: usize) {
    let n = skins().len();
    let i = i.min(n - 1);
    let mut st = skin_state().lock().unwrap();
    if st.to == i { return; }
    st.from = st.to;   // 过渡中途再切换：从当前目标色继续，避免跳回旧色
    st.to = i;
    st.t = 0.0;
}
/// 推进淡入，返回是否仍在过渡
fn advance_skin(dt: f32) -> bool {
    let mut st = skin_state().lock().unwrap();
    if st.t >= 1.0 { return false; }
    st.t = (st.t + dt / 0.32).min(1.0);
    if st.t >= 1.0 { st.from = st.to; }
    true
}

/// 统一取色入口：语义键跟随当前皮肤（含淡入），字面 #hex 原样解析
fn c(key: &str) -> Color32 {
    if key.starts_with('#') {
        return Color32::from_hex(key).unwrap_or(Color32::from_rgb(255, 0, 255));
    }
    let st = *skin_state().lock().unwrap();
    let list = skins();
    let a = pal_color(&list[st.from.min(list.len() - 1)], key);
    let b = pal_color(&list[st.to.min(list.len() - 1)], key);
    if st.t >= 1.0 || st.from == st.to { b } else { mix(a, b, st.t) }
}
/// 带透明度的颜色
fn ca(key: &str, a: u8) -> Color32 {
    let col = c(key);
    Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), a)
}

/// 皮肤预览色块：底色 + 面板色 + 主色条，选中带描边，悬停轻微抬升。
/// 用真实配色做预览，比单色圆点更直观，也让换肤按钮手感更统一。
fn skin_swatch(ui: &mut egui::Ui, skin: &Palette, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(26.0, 18.0), egui::Sense::click());
    // 悬停用缓动动画过渡（约 0.15s），不再一瞬间跳变，手感更柔和自然
    let hover_t = ui.ctx().animate_bool(ui.id().with("skin_swatch").with(skin.name), resp.hovered());
    let r = rect.shrink(1.0 - hover_t);
    let p = ui.painter();
    let round = Rounding::same(radius::XS);
    // 底色 → 内层面板 → 底部主色条
    p.rect_filled(r, round, skin.bg);
    let inner = r.shrink(3.0);
    p.rect_filled(inner, Rounding::same(2.0), skin.panel);
    let bar = egui::Rect::from_min_max(
        egui::pos2(inner.min.x, inner.max.y - 4.0),
        egui::pos2(inner.max.x, inner.max.y),
    );
    p.rect_filled(bar, Rounding::same(2.0), skin.accent);
    // 选中描边跟随当前皮肤主强调色；未选中用该皮肤自己的描边色
    let (stroke_col, width) = if selected {
        (c(theme::ACCENT2), 2.0_f32)
    } else {
        (skin.stroke, 1.0_f32)
    };
    p.rect_stroke(r, round, Stroke::new(width, stroke_col));
    resp
}

mod theme {
    // 语义色键：实际颜色由当前皮肤决定
    pub const BG: &str        = "bg";
    pub const PANEL: &str     = "panel";
    pub const CARD: &str      = "card";
    pub const STROKE: &str    = "stroke";
    pub const STROKE_HI: &str = "stroke_hi";
    pub const TEXT: &str      = "text";
    pub const TEXT2: &str     = "text2";
    pub const TEXT3: &str     = "text3";
    pub const ACCENT: &str    = "accent";
    pub const ACCENT2: &str   = "accent2";
    pub const ACCENT_INK: &str = "ink";
    pub const COOL: &str      = "cool";
    pub const MINT: &str      = "mint";
    pub const DONE: &str      = "done";
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
    add_keyword: String,
    add_open: bool,
    // 打开表单后自动聚焦输入框（首帧 request_focus 一次）
    focus_add: bool,
    // 待办选中/双击编辑
    sel_todo: Option<u64>,
    edit_todo: Option<u64>,
    edit_text: String,
    edit_keyword: String,
    edit_rect: Option<egui::Rect>,
    plan_name: String,
    plan_start: String,
    plan_end: String,
    plan_color: usize,
    plan_open: bool,
    // 阶段计划编辑
    edit_plan_id: Option<u64>,
    focus_plan: bool,
    // 点击表单外即取消；刚打开那一下不算点击外部
    plan_clicked_elsewhere: bool,
    plan_toggled: bool,
    // 新建阶段计划表单的安全宽度：按左栏实际宽度算，避免表单铺满滚动区而越过面板右缘
    left_form_max_w: f32,
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
    course_keyword: String,
    course_start_date: String,
    course_end_date: String,
    course_color: [f32; 3],
    course_slot_idx: usize,
    focus_course: bool,
    // AI 课表导入提示词弹窗
    prompt_open: bool,
    // 粘贴 AI 回复直接导入（免去手动存文件）
    paste_text: String,
    paste_msg: String,
    // 时间段设置面板
    slot_open: bool,
    slot_draft: Vec<(String, String)>,

    // 日期/时间选择器（每个输入框都配一个点选按钮）
    picker: Option<PickerState>,
    picker_pos: Pos2,
    picker_rect: Option<egui::Rect>,
    picker_toggled: bool,

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
enum CtxKind { None, PlanZone, Calendar, MonthCell(NaiveDate), Course(u64) }

// ═══════════════════════════════════════════════════════════════
// 日期 / 时间选择器：手输与点选并存
// 点小按钮弹出面板：日期可选年月日，时间可选时分秒（24 小时制、零填充），
// 空值或非法值时默认展示「现在」，也保留原来的手动输入方式。
// ═══════════════════════════════════════════════════════════════
#[derive(Clone, Copy, PartialEq)]
enum PickerKind { Date, Time }

#[derive(Clone, Copy)]
struct PickerState {
    id: &'static str,
    /// 时间段列表里的行号（其它字段固定为 0）
    idx: usize,
    kind: PickerKind,
    date: NaiveDate,
    h: u32,
    m: u32,
    s: u32,
}

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
        let db = data::load();
        set_skin_now(db.skin); // 恢复上次选中的皮肤，再据此生成控件样式（无需淡入）
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
            add_keyword: String::new(),
            add_open: false,
            focus_add: false,
            sel_todo: None,
            edit_todo: None,
            edit_text: String::new(),
            edit_keyword: String::new(),
            edit_rect: None,
            plan_name: String::new(),
            plan_start: String::new(),
            plan_end: String::new(),
            plan_color: 0,
            plan_open: false,
            edit_plan_id: None,
            focus_plan: false,
            plan_clicked_elsewhere: false,
            plan_toggled: false,
            left_form_max_w: 280.0,
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
            course_keyword: String::new(),
            course_start_date: String::new(),
            course_end_date: String::new(),
            course_color: data::default_course_color(),
            course_slot_idx: 0,
            focus_course: false,
            prompt_open: false,
            paste_text: String::new(),
            paste_msg: String::new(),
            slot_open: false,
            slot_draft: Vec::new(),
            picker: None,
            picker_pos: Pos2::new(120.0, 120.0),
            picker_rect: None,
            picker_toggled: false,
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
            keyword: self.add_keyword.trim().to_string(),
        });
        self.add_text.clear();
        self.add_keyword.clear();
        self.save_later();
    }

    fn open_plan_form(&mut self) {
        self.plan_open = true;
        self.plan_toggled = true;
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
            self.plan_toggled = true;
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
        self.add_start.clear(); self.add_end.clear(); self.add_keyword.clear();
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
                self.course_keyword = c.keyword.clone();
                self.course_start_date = c.start_date.clone(); self.course_end_date = c.end_date.clone();
            }
        } else {
            self.course_name.clear(); self.course_wd = self.selected.weekday().num_days_from_sunday() as u8;
            let slots = self.db.slots();
            self.course_slot_idx = 0;
            self.course_start = slots.get(0).map(|x| x.start.clone()).unwrap_or_else(|| "08:00".into());
            self.course_end = slots.get(0).map(|x| x.end.clone()).unwrap_or_else(|| "09:40".into());
            self.course_place.clear();
            self.course_keyword.clear();
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
                c.keyword = self.course_keyword.trim().to_string();
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
                keyword: self.course_keyword.trim().to_string(),
                start_date: self.course_start_date.trim().to_string(),
                end_date: self.course_end_date.trim().to_string(), color: col,
                skip_dates: Vec::new(),
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

    /// 把粘贴框里的 AI 回复直接解析导入，免去「存成 .json 再选文件」那一步
    fn import_ai_reply(&mut self) {
        match data::import_courses_from_reply(&self.paste_text) {
            Ok(mut cs) => {
                let n = cs.len();
                self.db.courses.append(&mut cs);
                self.paste_text.clear();
                self.paste_msg = format!("已直接导入 {} 节课，无需另存文件", n);
                self.io_msg = self.paste_msg.clone();
                self.save_later();
            }
            Err(e) => {
                self.paste_msg = format!("导入失败：{}", e);
                self.io_msg = self.paste_msg.clone();
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

    /// 取消阶段计划表单（取消按钮 / 点击外部 / Esc 共用）
    fn cancel_plan(&mut self) {
        self.plan_open = false;
        self.edit_plan_id = None;
        self.plan_name.clear();
        self.plan_start.clear();
        self.plan_end.clear();
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

    // 清空逻辑在数据层（Db::clear_day_*），这里只在真的改动后安排写盘。
    fn clear_day_plans(&mut self, d: NaiveDate) {
        if self.db.clear_day_plans(d) { self.save_later(); }
    }
    fn clear_day_todos(&mut self, d: NaiveDate) {
        if self.db.clear_day_todos(d) { self.save_later(); }
    }
    fn restore_day_courses(&mut self, d: NaiveDate) {
        if self.db.restore_day_courses(d) { self.save_later(); }
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
    // 跟随当前皮肤：深色皮肤用 Dark，浅色皮肤用 Light。
    // egui 0.29 的 dark_style / light_style 是两套， all_styles_mut 同时写入，
    // 于是哪怕主题被系统改动，自绘配色也不会被默认色冲掉。
    let want_dark = skin_dark();
    ctx.set_theme(if want_dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
    ctx.all_styles_mut(|style| {
        // 排版层级：display / title / body / label（严格 4 级）
        style.text_styles.insert(egui::TextStyle::Small,     FontId::new(11.0, egui::FontFamily::Proportional));
        style.text_styles.insert(egui::TextStyle::Body,      FontId::new(13.0, egui::FontFamily::Proportional));
        style.text_styles.insert(egui::TextStyle::Button,    FontId::new(12.5,egui::FontFamily::Proportional));
        style.text_styles.insert(egui::TextStyle::Heading,   FontId::new(20.0, egui::FontFamily::Proportional));
        style.text_styles.insert(egui::TextStyle::Monospace, FontId::new(11.0, egui::FontFamily::Monospace));
        style.spacing.item_spacing = Vec2::new(7.0, 5.0);
        style.spacing.button_padding = Vec2::new(11.0, 5.0);
        // 滚动条缩细、贴左栏右缘，不碍事
        style.spacing.scroll.bar_width = 5.0;
        style.spacing.scroll.bar_outer_margin = -9.0;
        style.spacing.scroll.bar_inner_margin = 1.0;
        let mut v = style.visuals.clone();
        v.dark_mode = want_dark;
        v.window_fill = c(theme::PANEL);
        v.panel_fill = c(theme::BG);
        v.extreme_bg_color = c(theme::CARD);
        v.override_text_color = Some(c(theme::TEXT));
        v.faint_bg_color = ca(theme::CARD, 150);
        v.code_bg_color = ca(theme::ACCENT, 28);
        v.hyperlink_color = c(theme::ACCENT2);
        // 按钮三态：底色 / 文字 / 圆角 / 轻微悬停抬升，手感更自然
        let r = Rounding::same(radius::SM);
        v.widgets.inactive.bg_fill = c(theme::CARD);
        v.widgets.inactive.bg_stroke = Stroke::new(0.6_f32, c(theme::STROKE));
        v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, c(theme::TEXT2));
        v.widgets.inactive.rounding = r;
        v.widgets.inactive.expansion = 0.0;
        v.widgets.hovered.bg_fill = mix(c(theme::CARD), c(theme::ACCENT), 0.22);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, c(theme::STROKE_HI));
        v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, c(theme::ACCENT2));
        v.widgets.hovered.rounding = r;
        v.widgets.hovered.expansion = 1.0;
        v.widgets.active.bg_fill = c(theme::ACCENT);
        v.widgets.active.fg_stroke = Stroke::new(1.0_f32, c(ACCENT_INK));
        v.widgets.active.rounding = r;
        v.widgets.active.expansion = 0.5;
        v.widgets.noninteractive.bg_fill = c(theme::PANEL);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, c(theme::TEXT3));
        v.widgets.noninteractive.rounding = r;
        v.selection.bg_fill = ca(theme::ACCENT, 70);
        v.selection.stroke = Stroke::new(1.0_f32, c(theme::ACCENT));
        // 弹窗 / 提示条：更柔的投影与圆角，浮层出现时不生硬
        v.window_shadow = egui::epaint::Shadow {
            offset: Vec2::new(0.0, 4.0), blur: 14.0, spread: 0.0,
            color: ca(theme::BG, 110),
        };
        v.popup_shadow = egui::epaint::Shadow {
            offset: Vec2::new(0.0, 3.0), blur: 10.0, spread: 0.0,
            color: ca(theme::BG, 90),
        };
        v.window_rounding = Rounding::same(radius::LG);
        style.visuals = v;
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // 主题跟随当前皮肤：浅色皮肤若被系统深色/旧偏好冲掉，控件会变回默认底色
        let want_theme = if skin_dark() { egui::Theme::Dark } else { egui::Theme::Light };
        if ctx.theme() != want_theme {
            ctx.set_theme(if skin_dark() { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
        }
        // ── 原生标题栏染成主面板底色（换皮肤后重新染色）──
        #[cfg(target_os = "windows")]
        if !self.dwm_applied {
            if let Some(h) = frame_hwnd(frame) {
                tint_titlebar(h, target_color(theme::BG), skin_dark());
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
        let t = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.last_frame = Instant::now();
        // 皮肤交叉淡入：过渡期间每帧同步刷新控件配色，全程平滑无跳变
        let fading = advance_skin(t);
        if fading { setup_style(ctx); }
        let animating = self.tick_anim(t) || fading;
        // 每帧先清掉「刚点开选择器」标记，中央面板里的点选按钮会重新置位
        self.picker_toggled = false;

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
                // 左栏宽度随窗口一起缩放：以内容宽的 25% 为基准（默认 1420 窗口≈347px），
                // 这样缩小窗口时左右两栏会一起收窄，而不是只压缩右栏。
                // 上限 352 防止超宽屏过胖，下限 220 保证日期与左侧表单仍可读。
                // right_left 由 left_w 算出，中缝与右栏左缘随之同步移动。
                let left_w = 352.0f32.min(area.width() * 0.25).max(220.0);
                // 表单可用宽 = 左栏宽 − 左右内边距(18×2) − 滚动条余量(16)，保证表单背景不出面板
                self.left_form_max_w = (left_w - 52.0).max(168.0);
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
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(left_rect), |u| {
                    // 背景手动画在固定 left_rect：egui 的 Frame 背景 = content_ui.min_rect()，
                    // 内容一溢出背景就铺出左栏、吃掉中缝；rect_filled 画固定矩形则永不串色。
                    u.painter().rect_filled(left_rect, Rounding::same(radius::LG), c(theme::PANEL));
                    u.painter().rect_stroke(left_rect, Rounding::same(radius::LG),
                        Stroke::new(1.0_f32, c(theme::STROKE)));
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
                            egui::ScrollArea::vertical().id_salt("left_lists_scroll")
                                .auto_shrink([false, false])
                                .scroll_bar_rect(sb_rect)
                                .show(ui, |ui| { self.left_lists(ui); });
                        });
                });
                // 右栏：绝对矩形，内部三段式固定分区
                ui.allocate_new_ui(egui::UiBuilder::new().max_rect(right_rect), |u| {
                    u.painter().rect_filled(right_rect, Rounding::same(radius::LG), c(theme::PANEL));
                    u.painter().rect_stroke(right_rect, Rounding::same(radius::LG),
                        Stroke::new(1.0_f32, c(theme::STROKE)));
                    egui::Frame::none()
                        .inner_margin(Margin::symmetric(18.0, 16.0)).show(u, |ui| {
                            self.right_panel(ui);
                        });
                });
            });

        self.handle_ctx(ctx);
        self.handle_picker(ctx);
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
        // 时钟只显示时分：时间不需要秒（进度条仍按秒平滑推进）
        ui.label(RichText::new(format!("{:02}:{:02}", now.hour(), now.minute()))
            .font(FontId::monospace(44.0)).color(c(theme::TEXT)));
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
                                .stroke(Stroke::new(1.0_f32, c(theme::STROKE_HI)))
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
        // 点击表单外取消（刚打开那一下不算；日期选择面板也不算）
        // 用 egui 的 clicked_elsewhere（基于点击发生时的 interact_pos），比 latest_pos 可靠
        if self.plan_open && !self.plan_toggled && self.plan_clicked_elsewhere {
            let in_picker = ui.input(|i| i.pointer.interact_pos().map(|p| {
                self.picker_rect.map(|r| r.expand(8.0).contains(p)).unwrap_or(false)
            }).unwrap_or(false));
            if !in_picker { self.cancel_plan(); }
        }
        self.plan_toggled = false;
        self.plan_clicked_elsewhere = false;
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
        // 表单曾经直接用 ui.available_width() 铺满滚动区，而滚动区宽度含滚动条与边距，
        // 表单背景(Frame 的 inner_margin 再加 10px)就越过左栏右缘。这里按左栏实测宽度
        // 锁死表单最大宽；allocate_ui_with_layout 只按实际内容高度推进父布局。
        let max_w = self.left_form_max_w.min(ui.available_width().max(80.0));
        let form = ui.allocate_ui_with_layout(
            Vec2::new(max_w, ui.available_height().max(160.0)),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                let fr = egui::Frame::none().fill(bg).rounding(Rounding::same(radius::SM))
                    .inner_margin(Margin::same(10.0)).show(ui, |ui| {
                        // TextEdit 自身会再加 4+4 内边距：扣掉它，宽度才真正贴住表单边界
                        let name_resp = ui.add(egui::TextEdit::singleline(&mut self.plan_name)
                            .hint_text("计划名称，如「项目冲刺」").desired_width(ui.available_width() - 8.0));
                        if self.focus_plan { name_resp.request_focus(); self.focus_plan = false; }
                        if name_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { self.save_plan(); }
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.plan_start).hint_text("起始 YYYY-MM-DD").desired_width(90.0));
                            dt_picker_btn(ui, "plan_start", 0, PickerKind::Date, &self.plan_start,
                                &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
                            ui.label(RichText::new("→").color(c(theme::TEXT3)));
                            ui.add(egui::TextEdit::singleline(&mut self.plan_end).hint_text("结束 YYYY-MM-DD").desired_width(90.0));
                            dt_picker_btn(ui, "plan_end", 0, PickerKind::Date, &self.plan_end,
                                &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
                        });
                        ui.horizontal_wrapped(|ui| {
                            for (i, col) in PLAN_COLORS.iter().enumerate() {
                                let on = i == self.plan_color;
                                let label = RichText::new(if on { "◉" } else { "●" }).color(rgb(*col)).size(if on {18.0} else {15.0});
                                if ui.add(egui::Button::new(label).frame(false)).clicked() { self.plan_color = i; }
                            }
                        });
                        ui.horizontal(|ui| {
                            let save_label = if self.edit_plan_id.is_some() { "保存修改" } else { "创建计划" };
                            if ui.button(save_label).clicked() { self.save_plan(); }
                            if ui.button("取消").clicked() { self.cancel_plan(); }
                        });
                    });
                fr.response
            },
        );
        self.plan_clicked_elsewhere = form.inner.clicked_elsewhere();
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
                    let pick_clicked = dt_picker_btn(ui, "jump_text", 0, PickerKind::Date, &self.jump_text,
                        &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
                    // 单行 TextEdit 收到 Enter 会先自己 surrender_focus，has_focus() 那帧已是 false，
                    // 必须用 lost_focus() && Enter 判定，否则回车只会关掉输入框却不跳转。
                    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if enter {
                        if let Some(d) = dt_parse_date(&self.jump_text) {
                            self.selected = d;
                            self.view = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
                            self.month_anim = 0.0; self.select_pulse = 0.0;
                        }
                        self.jump_open = false;
                    } else if resp.lost_focus() && !pick_clicked && self.picker.is_none() {
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
                    // ── 皮肤：一键换肤，整窗平滑淡入 ──
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("皮肤:").size(11.0).color(c(theme::TEXT2)));
                        let cur = active_skin();
                        for (i, skin) in skins().iter().enumerate() {
                            let on = i == cur;
                            if skin_swatch(ui, skin, on).on_hover_text(skin.name).clicked() {
                                set_skin(i);
                                self.db.skin = i;
                                self.save_later();
                                self.dwm_applied = false; // 标题栏按新皮肤重新染色
                                ui.ctx().request_repaint(); // 下一帧推进淡入
                            }
                        }
                        ui.add_space(2.0);
                        ui.label(RichText::new(skins()[cur].name).size(11.0).strong().color(c(theme::ACCENT2)));
                    });
                    section_rule(ui);
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
                        dt_picker_btn(ui, "io_date", 0, PickerKind::Date, &self.io_date,
                            &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
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
                        if ui.button("AI 导入提示词…").on_hover_text("复制提示词，连同课表文件发给 AI，就能得到可直接导入的 JSON").clicked() {
                            self.prompt_open = true;
                        }
                        if ui.button("×").clicked() { self.io_open = false; }
                    });
                    if !self.io_msg.is_empty() {
                        ui.label(RichText::new(&self.io_msg).size(10.5).color(c(theme::MINT)));
                    }
                });
        }
        // ── AI 课表导入提示词：复制后连同课表文件发给 AI，直接得到可导入的 JSON ──
        if self.prompt_open {
            egui::Window::new("AI 导入提示词")
                .resizable(true)
                .collapsible(false)
                .default_size(Vec2::new(640.0, 640.0))
                .min_size(Vec2::new(420.0, 420.0))
                .show(ui.ctx(), |ui| {
                    ui.label(RichText::new("怎么用").size(12.0).strong().color(c(theme::ACCENT2)));
                    ui.label(RichText::new("① 复制下面的提示词 → ② 把课表文件（截图 / 照片 / PDF / 文字）和提示词一起发给 AI → ③ 把 AI 的回复整段粘到下方 → ④ 点「粘贴并导入」，不用再存成文件。")
                        .size(11.0).color(c(theme::TEXT2)));
                    ui.add_space(3.0);
                    section_rule(ui);
                    egui::ScrollArea::vertical().auto_shrink(false).max_height(210.0).show(ui, |ui| {
                        egui::Frame::none().fill(c(theme::BG)).rounding(Rounding::same(radius::XS))
                            .inner_margin(Margin::symmetric(8.0, 8.0)).show(ui, |ui| {
                                ui.label(RichText::new(data::course_import_prompt())
                                    .size(11.0).color(c(theme::TEXT2)));
                            });
                    });
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        if ui.button("复制提示词").on_hover_text("复制到剪贴板").clicked() {
                            ui.ctx().copy_text(data::course_import_prompt().to_string());
                            self.io_msg = "提示词已复制，连同课表文件发给 AI 即可".into();
                        }
                    });
                    ui.add_space(4.0);
                    section_rule(ui);
                    ui.label(RichText::new("粘贴 AI 回复，直接导入")
                        .size(12.0).strong().color(c(theme::ACCENT2)));
                    ui.label(RichText::new("带代码块或前后说明文字都能识别，不用再另存 .json 文件。")
                        .size(11.0).color(c(theme::TEXT3)));
                    ui.add(egui::TextEdit::multiline(&mut self.paste_text)
                        .desired_width(f32::INFINITY)
                        .desired_rows(5)
                        .hint_text("把 AI 回复的整段文字粘到这里（Ctrl+V）…"));
                    if !self.paste_msg.is_empty() {
                        let is_err = self.paste_msg.starts_with("导入失败");
                        ui.label(RichText::new(&self.paste_msg).size(11.0)
                            .color(if is_err { c(theme::ACCENT2) } else { c(theme::MINT) }));
                    }
                    ui.horizontal(|ui| {
                        if ui.button("粘贴并导入").on_hover_text("直接读取上面粘贴的回复并导入课程").clicked() {
                            self.import_ai_reply();
                        }
                        if ui.button("清空").clicked() {
                            self.paste_text.clear();
                            self.paste_msg.clear();
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("关闭").clicked() { self.prompt_open = false; }
                        });
                    });
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
                        egui::ComboBox::from_id_salt("course_slot_combo").selected_text(cur_label).show_ui(ui, |ui| {
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
                        ui.label(RichText::new("关键词:").size(11.0).color(c(theme::TEXT3)));
                        ui.add(egui::TextEdit::singleline(&mut self.course_keyword)
                            .hint_text("选填，如 高数 / 模电 / 线代").desired_width(160.0))
                            .on_hover_text("按语义填 2~6 个字的短标签，日历小方块会显示它；留空则显示课程全名");
                        ui.label(RichText::new("留空则日历方块显示课程全名").size(9.5).color(c(theme::TEXT3)));
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("生效日期:").size(11.0).color(c(theme::TEXT3)));
                        ui.add(egui::TextEdit::singleline(&mut self.course_start_date).hint_text("起 YYYY-MM-DD").desired_width(110.0));
                        dt_picker_btn(ui, "course_start_date", 0, PickerKind::Date, &self.course_start_date,
                            &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
                        ui.label(RichText::new("–").size(11.0).color(c(theme::TEXT3)));
                        ui.add(egui::TextEdit::singleline(&mut self.course_end_date).hint_text("止 YYYY-MM-DD").desired_width(110.0));
                        dt_picker_btn(ui, "course_end_date", 0, PickerKind::Date, &self.course_end_date,
                            &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
                        ui.label(RichText::new("留空=永久").size(9.5).color(c(theme::TEXT3)));
                    });
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("上课时间:").size(11.0).color(c(theme::TEXT3)));
                        ui.add(egui::TextEdit::singleline(&mut self.course_start).hint_text("起 HH:MM").desired_width(80.0));
                        dt_picker_btn(ui, "course_start", 0, PickerKind::Time, &self.course_start,
                            &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
                        ui.label(RichText::new("–").size(11.0).color(c(theme::TEXT3)));
                        ui.add(egui::TextEdit::singleline(&mut self.course_end).hint_text("止 HH:MM").desired_width(80.0));
                        dt_picker_btn(ui, "course_end", 0, PickerKind::Time, &self.course_end,
                            &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
                        ui.label(RichText::new("可手输或点选，也可选上方时间段").size(9.5).color(c(theme::TEXT3)));
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
                            if filled { ui.painter().rect_stroke(rect.expand(2.0), Rounding::same(4.0), Stroke::new(2.0_f32, c(theme::TEXT))); }
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
                            dt_picker_btn(ui, "slot_start", i, PickerKind::Time, &*st,
                                &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
                            ui.label(RichText::new("–").size(11.0).color(c(theme::TEXT3)));
                            ui.add(egui::TextEdit::singleline(en).desired_width(62.0));
                            dt_picker_btn(ui, "slot_end", i, PickerKind::Time, &*en,
                                &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
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
            ui.painter().rect_filled(r, Rounding::same(radius::MD), ca(theme::BG, a));
        }

        section_rule(ui);

        // ③ 当日详情：独立滚动，任务再多也能看
        let remain = ui.available_height().max(110.0);
        egui::ScrollArea::vertical().id_salt("day_agenda_scroll")
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
        // 先只占位拿到精确矩形，等网格 top 确定后统一按悬停状态上色
        let mut base_x = ui.cursor().left();
        let mut head_rects: Vec<egui::Rect> = Vec::new();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            // 第一格：时段列表头
            let (lab_rect, _) = ui.allocate_exact_size(Vec2::new(lab_w, head_h), egui::Sense::hover());
            base_x = lab_rect.left();
            head_rects.push(lab_rect);
            for _ in days.iter() {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(col_w, head_h), egui::Sense::hover());
                head_rects.push(rect);
            }
        });

        // ── 时间段网格 ──
        let grid_top = ui.cursor().top();
        // 悬停联动：停在某天某时段 → 高亮那天的星期表头 + 那个时段表头
        let hover_pos = ui.ctx().input(|i| i.pointer.hover_pos());
        let hover = hover_pos.and_then(|p| {
            if p.y >= grid_top && p.y < grid_top + nslot as f32 * slot_h
               && p.x >= base_x + lab_w + gap {
                let di = ((p.x - (base_x + lab_w + gap)) / (col_w + gap)) as usize;
                let si = ((p.y - grid_top) / slot_h) as usize;
                if di < 7 && si < nslot { return Some((di, si)); }
            }
            None
        });
        let hover_slot = hover.map(|(_, si)| si);

        // 时段列表头：悬停某时段时整格高亮
        let lab_rect = head_rects[0];
        if hover_slot.is_some() {
            ui.painter().rect_filled(lab_rect, Rounding::same(radius::SM), c(theme::ACCENT));
        }
        ui.painter().text(lab_rect.center(), Align2::CENTER_CENTER, "时段",
            FontId::proportional(11.0),
            if hover_slot.is_some() { c(ACCENT_INK) } else { c(theme::TEXT3) });
        // 星期表头：悬停优先于今天/选中
        for (i, d) in days.iter().enumerate() {
            let wd = d.weekday().num_days_from_sunday() as u8; // 真实星期：0=周日
            let rect = head_rects[i + 1];
            let is_today = *d == self.today;
            let is_sel = *d == self.selected;
            let is_hover = hover.map(|(di, _)| di) == Some(i);
            let fill = if is_hover || is_today { c(theme::ACCENT) }
                       else if is_sel { ca(theme::ACCENT2, 60) }
                       else { c(theme::CARD) };
            ui.painter().rect_filled(rect, Rounding::same(radius::SM), fill);
            let col = if is_hover || is_today { c(ACCENT_INK) } else { c(theme::TEXT) };
            ui.painter().text(rect.center(), Align2::CENTER_CENTER,
                format!("{} {}", WEEK[wd as usize], d.day()), FontId::proportional(12.0), col);
        }

        // 时段标签：悬停时段所在行整格高亮 + 文字反色
        for (si, slot) in slots.iter().enumerate() {
            let yt = grid_top + si as f32 * slot_h;
            let cell_rect = egui::Rect::from_min_size(Pos2::new(base_x, yt), Vec2::new(lab_w, slot_h - gap));
            let is_hover = hover_slot == Some(si);
            ui.painter().rect_filled(cell_rect, Rounding::same(2.0),
                if is_hover { c(theme::ACCENT) } else { ca(theme::BG, 120) });
            // 时段标签：起止时间在方块内上下居中、拉开距离（约 1/3 与 2/3 高度处）
            let cx = cell_rect.center().x;
            let y1 = cell_rect.top() + cell_rect.height() / 3.0;
            let y2 = cell_rect.top() + cell_rect.height() * 2.0 / 3.0;
            ui.painter().text(Pos2::new(cx, y1), Align2::CENTER_CENTER, &slot.start,
                FontId::proportional(10.5), if is_hover { c(ACCENT_INK) } else { c(theme::TEXT) });
            ui.painter().text(Pos2::new(cx, y2), Align2::CENTER_CENTER, &slot.end,
                FontId::proportional(10.5), if is_hover { c(ACCENT_INK) } else { c(theme::TEXT2) });
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
                    text: data::course_label(co), color: rgb(co.color), is_course: true });
            }
            for t in self.db.todos_of(&dk) {
                if !t.start.is_empty() {
                    let name = if t.keyword.trim().is_empty() { t.text.clone() } else { t.keyword.trim().to_string() };
                    items.push(Item { id: t.id, start: t.start.clone(), end: t.end.clone(), text: name,
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
                // 悬停不再铺淡色高亮底（会冲淡卡片文字）：有内容的卡片改为轻微放大反馈
                let hover_cell = hover == Some((di, si));
                // 方块增强：有内容的格子画圆角卡片；颜色可跟随日程/优先级
                if !in_slot.is_empty() {
                    let first_col = in_slot[0].color;
                    let (fill, stroke_col) = if self.db.color_by_schedule {
                        (Color32::from_rgba_unmultiplied(first_col.r(), first_col.g(), first_col.b(), 46),
                         Color32::from_rgba_unmultiplied(first_col.r(), first_col.g(), first_col.b(), 150))
                    } else {
                        (ca(theme::CARD, 210), c(theme::STROKE))
                    };
                    // 轻微放大：平时略内缩，悬停展开到整格；亮度不变，文字依旧清晰
                    let card_rect = cr.expand(if hover_cell { -0.5 } else { -2.0 });
                    ui.painter().rect_filled(card_rect, Rounding::same(4.0), fill);
                    ui.painter().rect_stroke(card_rect, Rounding::same(4.0),
                        Stroke::new(if hover_cell { 1.5_f32 } else { 1.0_f32 }, stroke_col));
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
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(ar), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("全天").size(8.5).color(c(theme::TEXT3)));
                for t in all_todos.iter().take(2) {
                    let src = if t.keyword.trim().is_empty() { t.text.clone() } else { t.keyword.trim().to_string() };
                    let label = trunc_to_width(ui, &src, FontId::proportional(9.0), col_w - 5.0);
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
                    // 日程 / 每日计划在卡片中部画带关键词的语义色小方块，阶段计划在底部画纯色小方块
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
                        mix(c(theme::CARD), c(theme::ACCENT), 0.15 + pulse * 0.18)
                    } else if hovered { c(theme::STROKE) }
                      else if is_today { mix(c(theme::CARD), c(theme::ACCENT), 0.08) }
                      else { c(theme::CARD) };
                    ui.painter().rect_filled(rect, Rounding::same(radius::SM), bg);
                    // 描边层次
                    let border = if sel { c(theme::ACCENT) }
                                 else if marked { ca(theme::ACCENT, 150) }
                                 else if hovered { c(theme::STROKE_HI) }
                                 else { c(theme::STROKE) };
                    ui.painter().rect_stroke(rect, Rounding::same(radius::SM),
                        Stroke::new(if sel || marked { 1.5_f32 } else { 0.6_f32 }, border));
                    // 悬停抬升 + 顶部柔和圆角小矩形条（替代左侧光带/生硬横线）
                    if hovered && !sel {
                        // 整格轻轻抬起：两层极淡外发光，过渡柔和不清脆
                        ui.painter().rect_filled(rect.expand(2.2), Rounding::same(radius::SM + 2.0), ca(theme::ACCENT, 14));
                        ui.painter().rect_filled(rect.expand(1.1), Rounding::same(radius::SM + 1.0), ca(theme::ACCENT, 20));
                        // 顶部居中的短圆角条：柔光底 + 半透明主体；贴在格子上缘，不压日期数字
                        let bw = (rect.width() * 0.42).clamp(14.0, 40.0);
                        let bar = egui::Rect::from_min_size(
                            Pos2::new(rect.center().x - bw / 2.0, rect.top() + 0.3), Vec2::new(bw, 3.0));
                        ui.painter().rect_filled(bar.expand(2.4), Rounding::same(4.0), ca(theme::ACCENT, 30));
                        ui.painter().rect_filled(bar, Rounding::same(1.5), ca(theme::ACCENT, 205));
                    }
                    // 日程 / 每日计划 / 阶段计划统一到下方的紧凑内容流中绘制
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
                    // ── 分区布局：中部「每日计划 + 日程」带关键词的小方块；底部「阶段计划」彩色小方块 ──
                    // 每日计划与日程不再区分，统一成圆角小方块，每个方块包裹一个关键词；
                    // 阶段计划有重复性，不写文字，只用彩色方块，在卡片底部单独成区。
                    let a_main = if out { 150u8 } else { 255 };
                    let fade = |col: Color32, a: u8| Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), a);
                    let mut daily: Vec<(String, Color32, bool)> = Vec::new();
                    for co in self.db.courses_on(d.weekday().num_days_from_sunday() as u8, &k) {
                        daily.push((data::course_label(co), rgb(co.color), false));   // 日程（课程）
                    }
                    for t in todos.iter() {
                        let col = if t.done { c(theme::DONE) } else { rgb(prio_color(t.prio)) };
                        // 卡片显示关键词；未填关键词时退回完整事项文字
                        let label = if t.keyword.trim().is_empty() { t.text.clone() } else { t.keyword.trim().to_string() };
                        daily.push((label, col, t.done));             // 每日计划
                    }
                    let inner_x = rect.left() + 5.0;
                    let inner_w = (rect.width() - 10.0).max(1.0);

                    // 底部阶段计划区：小方块水平居中，占一行
                    let plan_sq = 7.0f32;
                    let plan_gap = 3.0f32;
                    let plans_cap = ((inner_w + plan_gap) / (plan_sq + plan_gap)).floor().max(1.0) as usize;
                    let plan_shown = plans.len().min(plans_cap);
                    let plan_band_h = if plan_shown > 0 { plan_sq + 5.0 } else { 0.0 };

                    // 中部每日计划 / 日程区：每条任务画成一个圆角小方块，方块本身包裹住一个关键词
                    // （方块宽度随关键词自适应并留白，不再横贯整格形成细长条）
                    let chip_h = 16.0f32;
                    let chip_gap_x = 3.0f32;
                    let chip_gap_y = 3.0f32;
                    let chip_pad_x = 5.0f32;
                    let content_top = rect.top() + if is_today { 28.0 } else { 24.0 };
                    let mid_bottom = rect.bottom() - plan_band_h - 2.0;
                    let font = FontId::proportional(9.5);
                    let max_label_w = (inner_w - chip_pad_x * 2.0 - 2.0).max(6.0);
                    let mut chips: Vec<(String, Color32, bool, f32)> = Vec::new();
                    for (txt, col, done) in &daily {
                        let label = trunc_to_width(ui, txt, font.clone(), max_label_w);
                        let tw = ui.fonts(|f| f.layout_no_wrap(label.clone(), font.clone(), Color32::WHITE)).rect.width();
                        chips.push((label, *col, *done, (tw + chip_pad_x * 2.0).min(inner_w).max(12.0)));
                    }
                    let mut cx = inner_x;
                    let mut cy = content_top;
                    let mut shown = 0usize;
                    for (label, col, done, cw) in &chips {
                        if cx + *cw > inner_x + inner_w && cx > inner_x {
                            cx = inner_x;
                            cy += chip_h + chip_gap_y;
                        }
                        if cy + chip_h > mid_bottom { break; }
                        let cr = egui::Rect::from_min_size(Pos2::new(cx, cy), Vec2::new(*cw, chip_h));
                        let fill_col = if *done { c(theme::DONE) } else { *col };
                        ui.painter().rect_filled(cr, Rounding::same(4.0),
                            Color32::from_rgba_unmultiplied(fill_col.r(), fill_col.g(), fill_col.b(), if out { 48 } else { 82 }));
                        ui.painter().rect_stroke(cr, Rounding::same(4.0), Stroke::new(0.9_f32, fade(fill_col, a_main)));
                        ui.painter().text(cr.center(), Align2::CENTER_CENTER, label.as_str(), font.clone(),
                            if *done { c(theme::DONE) } else { fade(c(theme::TEXT), a_main) });
                        cx += *cw + chip_gap_x;
                        shown += 1;
                    }
                    if daily.len() > shown {
                        ui.painter().text(Pos2::new(rect.right() - 5.0, mid_bottom - 1.0), Align2::RIGHT_BOTTOM,
                            format!("+{}", daily.len() - shown), FontId::proportional(8.5), fade(c(theme::TEXT3), a_main));
                    }
                    // 两区之间的柔和分隔线（仅两区都有内容时画），强化「按区域显示」
                    if plan_shown > 0 && shown > 0 {
                        ui.painter().line_segment(
                            [Pos2::new(inner_x + 2.0, mid_bottom + 1.5), Pos2::new(rect.right() - 7.0, mid_bottom + 1.5)],
                            Stroke::new(0.7_f32, ca(theme::STROKE, 110)));
                    }
                    // 底部阶段计划：纯彩色小方块，不写文字
                    if plan_shown > 0 {
                        let total_w = plan_shown as f32 * plan_sq + (plan_shown - 1) as f32 * plan_gap;
                        let sx = rect.center().x - total_w / 2.0;
                        let py = rect.bottom() - plan_sq - 2.0;
                        for (i, p) in plans.iter().take(plan_shown).enumerate() {
                            let sq = egui::Rect::from_min_size(
                                Pos2::new(sx + i as f32 * (plan_sq + plan_gap), py), Vec2::new(plan_sq, plan_sq));
                            ui.painter().rect_filled(sq.expand(1.0), Rounding::same(2.0), fade(rgb(p.color), if out { 55 } else { 80 }));
                            ui.painter().rect_filled(sq, Rounding::same(1.5), fade(rgb(p.color), a_main));
                        }
                    }
                    if resp.clicked() { self.select(d); }
                    if resp.secondary_clicked() { ctx_cell = Some(d); }
                }
            });
        }
        if let Some(d) = ctx_cell { self.select(d); self.ctx(CtxKind::MonthCell(d), ui_pos(ui)); }
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
            let add_resp = egui::Frame::none().fill(bg).rounding(Rounding::same(radius::SM))
                .inner_margin(Margin::symmetric(6.0, 4.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for p in [1u8, 2, 3] {
                            let label = RichText::new(format!("{} {}", if self.add_prio == p { "◉" } else { "●" },
                                ["", "高", "中", "低"][p as usize])).color(rgb(prio_color(p))).size(11.0);
                            if ui.add(egui::Button::new(label).frame(false)).clicked() { self.add_prio = p; }
                        }
                        // 时间段输入（可留空=全天）
                        ui.add(egui::TextEdit::singleline(&mut self.add_start).hint_text("HH:MM").desired_width(52.0));
                        dt_picker_btn(ui, "add_start", 0, PickerKind::Time, &self.add_start,
                            &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
                        ui.label(RichText::new("–").size(10.0).color(c(theme::TEXT3)));
                        ui.add(egui::TextEdit::singleline(&mut self.add_end).hint_text("HH:MM").desired_width(52.0));
                        dt_picker_btn(ui, "add_end", 0, PickerKind::Time, &self.add_end,
                            &mut self.picker, &mut self.picker_pos, &mut self.picker_toggled);
                        // 可选关键词：留空则卡片退回显示完整事项文字
                        ui.add(egui::TextEdit::singleline(&mut self.add_keyword)
                            .hint_text("关键词(选填)").desired_width(86.0));
                        let resp = ui.add(egui::TextEdit::singleline(&mut self.add_text)
                            .hint_text("写下计划事项，回车添加…").desired_width((ui.available_width() - 200.0).max(40.0)));
                        if self.focus_add { resp.request_focus(); self.focus_add = false; }
                        // 单行 TextEdit 收到 Enter 会先 surrender_focus，故用 lost_focus 判定；
                        // 保存后把焦点要回来（下一帧生效），可连续录入多条
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if !self.add_text.trim().is_empty() { self.add_todo(); }
                            self.focus_add = true;
                        }
                        if ui.button("添加").clicked() { self.add_todo(); self.focus_add = true; }
                    });
                });
            // 点击添加栏以外收起输入框；栏内点击（优先级/时段/关键词/按钮）不算
            let click_outside = ui.input(|i| {
                if !i.pointer.button_clicked(egui::PointerButton::Primary) { return false; }
                i.pointer.latest_pos().map(|p| {
                    // 点日期/时间面板里的任何东西都不算「点添加栏外面」
                    let in_picker = self.picker_rect.map(|r| r.expand(8.0).contains(p)).unwrap_or(false);
                    !add_resp.response.rect.contains(p) && !in_picker
                }).unwrap_or(false)
            });
            if click_outside { self.add_open = false; }
        }

        let k = dkey(&self.selected);
        // ════════ 每日详情左右分栏：左=每日计划(按重要度)，右=每日日程表(按时间) ════════
        // 用 egui 原生 ui.columns 分栏，自动处理 ScrollArea 内坐标，最可靠
        let todos: Vec<Todo> = self.db.todos_of(&k).into_iter().cloned().collect();
        let mut pending_toggle: Vec<(u64, bool)> = Vec::new();
        let mut pending_del: Vec<u64> = Vec::new();
        let mut pending_edit: Vec<(u64, String, String)> = Vec::new();
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
                        // 双击编辑：事项 + 关键词两个单行框，回车保存
                        let resp = ui.add(egui::TextEdit::singleline(&mut self.edit_text)
                            .desired_width((row_rect.width() - 178.0).max(70.0))
                            .hint_text("编辑事项…"));
                        ui.add(egui::TextEdit::singleline(&mut self.edit_keyword)
                            .desired_width(60.0).hint_text("关键词"));
                        self.edit_rect = Some(resp.rect);
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if enter {
                            let txt = self.edit_text.trim().to_string();
                            if !txt.is_empty() { pending_edit.push((t.id, txt, self.edit_keyword.trim().to_string())); }
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
                        // 有关键词时在事项前加一个小标签，列表里也能一眼认出
                        let disp = if t.keyword.trim().is_empty() { disp }
                                   else { format!("〔{}〕{}", t.keyword.trim(), disp) };
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
                                self.edit_keyword = t.keyword.clone();
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
                                .stroke(Stroke::new(1.0_f32, c(theme::STROKE_HI)))
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
            #[derive(Clone)] struct Row { start: String, end: String, text: String, place: String, col: Color32, course: bool, id: u64 }
            let mut rows: Vec<Row> = Vec::new();
            for c0 in self.db.courses_on(self.selected.weekday().num_days_from_sunday() as u8, &k) {
                rows.push(Row { start:c0.start.clone(), end:c0.end.clone(), text:c0.name.clone(),
                    place:c0.place.clone(), col:rgb(c0.color), course:true, id:c0.id });
            }
            for t in self.db.todos_of(&k) {
                if !t.start.is_empty() {
                    rows.push(Row { start:t.start.clone(), end:t.end.clone(), text:t.text.clone(),
                        place:String::new(),
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
                    // 地点也放进详情行：先量好「编辑」按钮与地点标签要占的宽度，课程名再按剩余宽度截断
                    let place = r0.place.trim();
                    let place_label = if place.is_empty() { String::new() } else { format!("· {}", place) };
                    let btn_reserve = if r0.course { 46.0_f32 } else { 6.0_f32 };
                    let place_w = if place_label.is_empty() { 0.0 } else {
                        ui.fonts(|f| f.layout_no_wrap(place_label.clone(), FontId::proportional(10.5), Color32::WHITE)).rect.width()
                    } + 6.0;
                    let name_avail = (ui.available_width() - btn_reserve - place_w).max(40.0);
                    let txt = trunc_to_width(ui, &r0.text, FontId::proportional(11.0), name_avail);
                    let name_resp = ui.label(RichText::new(txt).size(11.5).color(r0.col).strong());
                    let full = if place.is_empty() { r0.text.clone() } else { format!("{} · {}", r0.text, place) };
                    name_resp.on_hover_text(full);
                    if !place_label.is_empty() && ui.available_width() > place_w + 44.0 {
                        ui.label(RichText::new(place_label).size(10.5).color(c(theme::TEXT3)));
                    }
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
            self.edit_keyword = self.db.todos.iter().find(|x| x.id == id).map(|x| x.keyword.clone()).unwrap_or_default();
            self.sel_todo = Some(id);
            self.edit_rect = None;
        }
        for (id, txt, kw) in pending_edit {
            if let Some(item) = self.db.todos.iter_mut().find(|x| x.id == id) { item.text = txt; item.keyword = kw; }
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
                self.show_ctx = false; self.add_open = false; self.cancel_plan();
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

    // ═══════════ 日期/时间选择面板 ═══════════
    /// 日期面板里的单日格子：柔和圆角方片 + 悬停抬升 + 今天描边 + 选中实心。
    /// 数字统一字号居中，不再像默认小按钮那样"飘"在空白背景上。
    fn picker_day_cell(ui: &mut egui::Ui, day: u32, chosen: bool, is_today: bool) -> bool {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(28.0, 26.0), egui::Sense::click());
        if !ui.is_rect_visible(rect) { return resp.clicked(); }
        let hover = resp.hovered();
        // 平时内缩 1px、悬停占满，和皮肤预览块一致的柔和抬升反馈
        let r = if hover { rect } else { rect.shrink(1.0) };
        let round = Rounding::same(radius::SM);
        let p = ui.painter();
        let fill = if chosen {
            c(theme::ACCENT)
        } else if hover {
            mix(c(theme::CARD), c(theme::ACCENT), 0.30)
        } else {
            ca(theme::CARD, 110) // 极淡的底，让数字有"落点"又不抢眼
        };
        p.rect_filled(r, round, fill);
        let stroke = if chosen {
            Stroke::NONE
        } else if is_today {
            Stroke::new(1.3_f32, c(theme::ACCENT))
        } else {
            Stroke::new(1.0_f32, c(theme::STROKE))
        };
        p.rect_stroke(r, round, stroke);
        let fg = if chosen { c(theme::ACCENT_INK) }
                 else if is_today { c(theme::ACCENT2) }
                 else { c(theme::TEXT2) };
        let font = FontId::proportional(12.5);
        let txt = day.to_string();
        if chosen || is_today || hover {
            // 双绘模拟粗体：选中 / 今天 / 悬停的数字更有分量
            for dx in [-0.35_f32, 0.35] {
                p.text(rect.center() + Vec2::new(dx, 0.0), Align2::CENTER_CENTER,
                       &txt, font.clone(), fg);
            }
        } else {
            p.text(rect.center(), Align2::CENTER_CENTER, &txt, font, fg);
        }
        resp.clicked()
    }

    fn handle_picker(&mut self, ctx: &egui::Context) {
        // 点击面板外即关闭（刚点开的那一下不算）
        if self.picker.is_some() && !self.picker_toggled {
            let clicked_out = ctx.input(|i| {
                if !i.pointer.button_clicked(egui::PointerButton::Primary) { return false; }
                match (i.pointer.latest_pos(), self.picker_rect) {
                    (Some(pos), Some(r)) => !r.expand(8.0).contains(pos),
                    _ => false,
                }
            });
            if clicked_out {
                self.picker = None;
                self.picker_rect = None;
                self.picker_toggled = false;
                return;
            }
        }
        self.picker_toggled = false;
        let Some(mut cur) = self.picker else { self.picker_rect = None; return; };

        // 面板位置限制在窗口内，避免贴边被裁
        let mut pos = self.picker_pos;
        if let Some(ir) = ctx.input(|i| i.viewport().clone()).inner_rect {
            // 按弹窗实际尺寸留边：时间面板多了两条滑块，比日期面板更宽更高，
            // 继续用旧的小边距会把底部按钮顶出窗口，所以分别给足空间。
            let pw = if cur.kind == PickerKind::Time { 380.0 } else { 252.0 };
            let ph = if cur.kind == PickerKind::Time { 290.0 } else { 340.0 };
            // 输入框在窗口下半部时把时间面板翻到按钮上方，而不是顶着底边往下长
            if cur.kind == PickerKind::Time
                && self.picker_pos.y > ir.min.y + ir.height() * 0.5 {
                pos.y = self.picker_pos.y - 340.0;
            }
            pos.x = pos.x.max(ir.min.x + 4.0).min((ir.max.x - pw).max(ir.min.x + 4.0));
            pos.y = pos.y.max(ir.min.y + 4.0).min((ir.max.y - ph).max(ir.min.y + 4.0));
        }
        let today = self.today;
        let mut apply = false;
        let mut cancel = false;
        let mut set_now = false;
        let title = if cur.kind == PickerKind::Date { "选择日期" } else { "选择时间" };
        let resp = egui::Window::new(title)
            .id(egui::Id::new("dt_picker_win"))
            .fixed_pos(pos)
            .collapsible(false).resizable(false)
            .frame(egui::Frame::none()
                .fill(mix(c(theme::PANEL), c(theme::CARD), 0.5))
                .rounding(Rounding::same(radius::MD))
                .stroke(Stroke::new(1.0_f32, c(theme::STROKE_HI)))
                .inner_margin(Margin::same(10.0)))
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    if cur.kind != PickerKind::Time {
                        ui.label(RichText::new("日期 · 年月日").size(10.5).color(c(theme::TEXT3)));
                        let mut y = cur.date.year();
                        let mut m = cur.date.month() as i32;
                        ui.horizontal(|ui| {
                            if ui.small_button("‹").clicked() { m -= 1; }
                            ui.add(egui::DragValue::new(&mut y).range(1900..=2100).speed(0.5));
                            ui.label(RichText::new("年").size(11.0));
                            egui::ComboBox::from_id_salt("dt_month").width(54.0)
                                .selected_text(format!("{}月", m))
                                .show_ui(ui, |ui| {
                                    for mm in 1i32..=12 { ui.selectable_value(&mut m, mm, format!("{}月", mm)); }
                                });
                            if ui.small_button("›").clicked() { m += 1; }
                        });
                        while m < 1 { m += 12; y -= 1; }
                        while m > 12 { m -= 12; y += 1; }
                        y = y.clamp(1900, 2100);
                        let keep_day = cur.date.day().min(dt_days_in_month(y, m as u32));
                        if let Some(d) = NaiveDate::from_ymd_opt(y, m as u32, keep_day) { cur.date = d; }
                        // 星期头
                        egui::Grid::new("dt_week_head").num_columns(7).spacing(Vec2::new(3.0, 2.0)).show(ui, |ui| {
                            for w in WEEK {
                                ui.allocate_ui(Vec2::new(28.0, 18.0), |ui| {
                                    ui.centered_and_justified(|ui| {
                                        ui.label(RichText::new(w).size(10.0).color(c(theme::TEXT3)));
                                    });
                                });
                            }
                            ui.end_row();
                        });
                        // 月历：点哪天选哪天（圆角方片，数字居中、层次清晰）
                        egui::Grid::new("dt_days").num_columns(7).spacing(Vec2::new(3.0, 3.0)).show(ui, |ui| {
                            let first = NaiveDate::from_ymd_opt(y, m as u32, 1).unwrap();
                            let lead = first.weekday().num_days_from_sunday() as usize;
                            let dim = dt_days_in_month(y, m as u32);
                            let mut d = 1u32;
                            for idx in 0..42usize {
                                if idx < lead || d > dim {
                                    ui.allocate_ui(Vec2::new(28.0, 26.0), |_| {});
                                } else {
                                    let day = d; d += 1;
                                    let chosen = day == cur.date.day();
                                    let is_today = day == today.day() && m as u32 == today.month() && y == today.year();
                                    if Self::picker_day_cell(ui, day, chosen, is_today) {
                                        if let Some(nd) = NaiveDate::from_ymd_opt(y, m as u32, day) { cur.date = nd; }
                                    }
                                }
                                if idx % 7 == 6 { ui.end_row(); }
                            }
                        });
                    }
                    if cur.kind != PickerKind::Date {
                        ui.add_space(3.0);
                        ui.label(RichText::new("时间 · 时分（24 小时制，拖动滑块选择）").size(10.5).color(c(theme::TEXT3)));
                        ui.add_space(2.0);
                        // 两条滑块：时 0–23、分 0–59，零填充显示，填充色跟随当前皮肤。
                        ui.add(egui::Slider::new(&mut cur.h, 0..=23)
                            .text("时").integer().trailing_fill(true)
                            .custom_formatter(|n, _| format!("{:02}", n as i32)));
                        ui.add(egui::Slider::new(&mut cur.m, 0..=59)
                            .text("分").integer().trailing_fill(true)
                            .custom_formatter(|n, _| format!("{:02}", n as i32)));
                        ui.label(RichText::new(format!("已选 {:02}:{:02}", cur.h, cur.m))
                            .size(11.0).color(c(theme::ACCENT2)));
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("现在").clicked() { set_now = true; }
                        if ui.button("确定").clicked() { apply = true; }
                        if ui.button("取消").clicked() { cancel = true; }
                    });
                });
            });
        self.picker_rect = resp.map(|r| r.response.rect);

        if set_now {
            let now = Local::now();
            cur.date = now.date_naive();
            cur.h = now.hour(); cur.m = now.minute(); cur.s = 0; // 只取时分，不带秒
        }
        if cancel { self.picker = None; return; }
        if apply {
            // 时间一律写成 HH:MM，不产生秒
            let val = if cur.kind == PickerKind::Date { dkey(&cur.date) } else { dt_fmt_time(cur.h, cur.m, 0) };
            match cur.id {
                "plan_start" => self.plan_start = val,
                "plan_end" => self.plan_end = val,
                "io_date" => self.io_date = val,
                "jump_text" => {
                    self.jump_text = val;
                    if let Some(d) = dt_parse_date(&self.jump_text) {
                        self.selected = d;
                        self.view = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
                        self.month_anim = 0.0; self.select_pulse = 0.0;
                    }
                    self.jump_open = false;
                },
                "course_start_date" => self.course_start_date = val,
                "course_end_date" => self.course_end_date = val,
                "add_start" => self.add_start = val,
                "add_end" => self.add_end = val,
                "course_start" => self.course_start = val,
                "course_end" => self.course_end = val,
                "slot_start" => if let Some(s) = self.slot_draft.get_mut(cur.idx) { s.0 = val; },
                "slot_end" => if let Some(s) = self.slot_draft.get_mut(cur.idx) { s.1 = val; },
                _ => {}
            }
            self.picker = None;
        } else {
            self.picker = Some(cur);
        }
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
        // 月视图格子右键会带上当天日期，供「清空当天」两项使用
        let clear_day = match kind { CtxKind::MonthCell(d) => Some(d), _ => None };
        let mut close = false;
        let resp = egui::Window::new("ctx").id(egui::Id::new("ctx_menu")).fixed_pos(pos)
            .collapsible(false).resizable(false).title_bar(false)
            .frame(egui::Frame::none().fill(mix(c(theme::PANEL), c(theme::CARD), 0.5)).rounding(Rounding::same(radius::SM))
                .stroke(Stroke::new(1.0_f32, c(theme::STROKE_HI))).inner_margin(Margin::same(5.0)))
            .show(ctx, |ui| {
                match kind {
                    CtxKind::PlanZone => {
                        ui.label(RichText::new("阶段计划").size(10.0).color(c(theme::TEXT3)));
                        if ctx_btn(ui, "＋ 新建阶段计划") { self.open_plan_form(); close = true; }
                        if ctx_btn(ui, "回到今天") { self.goto_today(); close = true; }
                    }
                    CtxKind::Calendar | CtxKind::MonthCell(_) => {
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
                        if let Some(d) = clear_day {
                            ui.add_space(2.0);
                            ui.separator();
                            ui.add_space(2.0);
                            if ctx_btn(ui, "×  清空当天的阶段计划") { self.clear_day_plans(d); close = true; }
                            if ctx_btn(ui, "×  清空当天的每日计划和日程") { self.clear_day_todos(d); close = true; }
                            let kk = dkey(&d);
                            let can_restore = self.db.courses.iter()
                                .any(|c| c.skip_dates.iter().any(|s| s.as_str() == kk.as_str()));
                            if can_restore && ctx_btn(ui, "↺  恢复当天的课程") {
                                self.restore_day_courses(d); close = true;
                            }
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

// ── 日期时间选择器：解析 / 格式化 / 小按钮 ──
/// 宽松解析 YYYY-MM-DD，兼容 Y-M-D
fn dt_parse_date(s: &str) -> Option<NaiveDate> {
    let t = s.trim();
    if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") { return Some(d); }
    let p: Vec<&str> = t.split('-').collect();
    if p.len() == 3 {
        NaiveDate::from_ymd_opt(p[0].trim().parse().ok()?, p[1].trim().parse().ok()?, p[2].trim().parse().ok()?)
    } else { None }
}
/// 宽松解析 HH:MM / H:MM / HH:MM:SS，24 小时制
fn dt_parse_time(s: &str) -> Option<(u32, u32, u32)> {
    let p: Vec<&str> = s.trim().split(':').collect();
    if p.len() < 2 || p.len() > 3 { return None; }
    let h: u32 = p[0].trim().parse().ok()?;
    let m: u32 = p[1].trim().parse().ok()?;
    let sec: u32 = if p.len() == 3 { p[2].trim().parse().ok()? } else { 0 };
    if h < 24 && m < 60 && sec < 60 { Some((h, m, sec)) } else { None }
}
/// 秒为 0 时写成 HH:MM（兼容原有数据与排序），否则写 HH:MM:SS
fn dt_fmt_time(h: u32, m: u32, s: u32) -> String {
    if s == 0 { format!("{:02}:{:02}", h, m) } else { format!("{:02}:{:02}:{:02}", h, m, s) }
}
fn dt_days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 29 } else { 28 },
        _ => 30,
    }
}
/// 输入框旁的小「选」按钮：点击后记录要编辑的字段与面板位置
fn dt_picker_btn(
    ui: &mut egui::Ui,
    id: &'static str,
    idx: usize,
    kind: PickerKind,
    value: &str,
    state: &mut Option<PickerState>,
    pos: &mut Pos2,
    toggled: &mut bool,
) -> bool {
    let r = ui.add_sized([20.0, 20.0],
        egui::Button::new(RichText::new("选").size(10.5).color(c(theme::ACCENT2))))
        .on_hover_text(if kind == PickerKind::Date { "点选年月日" } else { "拖动滑块选择时分（24 小时制，不带秒）" });
    let clicked = r.clicked();
    if clicked {
        let now = Local::now();
        let mut st = PickerState {
            id, idx, kind, date: now.date_naive(),
            h: now.hour(), m: now.minute(), s: 0,
        };
        match kind {
            // 已有合法值就接着编辑它；空值 / 非法值则默认显示当前日期时间
            PickerKind::Date => if let Some(d) = dt_parse_date(value) { st.date = d; },
            PickerKind::Time => if let Some((h, m, _)) = dt_parse_time(value) { st.h = h; st.m = m; },
        }
        *state = Some(st);
        *pos = r.rect.left_bottom();
        *toggled = true;
    }
    clicked
}
fn rgb(col: [f32; 3]) -> Color32 {
    Color32::from_rgb((col[0] * 255.0) as u8, (col[1] * 255.0) as u8, (col[2] * 255.0) as u8)
}
fn short(k: &str) -> String {
    let p: Vec<&str> = k.split('-').collect();
    format!("{}/{}", p[1].parse::<u32>().unwrap_or(0), p[2].parse::<u32>().unwrap_or(0))
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

#[cfg(test)]
mod dt_tests {
    use super::*;

    #[test]
    fn date_parse_accepts_ymd_and_rejects_bad() {
        assert_eq!(dt_parse_date("2026-11-21"), NaiveDate::from_ymd_opt(2026, 11, 21));
        assert_eq!(dt_parse_date("2026-2-3"), NaiveDate::from_ymd_opt(2026, 2, 3));
        assert_eq!(dt_parse_date(""), None);
        assert_eq!(dt_parse_date("2026-13-01"), None);
    }

    #[test]
    fn time_parse_is_24h_and_validates_ranges() {
        assert_eq!(dt_parse_time("08:00"), Some((8, 0, 0)));
        assert_eq!(dt_parse_time("23:59:59"), Some((23, 59, 59)));
        assert_eq!(dt_parse_time("7:05"), Some((7, 5, 0)));
        assert_eq!(dt_parse_time("24:00"), None);
        assert_eq!(dt_parse_time("12:60"), None);
        assert_eq!(dt_parse_time("12:00:60"), None);
    }

    #[test]
    fn time_format_keeps_old_data_shape() {
        assert_eq!(dt_fmt_time(8, 0, 0), "08:00");
        assert_eq!(dt_fmt_time(0, 5, 0), "00:05");
        assert_eq!(dt_fmt_time(23, 59, 30), "23:59:30");
    }

    #[test]
    fn month_lengths_handle_leap_years() {
        assert_eq!(dt_days_in_month(2024, 2), 29);
        assert_eq!(dt_days_in_month(2026, 2), 28);
        assert_eq!(dt_days_in_month(2026, 4), 30);
        assert_eq!(dt_days_in_month(2026, 12), 31);
    }
}

#[cfg(test)]
mod skin_tests {
    use super::*;

    fn lum(col: Color32) -> i32 {
        (col.r() as i32 * 299 + col.g() as i32 * 587 + col.b() as i32 * 114) / 1000
    }

    #[test]
    fn every_skin_has_distinct_name_and_readable_text() {
        let list = skins();
        assert!(list.len() >= 8, "应至少内置八套皮肤");
        let mut names = std::collections::HashSet::new();
        for p in list {
            assert!(!p.name.is_empty() && names.insert(p.name), "皮肤名不能为空或重复");
            let diff = lum(p.text) - lum(p.bg);
            if p.dark {
                assert!(diff > 100, "深色皮肤「{}」文字不够亮", p.name);
            } else {
                assert!(diff < -100, "浅色皮肤「{}」文字不够暗", p.name);
            }
            assert!(lum(p.accent) != lum(p.bg), "皮肤「{}」主色与底色太接近", p.name);
        }
    }

    #[test]
    fn every_semantic_color_key_resolves_in_every_skin() {
        let keys = ["bg", "panel", "card", "stroke", "stroke_hi", "text", "text2", "text3",
                    "accent", "accent2", "ink", "cool", "mint", "done"];
        for p in skins() {
            for k in keys {
                let col = pal_color(p, k);
                assert_ne!(col, Color32::from_rgb(255, 0, 255), "皮肤「{}」缺颜色键 {}", p.name, k);
            }
        }
    }

    #[test]
    fn skin_switch_finishes_transition() {
        set_skin(2);
        assert_eq!(active_skin(), 2);
        assert!(advance_skin(1.0));
        assert!(!advance_skin(1.0), "淡入结束后不应继续过渡");
        set_skin_now(0);
        assert_eq!(active_skin(), 0);
    }
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
