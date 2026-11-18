//! 程序级设置（和手柄无关的那些），存在配置目录的 settings.json 里。

use joyime_core::{Layer, DEFAULT_LAYERS};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 点进输入框时在光标旁边显示方向提示
    pub hint: bool,
    /// 没有操作多少秒后收起方向提示，0 = 一直显示
    pub hint_secs: u32,
    /// 候选窗跟着文字光标走；false 就固定在屏幕底部
    pub follow_caret: bool,
    /// 启用哪些输入层（Layer::id），按 Layer::ALL 的顺序切换
    pub layers: Vec<String>,
    /// 中文层的标点用全角（，。？！）还是半角
    pub cn_fullwidth: bool,
    /// 字怎么打进程序："ime" 系统输入法（在输入法列表里选 JoyIME 才工作）/ "inject" 模拟键入 /
    /// "auto" 装了系统输入法就用它，没装就模拟键入
    pub output: String,
    /// 记住用户常用的字词，下次排在前面
    pub learn: bool,
    /// 界面语言："zh" / "en" / "auto" 跟随 Windows 显示语言（见 i18n.rs）
    pub lang: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hint: true,
            hint_secs: 4,
            follow_caret: true,
            layers: DEFAULT_LAYERS.iter().map(|l| l.id().to_string()).collect(),
            cn_fullwidth: true,
            output: "auto".into(),
            learn: true,
            lang: "auto".into(),
        }
    }
}

impl Settings {
    /// 启用的输入层，按固定顺序；一个都没选就用默认的
    pub fn layers(&self) -> Vec<Layer> {
        let on: Vec<Layer> = Layer::ALL.into_iter().filter(|l| self.layers.iter().any(|id| id == l.id())).collect();
        if on.is_empty() {
            DEFAULT_LAYERS.to_vec()
        } else {
            on
        }
    }
}

pub fn path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, s: &Settings) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(t) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(path, t);
    }
}
