//! 测试用的「宿主程序」：一个带富文本框的窗口，把输入法切到 JoyIME，等若干秒后把文本框里的字写进文件，然后退出。
//! 注意：Windows 默认所有窗口共用一个输入法（没开「允许为每个应用窗口设置不同的输入法」时），
//! 切到 JoyIME 会影响别的窗口，所以退出前切回原来的输入法。
//!
//! 先注册 DLL（tools/dev-install-ime.ps1），然后：
//!   cargo run -p joyime-tsf --example tsf_host --release -- <输出文件> <秒数>
//! 输出文件内容：TEXT=文本框里的字、ACTIVATED=有没有成功切到 JoyIME

use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{w, Result, GUID};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Input::KeyboardAndMouse::{SetFocus, HKL};
use windows::Win32::UI::TextServices::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CLSID_JOYIME: GUID = GUID::from_u128(0xb547c390_17e8_4d9d_873a_176aeac628e2);
const GUID_PROFILE: GUID = GUID::from_u128(0xd5230f03_7e6f_438d_9263_55a14b5d86c5);

static EDIT: AtomicIsize = AtomicIsize::new(0);
static mut OUT: String = String::new();
static mut ACTIVATED: bool = false;

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            let edit = HWND(EDIT.load(Ordering::Relaxed) as *mut _);
            let mut buf = [0u16; 4096];
            let n = GetWindowTextW(edit, &mut buf) as usize;
            let text = String::from_utf16_lossy(&buf[..n]);
            #[allow(static_mut_refs)]
            let _ = std::fs::write(&OUT, format!("TEXT={text}\nACTIVATED={}\n", ACTIVATED));
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_SIZE => {
            let edit = HWND(EDIT.load(Ordering::Relaxed) as *mut _);
            let (w, h) = ((lp.0 & 0xffff) as i32, ((lp.0 >> 16) & 0xffff) as i32);
            let _ = MoveWindow(edit, 0, 0, w, h, true);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| "tsf-host-out.txt".into());
    let secs: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(15);
    unsafe {
        OUT = out;
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        LoadLibraryW(w!("Msftedit.dll"))?;
        let hinst = GetModuleHandleW(None)?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: w!("JoyIMETsfHost"),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let main = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("JoyIMETsfHost"),
            w!("JoyIME 系统输入法测试"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            200,
            200,
            900,
            260,
            None,
            None,
            Some(hinst.into()),
            None,
        )?;
        let edit = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("RICHEDIT50W"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(0x0004), // ES_MULTILINE
            0,
            0,
            880,
            220,
            Some(main),
            None,
            Some(hinst.into()),
            None,
        )?;
        EDIT.store(edit.0 as isize, Ordering::Relaxed);
        // 让富文本框直接走 TSF（SES_USECTF）
        SendMessageW(edit, WM_USER + 204, Some(WPARAM(0x0001_0000)), Some(LPARAM(0x0001_0000)));
        let _ = SetForegroundWindow(main);
        let _ = SetFocus(Some(edit));

        // 切到 JoyIME，先记住原来的输入法
        let mgr: ITfInputProcessorProfileMgr =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
        let mut prev = TF_INPUTPROCESSORPROFILE::default();
        let had_prev = mgr.GetActiveProfile(&GUID_TFCAT_TIP_KEYBOARD, &mut prev).is_ok();
        ACTIVATED = mgr
            .ActivateProfile(
                TF_PROFILETYPE_INPUTPROCESSOR,
                0x0804,
                &CLSID_JOYIME,
                &GUID_PROFILE,
                HKL::default(),
                TF_IPPMF_FORPROCESS | TF_IPPMF_DONTCARECURRENTINPUTLANGUAGE,
            )
            .is_ok();
        let _ = SetFocus(Some(edit));

        SetTimer(Some(main), 1, secs * 1000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // 切回原来的输入法
        if had_prev && prev.clsid != CLSID_JOYIME {
            let _ = mgr.ActivateProfile(
                prev.dwProfileType,
                prev.langid,
                &prev.clsid,
                &prev.guidProfile,
                prev.hkl,
                TF_IPPMF_DONTCARECURRENTINPUTLANGUAGE,
            );
        }
    }
    Ok(())
}
