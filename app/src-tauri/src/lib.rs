//! JoyIME 桌面程序：托盘常驻，读摇杆打字；候选悬浮窗 + 设置窗口（按键映射、布局预设）。

mod caret;
mod engine;
mod focus;
#[cfg(windows)]
mod ime;
mod overlay;
mod profile;
mod script;
mod settings;

use engine::{Cmd, CmdTx};
use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder, WindowEvent};

fn send(tx: &State<CmdTx>, cmd: Cmd) {
    if let Ok(tx) = tx.lock() {
        let _ = tx.send(cmd);
    }
}

#[tauri::command]
fn get_meta() -> engine::Meta {
    engine::meta()
}

#[tauri::command]
fn apply_preset(tx: State<CmdTx>, preset: String) {
    send(&tx, Cmd::ApplyPreset(preset));
}

#[tauri::command]
fn set_binding(tx: State<CmdTx>, code: u32, action: Option<String>) {
    send(&tx, Cmd::SetBinding(code, action));
}

#[tauri::command]
fn set_button_name(tx: State<CmdTx>, code: u32, name: String) {
    send(&tx, Cmd::SetName(code, name));
}

#[tauri::command]
fn set_enabled(tx: State<CmdTx>, on: bool) {
    send(&tx, Cmd::SetEnabled(on));
}

#[tauri::command]
fn refresh(tx: State<CmdTx>) {
    send(&tx, Cmd::Refresh);
}

/// 当前设置和存放位置
type SettingsState = std::sync::Mutex<(std::path::PathBuf, settings::Settings)>;

#[tauri::command]
fn get_settings(state: State<SettingsState>) -> settings::Settings {
    state.lock().map(|s| s.1.clone()).unwrap_or_default()
}

#[tauri::command]
fn set_settings(state: State<SettingsState>, tx: State<CmdTx>, settings: settings::Settings) {
    if let Ok(mut s) = state.lock() {
        settings::save(&s.0, &settings);
        s.1 = settings.clone();
    }
    send(&tx, Cmd::SetSettings(settings));
}

/// 用户词库记了多少条
#[tauri::command]
fn userdict_len() -> usize {
    engine::USER_DICT_LEN.load(Ordering::Relaxed)
}

#[tauri::command]
fn clear_userdict(tx: State<CmdTx>) {
    send(&tx, Cmd::ClearUserDict);
}

/// 界面里的诊断信息写进开发日志（JOYIME_LOG）
#[tauri::command]
fn log_ui(msg: String) {
    engine::log(&format!("界面：{msg}"));
}

fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_meta,
            apply_preset,
            set_binding,
            set_button_name,
            set_enabled,
            refresh,
            get_settings,
            set_settings,
            userdict_len,
            clear_userdict,
            log_ui
        ])
        .setup(|app| {
            // --hidden：只进托盘，不弹设置窗口（不然它会抢走焦点）。
            // --service：系统输入法 DLL 在需要时把我们拉起来，也不弹窗口
            let hidden = std::env::args().any(|a| a == "--hidden" || a == "--service");

            // 系统输入法的管道服务端。管道名被占了说明已经有一个 JoyIME 在跑：
            // 带 --replace 就让它退出、自己接手（更新、测试用）；否则让它把设置窗口显示出来，自己退出
            #[cfg(windows)]
            let ime_server = {
                let replace = std::env::args().any(|a| a == "--replace");
                let start = || {
                    let handle = app.handle().clone();
                    ime::spawn(move || {
                        let h = handle.clone();
                        let _ = handle.run_on_main_thread(move || open_settings(&h));
                    })
                };
                let mut server = start();
                // 让位的那个一退，用户程序里的输入法 DLL 可能马上又拉起一个新的来抢管道；
                // 所以一直叫占着管道的那个退出，直到抢到为止（DLL 拉起一次后 10 秒内不会再拉）
                if replace {
                    for _ in 0..40 {
                        if server.is_ok() {
                            break;
                        }
                        ime::ask_running_instance_to_quit();
                        std::thread::sleep(std::time::Duration::from_millis(150));
                        server = start();
                    }
                }
                match server {
                    Ok(s) => Some(s),
                    Err(()) => {
                        if !hidden {
                            ime::ask_running_instance_to_show();
                        }
                        std::process::exit(0);
                    }
                }
            };
            #[cfg(not(windows))]
            let ime_server: Option<engine::ImeHandle> = None;
            let settings = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
                .title("JoyIME 摇杆输入法")
                .visible(!hidden)
                .focused(!hidden)
                .inner_size(1180.0, 760.0)
                .min_inner_size(960.0, 640.0)
                .center()
                .build()?;
            // 关掉设置窗口只是藏起来，程序继续在托盘里跑
            let w = settings.clone();
            settings.on_window_event(move |e| match e {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = w.hide();
                    engine::SETTINGS_FOCUSED.store(false, Ordering::Relaxed);
                }
                WindowEvent::Focused(f) => engine::SETTINGS_FOCUSED.store(*f, Ordering::Relaxed),
                _ => {}
            });

            // 候选悬浮窗：不抢焦点，不然字会打进它自己（见 overlay.rs）
            let overlay = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
                .title("JoyIME 候选")
                .inner_size(overlay::WIDTH, overlay::HEIGHT)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .resizable(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .focusable(false)
                .focused(false)
                .visible(false)
                .build()?;
            overlay::init(&overlay);
            // 窗口比面板大，透明的地方不能挡住下面程序的鼠标点击
            let _ = overlay.set_ignore_cursor_events(true);

            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "open", "打开设置", true, None::<&str>)?,
                    &MenuItem::with_id(app, "pause", "暂停输入", true, None::<&str>)?,
                    &MenuItem::with_id(app, "resume", "恢复输入", true, None::<&str>)?,
                    &MenuItem::with_id(app, "quit", "退出 JoyIME", true, None::<&str>)?,
                ],
            )?;
            TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().cloned().expect("缺少图标"))
                .tooltip("JoyIME 摇杆输入法")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "open" => open_settings(app),
                    "pause" => send(&app.state::<CmdTx>(), Cmd::SetEnabled(false)),
                    "resume" => send(&app.state::<CmdTx>(), Cmd::SetEnabled(true)),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        open_settings(tray.app_handle());
                    }
                })
                .build(app)?;

            let (tx, rx) = std::sync::mpsc::channel();
            app.manage::<CmdTx>(std::sync::Mutex::new(tx));
            // 开发用：JOYIME_CONFIG_DIR 指一个临时目录，测试时不碰用户自己的设置和词库
            let config = match std::env::var_os("JOYIME_CONFIG_DIR") {
                Some(d) => std::path::PathBuf::from(d),
                None => app.path().app_config_dir()?,
            };
            let settings_path = settings::path(&config);
            let current = settings::load(&settings_path);
            app.manage::<SettingsState>(std::sync::Mutex::new((settings_path, current.clone())));
            let focus = focus::spawn();
            engine::spawn(
                app.handle().clone(),
                rx,
                config.join("profiles.json"),
                config.join("userdict.txt"),
                current,
                focus,
                ime_server,
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("JoyIME 启动失败");
}
