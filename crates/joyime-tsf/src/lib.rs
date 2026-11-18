//! JoyIME 的 Windows 系统输入法（TSF 文本服务 DLL）。
//!
//! 用户在输入法列表里选了 JoyIME，Windows 就把这个 DLL 加载进正在打字的程序。它本身不读摇杆，
//! 只做三件事，其余都交给后台程序 joyime-app.exe（协议见 docs/ime-protocol.md）：
//! - 告诉后台「焦点在我这、JoyIME 是当前输入法」，以及光标在屏幕上的位置；
//! - 把后台发来的读音作为组字串显示在文字里（带虚线下划线）；
//! - 把后台发来的字上屏。
//!
//! 这个 DLL 会进到用户的每个程序里，崩了就是把别人的程序弄崩：所有对外入口都用 `guarded` 包住 panic，
//! 界面线程上绝不做会阻塞的事（管道、启动进程都在后台线程，见 pipe.rs）。
#![cfg(windows)]

mod log;
mod pipe;
mod register;
mod service;

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use windows::core::{implement, Error, Interface, Result, BOOL, GUID, HRESULT};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, E_FAIL, E_POINTER, HINSTANCE, S_FALSE, S_OK, TRUE};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_core::IUnknown;

/// 文本服务的 CLSID。发布后不能改（用户的输入法设置靠它记住 JoyIME）
pub const CLSID_JOYIME: GUID = GUID::from_u128(0xb547c390_17e8_4d9d_873a_176aeac628e2);
/// 简体中文下的语言配置
pub const GUID_PROFILE: GUID = GUID::from_u128(0xd5230f03_7e6f_438d_9263_55a14b5d86c5);
/// 组字串的显示属性（虚线下划线）
pub const GUID_DISPLAY_ATTRIBUTE: GUID = GUID::from_u128(0x375104d4_0a67_4302_8d61_493ed35899fd);
/// 简体中文
pub const LANGID_ZH_CN: u16 = 0x0804;

/// DLL 自己的模块句柄（DllMain 里记下），注册时要用它找 DLL 路径、建窗口类
static DLL_INSTANCE: AtomicIsize = AtomicIsize::new(0);
/// 还活着的 COM 对象数，DllCanUnloadNow 用
pub static LIVE_OBJECTS: AtomicUsize = AtomicUsize::new(0);

/// 放进每个交给 Windows 的 COM 对象里：对象活着，DllCanUnloadNow 就说不能卸载。
/// 少数一个，Windows 就可能在 TSF 还拿着我们的对象时卸掉 DLL，宿主程序直接崩。
pub struct Live;

impl Live {
    pub fn new() -> Live {
        LIVE_OBJECTS.fetch_add(1, Ordering::AcqRel);
        Live
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        LIVE_OBJECTS.fetch_sub(1, Ordering::AcqRel);
    }
}

pub fn dll_instance() -> HINSTANCE {
    HINSTANCE(DLL_INSTANCE.load(Ordering::Relaxed) as *mut c_void)
}

/// 包住 panic：出错返回 E_FAIL，绝不让 panic 穿过 COM / FFI 边界
pub fn guarded<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        log::log("panic 被拦下");
        Err(Error::from(E_FAIL))
    })
}

fn guarded_hr(f: impl FnOnce() -> HRESULT) -> HRESULT {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(E_FAIL)
}

#[unsafe(no_mangle)]
extern "system" fn DllMain(hinst: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        DLL_INSTANCE.store(hinst.0 as isize, Ordering::Relaxed);
    }
    TRUE
}

/// 类工厂：TSF 通过它创建文本服务对象
#[implement(IClassFactory)]
struct Factory {
    _live: Live,
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: windows::core::Ref<'_, IUnknown>,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> Result<()> {
        guarded(|| unsafe {
            if out.is_null() || iid.is_null() {
                return Err(E_POINTER.into());
            }
            *out = std::ptr::null_mut();
            if outer.is_some() {
                return Err(windows::Win32::Foundation::CLASS_E_NOAGGREGATION.into());
            }
            let obj: IUnknown = service::TextService::new().into();
            obj.query(iid, out).ok()
        })
    }

    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

#[unsafe(no_mangle)]
unsafe extern "system" fn DllGetClassObject(clsid: *const GUID, iid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    guarded_hr(|| unsafe {
        if clsid.is_null() || iid.is_null() || out.is_null() {
            return E_POINTER;
        }
        *out = std::ptr::null_mut();
        if *clsid != CLSID_JOYIME {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = Factory { _live: Live::new() }.into();
        factory.query(iid, out)
    })
}

#[unsafe(no_mangle)]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    if LIVE_OBJECTS.load(Ordering::Acquire) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

#[unsafe(no_mangle)]
extern "system" fn DllRegisterServer() -> HRESULT {
    guarded_hr(|| match register::register() {
        Ok(()) => S_OK,
        Err(e) => {
            log::log(&format!("注册失败：{e}"));
            // 注册到一半失败，把已经写进去的清掉，不留半套
            let _ = register::unregister();
            e.code()
        }
    })
}

#[unsafe(no_mangle)]
extern "system" fn DllUnregisterServer() -> HRESULT {
    guarded_hr(|| match register::unregister() {
        Ok(()) => S_OK,
        Err(e) => e.code(),
    })
}
