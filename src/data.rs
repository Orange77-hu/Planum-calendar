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
