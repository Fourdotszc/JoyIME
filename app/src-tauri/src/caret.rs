//! 找到当前键盘焦点：焦点控件能不能打字、文字光标在屏幕上的哪里。
//! 候选窗跟着光标走、点进输入框时弹方向提示，都靠这个。

/// 屏幕上的矩形，物理像素（和 Tauri 的 PhysicalPosition 一致，多显示器时可以是负数）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Focus {
    /// 焦点控件能不能输入文字（文本框、编辑区、浏览器里的输入框……）
    pub editable: bool,
    /// 文字光标（插入点）的位置；拿不到时 None
    pub caret: Option<Rect>,
    /// 焦点控件本身的位置；拿不到光标时候选窗贴着它放
    pub element: Option<Rect>,
    /// 前台窗口所属进程，调用方用来排除 JoyIME 自己的窗口
    pub process_id: u32,
    /// 用哪种办法拿到的："gui-caret" / "msaa" / "uia" / "none"，调试用
    pub source: &'static str,
}

#[cfg(windows)]
use std::{ffi::c_void, mem::size_of, ptr};

#[cfg(windows)]
use windows::{
    core::Interface,
    Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::Gdi::ClientToScreen,
        System::{
            Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED},
            Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayUnaccessData},
            Variant::VARIANT,
        },
        UI::{
            Accessibility::{
                AccessibleObjectFromWindow, CUIAutomation, IAccessible, IUIAutomation2,
                IUIAutomationTextPattern, IUIAutomationTextPattern2, IUIAutomationTextRange,
                IUIAutomationValuePattern, TextUnit_Character, UIA_DocumentControlTypeId,
                UIA_EditControlTypeId, UIA_TextPattern2Id, UIA_TextPatternId, UIA_ValuePatternId,
            },
            WindowsAndMessaging::{
                GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
                OBJID_CARET,
            },
        },
    },
};

/// 只能在创建它的那个线程上用（里面有 COM 对象）。
#[cfg(windows)]
pub struct Tracker {
    uia: Option<IUIAutomation2>,
    com_initialized: bool,
}

/// 非 Windows 没有对应的系统接口。
#[cfg(not(windows))]
pub struct Tracker;

#[cfg(windows)]
impl Tracker {
    pub fn new() -> Tracker {
        // 此对象固定由调用线程使用；成功初始化时才在 Drop 中配对 CoUninitialize。
        let com_initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };
        let uia = unsafe {
            CoCreateInstance::<_, IUIAutomation2>(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()
        };

        if let Some(uia) = &uia {
            // UIA 会跨进程 RPC；限制超时，避免失去响应的前台程序拖住输入循环。
            unsafe {
                let _ = uia.SetConnectionTimeout(200);
                let _ = uia.SetTransactionTimeout(200);
            }
        }

        Tracker { uia, com_initialized }
    }

    /// 查一次当前焦点。前台没有窗口时返回 None。要快：通常 < 5ms，最坏也不能卡超过 ~250ms。
    pub fn poll(&mut self) -> Option<Focus> {
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground.0.is_null() {
                return None;
            }

            let mut process_id = 0;
            let thread_id = GetWindowThreadProcessId(foreground, Some(&mut process_id));
            let mut gui = GUITHREADINFO {
                cbSize: size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            let has_gui = GetGUIThreadInfo(thread_id, &mut gui).is_ok();

            // 传统 Win32 编辑框和记事本通常走这里；rcCaret 是客户区坐标。
            // 本进程是 per-monitor DPI aware，换到屏幕后保持物理像素。
            if has_gui && !gui.hwndCaret.0.is_null() {
                let mut point = POINT { x: gui.rcCaret.left, y: gui.rcCaret.top };
                if ClientToScreen(gui.hwndCaret, &mut point).as_bool() {
                    return Some(Focus {
                        editable: true,
                        caret: Some(Rect {
                            x: point.x,
                            y: point.y,
                            w: gui.rcCaret.right.saturating_sub(gui.rcCaret.left),
                            h: gui.rcCaret.bottom.saturating_sub(gui.rcCaret.top),
                        }),
                        element: None,
                        process_id,
                        source: "gui-caret",
                    });
                }
            }

            // Chromium、Firefox、Electron 常通过 MSAA 暴露 OBJID_CARET；坐标已是屏幕坐标。
            let focus_window = if has_gui && !gui.hwndFocus.0.is_null() {
                gui.hwndFocus
            } else {
                foreground
            };
            if let Some(caret) = msaa_caret(focus_window) {
                return Some(Focus {
                    editable: true,
                    caret: Some(caret),
                    element: None,
                    process_id,
                    source: "msaa",
                });
            }

            // UIA 是最后的跨框架回退，也能在只知道可编辑性时提供控件矩形。
            // UIA 的屏幕矩形在本进程 DPI 上下文中也是物理像素。
            if let Some(uia) = &self.uia {
                if let Ok(element) = uia.GetFocusedElement() {
                    let element_rect = element.CurrentBoundingRectangle().ok().map(rect_from_win32);
                    let focusable = element
                        .CurrentIsKeyboardFocusable()
                        .map(|value| value.as_bool())
                        .unwrap_or(false);
                    let control_type = element.CurrentControlType().ok();
                    let value_writable = element
                        .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                        .and_then(|pattern| pattern.CurrentIsReadOnly())
                        .map(|readonly| !readonly.as_bool())
                        .unwrap_or(false);
                    let has_text = element
                        .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                        .is_ok();
                    let editable = focusable
                        && (control_type == Some(UIA_EditControlTypeId)
                            || control_type == Some(UIA_DocumentControlTypeId)
                            || value_writable
                            || has_text);

                    let caret = if editable {
                        element
                            .GetCurrentPatternAs::<IUIAutomationTextPattern2>(UIA_TextPattern2Id)
                            .ok()
                            .and_then(|pattern| caret_from_uia(&pattern))
                    } else {
                        None
                    };

                    return Some(Focus {
                        editable,
                        caret,
                        element: element_rect,
                        process_id,
                        source: "uia",
                    });
                }
            }

            Some(Focus {
                editable: false,
                caret: None,
                element: None,
                process_id,
                source: "none",
            })
        }
    }
}

#[cfg(windows)]
impl Drop for Tracker {
    fn drop(&mut self) {
        if self.com_initialized {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(not(windows))]
impl Tracker {
    pub fn new() -> Tracker {
        Tracker
    }

    pub fn poll(&mut self) -> Option<Focus> {
        None
    }
}

#[cfg(windows)]
unsafe fn msaa_caret(hwnd: HWND) -> Option<Rect> {
    let mut object: *mut c_void = ptr::null_mut();
    if AccessibleObjectFromWindow(hwnd, OBJID_CARET.0 as u32, &IAccessible::IID, &mut object).is_err()
        || object.is_null()
    {
        return None;
    }

    let accessible = IAccessible::from_raw(object);
    let mut x = 0;
    let mut y = 0;
    let mut w = 0;
    let mut h = 0;
    // CHILDID_SELF 必须是 VT_I4 的 0，不能用 VT_EMPTY。
    if accessible
        .accLocation(&mut x, &mut y, &mut w, &mut h, &VARIANT::from(0_i32))
        .is_ok()
        && (w != 0 || h != 0)
    {
        Some(Rect { x, y, w, h })
    } else {
        None
    }
}

#[cfg(windows)]
unsafe fn caret_from_uia(pattern: &IUIAutomationTextPattern2) -> Option<Rect> {
    let mut active = Default::default();
    let range = pattern.GetCaretRange(&mut active).ok()?;
    if let Some(rect) = uia_range_rect(&range) {
        return Some(rect);
    }

    // 折叠范围在行尾可能没有矩形；扩到当前字符后再取一次。
    if range.ExpandToEnclosingUnit(TextUnit_Character).is_ok() {
        uia_range_rect(&range)
    } else {
        None
    }
}

#[cfg(windows)]
unsafe fn uia_range_rect(range: &IUIAutomationTextRange) -> Option<Rect> {
    let array = range.GetBoundingRectangles().ok()?;
    if array.is_null() {
        return None;
    }
    let rect = safe_array_first_rect(array);
    let _ = SafeArrayDestroy(array);
    rect
}

#[cfg(windows)]
unsafe fn safe_array_first_rect(array: *mut windows::Win32::System::Com::SAFEARRAY) -> Option<Rect> {
    if array.is_null()
        || (*array).cDims != 1
        || (*array).rgsabound[0].cElements < 4
        || (*array).cbElements != size_of::<f64>() as u32
    {
        return None;
    }

    let mut data: *mut c_void = ptr::null_mut();
    if SafeArrayAccessData(array, &mut data).is_err() {
        return None;
    }
    if data.is_null() {
        let _ = SafeArrayUnaccessData(array);
        return None;
    }
    let values = data.cast::<f64>();
    let values = [*values, *values.add(1), *values.add(2), *values.add(3)];
    let _ = SafeArrayUnaccessData(array);
    rect_from_f64(values[0], values[1], values[2], values[3])
}

#[cfg(windows)]
fn rect_from_win32(rect: RECT) -> Rect {
    Rect {
        x: rect.left,
        y: rect.top,
        w: rect.right.saturating_sub(rect.left),
        h: rect.bottom.saturating_sub(rect.top),
    }
}

#[cfg(windows)]
fn rect_from_f64(x: f64, y: f64, w: f64, h: f64) -> Option<Rect> {
    Some(Rect {
        x: round_i32(x)?,
        y: round_i32(y)?,
        w: round_i32(w)?,
        h: round_i32(h)?,
    })
}

#[cfg(windows)]
fn round_i32(value: f64) -> Option<i32> {
    let rounded = value.round();
    if rounded.is_finite() && rounded >= i32::MIN as f64 && rounded <= i32::MAX as f64 {
        Some(rounded as i32)
    } else {
        None
    }
}
