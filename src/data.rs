// 数据层：待办 / 阶段计划 / 重点日，JSON 持久化
use chrono::{Duration, NaiveDate};
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
}

impl Db {
    pub fn seed() -> Self {
        let today = chrono::Local::now().date_naive();
        let dk = |n: i64| dkey(&(today + Duration::days(n)));
        let mut db = Db {
            todos: vec![
                Todo { id:101, date:dk(0), text:"评审新版首页设计稿".into(), prio:1, done:false },
                Todo { id:102, date:dk(0), text:"整理本周迭代待办".into(), prio:2, done:false },
                Todo { id:103, date:dk(0), text:"晨间运动 30 分钟".into(), prio:3, done:true  },
                Todo { id:104, date:dk(1), text:"和产品对齐下季度路线".into(), prio:1, done:false },
                Todo { id:105, date:dk(2), text:"周会材料准备".into(), prio:2, done:false },
                Todo { id:106, date:dk(4), text:"月度复盘报告初稿".into(), prio:1, done:false },
                Todo { id:107, date:dk(6), text:"整理书桌与工位".into(), prio:3, done:false },
                Todo { id:108, date:dk(-1),text:"提交费用报销".into(), prio:2, done:true  },
            ],
            mark_notes: HashMap::new(),
            plans: vec![
                Plan { id:1, name:"需求评审冲刺".into(), color:[0.44,0.70,0.79], start:dk(2), end:dk(5) },
                Plan { id:2, name:"季度复盘周".into(),   color:[0.79,0.49,0.55], start:dk(8), end:dk(11)},
            ],
            marks: HashMap::new(),
        };
        db.marks.insert(dk(1), true);
        db.marks.insert(dk(4), true);
        db
    }

    pub fn todos_of(&self, k: &str) -> Vec<&Todo> {
        let mut v: Vec<&Todo> = self.todos.iter().filter(|t| t.date == k).collect();
        v.sort_by(|a,b| (a.done as u8).cmp(&(b.done as u8)).then(a.prio.cmp(&b.prio)).then(a.id.cmp(&b.id)));
        v
    }
    pub fn plans_on(&self, k: &str) -> Vec<&Plan> {
        let mut v: Vec<&Plan> = self.plans.iter().filter(|p| p.start.as_str() <= k && k <= p.end.as_str()).collect();
        v.sort_by(|a,b| a.start.cmp(&b.start));
        v
    }
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


/// 导出文件目录（数据目录下 exports）
pub fn export_dir() -> PathBuf {
    let p = data_path().parent().unwrap_or_else(|| std::path::Path::new(".")).join("exports");
    let _ = fs::create_dir_all(&p);
    p
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
