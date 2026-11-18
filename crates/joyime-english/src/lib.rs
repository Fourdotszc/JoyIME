//! 英文九宫格猜词与前缀补全。

use std::collections::HashMap;
use std::sync::OnceLock;

struct Entry {
    word: String,
    t9: String,
}

pub struct EnglishEngine {
    entries: Vec<Entry>,
    exact_t9: HashMap<String, Vec<usize>>,
    t9_prefixes: HashMap<String, Vec<usize>>,
    word_index: HashMap<String, usize>,
}

impl EnglishEngine {
    /// 解析 data/english.txt 格式的词表文本（每行 `单词 词频`）。
    pub fn new(dict: &str) -> Self {
        let mut frequencies: HashMap<String, u64> = HashMap::new();
        for line in dict.lines() {
            let mut parts = line.split_whitespace();
            let (Some(word), Some(frequency), None) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let Ok(frequency) = frequency.parse::<u64>() else {
                continue;
            };
            if !valid_word(word) {
                continue;
            }
            frequencies
                .entry(word.to_owned())
                .and_modify(|old| *old = (*old).max(frequency))
                .or_insert(frequency);
        }

        let mut ordered: Vec<_> = frequencies.into_iter().collect();
        ordered.sort_unstable_by(|(left_word, left_frequency), (right_word, right_frequency)| {
            right_frequency.cmp(left_frequency).then_with(|| left_word.cmp(right_word))
        });

        let mut entries = Vec::with_capacity(ordered.len());
        let mut exact_t9: HashMap<String, Vec<usize>> = HashMap::new();
        let mut t9_prefixes: HashMap<String, Vec<usize>> = HashMap::new();
        let mut word_index = HashMap::new();
        for (word, _) in ordered {
            let t9 = t9_of(&word);
            if t9.is_empty() {
                continue;
            }
            let index = entries.len();
            for length in 1..=t9.len() {
                t9_prefixes.entry(t9[..length].to_owned()).or_default().push(index);
            }
            exact_t9.entry(t9.clone()).or_default().push(index);
            word_index.insert(word.clone(), index);
            entries.push(Entry { word, t9 });
        }

        Self { entries, exact_t9, t9_prefixes, word_index }
    }

    /// 内置词表（编译时嵌入 data/english.txt），第一次调用时解析，之后复用。
    pub fn builtin() -> &'static EnglishEngine {
        static BUILTIN: OnceLock<EnglishEngine> = OnceLock::new();
        BUILTIN.get_or_init(|| Self::new(include_str!("../../../data/english.txt")))
    }

    /// 九宫格猜词：digits 只含 '2'..='9'。
    /// 先给按键序列正好等于 digits 的词（按词频），再给按键序列以 digits 开头的更长词（按词频）。
    /// 例：query_t9("4663") 以 good、home、gone 开头；query_t9("466") 里有 good、home。
    /// 返回的是要直接上屏的写法（i → I，im 这类缩写带撇号：i'm → I'm）。
    pub fn query_t9(&self, digits: &str, limit: usize) -> Vec<String> {
        if limit == 0 || !valid_digits(digits) {
            return Vec::new();
        }
        let mut result = Vec::with_capacity(limit);
        if let Some(exact) = self.exact_t9.get(digits) {
            append_words(&mut result, exact, &self.entries, limit);
        }
        if result.len() < limit {
            if let Some(candidates) = self.t9_prefixes.get(digits) {
                for &index in candidates {
                    if self.entries[index].t9.len() > digits.len() {
                        result.push(display_word(&self.entries[index].word));
                        if result.len() == limit {
                            break;
                        }
                    }
                }
            }
        }
        result
    }

    /// 逐字母打字时的补全：以 prefix（小写字母和 '）开头的词，按词频，最多 limit 个。
    /// prefix 本身是词的话排第一。
    pub fn complete(&self, prefix: &str, limit: usize) -> Vec<String> {
        if limit == 0 || !valid_prefix(prefix) {
            return Vec::new();
        }
        let mut result = Vec::with_capacity(limit);
        if let Some(&index) = self.word_index.get(prefix) {
            result.push(display_word(&self.entries[index].word));
        }
        for entry in &self.entries {
            if entry.word != prefix && entry.word.starts_with(prefix) {
                result.push(display_word(&entry.word));
                if result.len() == limit {
                    break;
                }
            }
        }
        result
    }
}

fn append_words(result: &mut Vec<String>, indices: &[usize], entries: &[Entry], limit: usize) {
    for &index in indices {
        result.push(display_word(&entries[index].word));
        if result.len() == limit {
            break;
        }
    }
}

fn valid_word(word: &str) -> bool {
    !word.is_empty()
        && word.bytes().any(|byte| byte.is_ascii_lowercase())
        && word.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'\'')
}

fn valid_digits(digits: &str) -> bool {
    !digits.is_empty() && digits.bytes().all(|byte| matches!(byte, b'2'..=b'9'))
}

fn valid_prefix(prefix: &str) -> bool {
    !prefix.is_empty() && prefix.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'\'')
}

fn t9_of(word: &str) -> String {
    word.bytes().filter_map(letter_key).collect()
}

fn letter_key(letter: u8) -> Option<char> {
    match letter {
        b'a'..=b'c' => Some('2'),
        b'd'..=b'f' => Some('3'),
        b'g'..=b'i' => Some('4'),
        b'j'..=b'l' => Some('5'),
        b'm'..=b'o' => Some('6'),
        b'p'..=b's' => Some('7'),
        b't'..=b'v' => Some('8'),
        b'w'..=b'z' => Some('9'),
        b'\'' => None,
        _ => None,
    }
}

fn display_word(word: &str) -> String {
    match word {
        "i" => "I".to_owned(),
        "i'm" => "I'm".to_owned(),
        "i'll" => "I'll".to_owned(),
        "i've" => "I've".to_owned(),
        "i'd" => "I'd".to_owned(),
        _ => word.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::EnglishEngine;
    use std::time::{Duration, Instant};

    #[test]
    fn t9_common_words() {
        let engine = EnglishEngine::builtin();
        let words = engine.query_t9("4663", 5);
        assert!(words.iter().any(|word| word == "good"));
        assert!(words.iter().any(|word| word == "home"));
        assert!(words.iter().any(|word| word == "gone"));
        assert_eq!(engine.query_t9("43556", 3)[0], "hello");
        assert!(engine.query_t9("96753", 3).iter().any(|word| word == "world"));
    }

    #[test]
    fn t9_single_letters_and_contractions() {
        let engine = EnglishEngine::builtin();
        assert_eq!(engine.query_t9("4", 3)[0], "I");
        assert_eq!(engine.query_t9("2", 3)[0], "a");
        assert!(engine.query_t9("3668", 10).iter().any(|word| word == "don't"));
    }

    #[test]
    fn completions() {
        let engine = EnglishEngine::builtin();
        let words = engine.complete("hel", 5);
        assert!(words.iter().any(|word| word == "hello"));
        assert!(words.iter().any(|word| word == "help"));
        assert!(engine.complete("xyzq", 5).is_empty());
    }

    #[test]
    #[ignore]
    fn performance() {
        let start = Instant::now();
        let engine = EnglishEngine::builtin();
        let load_elapsed = start.elapsed();
        assert!(load_elapsed < Duration::from_millis(200));
        let start = Instant::now();
        let _ = engine.query_t9("4663", 5);
        let query_elapsed = start.elapsed();
        assert!(query_elapsed < Duration::from_millis(2));
        eprintln!("builtin: {load_elapsed:?}; query_t9: {query_elapsed:?}");
    }
}
