//! 拼音引擎。由 `prototype/pinyin.js` 移植，接口以本文件为准。

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

// Rust 当前的 MSRV 不允许在 const 中调用 ln；以下为对应的 IEEE-754 值。
const PY_PREFIX_PENALTY: f64 = 3.401_197_381_662_155_5;
const PY_ABBR_PENALTY: f64 = 2.302_585_092_994_046;
const PY_PER_EDGE: usize = 20;
const PY_MAX_CANDIDATES: usize = 1000;

/// 输入是字母还是九宫格数字。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// 字母和 '，例如 "nihao"、"xi'an"、"nh"
    Letters,
    /// 九宫格数字 2–9 和 '，例如 "64426"
    T9,
}

/// 一个候选。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// 候选的字或词
    pub text: String,
    /// 选它之后要从输入开头删掉多少个字符（含紧跟着的 '）
    pub raw_len: usize,
    /// 它对应的那段输入读作什么拼音，音节之间用 ' 隔开，例如 "ni'hao"、简拼 "n'h"
    pub reading: String,
}

#[derive(Clone)]
struct Entry {
    text: String,
    freq: f64,
    py: String,
    cuts: Vec<usize>,
}

/// 保留插入顺序的索引；`HashMap` 只用于按键取值。
struct Table {
    index: HashMap<String, Vec<Arc<Entry>>>,
    order: Vec<String>,
    keys: Vec<String>,
}

struct Tables {
    full: Table,
    abbr: Table,
}

pub struct PinyinEngine {
    letters: Tables,
    t9: OnceLock<Tables>,
    log_total: f64,
    /// 每个音节的单字词频之和（按插入顺序），九宫格拼音栏排序用
    syllables: Vec<(String, f64)>,
}

impl Table {
    fn empty() -> Self {
        Self {
            index: HashMap::new(),
            order: Vec::new(),
            keys: Vec::new(),
        }
    }

    fn push(&mut self, key: String, entry: Entry) {
        self.push_arc(key, Arc::new(entry));
    }

    fn push_arc(&mut self, key: String, entry: Arc<Entry>) {
        if let Some(entries) = self.index.get_mut(&key) {
            entries.push(entry);
        } else {
            self.order.push(key.clone());
            self.index.insert(key, vec![entry]);
        }
    }

    fn finish(&mut self) {
        for key in &self.order {
            self.index.get_mut(key).unwrap().sort_by(freq_desc);
        }
        self.keys = self.order.clone();
        // JS 的字符串排序是 UTF-16；这里的键均为 ASCII，字节序相同。
        self.keys.sort();
    }

    /// 找出严格长于 prefix 的键的前 limit 个词，按词频降序。
    /// 原型取得完整排序表后也只读取这前 limit 项，故可避免排序无用尾部。
    fn prefixed(&self, prefix: &str, limit: usize) -> Vec<Arc<Entry>> {
        let start = self.keys.partition_point(|key| key.as_str() < prefix);
        let mut hit: Vec<&Arc<Entry>> = Vec::new();
        for key in &self.keys[start..] {
            if !key.starts_with(prefix) {
                break;
            }
            if key != prefix {
                for entry in self.index.get(key).unwrap() {
                    // 在相同频率的项之后插入，保持 JS stable sort 的原始遍历顺序。
                    let position = hit.partition_point(|old| old.freq >= entry.freq);
                    if position < limit {
                        hit.insert(position, entry);
                        if hit.len() > limit {
                            hit.pop();
                        }
                    }
                }
            }
        }
        hit.into_iter().cloned().collect()
    }
}

fn freq_desc(a: &Arc<Entry>, b: &Arc<Entry>) -> Ordering {
    b.freq.partial_cmp(&a.freq).unwrap()
}

impl PinyinEngine {
    /// 解析 data/pinyin.txt 格式的词库文本。
    pub fn new(dict: &str) -> Self {
        let mut full = Table::empty();
        let mut abbr = Table::empty();
        let mut total = 0_f64;
        let mut syllable_freq: Vec<(String, f64)> = Vec::new();
        let mut syllable_index: HashMap<String, usize> = HashMap::new();

        for line in dict.split('\n').filter(|line| !line.is_empty()) {
            let mut columns = line.split('\t');
            let py = columns.next().unwrap();
            let syllables: Vec<&str> = py.split('\'').collect();
            let key = syllables.concat();
            let mut cuts = Vec::new();
            let mut position = 0;
            for syllable in syllables.iter().take(syllables.len().saturating_sub(1)) {
                position += syllable.len();
                cuts.push(position);
            }
            let initials: String = syllables
                .iter()
                .map(|syllable| syllable.as_bytes()[0] as char)
                .collect();
            let abbr_cuts: Vec<usize> = (1..syllables.len()).collect();

            for item in columns {
                let split = item.rfind(' ').unwrap();
                let text = item[..split].to_owned();
                let freq: f64 = item[split + 1..].trim().parse().unwrap();
                full.push(
                    key.clone(),
                    Entry {
                        text: text.clone(),
                        freq,
                        py: key.clone(),
                        cuts: cuts.clone(),
                    },
                );
                abbr.push(
                    initials.clone(),
                    Entry {
                        text,
                        freq,
                        py: initials.clone(),
                        cuts: abbr_cuts.clone(),
                    },
                );
                // 按文件顺序累加，与 JS 的 number 精确整数累加相同。
                total += freq;
                if syllables_len_is_one(&key, &cuts) {
                    let i = *syllable_index.entry(key.clone()).or_insert_with(|| {
                        syllable_freq.push((key.clone(), 0.0));
                        syllable_freq.len() - 1
                    });
                    syllable_freq[i].1 += freq;
                }
            }
        }
        full.finish();
        abbr.finish();
        Self {
            letters: Tables { full, abbr },
            t9: OnceLock::new(),
            log_total: total.ln(),
            syllables: syllable_freq,
        }
    }

    /// 九宫格数字串开头可能是哪些完整音节：长的在前，一样长按常用程度。
    /// 例：syllable_options("9426") 以 xian、xiao、zhao、zhan 开头，后面是 xia、zha、xi、yi、zi。
    pub fn syllable_options(&self, digits: &str) -> Vec<String> {
        let mut out: Vec<&(String, f64)> = self.syllables.iter().filter(|(s, _)| digits.starts_with(&to_t9(s))).collect();
        out.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then(b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal)));
        out.into_iter().map(|(s, _)| s.clone()).collect()
    }

    /// 内置词库（编译时嵌入 data/pinyin.txt），第一次调用时解析，之后复用。
    pub fn builtin() -> &'static PinyinEngine {
        static ENGINE: OnceLock<PinyinEngine> = OnceLock::new();
        ENGINE.get_or_init(|| Self::new(include_str!("../../../data/pinyin.txt")))
    }

    fn table(&self, mode: Mode) -> &Tables {
        match mode {
            Mode::Letters => &self.letters,
            Mode::T9 => self.t9.get_or_init(|| Tables {
                full: t9_table(&self.letters.full),
                abbr: t9_table(&self.letters.abbr),
            }),
        }
    }

    fn cost(&self, freq: f64) -> f64 {
        self.log_total - freq.ln()
    }

    /// 查候选，顺序和数量与 prototype/pinyin.js 的 `query(raw, mode)` 完全一致。
    pub fn query(&self, raw: &str, mode: Mode) -> Vec<Candidate> {
        self.query_with(raw, mode, "")
    }

    /// 带「已经定下的开头音节」的查询（九宫格拼音栏）：letters 是定下的音节连起来的字母，
    /// raw 里每个定下的音节后面要用 ' 隔开（例：定下 xi，raw = "94'26"，letters = "xi"）。
    /// 只留读音和定下的音节一致、并且不会把定下的音节拆开或接长的词。
    pub fn query_with(&self, raw: &str, mode: Mode, letters: &str) -> Vec<Candidate> {
        let tables = self.table(mode);
        // 输入限定为 ASCII，因此字节位置即 JS 的字符串下标。
        let mut bounds = Vec::new();
        let mut compact = String::new();
        let mut raw_pos = vec![0_usize];
        for (raw_index, byte) in raw.bytes().enumerate() {
            if byte == b'\'' {
                if !compact.is_empty() {
                    ensure_len(&mut bounds, compact.len() + 1);
                    bounds[compact.len()] = true;
                }
            } else {
                compact.push(byte as char);
            }
            ensure_len(&mut raw_pos, compact.len() + 1);
            raw_pos[compact.len()] = raw_index + 1;
        }
        let n = compact.len();
        if n == 0 {
            return Vec::new();
        }
        ensure_len(&mut bounds, n + 1);

        let mut best = vec![f64::INFINITY; n + 1];
        let mut back: Vec<Option<(usize, Arc<Entry>)>> = vec![None; n + 1];
        best[0] = 0.0;
        for i in 0..n {
            if best[i].is_infinite() {
                continue;
            }
            for edge in self.edges_from(tables, &compact, &bounds, letters, i, 1, 1) {
                let next = best[i] + edge.cost;
                if next < best[edge.end] {
                    best[edge.end] = next;
                    back[edge.end] = Some((i, edge.entry));
                }
            }
        }

        let mut candidates = Vec::new();
        let mut seen = HashSet::new();
        let mut add = |text: String, len: usize, reading: String| {
            if seen.contains(&text) || candidates.len() >= PY_MAX_CANDIDATES {
                return;
            }
            seen.insert(text.clone());
            let mut raw_len = raw_pos[len];
            while raw.as_bytes().get(raw_len) == Some(&b'\'') {
                raw_len += 1;
            }
            candidates.push(Candidate {
                text,
                raw_len,
                reading,
            });
        };

        if !best[n].is_infinite() {
            let mut parts = Vec::new();
            let mut position = n;
            while position > 0 {
                let (start, entry) = back[position].as_ref().unwrap();
                parts.push((entry.clone(), position - *start));
                position = *start;
            }
            parts.reverse();
            if parts.len() > 1 {
                let text = parts.iter().map(|(entry, _)| entry.text.as_str()).collect();
                let reading = parts
                    .iter()
                    .map(|(entry, len)| reading(entry, *len))
                    .collect::<Vec<_>>()
                    .join("'");
                add(text, n, reading);
            }
        }
        for edge in self.edges_from(tables, &compact, &bounds, letters, 0, PY_PER_EDGE, usize::MAX) {
            add(edge.entry.text.clone(), edge.end, reading(&edge.entry, edge.end));
        }
        candidates
    }

    #[allow(clippy::too_many_arguments)]
    fn edges_from(
        &self,
        tables: &Tables,
        input: &str,
        bounds: &[bool],
        letters: &str,
        start: usize,
        limit: usize,
        exact_limit: usize,
    ) -> Vec<Edge> {
        let n = input.len();
        let rest = &input[start..];
        // 有强制分隔时，前排词可能被 fits 过滤，须保留完整排序结果再过滤。
        let prefix_limit = if bounds[start + 1..].contains(&true) {
            usize::MAX
        } else {
            limit
        };
        let mut fuzzy = Vec::new();
        fuzzy.extend(self.take(
            tables.full.prefixed(rest, prefix_limit), bounds, letters, start, n, PY_PREFIX_PENALTY, limit,
        ));
        fuzzy.extend(self.take_opt(
            tables.abbr.index.get(rest), bounds, letters, start, n, PY_ABBR_PENALTY, limit,
        ));
        if rest.len() >= 2 {
            fuzzy.extend(self.take(
                tables.abbr.prefixed(rest, prefix_limit),
                bounds,
                letters,
                start,
                n,
                PY_ABBR_PENALTY + PY_PREFIX_PENALTY,
                limit,
            ));
        }
        fuzzy.sort_by(cost_asc);
        fuzzy.truncate(limit);

        let mut out = self.take_opt(
            tables.full.index.get(rest), bounds, letters, start, n, 0.0, exact_limit,
        );
        out.extend(fuzzy);
        out.sort_by(cost_asc);
        for end in (start + 1..n).rev() {
            let key = &input[start..end];
            let mut exact = self.take_opt(
                tables.full.index.get(key), bounds, letters, start, end, 0.0, exact_limit,
            );
            exact.extend(self.take_opt(
                tables.abbr.index.get(key), bounds, letters, start, end, PY_ABBR_PENALTY, limit,
            ));
            exact.sort_by(cost_asc);
            out.extend(exact);
        }
        out
    }

    fn take(
        &self,
        list: Vec<Arc<Entry>>,
        bounds: &[bool],
        letters: &str,
        start: usize,
        end: usize,
        penalty: f64,
        limit: usize,
    ) -> Vec<Edge> {
        self.take_slice(&list, bounds, letters, start, end, penalty, limit)
    }

    fn take_opt(
        &self,
        list: Option<&Vec<Arc<Entry>>>,
        bounds: &[bool],
        letters: &str,
        start: usize,
        end: usize,
        penalty: f64,
        limit: usize,
    ) -> Vec<Edge> {
        self.take_slice(list.map_or(&[], Vec::as_slice), bounds, letters, start, end, penalty, limit)
    }

    fn take_slice(
        &self,
        list: &[Arc<Entry>],
        bounds: &[bool],
        letters: &str,
        start: usize,
        end: usize,
        penalty: f64,
        limit: usize,
    ) -> Vec<Edge> {
        let mut out = Vec::new();
        for entry in list {
            if out.len() >= limit {
                break;
            }
            if fits(entry, bounds, letters, start, end) {
                out.push(Edge {
                    end,
                    entry: entry.clone(),
                    cost: self.cost(entry.freq) + penalty,
                });
            }
        }
        out
    }
}

#[derive(Clone)]
struct Edge {
    end: usize,
    entry: Arc<Entry>,
    cost: f64,
}

fn cost_asc(a: &Edge, b: &Edge) -> Ordering {
    a.cost.partial_cmp(&b.cost).unwrap()
}

/// 只有一个音节的词（单字）
fn syllables_len_is_one(_key: &str, cuts: &[usize]) -> bool {
    cuts.is_empty()
}

fn ensure_len<T: Default>(values: &mut Vec<T>, len: usize) {
    if values.len() < len {
        values.resize_with(len, T::default);
    }
}

fn fits(entry: &Entry, bounds: &[bool], letters: &str, start: usize, end: usize) -> bool {
    for position in start + 1..end {
        if bounds[position] && !entry.cuts.contains(&(position - start)) {
            return false;
        }
    }
    if bounds[end] && start + entry.py.len() > end && !entry.cuts.contains(&(end - start)) {
        return false;
    }
    let fixed = letters.len();
    if fixed == 0 {
        return true;
    }
    if end < fixed && !bounds[end] {
        return false;
    }
    for position in start..end.min(fixed) {
        if entry.py.as_bytes()[position - start] != letters.as_bytes()[position] {
            return false;
        }
    }
    for &cut in &entry.cuts {
        if start + cut < fixed && !bounds[start + cut] {
            return false;
        }
    }
    !(fixed > start
        && fixed < start + entry.py.len()
        && !entry.cuts.contains(&(fixed - start)))
}

fn reading(entry: &Entry, len: usize) -> String {
    let mut out = String::new();
    for position in 0..len {
        if entry.cuts.contains(&position) {
            out.push('\'');
        }
        out.push(entry.py.as_bytes()[position] as char);
    }
    out
}

fn t9_table(source: &Table) -> Table {
    let mut table = Table::empty();
    for key in &source.order {
        for entry in source.index.get(key).unwrap() {
            table.push_arc(to_t9(key), entry.clone());
        }
    }
    table.finish();
    table
}

/// 字母换成九宫格数字：abc→2 … wxyz→9，其他字符原样保留。
pub fn to_t9(s: &str) -> String {
    s.bytes()
        .map(|byte| match byte {
            b'a'..=b'c' => '2',
            b'd'..=b'f' => '3',
            b'g'..=b'i' => '4',
            b'j'..=b'l' => '5',
            b'm'..=b'o' => '6',
            b'p'..=b's' => '7',
            b't'..=b'v' => '8',
            b'w'..=b'z' => '9',
            _ => byte as char,
        })
        .collect()
}
