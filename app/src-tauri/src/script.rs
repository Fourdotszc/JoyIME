//! 回放脚本：开发时用来做端到端测试。设置环境变量 JOYIME_SCRIPT 指向脚本文件，
//! 程序就按脚本模拟摇杆操作（不读真摇杆），可以配合记事本检查字有没有真的打进去。
//!
//! 脚本每行一条，# 后面是注释：
//!   wait 3000          什么都不动，等 3000ms
//!   flicks 64426       九宫格：按数字依次推一下方向再回中位
//!   tap slot0          按一下某个功能键（功能 id 见 profile::ACTIONS）
//!   hold enter 800     按住某个功能键 800ms
//!   push right 70      推着某个方向 70ms（up ur right dr down dl left ul）
//!   chord space 800 slot1   按住第一个键，期间依次：数字 = 继续按住这么多毫秒，功能 id = 按一下那个键；最后松开

use crate::profile::parse_action;
use joyime_core::{Dir, Held};

/// 一段持续 ms 的摇杆状态
struct Step {
    ms: u64,
    dir: Option<Dir>,
    held: Held,
}

pub struct Script {
    steps: Vec<Step>,
    start: Option<u64>,
}

impl Script {
    pub fn load(path: &str) -> Result<Script, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("读不了脚本 {path}：{e}"))?;
        let mut steps = Vec::new();
        let idle = |ms| Step { ms, dir: None, held: Held::default() };
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            let bad = || format!("脚本第 {} 行看不懂：{line}", n + 1);
            let num = |i: usize| parts.get(i).and_then(|s| s.parse::<u64>().ok()).ok_or_else(bad);
            match parts[0] {
                "wait" => steps.push(idle(num(1)?)),
                "flicks" => {
                    for c in parts.get(1).ok_or_else(bad)?.chars() {
                        let d = Dir::ALL.into_iter().find(|d| d.phone_key() == c).ok_or_else(bad)?;
                        steps.push(Step { ms: 70, dir: Some(d), held: Held::default() });
                        steps.push(idle(50));
                    }
                }
                "tap" | "hold" => {
                    let a = parts.get(1).and_then(|s| parse_action(s)).ok_or_else(bad)?;
                    let ms = if parts[0] == "tap" { 50 } else { num(2)? };
                    steps.push(Step { ms, dir: None, held: Held::from_actions([a]) });
                    steps.push(idle(100));
                }
                "chord" => {
                    let base = parts.get(1).and_then(|s| parse_action(s)).ok_or_else(bad)?;
                    let only = Held::from_actions([base]);
                    steps.push(Step { ms: 50, dir: None, held: only });
                    for p in &parts[2..] {
                        if let Ok(ms) = p.parse::<u64>() {
                            steps.push(Step { ms, dir: None, held: only });
                        } else {
                            let a = parse_action(p).ok_or_else(bad)?;
                            steps.push(Step { ms: 50, dir: None, held: Held::from_actions([base, a]) });
                            steps.push(Step { ms: 50, dir: None, held: only });
                        }
                    }
                    steps.push(idle(100));
                }
                "push" => {
                    let names = ["up", "ur", "right", "dr", "down", "dl", "left", "ul"];
                    let i = names.iter().position(|s| Some(s) == parts.get(1)).ok_or_else(bad)?;
                    steps.push(Step { ms: num(2)?, dir: Some(Dir::ALL[i]), held: Held::default() });
                }
                _ => return Err(bad()),
            }
        }
        Ok(Script { steps, start: None })
    }

    /// 现在该是什么状态；脚本放完了返回 None
    pub fn frame(&mut self, now: u64) -> Option<(Option<Dir>, Held)> {
        let start = *self.start.get_or_insert(now);
        let mut t = now - start;
        for s in &self.steps {
            if t < s.ms {
                return Some((s.dir, s.held));
            }
            t -= s.ms;
        }
        None
    }
}
