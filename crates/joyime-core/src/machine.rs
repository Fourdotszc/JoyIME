//! 输入状态机：网页原型 prototype/app.js 里 handleInput 那一套的 Rust 版。
//!
//! 调用方每次读到摇杆状态（或者至少每 10ms）调一次 [`Machine::update`]，传入当前时间、方向和按住的功能，
//! 拿回要做的事（上屏文字、发退格 / 回车键）。界面用 [`Machine::view`] 取快照来画。
//! 这里不碰任何系统接口，所以可以直接写测试。

use crate::layout::{Action, Dir, Held, Layer, DEFAULT_LAYERS};
use crate::userdict::UserDict;
use joyime_english::EnglishEngine;
use joyime_pinyin::{to_t9, Mode, PinyinEngine};

/// 松杆后这么久内按键，仍算刚才的方向；在中位按键后这么久内推杆，算推到的方向
const LOOKBACK_MS: u64 = 80;
const LOOKAHEAD_MS: u64 = 60;
/// 九宫格：回中位时，取最后一个停够这么久的方向（从 ↗ 回中位可能擦过 ↑ 或 →，那一下很短）
const FLICK_STABLE_MS: u64 = 40;
const REPEAT_DELAY_MS: u64 = 400;
const REPEAT_RATE_MS: u64 = 60;
const PAGE_REPEAT_MS: u64 = 150;
/// 中文九宫格打字时按住空格：这么久之内松开（中间没按别的键）算点一下空格；按住超过这么久就亮出拼音栏
const SPACE_HOLD_MS: u64 = 300;
const MAX_COMP: usize = 40;
const MAX_EN_CANDIDATES: usize = 200;

/// 要发给当前程序的特殊键
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Backspace,
    Enter,
}

/// 状态机要调用方做的事
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// 把这段文字打进当前程序
    Commit(String),
    /// 发一个按键
    Key(Key),
    /// 这一下没对上任何字（界面可以闪一下这个方向，None = 中位）
    Miss(Option<Dir>),
}

/// 一个候选
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cand {
    pub text: String,
    /// 选它之后从拼写区开头删掉几个字符
    pub raw_len: usize,
    pub reading: String,
}

/// 正按住的空格（中文九宫格打字时）
#[derive(Clone, Copy)]
struct SpaceHold {
    since: u64,
    /// 按住期间按了别的键（组合），松开就不再算空格
    chorded: bool,
    /// 按住之前是不是已经在选拼音（选拼音键切过去的）
    was_focus: bool,
}

/// 给界面画的快照
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub enabled: bool,
    pub layer: Layer,
    pub dir: Option<Dir>,
    /// 拼写区原样（九宫格是数字）
    pub comp: String,
    /// 拼写区给人看的样子（九宫格换成读音 / 单词）
    pub comp_display: String,
    /// 当前这页候选
    pub page_cands: Vec<String>,
    pub page: usize,
    pub pages: usize,
    pub caps: Caps,
    /// 中文九宫格的拼音栏：已经定下的开头音节
    pub pinned: Vec<String>,
    /// 拼音栏里可选的音节：选拼音时是当前这页，平时是第一页（按住空格 + 上排键直接选）
    pub py_opts: Vec<String>,
    /// 正在选拼音（上排键选的是拼音不是候选）
    pub py_focus: bool,
    pub py_page: usize,
    pub py_pages: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Caps {
    #[default]
    Off,
    /// 下一个词首字母大写
    Once,
    /// 全部大写
    Lock,
}

struct Flick {
    /// 依次经过的方向和进入的时间
    segs: Vec<(Dir, u64)>,
    done: bool,
}

pub struct Machine {
    layers: Vec<Layer>,
    layer: usize,
    /// 有几个字母 / 候选键，决定每页几个候选
    slots: usize,
    enabled: bool,

    comp: String,
    cands: Vec<Cand>,
    page: usize,
    caps: Caps,
    /// 英文：上一个词后面自动补了空格，接着打标点要先把它删掉
    auto_space: bool,
    /// 英文：刚打完句号这类，下一个词首字母大写
    sentence_start: bool,
    /// 中文层的标点用全角（，。？！）还是半角（, . ? !）
    cn_fullwidth: bool,

    /// 用户词库：选过的字词排前面
    user: UserDict,
    learn: bool,
    /// 这次组字到现在为止选过的几段：按键串、字、选了几次。组字结束时，多段拼起来当一个词记住
    session_key: String,
    session_text: String,
    session_picks: u32,

    /// 中文九宫格的拼音栏（参考手机九键：重码时先定拼音，候选就只剩这个读音的字）
    pinned: Vec<String>,
    py_opts: Vec<String>,
    py_focus: bool,
    py_page: usize,
    /// 查候选用的串：九宫格定了音节时是「定下的音节的数字 ' 剩下的数字」，其他时候就是拼写区。
    /// 候选的 raw_len 是这个串里的位置
    query_raw: String,
    /// 按住空格选拼音（组合键）：中文九宫格打字时空格改成松开才算，按住期间上排键选拼音、翻页键翻拼音、退格撤销拼音
    space_hold: Option<SpaceHold>,

    dir: Option<Dir>,
    prev: Held,
    last_dir: Option<(Dir, u64)>,
    /// 中位按下、还在等推杆的字母键
    pending: Vec<(u8, u64)>,
    flick: Option<Flick>,
    bksp_repeat_at: u64,
    page_repeat_at: u64,
}

impl Default for Machine {
    fn default() -> Self {
        Machine::new(DEFAULT_LAYERS.to_vec(), 4)
    }
}

impl Machine {
    /// layers：切换顺序；slots：字母 / 候选键有几个（3 或 4）
    pub fn new(layers: Vec<Layer>, slots: usize) -> Self {
        assert!(!layers.is_empty());
        Machine {
            layers,
            layer: 0,
            slots: slots.clamp(1, 4),
            enabled: true,
            comp: String::new(),
            cands: Vec::new(),
            page: 0,
            caps: Caps::Off,
            auto_space: false,
            sentence_start: false,
            cn_fullwidth: true,
            user: UserDict::default(),
            learn: true,
            session_key: String::new(),
            session_text: String::new(),
            session_picks: 0,
            pinned: Vec::new(),
            py_opts: Vec::new(),
            py_focus: false,
            py_page: 0,
            query_raw: String::new(),
            space_hold: None,
            dir: None,
            prev: Held::default(),
            last_dir: None,
            pending: Vec::new(),
            flick: None,
            bksp_repeat_at: 0,
            page_repeat_at: 0,
        }
    }

    pub fn layer(&self) -> Layer {
        self.layers[self.layer]
    }

    pub fn set_layer(&mut self, layer: Layer) {
        if let Some(i) = self.layers.iter().position(|&l| l == layer) {
            self.layer = i;
            self.reset_comp();
            self.flick = None;
        }
    }

    /// 丢掉正在打的东西（焦点换到别的程序、JoyIME 被切走时）
    pub fn reset_input(&mut self) {
        self.reset_comp();
        self.pending.clear();
        self.flick = None;
    }

    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// 记不记用户选过的词
    pub fn set_learning(&mut self, on: bool) {
        self.learn = on;
    }

    pub fn user_dict(&self) -> &UserDict {
        &self.user
    }

    pub fn user_dict_mut(&mut self) -> &mut UserDict {
        &mut self.user
    }

    /// 换一个用户词库（从文件读进来的，或者重建状态机时从旧的那里拿过来的）
    pub fn set_user_dict(&mut self, d: UserDict) {
        self.user = d;
        self.refresh_cands();
    }

    pub fn take_user_dict(&mut self) -> UserDict {
        std::mem::take(&mut self.user)
    }

    /// 中文层（九宫格、全拼）打出的标点用全角还是半角
    pub fn set_cn_fullwidth(&mut self, on: bool) {
        self.cn_fullwidth = on;
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        self.reset_comp();
        self.pending.clear();
        self.flick = None;
    }

    pub fn view(&self) -> View {
        let pages = self.pages();
        View {
            enabled: self.enabled,
            layer: self.layer(),
            dir: self.dir,
            comp: self.comp.clone(),
            comp_display: self.comp_display(),
            page_cands: self.page_cands().iter().map(|c| c.text.clone()).collect(),
            page: self.page,
            pages,
            caps: self.caps,
            pinned: self.pinned.clone(),
            py_opts: if self.py_focus { self.page_py_opts().to_vec() } else { self.py_opts.iter().take(self.slots).cloned().collect() },
            py_focus: self.py_focus,
            py_page: self.py_page,
            py_pages: self.py_pages(),
        }
    }

    /// 推进一帧。now 是毫秒时间戳（单调递增即可）。
    pub fn update(&mut self, now: u64, dir: Option<Dir>, held: Held) -> Vec<Effect> {
        let mut fx = Vec::new();
        let prev = self.prev;
        let rose = |a: Action| held.contains(a) && !prev.contains(a);
        let held_long = |a: Action| held.contains(a) && prev.contains(a);
        self.dir = dir;

        if !self.enabled {
            if rose(Action::ToggleEnabled) {
                self.set_enabled(true);
            }
            self.prev = held;
            return fx;
        }

        if let Some(d) = dir {
            self.last_dir = Some((d, now));
        }
        if self.layer().is_t9() || self.layer() == Layer::Symbols {
            self.track_flick(now, dir, &mut fx);
        }

        let mut actions: Vec<Action> = (0..self.slots as u8).map(Action::Slot).collect();
        actions.extend([
            Action::Space,
            Action::Backspace,
            Action::Enter,
            Action::CycleLayer,
            Action::NextPage,
            Action::PrevPage,
            Action::Escape,
            Action::Shift,
            Action::ToggleEnabled,
            Action::PinyinSelect,
        ]);
        for a in actions {
            if !rose(a) {
                continue;
            }
            // 123 层：推着杆按字母键是打符号，这一推就不算数字了
            if self.layer() == Layer::Symbols && matches!(a, Action::Slot(_)) {
                if let Some(f) = &mut self.flick {
                    f.done = true;
                }
            }
            // 九宫格：摇杆还没回中位就按了键，先把这一推记上，再处理按键
            if self.layer().is_t9() {
                let pushed = self.flick.as_mut().filter(|f| !f.done).map(|f| {
                    f.done = true;
                    resolve_flick(&f.segs, now)
                });
                if let Some(d) = pushed {
                    self.t9_key(d);
                }
            }
            // 中文九宫格打字时，空格先不算，等松开或者组合
            if a == Action::Space && !self.comp.is_empty() && self.layer() == Layer::ChineseT9 {
                self.space_hold = Some(SpaceHold { since: now, chorded: false, was_focus: self.py_focus });
                continue;
            }
            if self.space_hold.is_some() && self.space_chord(a, now, dir, &mut fx) {
                continue;
            }
            self.press(a, now, dir, &mut fx);
            if !self.enabled {
                break;
            }
        }
        if let Some(h) = self.space_hold {
            if !held.contains(Action::Space) {
                // 松开空格：回到按住之前的状态；没组合、按得短，就是普通的空格（选第一个候选）
                self.space_hold = None;
                self.py_focus = h.was_focus && !self.py_opts.is_empty();
                if !h.chorded && now - h.since < SPACE_HOLD_MS {
                    self.press(Action::Space, now, dir, &mut fx);
                }
            } else if !self.py_focus && now - h.since >= SPACE_HOLD_MS && !self.py_opts.is_empty() {
                // 按住不放：亮出拼音栏，告诉用户现在上排键选的是拼音
                self.py_focus = true;
            }
        }

        // 中位按下的字母键：等到推杆就算那个方向，等太久就算中位
        let pending = std::mem::take(&mut self.pending);
        for (slot, t) in pending {
            if let Some(d) = dir {
                if self.layer() == Layer::Symbols && !held.contains(Action::Slot(slot)) {
                    // 123 层：按完松开了才推杆，是先打中位的字、再推下一个数字
                    self.commit_slot(None, slot, &mut fx);
                    continue;
                }
                if let Some(f) = &mut self.flick {
                    f.done = true; // 按住键再推杆，也是「方向 + 按键」
                }
                self.commit_slot(Some(d), slot, &mut fx);
            } else if now - t > LOOKAHEAD_MS {
                self.commit_slot(None, slot, &mut fx);
            } else {
                self.pending.push((slot, t));
            }
        }

        if held_long(Action::Backspace) && now >= self.bksp_repeat_at {
            self.backspace(&mut fx);
            self.bksp_repeat_at = now + REPEAT_RATE_MS;
        }
        // 按住翻页键连续翻（两个方向一起按住就不动）
        if !self.comp.is_empty() {
            let fwd = held_long(Action::CycleLayer) || held_long(Action::NextPage);
            let back = held_long(Action::Enter) || held_long(Action::PrevPage);
            let step = fwd as i32 - back as i32;
            if step != 0 && now >= self.page_repeat_at {
                self.flip_page(step);
                self.page_repeat_at = now + PAGE_REPEAT_MS;
            }
        }

        self.prev = held;
        fx
    }

    fn press(&mut self, a: Action, now: u64, dir: Option<Dir>, fx: &mut Vec<Effect>) {
        let layer = self.layer();
        let composing = !self.comp.is_empty();
        match a {
            Action::Slot(s) if layer.is_t9() => {
                if composing && self.py_focus {
                    self.pick_pinyin(self.py_page * self.slots + s as usize, fx);
                } else if composing {
                    self.pick(self.page * self.slots + s as usize, fx);
                } else {
                    self.quick_punct(s, fx);
                }
            }
            Action::Slot(s) => {
                // 123 层不用「松杆后再按」的缓冲：推完回中位再按，是要打中位的 5 0 . ,
                let recent = self
                    .last_dir
                    .filter(|&(_, t)| layer != Layer::Symbols && now - t <= LOOKBACK_MS)
                    .map(|(d, _)| d);
                if dir.is_some() {
                    self.commit_slot(dir, s, fx);
                } else if recent.is_some() {
                    self.commit_slot(recent, s, fx);
                } else {
                    self.pending.push((s, now));
                }
            }
            Action::Space => {
                if composing {
                    self.pick(self.page * self.slots, fx);
                } else {
                    self.commit(" ".into(), fx);
                }
            }
            Action::Backspace => {
                self.backspace(fx);
                self.bksp_repeat_at = now + REPEAT_DELAY_MS;
            }
            Action::Enter | Action::CycleLayer => {
                if composing && a == Action::Enter && self.cands.is_empty() && layer == Layer::Pinyin {
                    // 拼音层查不到候选：回车把字母原样上屏
                    let raw = self.comp.replace('\'', "");
                    self.reset_comp();
                    self.commit(raw, fx);
                } else if composing {
                    self.flip_page(if a == Action::CycleLayer { 1 } else { -1 });
                    self.page_repeat_at = now + REPEAT_DELAY_MS;
                } else if a == Action::CycleLayer {
                    self.layer = (self.layer + 1) % self.layers.len();
                    self.flick = None;
                } else {
                    self.auto_space = false;
                    fx.push(Effect::Key(Key::Enter));
                }
            }
            Action::NextPage | Action::PrevPage => {
                if composing {
                    self.flip_page(if a == Action::NextPage { 1 } else { -1 });
                    self.page_repeat_at = now + REPEAT_DELAY_MS;
                }
            }
            Action::Escape => self.reset_comp(),
            Action::Shift => {
                self.caps = match self.caps {
                    Caps::Off => Caps::Once,
                    Caps::Once => Caps::Lock,
                    Caps::Lock => Caps::Off,
                };
                self.refresh_cands();
            }
            Action::ToggleEnabled => self.set_enabled(false),
            Action::PinyinSelect => {
                if self.py_focus {
                    self.py_focus = false;
                } else if composing && layer == Layer::ChineseT9 && !self.py_opts.is_empty() {
                    self.py_focus = true;
                    self.py_page = 0;
                } else {
                    fx.push(Effect::Miss(None));
                }
            }
        }
    }

    // ---------- 九宫格推杆 ----------

    /// 摇杆离开中位到回中位算一下；回中位时把这一下记成一个键
    fn track_flick(&mut self, now: u64, dir: Option<Dir>, fx: &mut Vec<Effect>) {
        if let Some(d) = dir {
            let f = self.flick.get_or_insert_with(|| Flick { segs: Vec::new(), done: false });
            if f.segs.last().map(|s| s.0) != Some(d) {
                f.segs.push((d, now));
            }
        } else if let Some(f) = self.flick.take() {
            if f.done {
                return;
            }
            let d = resolve_flick(&f.segs, now);
            if self.layer() == Layer::Symbols {
                self.commit_digit(d, fx);
            } else {
                self.t9_key(d);
            }
        }
    }

    /// 123 层：方向对应的数字（按手机键盘排：↖1 ↑2 ↗3 ←4 →6 ↙7 ↓8 ↘9，5 和 0 在中位按键上）
    fn commit_digit(&mut self, d: Dir, fx: &mut Vec<Effect>) {
        if let Some(c) = Layer::Symbols.group(Some(d)).chars().next() {
            self.commit(c.to_string(), fx);
        }
    }

    fn t9_key(&mut self, d: Dir) {
        if self.comp.len() < MAX_COMP {
            self.comp.push(d.phone_key());
            self.update_comp();
        }
    }

    // ---------- 字母层 ----------

    fn commit_slot(&mut self, dir: Option<Dir>, slot: u8, fx: &mut Vec<Effect>) {
        let layer = self.layer();
        if layer == Layer::Pinyin && dir.is_none() && !self.comp.is_empty() {
            self.pick(self.page * self.slots + slot as usize, fx);
            return;
        }
        let Some(ch) = layer.group(dir).chars().nth(slot as usize) else {
            fx.push(Effect::Miss(dir));
            return;
        };
        if layer == Layer::Pinyin {
            self.pinyin_key(ch, fx);
        } else {
            self.commit(ch.to_string(), fx);
        }
    }

    fn pinyin_key(&mut self, ch: char, fx: &mut Vec<Effect>) {
        if ch.is_ascii_lowercase() {
            if self.comp.len() < MAX_COMP {
                self.comp.push(ch);
                self.update_comp();
            }
        } else if ch == '\'' && !self.comp.is_empty() {
            if !self.comp.ends_with('\'') {
                self.comp.push('\''); // 音节分隔：xi'an
                self.update_comp();
            }
        } else {
            self.commit_all(fx); // 标点：先把拼音整段上屏
            let p = if self.cn_fullwidth { cn_punct(ch).to_string() } else { ch.to_string() };
            self.commit(p, fx);
        }
    }

    // ---------- 拼写区和候选 ----------

    fn update_comp(&mut self) {
        self.page = 0;
        self.py_page = 0;
        if self.comp.is_empty() {
            self.pinned.clear();
            self.py_focus = false;
        }
        self.refresh_cands();
    }

    fn refresh_cands(&mut self) {
        self.cands = if self.comp.is_empty() {
            Vec::new()
        } else {
            match self.layer() {
                Layer::ChineseT9 => {
                    // 定下的音节换成数字、用 ' 隔开，剩下的数字接在后面；剩下的数字开头可能是哪些音节，放进拼音栏
                    let fixed_len: usize = self.pinned.iter().map(String::len).sum();
                    let rest = self.comp[fixed_len.min(self.comp.len())..].to_string();
                    let mut parts: Vec<String> = self.pinned.iter().map(|s| to_t9(s)).collect();
                    if !rest.is_empty() {
                        parts.push(rest.clone());
                    }
                    self.query_raw = parts.join("'");
                    let engine = PinyinEngine::builtin();
                    self.py_opts = if rest.is_empty() { Vec::new() } else { engine.syllable_options(&rest) };
                    if self.py_opts.is_empty() {
                        self.py_focus = false;
                    }
                    engine
                        .query_with(&self.query_raw, Mode::T9, &self.pinned.concat())
                        .into_iter()
                        .map(|c| Cand { text: c.text, raw_len: c.raw_len, reading: c.reading })
                        .collect()
                }
                Layer::Pinyin => {
                    self.query_raw = self.comp.clone();
                    PinyinEngine::builtin()
                        .query(&self.comp, Mode::Letters)
                        .into_iter()
                        .map(|c| Cand { text: c.text, raw_len: c.raw_len, reading: c.reading })
                        .collect()
                }
                Layer::EnglishT9 => {
                    self.query_raw = self.comp.clone();
                    let n = self.comp.len();
                    EnglishEngine::builtin()
                        .query_t9(&self.comp, MAX_EN_CANDIDATES)
                        .into_iter()
                        .map(|w| {
                            let text = self.apply_caps(&w);
                            Cand { reading: text.chars().take(n).collect(), text, raw_len: n }
                        })
                        .collect()
                }
                _ => Vec::new(),
            }
        };
        self.boost_user_words();
        let pages = self.pages();
        self.page = self.page.min(pages - 1);
    }

    /// 用户在这串按键上选过的词排最前；然后是引擎的第一个；再插上用户常用的、按键串以这串开头的词（打到一半联想）；
    /// 最后是引擎的其他候选。不重复。
    fn boost_user_words(&mut self) {
        let layer = self.layer();
        // 定了拼音就是用户在自己挑读音，不再插常用词（读音可能对不上）
        if !layer.composes() || self.comp.is_empty() || self.user.is_empty() || !self.pinned.is_empty() {
            return;
        }
        let key = self.comp.replace('\'', "");
        let exact: Vec<String> = self.user.lookup(layer, &key).into_iter().map(String::from).collect();
        let pred: Vec<String> = self.user.predictions(layer, &key, 2).into_iter().map(String::from).collect();
        if exact.is_empty() && pred.is_empty() {
            return;
        }
        let engine = std::mem::take(&mut self.cands);
        let fallback = engine.first().map_or_else(|| key.clone(), |c| c.reading.clone());
        let make = |w: &str| {
            let text = if layer == Layer::EnglishT9 { self.apply_caps(w) } else { w.to_string() };
            let reading = engine.iter().find(|c| c.text == text).map_or_else(|| fallback.clone(), |c| c.reading.clone());
            Cand { text, raw_len: self.comp.len(), reading }
        };
        let mut out: Vec<Cand> = Vec::new();
        let push = |out: &mut Vec<Cand>, c: Cand| {
            if !out.iter().any(|o| o.text == c.text) {
                out.push(c);
            }
        };
        for w in &exact {
            push(&mut out, make(w));
        }
        let mut rest = engine.clone().into_iter();
        if let Some(first) = rest.next() {
            push(&mut out, first);
        }
        for w in &pred {
            push(&mut out, make(w));
        }
        for c in rest {
            push(&mut out, c);
        }
        self.cands = out;
    }

    fn apply_caps(&self, w: &str) -> String {
        match self.caps {
            Caps::Lock => w.to_uppercase(),
            Caps::Once => capitalize(w),
            Caps::Off if self.sentence_start => capitalize(w),
            Caps::Off => w.to_string(),
        }
    }

    fn pick(&mut self, i: usize, fx: &mut Vec<Effect>) {
        let Some(c) = self.cands.get(i).cloned() else {
            fx.push(Effect::Miss(None));
            return;
        };
        let layer = self.layer();
        // raw_len 是查询串里的位置；九宫格定了音节时查询串里有 '，换成用掉了几个数字，被盖住的定下音节一起去掉
        let used = if layer == Layer::ChineseT9 {
            let used = self.query_raw[..c.raw_len.min(self.query_raw.len())].replace('\'', "").len();
            let mut acc = 0;
            while let Some(first) = self.pinned.first() {
                if acc + first.len() > used {
                    break;
                }
                acc += first.len();
                self.pinned.remove(0);
            }
            used.min(self.comp.len())
        } else {
            c.raw_len.min(self.comp.len())
        };
        self.py_focus = false;
        if self.learn && layer.composes() {
            let key = self.comp[..used].replace('\'', "");
            self.user.learn(layer, &key, &learned_form(layer, &c.text));
            self.session_key.push_str(&key);
            self.session_text.push_str(&c.text);
            self.session_picks += 1;
        }
        self.comp.drain(..used);
        if self.comp.is_empty() {
            // 一次组字分几段选完的，拼起来当一个词记住（英文一个词一个词地选，不拼）
            if self.learn && self.session_picks >= 2 && layer != Layer::EnglishT9 {
                let (k, t) = (self.session_key.clone(), self.session_text.clone());
                self.user.learn(layer, &k, &t);
            }
            self.reset_session();
        }
        if self.layer() == Layer::EnglishT9 {
            // 英文选完一个词自动补空格
            self.commit(format!("{} ", c.text), fx);
            self.auto_space = true;
            self.sentence_start = false;
            if self.caps == Caps::Once {
                self.caps = Caps::Off;
            }
        } else {
            self.commit(c.text, fx);
        }
        self.update_comp();
    }

    /// 依次选第一个候选，直到拼写区打完；查不到的部分按字母原样上屏
    fn commit_all(&mut self, fx: &mut Vec<Effect>) {
        for _ in 0..MAX_COMP {
            if self.comp.is_empty() || self.cands.is_empty() {
                break;
            }
            self.pick(0, fx);
        }
        if !self.comp.is_empty() && self.layer() == Layer::Pinyin {
            let raw = self.comp.replace('\'', "");
            self.reset_comp();
            self.commit(raw, fx);
        }
    }

    /// 九宫格层没在打字时，字母 / 候选键直接出常用标点
    fn quick_punct(&mut self, slot: u8, fx: &mut Vec<Effect>) {
        let i = slot as usize;
        if self.layer() == Layer::ChineseT9 {
            let set = if self.cn_fullwidth { ["，", "。", "？", "！"] } else { [",", ".", "?", "!"] };
            if let Some(p) = set.get(i) {
                self.commit(p.to_string(), fx);
            }
            return;
        }
        let Some(&p) = [",", ".", "?", "!"].get(i) else { return };
        if self.auto_space {
            // "hello " + "," → "hello, "
            fx.push(Effect::Key(Key::Backspace));
        }
        self.commit(format!("{p} "), fx);
        self.auto_space = true;
        self.sentence_start = p != ",";
        self.refresh_cands();
    }

    fn commit(&mut self, text: String, fx: &mut Vec<Effect>) {
        if text.is_empty() {
            return;
        }
        self.auto_space = false;
        fx.push(Effect::Commit(text));
    }

    fn backspace(&mut self, fx: &mut Vec<Effect>) {
        let fixed_len: usize = self.pinned.iter().map(String::len).sum();
        if self.comp.is_empty() {
            self.auto_space = false;
            fx.push(Effect::Key(Key::Backspace));
        } else if self.py_focus || (!self.pinned.is_empty() && self.comp.len() <= fixed_len) {
            // 选拼音时、或者数字都被定下的音节盖住了：退格先撤销最后定下的音节；没有可撤销的就回到选候选
            if self.pinned.pop().is_none() {
                self.py_focus = false;
            }
            self.update_comp();
        } else {
            self.comp.pop();
            self.update_comp();
        }
    }

    /// 按住空格时按了别的键：上排键选拼音、翻页键翻拼音、退格撤销拼音。返回 true = 这个键被组合用掉了
    fn space_chord(&mut self, a: Action, now: u64, dir: Option<Dir>, fx: &mut Vec<Effect>) -> bool {
        let Some(h) = &mut self.space_hold else { return false };
        match a {
            Action::Slot(s) => {
                h.chorded = true;
                if self.py_opts.is_empty() {
                    fx.push(Effect::Miss(None));
                    return true;
                }
                if !self.py_focus {
                    // 还没亮出拼音栏就按了：选的是平时显示的那一页（第一页）
                    self.py_focus = true;
                    self.py_page = 0;
                }
                self.pick_pinyin(self.py_page * self.slots + s as usize, fx);
                // 定完了还按住：拼音栏有下一个音节就接着亮着
                self.py_focus = !self.py_opts.is_empty();
                true
            }
            Action::Enter | Action::CycleLayer | Action::NextPage | Action::PrevPage => {
                h.chorded = true;
                if self.py_opts.is_empty() {
                    fx.push(Effect::Miss(None));
                } else {
                    self.py_focus = true;
                    self.press(a, now, dir, fx);
                }
                true
            }
            Action::Backspace => {
                h.chorded = true;
                if self.pinned.pop().is_some() {
                    self.update_comp();
                    self.py_focus = !self.py_opts.is_empty();
                } else {
                    fx.push(Effect::Miss(None));
                }
                true
            }
            _ => false,
        }
    }

    /// 拼音栏选了一个音节：定下它，候选只剩这个读音的；后面还有数字就接着选下一个音节
    fn pick_pinyin(&mut self, i: usize, fx: &mut Vec<Effect>) {
        let Some(s) = self.py_opts.get(i).cloned() else {
            fx.push(Effect::Miss(None));
            return;
        };
        self.pinned.push(s);
        self.update_comp();
    }

    fn reset_comp(&mut self) {
        self.space_hold = None;
        self.comp.clear();
        self.cands.clear();
        self.page = 0;
        self.pinned.clear();
        self.py_opts.clear();
        self.py_focus = false;
        self.py_page = 0;
        self.query_raw.clear();
        self.reset_session();
    }

    fn reset_session(&mut self) {
        self.session_key.clear();
        self.session_text.clear();
        self.session_picks = 0;
    }

    fn pages(&self) -> usize {
        self.cands.len().div_ceil(self.slots).max(1)
    }

    /// 翻页：选拼音时翻拼音栏，否则翻候选。step = 1 下一页，-1 上一页，两头循环（第 1 页往前翻到最后一页）
    fn flip_page(&mut self, step: i32) {
        if self.py_focus {
            let pages = self.py_pages() as i32;
            self.py_page = (self.py_page as i32 + step).rem_euclid(pages) as usize;
            return;
        }
        let pages = self.pages() as i32;
        self.page = (self.page as i32 + step).rem_euclid(pages) as usize;
    }

    fn py_pages(&self) -> usize {
        self.py_opts.len().div_ceil(self.slots).max(1)
    }

    fn page_py_opts(&self) -> &[String] {
        let start = (self.py_page * self.slots).min(self.py_opts.len());
        let end = (start + self.slots).min(self.py_opts.len());
        &self.py_opts[start..end]
    }

    fn page_cands(&self) -> &[Cand] {
        let start = (self.page * self.slots).min(self.cands.len());
        let end = (start + self.slots).min(self.cands.len());
        &self.cands[start..end]
    }

    /// 拼写区给人看的样子：九宫格换成第一个候选的读音，没覆盖到的部分仍显示数字
    fn comp_display(&self) -> String {
        if !self.layer().is_t9() {
            return self.comp.clone();
        }
        match self.cands.first() {
            None => {
                // 查不到：定下的音节照写，剩下的显示数字
                let fixed_len: usize = self.pinned.iter().map(String::len).sum();
                let mut parts = self.pinned.clone();
                if self.comp.len() > fixed_len {
                    parts.push(self.comp[fixed_len..].to_string());
                }
                parts.join("'")
            }
            Some(c) => {
                let rest = self.query_raw.get(c.raw_len..).unwrap_or("").replace('\'', "");
                if rest.is_empty() {
                    c.reading.clone()
                } else {
                    format!("{}'{}", c.reading, rest)
                }
            }
        }
    }
}

/// 提前载入词库、建好九宫格表。第一次查询要几十到几百毫秒，
/// 放到用户第一次推杆时做会卡住输入线程，那期间推的杆全丢了。程序启动时在后台线程调一次。
pub fn warm_up() {
    PinyinEngine::builtin().query("2", Mode::T9);
    EnglishEngine::builtin().query_t9("2", 1);
}

/// 取最后一个停够 FLICK_STABLE_MS 的方向；都不够就取停得最久的
fn resolve_flick(segs: &[(Dir, u64)], now: u64) -> Dir {
    let mut longest = (segs[0].0, 0);
    for k in (0..segs.len()).rev() {
        let end = segs.get(k + 1).map_or(now, |s| s.1);
        let dur = end - segs[k].1;
        if dur >= FLICK_STABLE_MS {
            return segs[k].0;
        }
        if dur > longest.1 {
            longest = (segs[k].0, dur);
        }
    }
    longest.0
}

fn cn_punct(ch: char) -> &'static str {
    match ch {
        ',' => "，",
        '.' => "。",
        '?' => "？",
        '!' => "！",
        '\'' => "'",
        '-' => "-",
        _ => "",
    }
}

/// 存进用户词库的写法：英文存小写（显示时再按大小写状态改），I / I'm 这类保持大写
fn learned_form(layer: Layer, text: &str) -> String {
    if layer != Layer::EnglishT9 {
        return text.to_string();
    }
    let lower = text.to_lowercase();
    if lower == "i" || lower.starts_with("i'") {
        capitalize(&lower)
    } else {
        lower
    }
}

fn capitalize(w: &str) -> String {
    let mut cs = w.chars();
    match cs.next() {
        Some(f) => f.to_uppercase().chain(cs).collect(),
        None => String::new(),
    }
}
