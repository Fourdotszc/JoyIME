//! 注册 / 反注册：COM 服务器、TSF 语言配置和类别，以及告诉 DLL 后台程序在哪。
//! `regsvr32` 以管理员身份调用 DllRegisterServer，所以写的是 HKLM。
//! 32 位 DLL 由 32 位 regsvr32 注册，写进注册表的 32 位视图，32 位程序就会加载 32 位 DLL。

use crate::{CLSID_JOYIME, GUID_DISPLAY_ATTRIBUTE, GUID_PROFILE, LANGID_ZH_CN};
use windows::core::{Result, GUID, HSTRING, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, MAX_PATH};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Input::KeyboardAndMouse::HKL;
use windows::Win32::UI::TextServices::*;

const DESCRIPTION: &str = "JoyIME 摇杆输入法";

fn clsid_key() -> String {
    format!(r"SOFTWARE\Classes\CLSID\{{{:?}}}", CLSID_JOYIME)
}

/// TSF 会去问这些类别：键盘输入法、能在商店应用里用、在任务栏显示、提供组字串显示属性
fn categories() -> [GUID; 4] {
    [
        GUID_TFCAT_TIP_KEYBOARD,
        GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT,
        GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT,
        GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
    ]
}

pub fn dll_path() -> String {
    let mut buf = [0u16; MAX_PATH as usize * 2];
    let n = unsafe { GetModuleFileNameW(Some(crate::dll_instance().into()), &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n])
}

fn dll_dir() -> std::path::PathBuf {
    std::path::Path::new(&dll_path()).parent().map(|p| p.to_path_buf()).unwrap_or_default()
}

/// DLL 旁边的文件：先找 DLL 同目录，再找上一级。
/// 安装后的目录：JoyIME/joyime_tsf.dll（64 位）、JoyIME/x86/joyime_tsf.dll（32 位），程序和图标在 JoyIME/ 下
pub fn find_near(name: &str) -> Option<std::path::PathBuf> {
    let dir = dll_dir();
    let found = [Some(dir.as_path()), dir.parent()].into_iter().flatten().map(|d| d.join(name)).find(|p| p.exists());
    found
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn set_string(root: HKEY, subkey: &str, name: Option<&str>, value: &str) -> Result<()> {
    let mut key = HKEY::default();
    unsafe {
        RegCreateKeyExW(
            root,
            &HSTRING::from(subkey),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
        .ok()?;
        let mut data: Vec<u8> = wide(value).iter().flat_map(|c| c.to_le_bytes()).collect();
        data.extend_from_slice(&[0, 0]);
        let name = name.map(HSTRING::from);
        let r = RegSetValueExW(
            key,
            name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr())),
            None,
            REG_SZ,
            Some(&data),
        )
        .ok();
        let _ = RegCloseKey(key);
        r
    }
}

/// 读一个字符串值。current_user = true 读 HKCU，否则 HKLM
pub fn read_string(current_user: bool, subkey: &str, name: &str) -> Option<String> {
    let root = if current_user { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE };
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(subkey),
            &HSTRING::from(name),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        )
    };
    if r.is_err() {
        return None;
    }
    let n = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..n.min(buf.len())]))
}

fn delete_tree(root: HKEY, subkey: &str) -> Result<()> {
    let r = unsafe { RegDeleteTreeW(root, &HSTRING::from(subkey)) };
    if r.is_ok() || r == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        r.ok()
    }
}

pub fn register() -> Result<()> {
    let dll = dll_path();
    crate::log::log(&format!("注册 {dll}"));

    // 1. COM 进程内服务器
    let key = clsid_key();
    set_string(HKEY_LOCAL_MACHINE, &key, None, "JoyIME")?;
    set_string(HKEY_LOCAL_MACHINE, &format!(r"{key}\InprocServer32"), None, &dll)?;
    set_string(HKEY_LOCAL_MACHINE, &format!(r"{key}\InprocServer32"), Some("ThreadingModel"), "Apartment")?;

    // 2. TSF：语言配置和类别
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let profiles: ITfInputProcessorProfileMgr =
            CoCreateInstance(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)?;
        let icon = find_near("joyime.ico").map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        profiles.RegisterProfile(
            &CLSID_JOYIME,
            LANGID_ZH_CN,
            &GUID_PROFILE,
            &wide(DESCRIPTION),
            &wide(&icon),
            0,
            HKL::default(),
            0,
            true,
            0,
        )?;
        let cats: ITfCategoryMgr = CoCreateInstance(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)?;
        for cat in categories() {
            cats.RegisterCategory(&CLSID_JOYIME, &cat, &CLSID_JOYIME)?;
        }
        // 显示属性也要登记，TSF 才会来问我们组字串怎么画
        cats.RegisterCategory(&CLSID_JOYIME, &GUID_TFCAT_DISPLAYATTRIBUTEPROPERTY, &GUID_DISPLAY_ATTRIBUTE)?;
    }

    // 3. 后台程序在哪：DLL 旁边的 joyime-app.exe（开发时没有，用 HKCU 里开发脚本写的路径）
    if let Some(service) = find_near("joyime-app.exe") {
        set_string(HKEY_LOCAL_MACHINE, r"SOFTWARE\JoyIME", Some("ServicePath"), &service.to_string_lossy())?;
    }
    Ok(())
}

pub fn unregister() -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if let Ok(profiles) =
            CoCreateInstance::<_, ITfInputProcessorProfileMgr>(&CLSID_TF_InputProcessorProfiles, None, CLSCTX_INPROC_SERVER)
        {
            let _ = profiles.UnregisterProfile(&CLSID_JOYIME, LANGID_ZH_CN, &GUID_PROFILE, 0);
        }
        if let Ok(cats) = CoCreateInstance::<_, ITfCategoryMgr>(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER) {
            for cat in categories() {
                let _ = cats.UnregisterCategory(&CLSID_JOYIME, &cat, &CLSID_JOYIME);
            }
            let _ = cats.UnregisterCategory(&CLSID_JOYIME, &GUID_TFCAT_DISPLAYATTRIBUTEPROPERTY, &GUID_DISPLAY_ATTRIBUTE);
        }
    }
    delete_tree(HKEY_LOCAL_MACHINE, &clsid_key())?;
    delete_tree(HKEY_LOCAL_MACHINE, r"SOFTWARE\JoyIME")?;
    Ok(())
}
