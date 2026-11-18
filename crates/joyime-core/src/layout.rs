//! 方向、按键功能和输入层：都是纯数据，设置界面和状态机共用。

/// 摇杆方向，顺时针从上开始编号 0–7。中位用 `Option<Dir>` 的 `None` 表示。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
}

impl Dir {
    pub const ALL: [Dir; 8] = [
        Dir::Up,
        Dir::UpRight,
        Dir::Right,
        Dir::DownRight,
        Dir::Down,
        Dir::DownLeft,
        Dir::Left,
        Dir::UpLeft,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// 由 x、y（右、下为正）算方向，离中心不到 0.5 算中位。
    pub fn from_xy(x: f32, y: f32) -> Option<Dir> {
        if x.hypot(y) < 0.5 {
            return None;
        }
        let a = x.atan2(-y); // 0 = 上，顺时针为正
        let i = (a / std::f32::consts::FRAC_PI_4).round() as i32;
        Some(Dir::ALL[i.rem_euclid(8) as usize])
    }

    /// 这个方向是手机九宫格的哪个键。5（jkl）在中位选不到，挪到了 ↖。
    pub fn phone_key(self) -> char {
        ['2', '3', '6', '9', '8', '7', '4', '5'][self.index()]
    }
}

/// 按键可以分配的功能。设置界面里给每个物理按键选一个。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// 第 n 个字母键 / 候选键（0 起）。字母层里选组里第几个字，打字时选第几个候选
    Slot(u8),
    /// 空格；打字时选第一个候选
    Space,
    /// 退格；打字时删一个拼音字母 / 一个键
    Backspace,
    /// 回车；打字时是上一页
    Enter,
    /// 切换输入层；打字时是下一页
    CycleLayer,
    /// 只翻页，不换层（按键多的摇杆用）
    NextPage,
    PrevPage,
    /// 清掉正在打的拼音
    Escape,
    /// 英文：下一个词首字母大写，连按两下锁定大写
    Shift,
    /// 暂停 / 恢复输入法（玩游戏时用）
    ToggleEnabled,
    /// 中文九宫格打字时：上排键改成选拼音（9426 选 xian / xiao …），再按一次回到选候选
    PinyinSelect,
}

impl Action {
    /// 用于位集合的编号
    fn bit(self) -> u32 {
        match self {
            Action::Slot(n) => n.min(7) as u32,
            Action::Space => 8,
            Action::Backspace => 9,
            Action::Enter => 10,
            Action::CycleLayer => 11,
            Action::NextPage => 12,
            Action::PrevPage => 13,
            Action::Escape => 14,
            Action::Shift => 15,
            Action::ToggleEnabled => 16,
            Action::PinyinSelect => 17,
        }
    }
}

/// 这一帧按住了哪些功能。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Held(u32);

impl Held {
    pub fn insert(&mut self, a: Action) {
        self.0 |= 1 << a.bit();
    }
    pub fn contains(self, a: Action) -> bool {
        self.0 & (1 << a.bit()) != 0
    }
    pub fn from_actions(actions: impl IntoIterator<Item = Action>) -> Self {
        let mut h = Held::default();
        for a in actions {
            h.insert(a);
        }
        h
    }
}

/// 输入层。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    /// 中文九宫格：只推方向，自动猜拼音
    ChineseT9,
    /// 中文全拼：方向 + 按键逐个打拼音字母
    Pinyin,
    /// 英文九宫格：只推方向，自动猜单词
    EnglishT9,
    /// 小写字母，直接上屏
    Lower,
    /// 大写字母，直接上屏
    Upper,
    /// 数字和符号：推一下 = 那个方向的数字（手机键盘排法），方向 + 按键 = 符号，中位按键 = 5 0 . ,
    Symbols,
}

impl Layer {
    pub const ALL: [Layer; 6] = [
        Layer::ChineseT9,
        Layer::EnglishT9,
        Layer::Symbols,
        Layer::Pinyin,
        Layer::Lower,
        Layer::Upper,
    ];

    /// 存设置用的名字
    pub fn id(self) -> &'static str {
        match self {
            Layer::ChineseT9 => "cn9",
            Layer::Pinyin => "pinyin",
            Layer::EnglishT9 => "en9",
            Layer::Lower => "lower",
            Layer::Upper => "upper",
            Layer::Symbols => "num",
        }
    }

    pub fn from_id(id: &str) -> Option<Layer> {
        Layer::ALL.into_iter().find(|l| l.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Layer::ChineseT9 => "九",
            Layer::Pinyin => "拼",
            Layer::EnglishT9 => "英",
            Layer::Lower => "abc",
            Layer::Upper => "ABC",
            Layer::Symbols => "123",
        }
    }

    /// 只推方向就算按一次键的层
    pub fn is_t9(self) -> bool {
        matches!(self, Layer::ChineseT9 | Layer::EnglishT9)
    }

    /// 打的东西先进拼写区、再选候选的层
    pub fn composes(self) -> bool {
        matches!(self, Layer::ChineseT9 | Layer::Pinyin | Layer::EnglishT9)
    }

    /// 某个方向（None = 中位）上这一层排的字，最多 4 个，对应 4 个字母键。
    /// 九宫格层返回这个键上的字母，只是给界面看的。
    pub fn group(self, dir: Option<Dir>) -> &'static str {
        // 每组 4 个；3 个字母的组，第 4 格放常用标点
        const ABC: [&str; 8] = ["abc,", "def.", "mno'", "wxyz", "tuv-", "pqrs", "ghi?", "jkl!"];
        const ABC_UP: [&str; 8] = ["ABC,", "DEF.", "MNO'", "WXYZ", "TUV-", "PQRS", "GHI?", "JKL!"];
        const T9: [&str; 8] = ["abc", "def", "mno", "wxyz", "tuv", "pqrs", "ghi", "jkl"];
        const SYM: [&str; 8] = ["2+-=", "3*/^", "6:;\"", "9$\\|", "8[]~", "7<>_", "4@#&", "1()%"];
        match (self, dir) {
            (Layer::Symbols, None) => "50.,",
            (_, None) => "",
            (Layer::ChineseT9 | Layer::EnglishT9, Some(d)) => T9[d.index()],
            (Layer::Pinyin | Layer::Lower, Some(d)) => ABC[d.index()],
            (Layer::Upper, Some(d)) => ABC_UP[d.index()],
            (Layer::Symbols, Some(d)) => SYM[d.index()],
        }
    }
}

/// 默认启用的输入层和切换顺序：常用的放前面（中文、英文、数字）。全拼默认不开，在设置里可以打开
pub const DEFAULT_LAYERS: [Layer; 5] = [
    Layer::ChineseT9,
    Layer::EnglishT9,
    Layer::Symbols,
    Layer::Lower,
    Layer::Upper,
];
