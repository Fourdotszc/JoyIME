//! 状态机测试：按 16ms 一帧模拟摇杆操作，场景和网页原型里测过的一样，再加上英文和新功能。

use joyime_core::{Action, Dir, Effect, Held, Key, Layer, Machine};

struct Sim {
    m: Machine,
    t: u64,
    out: Vec<Effect>,
}

impl Sim {
    fn new(layer: Layer) -> Sim {
        let mut m = Machine::new(Layer::ALL.to_vec(), 4);
        m.set_layer(layer);
        Sim { m, t: 1000, out: Vec::new() }
    }

    /// 保持 dir 和 held 这么多毫秒（按 16ms 一帧推进）
    fn hold(&mut self, ms: u64, dir: Option<Dir>, held: &[Action]) {
        let h = Held::from_actions(held.iter().copied());
        let end = self.t + ms;
        while self.t < end {
            let fx = self.m.update(self.t, dir, h);
            self.out.extend(fx);
            self.t += 16;
        }
    }

    fn idle(&mut self, ms: u64) {
        self.hold(ms, None, &[]);
    }

    /// 九宫格：推一下回中位
    fn flick(&mut self, d: Dir) {
        self.hold(70, Some(d), &[]);
        self.idle(50);
    }

    /// 按九宫格数字串推杆
    fn flicks(&mut self, digits: &str) {
        for c in digits.chars() {
            self.flick(dir_of(c));
        }
    }

    fn press(&mut self, a: Action) {
        self.hold(50, None, &[a]);
        self.idle(100);
    }

    /// 字母层：推方向 + 按第 slot 个键
    fn letter(&mut self, d: Dir, slot: u8) {
        self.hold(30, Some(d), &[]);
        self.hold(50, Some(d), &[Action::Slot(slot)]);
        self.hold(10, Some(d), &[]);
        self.idle(100);
    }

    /// 字母层：按字母打字
    fn type_letters(&mut self, s: &str) {
        for ch in s.chars() {
            let (d, slot) = Dir::ALL
                .iter()
                .find_map(|&d| Layer::Pinyin.group(Some(d)).find(ch).map(|i| (d, i as u8)))
                .expect("字母不在布局里");
            self.letter(d, slot);
        }
    }

    fn take(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.out)
    }

    fn committed(&mut self) -> String {
        self.take()
            .into_iter()
            .filter_map(|e| match e {
                Effect::Commit(s) => Some(s),
                _ => None,
            })
            .collect()
    }
}

fn dir_of(digit: char) -> Dir {
    *Dir::ALL.iter().find(|d| d.phone_key() == digit).unwrap()
}

#[test]
fn t9_nihao() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("64426");
    let v = s.m.view();
    assert_eq!(v.comp, "64426");
    assert_eq!(v.comp_display, "ni'hao");
    assert_eq!(v.page_cands[0], "你好");
    s.press(Action::Slot(0));
    assert_eq!(s.committed(), "你好");
    assert_eq!(s.m.view().comp, "");
}

#[test]
fn t9_sentence_with_space() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("9694482664");
    s.press(Action::Space);
    assert_eq!(s.committed(), "我喜欢你");
}

#[test]
fn flick_grazing_neighbours_counts_the_held_direction() {
    let mut s = Sim::new(Layer::ChineseT9);
    // 先擦过 ↑ 16ms，停在 ↗ 80ms，回中位时又擦过 → 16ms
    s.hold(16, Some(Dir::Up), &[]);
    s.hold(80, Some(Dir::UpRight), &[]);
    s.hold(16, Some(Dir::Right), &[]);
    s.idle(50);
    assert_eq!(s.m.view().comp, "3");
}

#[test]
fn pick_while_stick_still_out_records_the_flick_first() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("342662"); // dianna
    // 最后一个 6（→）推着不放就按候选 1
    s.hold(60, Some(Dir::Right), &[]);
    s.hold(40, Some(Dir::Right), &[Action::Slot(0)]);
    s.hold(40, Some(Dir::Right), &[]);
    s.idle(50);
    assert_eq!(s.committed(), "电脑");
    assert_eq!(s.m.view().comp, "", "回中位时这一推不能再记一次");
}

#[test]
fn t9_quick_punctuation_when_idle() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.press(Action::Slot(0));
    s.press(Action::Slot(1));
    assert_eq!(s.committed(), "，。");
}

#[test]
fn pinyin_letters_and_center_pick() {
    let mut s = Sim::new(Layer::Pinyin);
    s.type_letters("nihao");
    assert_eq!(s.m.view().page_cands[0], "你好");
    // 中位按字母 1 = 选第 1 个候选（中位按下后要等 60ms 看会不会推杆）
    s.press(Action::Slot(0));
    assert_eq!(s.committed(), "你好");
}

#[test]
fn pinyin_punctuation_commits_composition_first() {
    let mut s = Sim::new(Layer::Pinyin);
    s.type_letters("lvse");
    s.letter(Dir::UpRight, 3); // def. 的第 4 个 = .
    assert_eq!(s.committed(), "绿色。");
}

#[test]
fn lower_layer_input_buffers() {
    let mut s = Sim::new(Layer::Lower);
    // 先松杆 50ms 再按：仍算刚才的方向
    s.hold(80, Some(Dir::UpLeft), &[]);
    s.idle(48);
    s.hold(50, None, &[Action::Slot(2)]);
    s.idle(100);
    // 先按键 32ms 再推：算推到的方向
    s.hold(32, None, &[Action::Slot(2)]);
    s.hold(50, Some(Dir::Right), &[Action::Slot(2)]);
    s.idle(100);
    assert_eq!(s.committed(), "lo");
    // 中位按、等很久也没推：abc 层中位没字，什么都不出
    s.press(Action::Slot(0));
    assert_eq!(s.take(), vec![Effect::Miss(None)]);
}

#[test]
fn paging_both_ways_and_wraps() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("9436"); // zhen，几百个候选
    let pages = s.m.view().pages;
    assert!(pages > 10);
    s.press(Action::Enter);
    assert_eq!(s.m.view().page, pages - 1, "第 1 页往前翻到最后一页");
    s.press(Action::CycleLayer);
    s.press(Action::CycleLayer);
    assert_eq!(s.m.view().page, 1);
    // 按住切换 1 秒：按下翻 1 页，400ms 后每 150ms 再翻 1 页
    s.hold(1000, None, &[Action::CycleLayer]);
    s.idle(50);
    assert_eq!(s.m.view().page, 1 + 1 + 4);
    assert_eq!(s.m.view().layer, Layer::ChineseT9, "打字时切换只翻页不换层");
}

#[test]
fn idle_keys_go_to_the_app() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.press(Action::Backspace);
    s.press(Action::Enter);
    s.press(Action::Space);
    assert_eq!(
        s.take(),
        vec![Effect::Key(Key::Backspace), Effect::Key(Key::Enter), Effect::Commit(" ".into())]
    );
    s.press(Action::CycleLayer);
    assert_eq!(s.m.view().layer, Layer::EnglishT9);
}

#[test]
fn english_t9_auto_space_and_smart_punctuation() {
    let mut s = Sim::new(Layer::EnglishT9);
    s.flicks("43556");
    assert_eq!(s.m.view().page_cands[0], "hello");
    s.press(Action::Slot(0));
    assert_eq!(s.committed(), "hello ");
    // 空闲时字母键 2 = 句号：先删掉自动补的空格
    s.press(Action::Slot(1));
    assert_eq!(s.take(), vec![Effect::Key(Key::Backspace), Effect::Commit(". ".into())]);
    // 句号后面的词首字母自动大写
    s.flicks("96753");
    assert_eq!(s.m.view().page_cands[0], "World");
    s.press(Action::Space);
    assert_eq!(s.committed(), "World ");
}

#[test]
fn english_shift_once_and_lock() {
    let mut s = Sim::new(Layer::EnglishT9);
    s.press(Action::Shift);
    s.flicks("4663");
    assert_eq!(s.m.view().page_cands[0], "Good");
    s.press(Action::Slot(0));
    s.flicks("4663");
    assert_eq!(s.m.view().page_cands[0], "good", "一次性大写只管一个词");
    s.press(Action::Escape);
    s.press(Action::Shift);
    s.press(Action::Shift);
    s.flicks("4663");
    assert_eq!(s.m.view().page_cands[0], "GOOD");
}

#[test]
fn toggle_pauses_everything() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.press(Action::ToggleEnabled);
    assert!(!s.m.view().enabled);
    s.flicks("64426");
    s.press(Action::Slot(0));
    assert_eq!(s.m.view().comp, "");
    assert!(s.take().is_empty());
    s.press(Action::ToggleEnabled);
    assert!(s.m.view().enabled);
    s.flicks("64");
    assert_eq!(s.m.view().comp, "64");
}

#[test]
fn symbols_flicks_type_digits_like_a_phone_keypad() {
    let mut s = Sim::new(Layer::Symbols);
    // 2026：↑ = 2，中位按字母 2 = 0，↑ = 2，→ = 6
    s.flick(Dir::Up);
    s.press(Action::Slot(1));
    s.flick(Dir::Up);
    s.flick(Dir::Right);
    s.idle(100);
    assert_eq!(s.committed(), "2026");
    // ↖ = 1，中位字母 1 = 5，↘ = 9，中位字母 3 = .
    s.flick(Dir::UpLeft);
    s.press(Action::Slot(0));
    s.flick(Dir::DownRight);
    s.press(Action::Slot(2));
    s.idle(100);
    assert_eq!(s.committed(), "159.");
}

#[test]
fn symbols_direction_plus_button_is_a_symbol_not_a_digit() {
    let mut s = Sim::new(Layer::Symbols);
    // 推着 ↑ 按字母 2 = +，回中位后不再打 2
    s.letter(Dir::Up, 1);
    s.idle(100);
    assert_eq!(s.committed(), "+");
    // 按住键再推杆：也是方向 + 按键
    s.hold(32, None, &[Action::Slot(3)]);
    s.hold(60, Some(Dir::Down), &[Action::Slot(3)]);
    s.idle(150);
    assert_eq!(s.committed(), "~");
}

#[test]
fn symbols_release_then_press_is_digit_then_center_key() {
    // 数字层里中位的 5 0 . , 很常用：推完回中位紧接着按键，是「数字 + 中位的字」，不是符号
    let mut s = Sim::new(Layer::Symbols);
    s.hold(80, Some(Dir::Left), &[]);
    s.idle(32);
    s.press(Action::Slot(1));
    s.idle(100);
    assert_eq!(s.committed(), "40");
    // 按一下中位键马上松开，接着就推杆：先打中位的字，再打数字
    s.hold(30, None, &[Action::Slot(1)]);
    s.hold(70, Some(Dir::Up), &[]);
    s.idle(100);
    assert_eq!(s.committed(), "02");
}

#[test]
fn symbols_digit_then_backspace_keeps_order() {
    let mut s = Sim::new(Layer::Symbols);
    s.hold(70, Some(Dir::Up), &[]);
    s.idle(30); // 数字还在等，紧接着按退格
    s.press(Action::Backspace);
    assert_eq!(s.take(), vec![Effect::Commit("2".into()), Effect::Key(Key::Backspace)]);
}

#[test]
fn chinese_punctuation_can_be_half_width() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.m.set_cn_fullwidth(false);
    s.press(Action::Slot(0));
    s.press(Action::Slot(1));
    assert_eq!(s.committed(), ",.");
    let mut s = Sim::new(Layer::Pinyin);
    s.m.set_cn_fullwidth(false);
    s.type_letters("nihao");
    s.letter(Dir::Up, 3); // abc, 的第 4 个 = ,
    assert_eq!(s.committed(), "你好,");
}

#[test]
fn default_layers_leave_out_full_pinyin() {
    let m = Machine::default();
    assert!(!m.layers().contains(&Layer::Pinyin));
    assert_eq!(Layer::from_id("pinyin"), Some(Layer::Pinyin));
}

#[test]
fn numbers_are_third_in_the_cycle() {
    let mut s = Sim { m: Machine::default(), t: 1000, out: Vec::new() };
    s.press(Action::CycleLayer);
    s.press(Action::CycleLayer);
    assert_eq!(s.m.view().layer, Layer::Symbols);
}

/// 往后翻页直到找到这个候选，选它；返回它原来排第几（0 起）
fn find_and_pick(s: &mut Sim, which: &str) -> usize {
    for page in 0..300 {
        let v = s.m.view();
        if let Some(i) = v.page_cands.iter().position(|c| c == which) {
            s.press(Action::Slot(i as u8));
            return page * 4 + i;
        }
        s.press(Action::NextPage);
    }
    panic!("翻了 300 页也没找到「{which}」");
}

/// 打一串九宫格键，选指定的候选；返回它原来排第几
fn pick_t9(s: &mut Sim, digits: &str, which: &str) -> usize {
    s.flicks(digits);
    let i = find_and_pick(s, which);
    s.take();
    i
}

#[test]
fn picked_words_move_to_the_front() {
    let mut s = Sim::new(Layer::ChineseT9);
    // 9426 默认第一个是「小」，「先」在后面
    assert_eq!(pick_t9(&mut s, "9426", "先"), 1);
    s.flicks("9426");
    assert_eq!(s.m.view().page_cands[0], "先");
    assert_eq!(s.m.view().page_cands[1], "小", "引擎原来的第一个紧跟在后面");
}

#[test]
fn phrases_built_from_several_picks_are_remembered() {
    let mut s = Sim::new(Layer::ChineseT9);
    // 打 94263426626，先只选「先」（只盖住前 4 个键），再选「电脑」：组字分两段完成
    s.flicks("94263426626");
    find_and_pick(&mut s, "先");
    find_and_pick(&mut s, "电脑");
    assert_eq!(s.committed(), "先电脑");
    // 下次打同一串键，整个词就在第一个
    s.flicks("94263426626");
    assert_eq!(s.m.view().page_cands[0], "先电脑");
}

#[test]
fn frequent_words_are_predicted_from_a_prefix() {
    // 用户自己造的词「先电脑」（引擎里没有），记住后只打前 4 个键（9426 = xian）就能联想出来
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("94263426626");
    find_and_pick(&mut s, "先");
    find_and_pick(&mut s, "电脑");
    s.take();
    s.flicks("9426");
    assert!(s.m.view().page_cands.contains(&"先电脑".to_string()), "{:?}", s.m.view().page_cands);
}

#[test]
fn english_words_are_learned_too() {
    let mut s = Sim::new(Layer::EnglishT9);
    s.flicks("4663");
    let cands = s.m.view().page_cands;
    assert_ne!(cands[0], "home");
    let i = cands.iter().position(|c| c == "home").unwrap();
    s.press(Action::Slot(i as u8));
    s.take();
    s.flicks("4663");
    assert_eq!(s.m.view().page_cands[0], "home");
}

#[test]
fn user_dict_survives_save_and_load() {
    let mut s = Sim::new(Layer::ChineseT9);
    pick_t9(&mut s, "9426", "先");
    let text = s.m.user_dict().to_text();
    assert!(text.contains("cn9\t9426\t先\t1\t"));
    let mut s2 = Sim::new(Layer::ChineseT9);
    s2.m.set_user_dict(joyime_core::UserDict::parse(&text));
    s2.flicks("9426");
    assert_eq!(s2.m.view().page_cands[0], "先");
}

#[test]
fn learning_can_be_turned_off() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.m.set_learning(false);
    pick_t9(&mut s, "9426", "先");
    assert!(s.m.user_dict().is_empty());
    s.flicks("9426");
    assert_eq!(s.m.view().page_cands[0], "小");
}

#[test]
fn six_button_sticks_page_by_three() {
    let m = Machine::new(joyime_core::DEFAULT_LAYERS.to_vec(), 3);
    let mut s = Sim { m, t: 1000, out: Vec::new() };
    s.flicks("64426");
    assert_eq!(s.m.view().page_cands.len(), 3);
}

/// 选拼音状态下翻拼音栏，找到 syl 并选它
fn pick_syllable(s: &mut Sim, syl: &str) {
    assert!(s.m.view().py_focus, "应该在选拼音");
    for _ in 0..30 {
        let v = s.m.view();
        if let Some(i) = v.py_opts.iter().position(|o| o == syl) {
            s.press(Action::Slot(i as u8));
            return;
        }
        s.press(Action::NextPage);
    }
    panic!("拼音栏里找不到 {syl}");
}

#[test]
fn pinyin_bar_narrows_candidates_to_one_reading() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("9426");
    let v = s.m.view();
    assert!(!v.py_focus);
    assert!(v.py_opts.contains(&"xian".to_string()), "平时也显示几个拼音：{:?}", v.py_opts);
    s.press(Action::PinyinSelect);
    let v = s.m.view();
    assert!(v.py_focus);
    assert_eq!(v.py_opts, ["xian", "xiao", "zhao", "zhan"]);
    s.press(Action::Slot(0));
    let v = s.m.view();
    assert_eq!(v.pinned, ["xian"]);
    assert!(!v.py_focus, "数字都定完了就回到选候选");
    assert_eq!(v.page_cands[0], "先");
    assert!(!v.page_cands.iter().any(|c| c == "西安" || c == "小"), "{:?}", v.page_cands);
    assert_eq!(v.comp_display, "xian");
    s.press(Action::Slot(0));
    assert_eq!(s.committed(), "先");
    assert!(s.m.view().pinned.is_empty());
}

#[test]
fn pinyin_bar_picks_syllable_by_syllable() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("9426");
    s.press(Action::PinyinSelect);
    pick_syllable(&mut s, "xi");
    let v = s.m.view();
    assert_eq!(v.pinned, ["xi"]);
    assert!(v.py_focus, "后面还有数字，接着选下一个音节");
    assert_eq!(v.py_page, 0);
    pick_syllable(&mut s, "an");
    let v = s.m.view();
    assert_eq!(v.pinned, ["xi", "an"]);
    assert_eq!(v.page_cands[0], "西安");
    s.press(Action::Slot(0));
    assert_eq!(s.committed(), "西安");
}

#[test]
fn pinyin_bar_backspace_unpins_first() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("9426");
    s.press(Action::PinyinSelect);
    s.press(Action::Slot(0)); // xian
    assert_eq!(s.m.view().pinned, ["xian"]);
    s.press(Action::Backspace);
    let v = s.m.view();
    assert!(v.pinned.is_empty());
    assert_eq!(v.comp, "9426", "撤销拼音不删数字");
    s.press(Action::Backspace);
    assert_eq!(s.m.view().comp, "942");
    assert!(s.committed().is_empty());
}

#[test]
fn pinyin_bar_toggle_back_and_partial_pick() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("94264426");
    s.press(Action::PinyinSelect);
    pick_syllable(&mut s, "xi");
    // 再按一次回到选候选，挑一个单字：只用掉 xi 的两个数字，定下的 xi 一起去掉
    s.press(Action::PinyinSelect);
    assert!(!s.m.view().py_focus);
    let i = find_and_pick(&mut s, "西");
    assert!(i < 40);
    assert_eq!(s.committed(), "西");
    let v = s.m.view();
    assert_eq!(v.comp, "264426");
    assert!(v.pinned.is_empty());
}

#[test]
fn pinyin_select_does_nothing_when_not_composing() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.press(Action::PinyinSelect);
    assert!(!s.m.view().py_focus);
    assert!(s.committed().is_empty());
}

/// 按住空格，期间依次按 keys 里的键（每个按 50ms），再松开空格
fn space_chord(s: &mut Sim, keys: &[Action]) {
    s.hold(50, None, &[Action::Space]);
    for &k in keys {
        s.hold(50, None, &[Action::Space, k]);
        s.hold(50, None, &[Action::Space]);
    }
    s.idle(100);
}

#[test]
fn hold_space_plus_top_button_picks_pinyin() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("96");
    let opts = s.m.view().py_opts;
    let yo = opts.iter().position(|o| o == "yo").expect("拼音栏里应该有 yo");
    assert!(yo < 4, "{opts:?}");
    space_chord(&mut s, &[Action::Slot(yo as u8)]);
    let v = s.m.view();
    assert_eq!(v.pinned, ["yo"]);
    assert!(!v.py_focus, "松开空格回到选候选");
    assert!(s.committed().is_empty(), "组合过的空格不上屏");
    assert_eq!(v.comp_display, "yo");
    s.press(Action::Slot(0));
    assert_eq!(s.committed(), "哟");
}

#[test]
fn tapping_space_still_picks_the_first_candidate() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("64426");
    s.press(Action::Space);
    assert_eq!(s.committed(), "你好");
}

#[test]
fn holding_space_shows_the_pinyin_bar_and_cancels_on_release() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("9426");
    s.hold(400, None, &[Action::Space]);
    assert!(s.m.view().py_focus, "按住一会儿亮出拼音栏");
    s.idle(100);
    let v = s.m.view();
    assert!(!v.py_focus);
    assert_eq!(v.comp, "9426");
    assert!(s.committed().is_empty(), "按住太久不算空格");
}

#[test]
fn hold_space_chord_pages_and_unpins() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("9426");
    // 按住空格：翻到拼音第 2 页选第 1 个，再撤销
    s.hold(50, None, &[Action::Space]);
    s.hold(50, None, &[Action::Space, Action::CycleLayer]);
    s.hold(50, None, &[Action::Space]);
    let v = s.m.view();
    assert!(v.py_focus);
    assert_eq!(v.py_page, 1);
    let second = v.py_opts[0].clone();
    s.hold(50, None, &[Action::Space, Action::Slot(0)]);
    s.hold(50, None, &[Action::Space]);
    assert_eq!(s.m.view().pinned, [second.clone()]);
    s.hold(50, None, &[Action::Space, Action::Backspace]);
    s.hold(50, None, &[Action::Space]);
    s.idle(100);
    let v = s.m.view();
    assert!(v.pinned.is_empty());
    assert_eq!(v.comp, "9426");
    assert!(s.committed().is_empty());
}

#[test]
fn hold_space_picks_syllables_in_a_row() {
    let mut s = Sim::new(Layer::ChineseT9);
    s.flicks("64426");
    let ni = s.m.view().py_opts.iter().position(|o| o == "ni").unwrap();
    s.hold(50, None, &[Action::Space]);
    s.hold(50, None, &[Action::Space, Action::Slot(ni as u8)]);
    s.hold(50, None, &[Action::Space]);
    let v = s.m.view();
    assert_eq!(v.pinned, ["ni"]);
    assert!(v.py_focus, "还按着空格、后面还有数字：接着选下一个音节");
    let hao = v.py_opts.iter().position(|o| o == "hao").unwrap();
    s.hold(50, None, &[Action::Space, Action::Slot(hao as u8)]);
    s.idle(100);
    assert_eq!(s.m.view().pinned, ["ni", "hao"]);
    s.press(Action::Slot(0));
    assert_eq!(s.committed(), "你好");
}
