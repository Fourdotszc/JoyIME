//! 按键映射：每个物理按键（原始编号）分配一个功能。按手柄型号分别保存。

use gilrs::{Button, Gamepad};
use joyime_core::Action;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

/// 可以分配给按键的功能：(id, 显示名, 说明)。设置界面的下拉框按这个顺序列。
pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("slot0", "字母 / 候选 1", "字母层选组里第 1 个字；打字时选第 1 个候选；九宫格空闲时打 ，"),
    ("slot1", "字母 / 候选 2", "第 2 个字 / 候选；九宫格空闲时打 。"),
    ("slot2", "字母 / 候选 3", "第 3 个字 / 候选；九宫格空闲时打 ？"),
    ("slot3", "字母 / 候选 4", "第 4 个字 / 候选；九宫格空闲时打 ！"),
    ("space", "空格", "空格；打字时选第 1 个候选；中文九宫格打字时按住它 + 上排键 = 选拼音"),
    ("backspace", "退格", "退格；打字时删一个键"),
    ("enter", "回车 / 上一页", "回车；打字时往前翻一页，按住连翻"),
    ("cycle_layer", "切换 / 下一页", "切换输入层；打字时往后翻一页，按住连翻"),
    ("next_page", "下一页", "只翻页，不换层"),
    ("prev_page", "上一页", "只翻页，不换层"),
    ("escape", "清空", "清掉正在打的拼音 / 按键"),
    ("shift", "大写", "英文：下一个词首字母大写，再按一次锁定大写"),
    ("toggle", "开关输入法", "暂停 / 恢复（玩游戏时用）"),
    ("pinyin", "选拼音", "中文九宫格打字时，上排键改成选拼音，再按一次回到选候选（也可以不用它：按住空格 + 上排键）"),
];

pub fn parse_action(id: &str) -> Option<Action> {
    Some(match id {
        "slot0" => Action::Slot(0),
        "slot1" => Action::Slot(1),
        "slot2" => Action::Slot(2),
        "slot3" => Action::Slot(3),
        "space" => Action::Space,
        "backspace" => Action::Backspace,
        "enter" => Action::Enter,
        "cycle_layer" => Action::CycleLayer,
        "next_page" => Action::NextPage,
        "prev_page" => Action::PrevPage,
        "escape" => Action::Escape,
        "shift" => Action::Shift,
        "toggle" => Action::ToggleEnabled,
        "pinyin" => Action::PinyinSelect,
        _ => return None,
    })
}

/// 预设布局：按「标准按键位置」分配功能。名字是 SDL 手柄映射库里的标准名字，
/// 街机摇杆在 XInput 下上排是 X Y RB LB、下排是 A B RT LT。
pub const PRESETS: &[(&str, &str, &[(Button, &str)])] = &[
    (
        "arcade8",
        "街机摇杆 · 8 键",
        &[
            (Button::West, "slot0"),
            (Button::North, "slot1"),
            (Button::RightTrigger, "slot2"),
            (Button::LeftTrigger, "slot3"),
            (Button::South, "space"),
            (Button::East, "backspace"),
            (Button::RightTrigger2, "cycle_layer"),
            (Button::LeftTrigger2, "enter"),
            (Button::Select, "escape"),
            (Button::Start, "pinyin"),
            (Button::Mode, "toggle"),
        ],
    ),
    (
        "arcade6",
        "街机摇杆 · 6 键",
        &[
            (Button::West, "slot0"),
            (Button::North, "slot1"),
            (Button::RightTrigger, "slot2"),
            (Button::South, "slot3"),
            (Button::East, "space"),
            (Button::RightTrigger2, "backspace"),
            (Button::Start, "cycle_layer"),
            (Button::Select, "enter"),
            (Button::Mode, "toggle"),
        ],
    ),
    (
        "gamepad",
        "游戏手柄",
        &[
            (Button::West, "slot0"),
            (Button::North, "slot1"),
            (Button::East, "slot2"),
            (Button::South, "slot3"),
            (Button::RightTrigger, "space"),
            (Button::LeftTrigger, "backspace"),
            (Button::RightTrigger2, "cycle_layer"),
            (Button::LeftTrigger2, "enter"),
            (Button::Select, "escape"),
            (Button::Start, "pinyin"),
            (Button::Mode, "toggle"),
        ],
    ),
];

/// 街机摇杆上的按键位置，和它在标准映射里通常对应的按键。
/// 「检测按键」时用户按位置依次按一遍，记下来的位置优先于标准映射（有的摇杆标准映射是错位的，有的根本没映射）。
pub const POSITIONS: &[(&str, &str, Button)] = &[
    ("top1", "上排 1", Button::West),
    ("top2", "上排 2", Button::North),
    ("top3", "上排 3", Button::RightTrigger),
    ("top4", "上排 4", Button::LeftTrigger),
    ("bot1", "下排 1", Button::South),
    ("bot2", "下排 2", Button::East),
    ("bot3", "下排 3", Button::RightTrigger2),
    ("bot4", "下排 4", Button::LeftTrigger2),
    ("select", "Select / Back", Button::Select),
    ("start", "Start", Button::Start),
    ("home", "Home", Button::Mode),
];

/// 一个手柄型号的设置
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// 用的哪个预设；用户改过任意一个键就变成 "custom"
    pub preset: String,
    /// 原始按键编号 → 功能 id。JSON 的键只能是字符串，所以编号也存成字符串
    pub bindings: BTreeMap<String, String>,
    /// 「检测按键」记下的位置：原始按键编号 → 位置 id（POSITIONS 里的 top1、bot2 …）
    #[serde(default)]
    pub names: BTreeMap<String, String>,
}

impl Profile {
    /// 按预设生成：只分配这个手柄真的有的按键。保留已经检测过的按键位置。
    pub fn from_preset(preset: &str, pad: &Gamepad, names: BTreeMap<String, String>) -> Profile {
        let roles = PRESETS.iter().find(|p| p.0 == preset).map_or(PRESETS[0].2, |p| p.2);
        let mut bindings = BTreeMap::new();
        for &(btn, action) in roles {
            if let Some(code) = code_for(btn, pad, &names) {
                bindings.insert(code.to_string(), action.to_string());
            }
        }
        Profile { preset: preset.to_string(), bindings, names }
    }

    pub fn action_for(&self, code: u32) -> Option<Action> {
        self.bindings.get(&code.to_string()).and_then(|a| parse_action(a))
    }

    /// 字母 / 候选键有几个（决定每页几个候选）
    pub fn slots(&self) -> usize {
        let n = (0..4).filter(|i| self.bindings.values().any(|a| *a == format!("slot{i}"))).count();
        n.max(1)
    }
}

/// 标准按键 btn 在这个手柄上是哪个原始编号：先看检测过的位置，再看标准映射
fn code_for(btn: Button, pad: &Gamepad, names: &BTreeMap<String, String>) -> Option<u32> {
    let learned = POSITIONS
        .iter()
        .find(|p| p.2 == btn)
        .and_then(|p| names.iter().find(|(_, pos)| *pos == p.0))
        .and_then(|(code, _)| code.parse().ok());
    learned.or_else(|| pad.button_code(btn).map(|c| c.into_u32()))
}

/// 所有手柄型号的设置，存在 profiles.json 里
#[derive(Default, Serialize, Deserialize)]
pub struct Store {
    pub profiles: HashMap<String, Profile>,
    #[serde(skip)]
    path: PathBuf,
}

impl Store {
    pub fn load(path: PathBuf) -> Store {
        let mut s: Store = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        s.path = path;
        s
    }

    pub fn save(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&self.path, t);
        }
    }

    /// 取这个手柄的设置，没有就按 8 键街机预设建一个。
    /// 没改过的预设：新版本给预设加了功能（比如 Start = 选拼音）就补上，只补空着的键
    pub fn for_pad(&mut self, pad: &Gamepad) -> &mut Profile {
        let p = self
            .profiles
            .entry(pad_key(pad))
            .or_insert_with(|| Profile::from_preset("arcade8", pad, BTreeMap::new()));
        if p.preset != "custom" {
            let fresh = Profile::from_preset(&p.preset, pad, p.names.clone());
            for (code, action) in fresh.bindings {
                p.bindings.entry(code).or_insert(action);
            }
        }
        p
    }
}

/// 手柄型号的键：SDL 风格的 GUID，同一型号的手柄一样
pub fn pad_key(pad: &Gamepad) -> String {
    pad.uuid().iter().map(|b| format!("{b:02x}")).collect()
}
