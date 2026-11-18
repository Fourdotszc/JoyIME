//! 界面语言。界面上的字大多在网页里翻译（ui/i18n.js），这里只管后台自己显示的：托盘菜单、窗口标题。

/// 设置里的语言（"zh" / "en" / "auto"）→ 实际用的语言。auto 跟随 Windows 显示语言：中文用中文，其他都用英文
pub fn resolve(setting: &str) -> &'static str {
    match setting {
        "zh" => "zh",
        "en" => "en",
        _ => system(),
    }
}

#[cfg(windows)]
fn system() -> &'static str {
    // 语言 ID 的低 10 位是主语言，0x04 = 中文（简体、繁体都算）
    let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    if id & 0x3ff == 0x04 {
        "zh"
    } else {
        "en"
    }
}

#[cfg(not(windows))]
fn system() -> &'static str {
    match std::env::var("LANG") {
        Ok(l) if l.starts_with("zh") => "zh",
        _ => "en",
    }
}

/// 后台自己显示的几段字
pub fn text(lang: &str, key: &str) -> &'static str {
    let en = lang == "en";
    match key {
        "title" => if en { "JoyIME" } else { "JoyIME 摇杆输入法" },
        "open" => if en { "Open settings" } else { "打开设置" },
        "pause" => if en { "Pause input" } else { "暂停输入" },
        "resume" => if en { "Resume input" } else { "恢复输入" },
        "quit" => if en { "Quit JoyIME" } else { "退出 JoyIME" },
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_setting_wins() {
        assert_eq!(resolve("zh"), "zh");
        assert_eq!(resolve("en"), "en");
        assert!(["zh", "en"].contains(&resolve("auto")));
        assert_eq!(text("en", "quit"), "Quit JoyIME");
        assert_eq!(text("zh", "quit"), "退出 JoyIME");
    }
}
