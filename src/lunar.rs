// 农历 / 干支 / 节气 / 节日 —— 从原 JS 版移植
use chrono::{Datelike, NaiveDate};

const LUNAR_INFO: [u32; 201] = [
0x04bd8,0x04ae0,0x0a570,0x054d5,0x0d260,0x0d950,0x16554,0x056a0,0x09ad0,0x055d2,
0x04ae0,0x0a5b6,0x0a4d0,0x0d250,0x1d255,0x0b540,0x0d6a0,0x0ada2,0x095b0,0x14977,
0x04970,0x0a4b0,0x0b4b5,0x06a50,0x06d40,0x1ab54,0x02b60,0x09570,0x052f2,0x04970,
0x06566,0x0d4a0,0x0ea50,0x06e95,0x05ad0,0x02b60,0x186e3,0x092e0,0x1c8d7,0x0c950,
0x0d4a0,0x1d8a6,0x0b550,0x056a0,0x1a5b4,0x025d0,0x092d0,0x0d2b2,0x0a950,0x0b557,
0x06ca0,0x0b550,0x15355,0x04da0,0x0a5b0,0x14573,0x052b0,0x0a9a8,0x0e950,0x06aa0,
0x0aea6,0x0ab50,0x04b60,0x0aae4,0x0a570,0x05260,0x0f263,0x0d950,0x05b57,0x056a0,
0x096d0,0x04dd5,0x04ad0,0x0a4d0,0x0d4d4,0x0d250,0x0d558,0x0b540,0x0b6a0,0x195a6,
0x095b0,0x049b0,0x0a974,0x0a4b0,0x0b27a,0x06a50,0x06d40,0x0af46,0x0ab60,0x09570,
0x04af5,0x04970,0x064b0,0x074a3,0x0ea50,0x06b58,0x055c0,0x0ab60,0x096d5,0x092e0,
0x0c960,0x0d954,0x0d4a0,0x0da50,0x07552,0x056a0,0x0abb7,0x025d0,0x092d0,0x0cab5,
0x0a950,0x0b4a0,0x0baa4,0x0ad50,0x055d9,0x04ba0,0x0a5b0,0x15176,0x052b0,0x0a930,
0x07954,0x06aa0,0x0ad50,0x05b52,0x04b60,0x0a6e6,0x0a4e0,0x0d260,0x0ea65,0x0d530,
0x05aa0,0x076a3,0x096d0,0x04afb,0x04ad0,0x0a4d0,0x1d0b6,0x0d250,0x0d520,0x0dd45,
0x0b5a0,0x056d0,0x055b2,0x049b0,0x0a577,0x0a4b0,0x0aa50,0x1b255,0x06d20,0x0ada0,
0x14b63,0x09370,0x049f8,0x04970,0x064b0,0x168a6,0x0ea50,0x06b20,0x1a6c4,0x0aae0,
0x0a2e0,0x0d2e3,0x0c960,0x0d557,0x0d4a0,0x0da50,0x05d55,0x056a0,0x0a6d0,0x055d4,
0x052d0,0x0a9b8,0x0a950,0x0b4a0,0x0b6a6,0x0ad50,0x055a0,0x0aba4,0x0a5b0,0x052b0,
0x0b273,0x06930,0x07337,0x06aa0,0x0ad50,0x14b55,0x04b60,0x0a570,0x054e4,0x0d160,
0x0e968,0x0d520,0x0daa0,0x16aa6,0x056d0,0x04ae0,0x0a9d4,0x0a2d0,0x0d150,0x0f252,
0x0d520,
];

const GAN: [&str;10] = ["甲","乙","丙","丁","戊","己","庚","辛","壬","癸"];
const ZHI: [&str;12] = ["子","丑","寅","卯","辰","巳","午","未","申","酉","戌","亥"];
const ANIMALS: [&str;12] = ["鼠","牛","虎","兔","龙","蛇","马","羊","猴","鸡","狗","猪"];
const L_MONTHS: [&str;12] = ["正月","二月","三月","四月","五月","六月","七月","八月","九月","十月","冬月","腊月"];
const L_DAYS: [&str;30] = ["初一","初二","初三","初四","初五","初六","初七","初八","初九","初十","十一","十二","十三","十四","十五","十六","十七","十八","十九","二十","廿一","廿二","廿三","廿四","廿五","廿六","廿七","廿八","廿九","三十"];
const TERMS: [&str;24] = ["小寒","大寒","立春","雨水","惊蛰","春分","清明","谷雨","立夏","小满","芒种","夏至","小暑","大暑","立秋","处暑","白露","秋分","寒露","霜降","立冬","小雪","大雪","冬至"];
const S_TERM_INFO: [i64;24] = [0,21208,42467,63836,85337,107014,128867,150921,173149,195551,218072,240693,263343,285989,308563,331033,353350,375494,397447,419210,440795,462224,483532,504758];

fn leap_month(y: i32) -> u32 { LUNAR_INFO[(y-1900) as usize] & 0xf }
fn leap_days(y: i32) -> i32 {
    if leap_month(y) != 0 { if LUNAR_INFO[(y-1900) as usize] & 0x10000 != 0 { 30 } else { 29 } } else { 0 }
}
fn month_days(y: i32, m: u32) -> i32 {
    if LUNAR_INFO[(y-1900) as usize] & (0x10000 >> m) != 0 { 30 } else { 29 }
}
fn l_year_days(y: i32) -> i32 {
    let mut s = 348i32;
    let mut i = 0x8000u32;
    while i > 0x8 { if LUNAR_INFO[(y-1900) as usize] & i != 0 { s += 1; } i >>= 1; }
    s + leap_days(y)
}
fn days_from_ymd(y: i32, m: u32, d: u32) -> i64 {
    // 用 chrono 简化：与 1900-01-31 的天数差
    let base = NaiveDate::from_ymd_opt(1900,1,31).unwrap();
    let t = NaiveDate::from_ymd_opt(y,m,d).unwrap();
    (t - base).num_days()
}

#[derive(Clone, Debug)]
pub struct Lunar { pub year: i32, pub month: u32, pub day: u32, pub is_leap: bool }

pub fn solar2lunar(date: &NaiveDate) -> Lunar {
    let mut offset = days_from_ymd(date.year(), date.month(), date.day());
    let mut i = 1900i32;
    while i < 2101 && offset > 0 {
        let temp = l_year_days(i) as i64;
        offset -= temp;
        if offset < 0 { offset += temp; break; }
        i += 1;
    }
    let year = i;
    let leap = leap_month(year);
    let mut is_leap = false;
    let mut m = 1u32;
    while m < 13 && offset > 0 {
        let mut temp;
        if leap > 0 && m == leap + 1 && !is_leap {
            m -= 1; is_leap = true; temp = leap_days(year) as i64;
        } else {
            temp = month_days(year, m) as i64;
        }
        if is_leap && m == leap + 1 { is_leap = false; }
        offset -= temp;
        m += 1;
    }
    if offset == 0 && leap > 0 && m == leap + 1 {
        if is_leap { is_leap = false; } else { is_leap = true; m -= 1; }
    }
    if offset < 0 { offset += 30; m -= 1; }
    Lunar { year, month: m, day: (offset + 1) as u32, is_leap }
}

pub fn gz(idx: i32) -> String {
    let i = ((idx % 60) + 60) % 60;
    format!("{}{}", GAN[(i%10) as usize], ZHI[(i%12) as usize])
}
pub fn year_gz(l: &Lunar) -> String { gz(l.year - 1984) }
pub fn animal(l: &Lunar) -> &'static str { ANIMALS[(((l.year-1984)%12+12)%12) as usize] }
pub fn day_gz(date: &NaiveDate) -> String {
    let d = days_from_ymd(date.year(), date.month(), date.day()) - days_from_ymd(1900,1,1);
    gz(d as i32 + 10)
}

fn term_day(y: i32, n: usize) -> u32 {
    let ms = 31556925974.7 * ((y-1900) as f64) + S_TERM_INFO[n-1] as f64 * 60000.0
        + NaiveDate::from_ymd_opt(1900,1,6).unwrap().and_hms_opt(2,5,0).unwrap().and_utc().timestamp_millis() as f64;
    // 简化：用 UTC 计算
    let secs = (ms / 1000.0) as i64;
    let dt = chrono::DateTime::from_timestamp(secs, 0).unwrap();
    dt.date_naive().day()
}
pub fn term_of_date(date: &NaiveDate) -> Option<&'static str> {
    let y = date.year(); let m = date.month() as usize; let d = date.day();
    if d == term_day(y, 2*m-1) { return Some(TERMS[2*m-2]); }
    if d == term_day(y, 2*m) { return Some(TERMS[2*m-1]); }
    None
}

pub struct TermAround { pub last: Option<(String, NaiveDate)>, pub next: Option<(String, NaiveDate)> }
pub fn term_around(date: &NaiveDate) -> TermAround {
    let y = date.year();
    let mut res: Vec<(String, NaiveDate)> = Vec::new();
    for k in -1..=1 {
        for n in 1..=24 {
            let day = term_day(y+k, n);
            if let Some(d) = NaiveDate::from_ymd_opt(y+k, ((n-1)/2+1) as u32, day) {
                res.push((TERMS[n-1].to_string(), d));
            }
        }
    }
    res.sort_by(|a,b| a.1.cmp(&b.1));
    let mut last = None; let mut next = None;
    for (name, d) in res {
        if d <= *date { last = Some((name,d)); }
        else { next = Some((name,d)); break; }
    }
    TermAround { last, next }
}

const L_FEST: [(&str,&str);10] = [("1-1","春节"),("1-15","元宵节"),("2-2","龙抬头"),("5-5","端午节"),("7-7","七夕"),("7-15","中元节"),("8-15","中秋节"),("9-9","重阳节"),("12-8","腊八节"),("12-23","小年")];
const S_FEST: [(&str,&str);12] = [("1-1","元旦"),("2-14","情人节"),("3-8","妇女节"),("3-12","植树节"),("5-1","劳动节"),("5-4","青年节"),("6-1","儿童节"),("7-1","建党节"),("8-1","建军节"),("9-10","教师节"),("10-1","国庆节"),("12-25","圣诞节")];

pub fn festival_of(date: &NaiveDate, l: &Lunar) -> Option<String> {
    let sk = format!("{}-{}", date.month(), date.day());
    for (k,v) in S_FEST { if k == sk { return Some(v.to_string()); } }
    if !l.is_leap {
        let lk = format!("{}-{}", l.month, l.day);
        for (k,v) in L_FEST { if k == lk { return Some(v.to_string()); } }
        if l.month == 12 && l.day as i32 == month_days(l.year, 12) { return Some("除夕".to_string()); }
    }
    term_of_date(date).map(|s| s.to_string())
}

pub fn lunar_text(l: &Lunar) -> String {
    format!("{}{}年{}{}{}", year_gz(l), animal(l), if l.is_leap {"闰"} else {""}, L_MONTHS[(l.month-1) as usize], L_DAYS[(l.day-1) as usize])
}
pub fn lunar_short(l: &Lunar) -> String {
    format!("{}{}", if l.is_leap {"闰"} else {""}, L_MONTHS[(l.month-1) as usize])
}
pub fn lunar_day(l: &Lunar) -> &'static str { L_DAYS[(l.day-1) as usize] }

#[cfg(test)]
mod tests {
    use super::*;
    fn check(y:i32,m:u32,d:u32, expect:&str) {
        let dt = NaiveDate::from_ymd_opt(y,m,d).unwrap();
        let l = solar2lunar(&dt);
        let got = format!("{}{}{}{}", if l.is_leap{"闰"}else{""}, L_MONTHS[(l.month-1) as usize], L_DAYS[(l.day-1) as usize], "");
        println!("{}-{:02}-{:02} => {} (期望 {}) {}", y,m,d,got,expect, if got==expect{"OK"}else{"FAIL"});
    }
    #[test]
    fn lunar_samples() {
        check(2026,11,18,"十月初十");
        check(2026,11,20,"十月十二");
        check(2025,1,29,"正月初一");   // 2025春节
        check(2026,2,17,"正月初一");   // 2026春节
        check(2024,2,10,"正月初一");   // 2024春节
    }
}
