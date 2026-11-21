// 数据层：待办 / 阶段计划 / 重点日，JSON 持久化
use chrono::{Datelike, Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub fn dkey(d: &NaiveDate) -> String { d.format("%Y-%m-%d").to_string() }
pub fn parse_key(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_else(|_| chrono::Local::now().date_naive())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: u64,
    pub date: String,
    pub text: String,
    pub prio: u8,
    pub done: bool,
    /// 时段起止 "HH:MM"，空串表示全天任务
    #[serde(default)] pub start: String,
    #[serde(default)] pub end: String,
    /// 卡片上显示的关键词（空则用事项文字）
    #[serde(default)] pub keyword: String,
}

/// 每周重复课程
#[derive(Clone, Serialize, Deserialize)]
pub struct Course {
    pub id: u64,
    pub name: String,
    /// 0=周日 .. 6=周六
    pub weekday: u8,
    pub start: String,
    pub end: String,
    #[serde(default)] pub place: String,
    /// 日历方块上显示的短关键词（空则用课程名）
    #[serde(default)] pub keyword: String,
    /// 生效起止日期 YYYY-MM-DD（空串=永久有效）
    #[serde(default)] pub start_date: String,
    #[serde(default)] pub end_date: String,
    /// 日程颜色 rgb 0..1（同名课程同色）
    #[serde(default = "default_course_color")] pub color: [f32; 3],
    /// 单日排除：月视图右键「清空当天的每日计划和日程」只跳过这一天，不删除整门周重复课程
    #[serde(default)] pub skip_dates: Vec<String>,
}

/// 周视图时间段卡片（所有天共用同一组，数量可自定义）
#[derive(Clone, Serialize, Deserialize)]
pub struct Slot {
    pub start: String,
    pub end: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: u64,
    pub name: String,
    pub color: [f32;3], // rgb 0..1
    pub start: String,
    pub end: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Db {
    pub todos: Vec<Todo>,
    pub marks: HashMap<String, bool>,
    /// 重点日备注：key=日期, value=备注文本（空串=无备注）
    #[serde(default)]
    pub mark_notes: HashMap<String, String>,
    pub plans: Vec<Plan>,
    /// 每周重复课程
    #[serde(default)] pub courses: Vec<Course>,
    /// 周视图时间段卡片（空则用默认）
    #[serde(default)] pub slots: Vec<Slot>,
    /// 每日计划排序：true=按时间, false=按重要度（默认按重要度）
    #[serde(default = "default_plan_by_time")] pub by_time: bool,
    /// 每日日程表排序：true=按时间, false=按重要度（默认按时间）
    #[serde(default = "default_schedule_by_time")] pub schedule_by_time: bool,
    /// 周视图方块颜色是否跟随日程/优先级颜色
    #[serde(default = "default_color_by_schedule")] pub color_by_schedule: bool,
    /// 周视图一周起点：true=周日开始, false=周一开始
    #[serde(default = "default_week_start_sunday")] pub week_start_sunday: bool,
    /// 启动默认视图：true=周视图, false=月视图
    #[serde(default)] pub default_week_view: bool,
    /// 周视图时段卡片文字：true=居中, false=左对齐
    #[serde(default = "default_card_center")] pub card_center: bool,
    /// 当前皮肤序号（0=曜夜，1=晨雾，2=森屿，3=霞光，4=深海，5=暖阳，6=星黛，7=青瓷）
    #[serde(default)] pub skin: usize,
}

impl Db {
    pub fn seed() -> Self {
        let today = chrono::Local::now().date_naive();
        let dk = |n: i64| dkey(&(today + Duration::days(n)));
        let mut db = Db {
            todos: vec![
                Todo { id:101, date:dk(0), text:"评审新版首页设计稿".into(), prio:1, done:false, start:String::new(), end:String::new(), keyword:"设计稿".into() },
                Todo { id:102, date:dk(0), text:"整理本周迭代待办".into(), prio:2, done:false, start:String::new(), end:String::new(), keyword:"迭代".into() },
                Todo { id:103, date:dk(0), text:"晨间运动 30 分钟".into(), prio:3, done:true,  start:String::new(), end:String::new(), keyword:"运动".into() },
                Todo { id:104, date:dk(1), text:"和产品对齐下季度路线".into(), prio:1, done:false, start:String::new(), end:String::new(), keyword:"路线".into() },
                Todo { id:105, date:dk(2), text:"周会材料准备".into(), prio:2, done:false, start:String::new(), end:String::new(), keyword:"周会".into() },
                Todo { id:106, date:dk(4), text:"月度复盘报告初稿".into(), prio:1, done:false, start:String::new(), end:String::new(), keyword:"复盘".into() },
                Todo { id:107, date:dk(6), text:"整理书桌与工位".into(), prio:3, done:false, start:String::new(), end:String::new(), keyword:"整理".into() },
                Todo { id:108, date:dk(-1),text:"提交费用报销".into(), prio:2, done:true,  start:String::new(), end:String::new(), keyword:"报销".into() },
            ],
            mark_notes: HashMap::new(),
            plans: vec![
                Plan { id:1, name:"需求评审冲刺".into(), color:[0.44,0.70,0.79], start:dk(2), end:dk(5) },
                Plan { id:2, name:"季度复盘周".into(),   color:[0.79,0.49,0.55], start:dk(8), end:dk(11)},
            ],
            marks: HashMap::new(),
            courses: Vec::new(),
            slots: default_slots(),
            by_time: false,
            schedule_by_time: true,
            color_by_schedule: true,
            week_start_sunday: false,
            default_week_view: false,
            card_center: true,
            skin: 0,
        };
        db.marks.insert(dk(1), true);
        db.marks.insert(dk(4), true);
        db
    }

    /// 当日待办：按 by_time 排序（有时段的按开始时间在前，全天项在后）；否则按重要度
    pub fn todos_of(&self, k: &str) -> Vec<&Todo> {
        let mut v: Vec<&Todo> = self.todos.iter().filter(|t| t.date == k).collect();
        v.sort_by(|a,b| {
            (a.done as u8).cmp(&(b.done as u8))
            .then(if self.by_time {
                // 有时间的排前面并按开始时间排序，全天项按重要度排后面
                let at = a.start.is_empty(); let bt = b.start.is_empty();
                at.cmp(&bt)
                    .then(a.start.cmp(&b.start))
                    .then(a.prio.cmp(&b.prio))
            } else {
                a.prio.cmp(&b.prio)
            })
            .then(a.id.cmp(&b.id))
        });
        v
    }
    /// 某天的课程（按周几匹配，按开始时间排序）
    /// 某天的课程：按周几匹配 + 生效日期范围过滤（date_key="" 不过滤日期）
    pub fn courses_on(&self, weekday: u8, date_key: &str) -> Vec<&Course> {
        let mut v: Vec<&Course> = self.courses.iter().filter(|c| {
            if c.weekday != weekday { return false; }
            // 被「清空当天日程」排除过的日期不再显示（课程本身保留，其它周照常）
            if c.skip_dates.iter().any(|s| s.as_str() == date_key) { return false; }
            if date_key.is_empty() { return true; }
            if !c.start_date.is_empty() && date_key < c.start_date.as_str() { return false; }
            if !c.end_date.is_empty() && date_key > c.end_date.as_str() { return false; }
            true
        }).collect();
        v.sort_by(|a,b| a.start.cmp(&b.start).then(a.id.cmp(&b.id)));
        v
    }
    /// 取时间段（空则用默认）
    pub fn slots(&self) -> Vec<Slot> {
        if self.slots.is_empty() { default_slots() } else { self.slots.clone() }
    }
    pub fn plans_on(&self, k: &str) -> Vec<&Plan> {
        let mut v: Vec<&Plan> = self.plans.iter().filter(|p| p.start.as_str() <= k && k <= p.end.as_str()).collect();
        v.sort_by(|a,b| a.start.cmp(&b.start));
        v
    }

    /// 清空某天的阶段计划：只把这一天从日期区间里挖掉，必要时拆成前后两段。
    /// 返回是否真的改动了数据，方便调用方决定何时写盘。
    pub fn clear_day_plans(&mut self, d: NaiveDate) -> bool {
        let k = dkey(&d);
        let k_prev = dkey(&(d - Duration::days(1)));
        let k_next = dkey(&(d + Duration::days(1)));
        let mut next_id = self.plans.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        let mut out: Vec<Plan> = Vec::new();
        let mut changed = false;
        for p in self.plans.iter().cloned() {
            if p.start.as_str() <= k.as_str() && k.as_str() <= p.end.as_str() {
                changed = true;
                if p.start.as_str() < k.as_str() {
                    let mut left = p.clone();
                    left.end = k_prev.clone();
                    out.push(left);
                }
                if k.as_str() < p.end.as_str() {
                    let mut right = p.clone();
                    right.start = k_next.clone();
                    // 左边那段沿用了原 id，右边这段另给一个新 id，保证编辑互不串台
                    if p.start.as_str() < k.as_str() { right.id = next_id; next_id += 1; }
                    out.push(right);
                }
            } else {
                out.push(p);
            }
        }
        if changed { self.plans = out; }
        changed
    }

    /// 清空某天的每日计划和日程：当天待办按日期删除；周重复课程只跳过这一天，
    /// 课程本身和其它周都保留（可用 restore_day_courses 恢复）。
    pub fn clear_day_todos(&mut self, d: NaiveDate) -> bool {
        let k = dkey(&d);
        let before = self.todos.len();
        self.todos.retain(|t| t.date != k);
        let mut changed = before != self.todos.len();
        let wd = d.weekday().num_days_from_sunday() as u8;
        for c in self.courses.iter_mut() {
            if c.weekday == wd
                && (c.start_date.is_empty() || c.start_date.as_str() <= k.as_str())
                && (c.end_date.is_empty() || k.as_str() <= c.end_date.as_str())
                && !c.skip_dates.iter().any(|s| s.as_str() == k.as_str()) {
                c.skip_dates.push(k.clone());
                changed = true;
            }
        }
        changed
    }

    /// 恢复某天被「清空」的周重复课程。
    pub fn restore_day_courses(&mut self, d: NaiveDate) -> bool {
        let k = dkey(&d);
        let mut changed = false;
        for c in self.courses.iter_mut() {
            let before = c.skip_dates.len();
            c.skip_dates.retain(|s| s.as_str() != k.as_str());
            changed |= before != c.skip_dates.len();
        }
        changed
    }
}

fn default_week_start_sunday() -> bool { false }

/// 日程默认色（未指定颜色时用）
pub fn default_course_color() -> [f32; 3] { [0.95, 0.63, 0.29] }

/// 按索引取色板颜色（同名课程共用同一索引 = 同色）
pub fn color_for_course_index(idx: usize, palette: &[[f32; 3]]) -> [f32; 3] {
    palette[idx % palette.len()]
}
fn default_card_center() -> bool { true }
fn default_plan_by_time() -> bool { false }
fn default_schedule_by_time() -> bool { true }
fn default_color_by_schedule() -> bool { true }

/// 默认周视图时间段（上午两节 + 下午两节 + 晚上）
pub fn default_slots() -> Vec<Slot> {
    vec![
        Slot { start:"08:00".into(), end:"09:40".into() },
        Slot { start:"10:00".into(), end:"11:40".into() },
        Slot { start:"14:00".into(), end:"15:40".into() },
        Slot { start:"16:00".into(), end:"17:40".into() },
        Slot { start:"19:00".into(), end:"21:00".into() },
    ]
}

/// 日历方块上的课程标签：填了语义关键词就用关键词，否则退回课程全名
pub fn course_label(c: &Course) -> String {
    if c.keyword.trim().is_empty() { c.name.clone() } else { c.keyword.trim().to_string() }
}

/// 发给 AI 的「课表 → 可导入 JSON」提示词模板。
/// 用户把这份提示词连同自己的课表文件（截图 / PDF / 照片 / 文字）一起发给 AI，
/// 再把 AI 回复整段粘进设置面板的粘贴框，点「粘贴并导入」即可，不必另存文件。
pub fn course_import_prompt() -> &'static str {
    r#"你是「Planum 计划日历」的课表整理助手。我会附上一份课表文件（截图、照片、PDF 或文字），请把它整理成可以直接导入 Planum 的 JSON。

【输出要求】
- 只输出一个 JSON 对象，不要任何解释、说明或 Markdown 代码块。
- 顶层是 {"courses": [...]}，数组里每一条是一节课。

【字段规则】
- name：课程全名，按文件原文写。
- weekday：0=周日、1=周一、2=周二……6=周六。也可以改用 day：1=周一……7=周日（两者选一个即可）。
- start / end：24 小时制 "HH:MM"，如 "08:00" / "09:40"。若文件只写第几节，请换算：第1-2节=08:00-09:40，第3-4节=10:00-11:40，第5-6节=14:00-15:40，第7-8节=16:00-17:40，晚上=19:00-21:00。
- place：上课教室 / 地点，按文件原文写；文件里没有就填 ""。
- keyword：按语义生成的短关键词，2~6 个字，用于日历上的彩色小方块。要提炼课程名里最能代表这门课的核心语义，长名字必须缩写，例如：高等数学→高数、线性代数→线代、概率论与数理统计→概率、模拟电子技术基础→模电、数字电子技术基础→数电、大学英语（二）→英语、马克思主义基本原理→马原。不同课程之间要能区分，不要用「课程」「上课」「其他」这类空词；原文件若已有简称就优先用简称。
- start_date / end_date：学期或课程的生效起止日期，格式 "YYYY-MM-DD"；文件没写就都填 ""（表示永久有效）。
- color、skip_dates 不用写，程序会自动生成。

【输出示例】
{
  "courses": [
    {"name":"高等数学","weekday":1,"start":"08:00","end":"09:40","place":"A301","keyword":"高数","start_date":"2026-09-01","end_date":"2027-01-15"},
    {"name":"模拟电子技术基础","day":3,"start":"14:00","end":"15:40","place":"实验楼201","keyword":"模电","start_date":"","end_date":""}
  ]
}

【最后检查】
确认 JSON 合法：全部使用双引号、没有多余逗号、每条都有 name / start / end。"#
}

/// 从 JSON 导入课表。兼容：顶层数组 / {"courses":[...]}，字段名支持 weekday/day(0=周日或1=周一按 monday1 判定)
pub fn import_courses(json: &str) -> Result<Vec<Course>, String> {
    #[derive(Deserialize)]
    struct Wrap { courses: serde_json::Value }
    let raw: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let arr = if raw.is_array() { raw } else {
        let w: Wrap = serde_json::from_value(raw).map_err(|e| e.to_string())?;
        w.courses
    };
    let list = arr.as_array().ok_or("课表 JSON 需为数组或含 courses 数组")?;
    let mut out = Vec::new();
    for (i, v) in list.iter().enumerate() {
        let g = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("");
        let name = g("name").trim();
        if name.is_empty() { continue; }
        let wd_raw = v.get("weekday").or_else(|| v.get("day"))
            .and_then(|x| x.as_u64()).unwrap_or(0) as u8;
        // day: 1=周一..7=周日 转成 0=周日..6=周六；weekday: 0=周日..6=周六
        let weekday = if v.get("day").is_some() {
            if wd_raw == 7 { 0 } else { wd_raw.min(6) }
        } else { wd_raw.min(6) };
        let start = g("start").to_string(); let end = g("end").to_string();
        if start.is_empty() || end.is_empty() { continue; }
        let place = if !g("place").is_empty() { g("place") } else { g("location") };
        // 短关键词：AI 导入提示词会按语义生成；也兼容 tag 字段
        let keyword = if !g("keyword").is_empty() { g("keyword") } else { g("tag") };
        // 生效日期范围（可选）；兼容 start_date/date_start 与 end_date/date_end
        let sd = if !g("start_date").is_empty() { g("start_date") } else { g("date_start") };
        let ed = if !g("end_date").is_empty() { g("end_date") } else { g("date_end") };
        out.push(Course {
            id: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64 + i as u64,
            name: name.to_string(), weekday, start, end,
            place: place.to_string(), keyword: keyword.to_string(),
            start_date: sd.to_string(), end_date: ed.to_string(),
            color: default_course_color(), skip_dates: Vec::new(),
        });
    }
    if out.is_empty() { return Err("未解析到有效日程（需 name/start/end）".into()); }
    // 同名同色、不同名不同色：按课程名首次出现顺序分配色板槽位
    let mut name_color: std::collections::HashMap<String, [f32; 3]> = std::collections::HashMap::new();
    for c in out.iter_mut() {
        let n = name_color.len();
        let col = *name_color.entry(c.name.clone()).or_insert_with(|| {
            color_for_course_index(n, &PLAN_COLORS)
        });
        c.color = col;
    }
    Ok(out)
}

/// 从 AI 回复里抠出真正要解析的 JSON：容忍 ```json 代码块和前后寒暄文字。
/// AI 经常回「好的，这是你的课表：```json {...}``` 如需调整告诉我」，
/// 直接整段塞给 serde 会失败，所以先把 JSON 本体摘出来。
pub fn extract_reply_json(reply: &str) -> Result<String, String> {
    let s = reply.trim();
    if s.is_empty() {
        return Err("先把 AI 的回复粘贴进来，再点导入".into());
    }
    // 优先取 Markdown 代码块（```json 或 ``` 都认）
    if let Some(pos) = s.find("```") {
        let after = &s[pos + 3..];
        let body = after.find('\n').map(|i| &after[i + 1..]).unwrap_or(after);
        if let Some(end) = body.find("```") {
            let block = body[..end].trim();
            if block.starts_with('{') || block.starts_with('[') {
                return Ok(block.to_string());
            }
        }
    }
    // 否则扫描所有成对的花括号：说明文字里可能也有 {...}，
    // 逐个试解析，只认「能解析、且是数组或含 courses」的那一段。
    let candidates = json_slices(s);
    for cand in &candidates {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(cand) {
            if v.is_array() || v.get("courses").is_some() {
                return Ok(cand.to_string());
            }
        }
    }
    // 一个都没通过校验时，把第一个候选交给解析函数，让报错更具体
    if let Some(first) = candidates.first() {
        return Ok(first.to_string());
    }
    Err("没在粘贴内容里找到课程 JSON，请确认 AI 回复里有课表数据".into())
}

/// 找出一段文本里所有顶层配对的 JSON 对象 / 数组（忽略字符串里的括号）。
fn json_slices(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'{' && b[i] != b'[' { i += 1; continue; }
        let mut depth = 0i32;
        let mut in_str = false;
        let mut escaped = false;
        let mut end = None;
        for (j, &ch) in b[i..].iter().enumerate() {
            if in_str {
                if escaped { escaped = false; }
                else if ch == b'\\' { escaped = true; }
                else if ch == b'"' { in_str = false; }
                continue;
            }
            match ch {
                b'"' => in_str = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 { end = Some(i + j + 1); break; }
                }
                _ => {}
            }
        }
        match end {
            Some(e) => { out.push(&s[i..e]); i = e; }
            None => i += 1,
        }
    }
    out
}

/// 粘贴 AI 回复 → 直接解析成课表（省掉手动存文件那一步）。
pub fn import_courses_from_reply(reply: &str) -> Result<Vec<Course>, String> {
    let json = extract_reply_json(reply)?;
    import_courses(&json)
}

fn data_path() -> PathBuf {
    let dir = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("DeskCalendarEgui");
    let _ = fs::create_dir_all(&dir);
    dir.join("calendar-data.json")
}

pub fn load() -> Db {
    let p = data_path();
    match fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| Db::seed()),
        Err(_) => Db::seed(),
    }
}

pub fn save(db: &Db) {
    let p = data_path();
    if let Ok(s) = serde_json::to_string_pretty(db) { let _ = fs::write(p, s); }
}

pub const PLAN_COLORS: [[f32;3];8] = [
    [0.44,0.70,0.79], [0.79,0.55,0.44], [0.56,0.68,0.48], [0.79,0.49,0.55],
    [0.65,0.56,0.79], [0.79,0.69,0.44], [0.44,0.79,0.65], [0.79,0.44,0.44],
];
pub fn prio_color(p: u8) -> [f32;3] {
    match p { 1 => [1.0,0.49,0.41], 3 => [0.50,0.70,0.88], _ => [0.95,0.63,0.29] }
}


/// 按范围过滤待办。scope: day=精确日期, week=该日起7天, month=YYYY-MM, year=YYYY
pub fn todos_in_scope(db: &Db, scope: &str, center: NaiveDate) -> Vec<Todo> {
    let ck = dkey(&center);
    match scope {
        "week" => {
            let hi = dkey(&(center + Duration::days(6)));
            db.todos.iter().filter(|t| t.date.as_str() >= ck.as_str() && t.date.as_str() <= hi.as_str()).cloned().collect()
        }
        "month" => {
            let pre = center.format("%Y-%m").to_string();
            db.todos.iter().filter(|t| t.date.starts_with(&pre)).cloned().collect()
        }
        "year" => {
            let pre = center.format("%Y").to_string();
            db.todos.iter().filter(|t| t.date.starts_with(&pre)).cloned().collect()
        }
        _ => db.todos.iter().filter(|t| t.date == ck).cloned().collect(),
    }
}

/// 可序列化的导出包
#[derive(Serialize, Deserialize)]
struct ExportData {
    scope: String,
    todos: Vec<Todo>,
    plans: Vec<Plan>,
    marks: HashMap<String, bool>,
}

/// 导出为 JSON（筛选待办 + 覆盖到的阶段计划 + 重点日）
pub fn export_json(db: &Db, scope: &str, center: NaiveDate) -> String {
    let todos = todos_in_scope(db, scope, center);
    let dates: std::collections::HashSet<String> = todos.iter().map(|t| t.date.clone()).collect();
    let plans: Vec<Plan> = db.plans.iter().filter(|p| {
        dates.iter().any(|dk| p.start.as_str() <= dk.as_str() && dk.as_str() <= p.end.as_str())
    }).cloned().collect();
    let marks: HashMap<String, bool> = db.marks.iter()
        .filter(|(k, _)| dates.contains(k.as_str())).map(|(k, v)| (k.clone(), *v)).collect();
    let pack = ExportData { scope: scope.to_string(), todos, plans, marks };
    serde_json::to_string_pretty(&pack).unwrap_or_default()
}

/// 从 JSON 导入（按 id 去重合并），返回 (新增待办数, 新增计划数)
pub fn import_json(db: &mut Db, json: &str) -> Result<(usize, usize), String> {
    let pack: ExportData = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut nt = 0; let mut np = 0;
    for t in pack.todos {
        if !db.todos.iter().any(|x| x.id == t.id) { db.todos.push(t); nt += 1; }
    }
    for p in pack.plans {
        if !db.plans.iter().any(|x| x.id == p.id) { db.plans.push(p); np += 1; }
    }
    for (k, v) in pack.marks { db.marks.entry(k).or_insert(v); }
    Ok((nt, np))
}


/// 导出为 Markdown：按日期分组，同一天的计划合并到一个日期标题下，不含 id。
/// 阶段计划单独一节。
pub fn export_markdown(db: &Db, scope: &str, center: NaiveDate) -> String {
    let todos = todos_in_scope(db, scope, center);
    let mut out = String::new();
    let scope_name = match scope {
        "day" => "当日", "week" => "本周", "month" => "本月", "year" => "本年", _ => "计划"
    };
    out.push_str(&format!("# {}计划 · {}\n\n", scope_name, dkey(&center)));

    // 按日期分组（BTreeMap 天然按日期升序）
    let mut by_day: std::collections::BTreeMap<String, Vec<&Todo>> = std::collections::BTreeMap::new();
    for t in &todos { by_day.entry(t.date.clone()).or_default().push(t); }
    if by_day.is_empty() {
        out.push_str("_（该范围内没有每日计划）_\n\n");
    } else {
        out.push_str("## 每日计划\n\n");
        for (day, list) in by_day {
            out.push_str(&format!("### {}\n\n", day));
            for t in list {
                let mark = if t.done { "x" } else { " " };
                let prio = ["", "高", "中", "低"][t.prio as usize];
                out.push_str(&format!("- [{}] {}（{}）\n", mark, t.text, prio));
            }
            out.push('\n');
        }
    }

    // 关联到的阶段计划（覆盖范围内任一天）
    let dates: std::collections::HashSet<&str> = todos.iter().map(|t| t.date.as_str()).collect();
    let plans: Vec<&Plan> = db.plans.iter().filter(|pl| {
        dates.iter().any(|dk| pl.start.as_str() <= *dk && *dk <= pl.end.as_str())
    }).collect();
    if !plans.is_empty() {
        out.push_str("## 阶段计划\n\n");
        for pl in plans {
            out.push_str(&format!("- **{}**（{} → {}）\n", pl.name, pl.start, pl.end));
        }
        out.push('\n');
    }

    // 重点日备注
    let marked: Vec<(&String, &bool)> = db.marks.iter()
        .filter(|(k, v)| dates.contains(k.as_str()) && **v)
        .collect();
    if !marked.is_empty() {
        out.push_str("## 重点日\n\n");
        for (k, _) in marked {
            let note = db.mark_notes.get(k.as_str()).map(|x| x.as_str()).unwrap_or("");
            if note.is_empty() { out.push_str(&format!("- ★ {}\n", k)); }
            else { out.push_str(&format!("- ★ {}：{}\n", k, note)); }
        }
    }
    out
}

#[cfg(test)]
mod clear_day_tests {
    use super::*;
    use chrono::Datelike;

    fn todo(id: u64, date: &str) -> Todo {
        Todo { id, date: date.into(), text: format!("t{}", id), prio: 1, done: false,
               start: String::new(), end: String::new(), keyword: String::new() }
    }
    fn plan(id: u64, start: &str, end: &str) -> Plan {
        Plan { id, name: format!("p{}", id), color: [0.5, 0.5, 0.5], start: start.into(), end: end.into() }
    }
    fn course(id: u64, weekday: u8) -> Course {
        Course { id, name: format!("c{}", id), weekday, start: "08:00".into(), end: "09:00".into(),
                 place: String::new(), keyword: String::new(), start_date: String::new(), end_date: String::new(),
                 color: default_course_color(), skip_dates: Vec::new() }
    }

    #[test]
    fn clearing_middle_day_splits_plan_and_keeps_other_days() {
        let mut db = Db::default();
        db.plans.push(plan(1, "2026-11-20", "2026-11-25"));
        let mid = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        assert!(db.clear_day_plans(mid));
        assert_eq!(db.plans.len(), 2);
        assert_eq!(db.plans[0].start, "2026-11-20");
        assert_eq!(db.plans[0].end, "2026-11-21");
        assert_eq!(db.plans[1].start, "2026-11-23");
        assert_eq!(db.plans[1].end, "2026-11-25");
        assert_ne!(db.plans[0].id, db.plans[1].id);
        assert!(db.plans_on("2026-11-22").is_empty());
        assert_eq!(db.plans_on("2026-11-24").len(), 1);
        assert!(!db.clear_day_plans(mid)); // 已清空那天，再点不该改动
    }

    #[test]
    fn clearing_edge_day_shrinks_plan_and_single_day_plan_disappears() {
        let mut db = Db::default();
        db.plans.push(plan(2, "2026-11-20", "2026-11-22"));
        db.plans.push(plan(3, "2026-11-24", "2026-11-24"));
        assert!(db.clear_day_plans(NaiveDate::from_ymd_opt(2026, 11, 20).unwrap()));
        assert_eq!(db.plans.len(), 2);
        assert_eq!(db.plans[0].start, "2026-11-21");
        assert!(db.clear_day_plans(NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()));
        assert_eq!(db.plans.len(), 1);
        assert!(db.plans.iter().all(|p| p.id != 3));
    }

    #[test]
    fn clearing_day_todos_and_courses_is_day_scoped_and_reversible() {
        let mut db = Db::default();
        let d = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap(); // 周六
        let next = d + Duration::days(7);
        db.todos.push(todo(1, &dkey(&d)));
        db.todos.push(todo(2, &dkey(&next)));
        let wd = d.weekday().num_days_from_sunday() as u8;
        db.courses.push(course(9, wd));
        let k = dkey(&d);
        assert_eq!(db.courses_on(wd, &k).len(), 1);
        assert!(db.clear_day_todos(d));
        assert_eq!(db.todos.len(), 1);
        assert_eq!(db.todos[0].date, dkey(&next));
        assert!(db.courses_on(wd, &k).is_empty());
        assert_eq!(db.courses_on(wd, &dkey(&next)).len(), 1); // 别的周照常显示
        assert!(db.restore_day_courses(d));
        assert_eq!(db.courses_on(wd, &k).len(), 1);
        assert!(!db.restore_day_courses(d));
    }
}

#[cfg(test)]
mod import_tests {
    use super::*;

    #[test]
    fn prompt_covers_semantic_keywords_and_import_schema() {
        let p = course_import_prompt();
        assert!(p.contains("keyword"));
        assert!(p.contains("语义"));
        assert!(p.contains("weekday"));
        assert!(p.contains("place"));
        assert!(p.contains("courses"));
    }

    #[test]
    fn import_reads_keyword_place_and_semantic_tag_alias() {
        let json = r#"{"courses":[
            {"name":"高等数学","weekday":1,"start":"08:00","end":"09:40","place":"A301","keyword":"高数"},
            {"name":"模拟电子技术基础","day":3,"start":"14:00","end":"15:40","place":"实验楼201","tag":"模电"}
        ]}"#;
        let cs = import_courses(json).unwrap();
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0].keyword, "高数");
        assert_eq!(cs[0].place, "A301");
        assert_eq!(cs[1].keyword, "模电");
        assert_eq!(cs[1].weekday, 3);
        assert_eq!(cs[1].place, "实验楼201");
    }

    #[test]
    fn prompt_example_itself_imports_cleanly() {
        let p = course_import_prompt();
        let start = p.find("{\n  \"courses\"").expect("提示词里应有示例 JSON");
        let end = start + p[start..].find("\n}").expect("示例 JSON 应有结尾") + 2;
        let json = &p[start..end];
        let cs = import_courses(json).expect("提示词示例必须能直接导入");
        assert_eq!(cs.len(), 2);
        assert!(cs.iter().all(|c| !c.keyword.is_empty()), "示例每条都该带语义关键词");
        assert!(cs.iter().all(|c| !c.place.is_empty()), "示例每条都该带地点");
        assert_eq!(course_label(&cs[0]), "高数");
        assert_eq!(course_label(&cs[1]), "模电");
    }

    #[test]
    fn pasted_reply_with_code_fence_imports() {
        let reply = r#"好的，已按你的课表整理：
```json
{"courses":[{"name":"高等数学","weekday":1,"start":"08:00","end":"09:40","keyword":"高数"}]}
```
如果还要调整告诉我。"#;
        let cs = import_courses_from_reply(reply).expect("代码块里的 JSON 应能直接导入");
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].keyword, "高数");
    }

    #[test]
    fn pasted_reply_with_chatter_and_braces_imports() {
        // 前后寒暄 + 说明文字里带花括号 + 地点名里也有花括号，都不能干扰提取
        let reply = r#"这是结果（格式为 {"courses":[...]}，注意 start/end）：
        {
          "courses": [
            {"name":"模电","weekday":3,"start":"14:00","end":"15:40","place":"实验楼{201}","keyword":"模电"}
          ]
        }
        需要和我说。"#;
        let cs = import_courses_from_reply(reply).expect("带说明文字的 JSON 应能提取");
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].place, "实验楼{201}");
    }

    #[test]
    fn pasted_reply_without_json_reports_friendly_error() {
        let e = match import_courses_from_reply("抱歉，我没看懂这张课表") {
            Ok(_) => panic!("这段回复里没有课表，应该报错"),
            Err(e) => e,
        };
        assert!(e.contains("JSON"));
        assert!(import_courses_from_reply("   ").is_err());
    }
}
