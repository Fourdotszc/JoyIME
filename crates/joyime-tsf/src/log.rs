//! 调试日志。默认什么都不写；设了环境变量 JOYIME_TSF_LOG，或者注册表
//! HKCU\Software\JoyIME 的 TsfLog（字符串，文件路径），就往那个文件追加。
//! DLL 同时在很多进程里，每行带上进程号和线程号。

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

fn path() -> Option<&'static PathBuf> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        std::env::var_os("JOYIME_TSF_LOG")
            .map(PathBuf::from)
            .or_else(|| crate::register::read_string(true, r"Software\JoyIME", "TsfLog").map(PathBuf::from))
    })
    .as_ref()
}

pub fn log(msg: &str) {
    let Some(p) = path() else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let tid = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
        let _ = writeln!(f, "[{} {}] {msg}", std::process::id(), tid);
    }
}
