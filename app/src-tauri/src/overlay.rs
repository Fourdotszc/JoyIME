//! 候选悬浮窗的显示和隐藏。
//!
//! 三个坑：
//! 1. 悬浮窗绝不能抢焦点，不然选好的字会打进悬浮窗自己，而不是用户正在用的程序。
//!    Windows 上光靠 focusable(false) 不够，show() 仍会激活窗口，所以直接用 Win32：
//!    加 WS_EX_NOACTIVATE，显示时用 SWP_NOACTIVATE。
//! 2. 窗口操作都要到主线程执行，同步调用会把输入线程卡住几百毫秒，期间推的杆就丢了。
//!    所以这里全部用 run_on_main_thread 丢过去，不等结果。
//! 3. 多显示器缩放比例不同时，把窗口挪到另一块屏上，物理尺寸不会跟着变，内容就挤成一团。
//!    所以每次显示都按那块屏的缩放比例重新算物理尺寸。
//! 窗口本身比面板大、背景透明，并且设成点不到（ignore cursor events），不会挡住下面的东西。

use crate::caret::Rect;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/// 悬浮窗的逻辑尺寸（界面按这个尺寸设计，见 ui/overlay.css）
pub const WIDTH: f64 = 900.0;
pub const HEIGHT: f64 = 190.0;

/// 建好悬浮窗后调一次：设成永不激活的工具窗口
pub fn init(w: &WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = w.hwnd() {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        unsafe {
            let h = hwnd.0 as _;
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST) as isize);
        }
    }
    #[cfg(not(windows))]
    let _ = w;
}

/// 悬浮窗的样子：compose 打字中 / hint 刚进输入框的方向提示 / toast 换层、开关的提示
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Compose,
    Hint,
    Toast,
}

#[derive(Clone, Serialize)]
struct Placement {
    mode: Mode,
    /// 放在光标上方（下面放不下）时，面板要贴着窗口底边
    above: bool,
}

pub fn hide(app: &AppHandle) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = app2.get_webview_window("overlay") {
            hide_window(&w);
        }
    });
}

/// 显示悬浮窗，不等它做完。anchor 是文字光标（或输入框）在屏幕上的位置，
/// 有的话贴着它放（下方放不下就放上方），没有就放在鼠标所在屏幕的下方中间。
pub fn show(app: &AppHandle, mode: Mode, anchor: Option<Rect>) {
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(w) = app2.get_webview_window("overlay") else { return };
        // 开发用：JOYIME_OVERLAY_MONITOR=序号 固定放在某块屏上；写成 0,1 就每次弹出轮流换屏，测多屏缩放
        let forced = std::env::var("JOYIME_OVERLAY_MONITOR").ok().and_then(|s| {
            static SHOWN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let list: Vec<usize> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            let n = SHOWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let i = *list.get(n % list.len().max(1))?;
            app2.available_monitors().ok()?.into_iter().nth(i)
        });
        let anchor = if forced.is_some() { None } else { anchor };
        let monitor = forced
            .or_else(|| anchor.and_then(|a| app2.monitor_from_point(a.x as f64, a.y as f64).ok().flatten()))
            .or_else(|| {
                let c = app2.cursor_position().ok()?;
                app2.monitor_from_point(c.x, c.y).ok().flatten()
            })
            .or_else(|| app2.primary_monitor().ok().flatten());
        let Some(m) = monitor else {
            show_at(&w, None);
            return;
        };
        let scale = m.scale_factor();
        let size = PhysicalSize::new((WIDTH * scale).round() as u32, (HEIGHT * scale).round() as u32);
        let (w_px, h_px) = (size.width as i32, size.height as i32);
        let area = m.work_area();
        let (left, top) = (area.position.x, area.position.y);
        let (right, bottom) = (left + area.size.width as i32, top + area.size.height as i32);
        let gap = (2.0 * scale) as i32;
        // 面板在窗口里有 6px 内边距（overlay.css 的 body padding），减掉它，面板左边正好对着光标
        let pad = (6.0 * scale) as i32;
        let (x, y, above) = match anchor {
            Some(a) => {
                let below = a.y + a.h + gap - pad;
                let (y, above) = if below + h_px <= bottom {
                    (below, false)
                } else {
                    (a.y - gap - h_px + pad, true)
                };
                (a.x - pad, y, above)
            }
            None => ((left + right - w_px) / 2, bottom - h_px - (40.0 * scale) as i32, false),
        };
        let x = x.clamp(left, (right - w_px).max(left));
        let y = y.clamp(top, (bottom - h_px).max(top));
        let _ = w.emit("overlay", Placement { mode, above });
        show_at(&w, Some((PhysicalPosition::new(x, y), size)));
    });
}

#[cfg(windows)]
fn show_at(w: &WebviewWindow, place: Option<(PhysicalPosition<i32>, PhysicalSize<u32>)>) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let Ok(hwnd) = w.hwnd() else { return };
    let h = hwnd.0 as _;
    let flags = SWP_NOACTIVATE | SWP_SHOWWINDOW;
    unsafe {
        match place {
            Some((p, s)) => {
                // 挪到缩放比例不同的屏上时，Windows 会按比例把窗口再缩放一次（WM_DPICHANGED），
                // 所以先挪过去让它缩放完，再设成想要的尺寸
                SetWindowPos(h, HWND_TOPMOST, p.x, p.y, 0, 0, flags | SWP_NOSIZE);
                SetWindowPos(h, HWND_TOPMOST, p.x, p.y, s.width as i32, s.height as i32, flags);
            }
            None => {
                SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, flags | SWP_NOMOVE | SWP_NOSIZE);
            }
        }
    }
}

#[cfg(not(windows))]
fn show_at(w: &WebviewWindow, place: Option<(PhysicalPosition<i32>, PhysicalSize<u32>)>) {
    if let Some((p, s)) = place {
        let _ = w.set_size(s);
        let _ = w.set_position(p);
    }
    let _ = w.show();
}

#[cfg(windows)]
fn hide_window(w: &WebviewWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    if let Ok(hwnd) = w.hwnd() {
        unsafe {
            ShowWindow(hwnd.0 as _, SW_HIDE);
        }
    }
}

#[cfg(not(windows))]
fn hide_window(w: &WebviewWindow) {
    let _ = w.hide();
}
