//! 用户词库：记住用户在每串按键上选了什么、选了几次，下次排在前面。
//!
//! - 键是「输入层 + 按键串」（九宫格是数字，全拼是字母，去掉 '），值是选过的字词和次数、最近一次的时间。
//! - 存成纯文本，一行一条：`层<TAB>按键<TAB>字词<TAB>次数<TAB>最近`，读写由调用方（桌面程序）负责。
//! - 只存在用户自己的电脑上。

use crate::layout::Layer;
use std::collections::HashMap;

#[derive(Clone, Debug)]
struct Entry {
    word: String,
    count: u32,
    /// 逻辑时钟，越大越近
    last: u64,
}

#[derive(Clone, Debug, Default)]
pub struct UserDict {
    entries: HashMap<(Layer, String), Vec<Entry>>,
    clock: u64,
    dirty: bool,
}

/// 一条按键串最多记几个词，太多了没意义
const MAX_PER_KEY: usize = 12;

impl UserDict {
    pub fn parse(text: &str) -> UserDict {
        let mut d = UserDict::default();
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            let [layer, key, word, count, last] = f[..] else { continue };
            let (Some(layer), Ok(count), Ok(last)) = (Layer::from_id(layer), count.parse(), last.parse()) else {
                continue;
            };
            if key.is_empty() || word.is_empty() {
                continue;
            }
            d.clock = d.clock.max(last);
            d.entries
                .entry((layer, key.to_string()))
                .or_default()
                .push(Entry { word: word.to_string(), count, last });
        }
        for list in d.entries.values_mut() {
            sort(list);
        }
        d
    }

    pub fn to_text(&self) -> String {
        let mut keys: Vec<_> = self.entries.keys().collect();
        keys.sort_by(|a, b| (a.0.id(), &a.1).cmp(&(b.0.id(), &b.1)));
        let mut out = String::new();
        for k in keys {
            for e in &self.entries[k] {
                out.push_str(&format!("{}\t{}\t{}\t{}\t{}\n", k.0.id(), k.1, e.word, e.count, e.last));
            }
        }
        out
    }

    /// 记一次「在这串按键上选了这个词」
    pub fn learn(&mut self, layer: Layer, key: &str, word: &str) {
        if key.is_empty() || word.is_empty() || word.contains(['\t', '\n']) {
            return;
        }
        self.clock += 1;
        let list = self.entries.entry((layer, key.to_string())).or_default();
        match list.iter_mut().find(|e| e.word == word) {
            Some(e) => {
                e.count = e.count.saturating_add(1);
                e.last = self.clock;
            }
            None => list.push(Entry { word: word.to_string(), count: 1, last: self.clock }),
        }
        sort(list);
        list.truncate(MAX_PER_KEY);
        self.dirty = true;
    }

    /// 这串按键上选过的词，常用的、最近用的在前
    pub fn lookup(&self, layer: Layer, key: &str) -> Vec<&str> {
        self.entries
            .get(&(layer, key.to_string()))
            .map(|l| l.iter().map(|e| e.word.as_str()).collect())
            .unwrap_or_default()
    }

    /// 按键串以 prefix 开头、但更长的那些记录里选过的词（打到一半就能联想出常用的词），按次数排
    pub fn predictions(&self, layer: Layer, prefix: &str, limit: usize) -> Vec<&str> {
        let mut found: Vec<&Entry> = self
            .entries
            .iter()
            .filter(|((l, k), _)| *l == layer && k.len() > prefix.len() && k.starts_with(prefix))
            .flat_map(|(_, list)| list.iter())
            .collect();
        found.sort_by(|a, b| b.count.cmp(&a.count).then(b.last.cmp(&a.last)));
        let mut out: Vec<&str> = Vec::new();
        for e in found {
            if !out.contains(&e.word.as_str()) {
                out.push(&e.word);
            }
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    /// 一共记了多少条
    pub fn len(&self) -> usize {
        self.entries.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.dirty = true;
    }

    /// 自从上次问过以来有没有变（调用方据此决定要不要存盘）
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

/// 次数多的在前；次数一样，最近用的在前
fn sort(list: &mut [Entry]) {
    list.sort_by(|a, b| b.count.cmp(&a.count).then(b.last.cmp(&a.last)));
}
