use std::time::{Duration, Instant};

use joyime_pinyin::{Mode, PinyinEngine};
use serde::Deserialize;

#[derive(Deserialize)]
struct Golden {
    mode: String,
    raw: String,
    count: usize,
    top: Vec<Expected>,
    contains: Vec<String>,
}

#[derive(Deserialize)]
struct Expected {
    w: String,
    #[serde(rename = "rawLen")]
    raw_len: usize,
    reading: String,
}

fn golden_cases() -> Vec<Golden> {
    serde_json::from_str(include_str!("golden.json")).unwrap()
}

fn mode(name: &str) -> Mode {
    match name {
        "letters" => Mode::Letters,
        "t9" => Mode::T9,
        other => panic!("未知 mode: {other}"),
    }
}

#[test]
fn matches_js_golden() {
    let engine = PinyinEngine::builtin();
    for (case_index, case) in golden_cases().iter().enumerate() {
        let candidates = engine.query(&case.raw, mode(&case.mode));
        assert_eq!(
            candidates.len(),
            case.count,
            "第 {case_index} 组 mode={} raw={:?} 的候选总数",
            case.mode,
            case.raw
        );
        for (candidate_index, (actual, expected)) in candidates.iter().zip(&case.top).enumerate() {
            assert_eq!(
                actual.text, expected.w,
                "第 {case_index} 组 mode={} raw={:?}，第 {candidate_index} 个的 text",
                case.mode, case.raw
            );
            assert_eq!(
                actual.raw_len, expected.raw_len,
                "第 {case_index} 组 mode={} raw={:?}，第 {candidate_index} 个的 raw_len",
                case.mode, case.raw
            );
            assert_eq!(
                actual.reading, expected.reading,
                "第 {case_index} 组 mode={} raw={:?}，第 {candidate_index} 个的 reading",
                case.mode, case.raw
            );
        }
        for text in &case.contains {
            assert!(
                candidates.iter().any(|candidate| candidate.text == *text),
                "第 {case_index} 组 mode={} raw={:?} 缺少 contains 候选 {:?}",
                case.mode,
                case.raw,
                text
            );
        }
    }
}

#[test]
#[ignore = "在 release 下执行：cargo test -p joyime-pinyin --release -- --ignored"]
fn release_performance() {
    let started = Instant::now();
    let engine = PinyinEngine::builtin();
    let load = started.elapsed();
    assert!(
        load < Duration::from_millis(300),
        "builtin 载入耗时 {:.3}ms，超过 300ms",
        load.as_secs_f64() * 1000.0
    );

    // T9 索引按设计延迟创建，不把一次性建表时间混入单次查询延迟。
    let _ = engine.query("2", Mode::T9);
    let cases = golden_cases();
    let mut slowest = Duration::ZERO;
    let mut slowest_case = "";
    for case in &cases {
        let started = Instant::now();
        let _ = engine.query(&case.raw, mode(&case.mode));
        let elapsed = started.elapsed();
        if elapsed > slowest {
            slowest = elapsed;
            slowest_case = &case.raw;
        }
        assert!(
            elapsed < Duration::from_millis(5),
            "mode={} raw={:?} 耗时 {:.3}ms，超过 5ms",
            case.mode,
            case.raw,
            elapsed.as_secs_f64() * 1000.0
        );
    }
    eprintln!(
        "builtin={:.3}ms，最慢 query {:?}={:.3}ms",
        load.as_secs_f64() * 1000.0,
        slowest_case,
        slowest.as_secs_f64() * 1000.0
    );
}

/// 九宫格拼音栏：数字串开头可能的音节，以及定下音节之后的查询（和 JS 原型的拼音栏行为一致）
#[test]
fn syllable_options_and_pinned_query() {
    let e = PinyinEngine::builtin();
    let opts = e.syllable_options("9426");
    assert_eq!(&opts[..4], ["xian", "xiao", "zhao", "zhan"]);
    assert!(opts.contains(&"xi".to_string()) && opts.contains(&"yi".to_string()));

    // 定下 xian：只剩读 xian 的字，不给 西安（xi'an，会把定下的音节拆开）也不给 想（xiang，会接长）
    let c = e.query_with("9426", Mode::T9, "xian");
    assert_eq!(c[0].text, "先");
    assert!(c.iter().all(|x| x.text != "西安" && x.text != "想" && x.text != "小"));

    // 定下 xi：后面的 26 按 an 等继续查
    let c = e.query_with("94'26", Mode::T9, "xi");
    assert_eq!(c[0].text, "西安");
    assert!(c.iter().all(|x| x.reading.starts_with("xi")));
}
