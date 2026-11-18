//! 输入线程：读摇杆 → 状态机 → 把字打进当前程序，同时把状态推给悬浮窗和设置窗口。

use crate::profile::{self, Profile, Store, ACTIONS, POSITIONS, PRESETS};
use crate::caret::Rect;
use crate::focus;
use crate::overlay::{self, Mode};
use crate::settings::Settings as AppSettings;
use crate::script::Script;
use std::sync::atomic::{AtomicBool, Ordering};
use enigo::{Direction, Enigo, Keyboard, Settings};
use gilrs::{Axis, Button, EventType, Gamepad, GamepadId, Gilrs, MappingSource};
use joyime_core::{Caps, Dir, Effect, Held, Key, Layer, Machine, UserDict, View};
use serde::Serialize;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// 设置界面发给输入线程的请求
pub enum Cmd {
    ApplyPreset(String),
    /// 原始按键编号，功能 id（None = 不分配）
    SetBinding(u32, Option<String>),
    SetName(u32, String),
    SetEnabled(bool),
    /// 「输入」页的设置改了
    SetSettings(AppSettings),
    /// 清空用户词库
    ClearUserDict,
    /// 把当前状态重新推一遍（设置窗口刚打开时）
    Refresh,
}

/// 推给悬浮窗和设置窗口的输入状态
#[derive(Serialize, Clone, PartialEq)]
struct ViewDto {
    enabled: bool,
    layer: &'static str,
    layers: Vec<&'static str>,
    dir: Option<usize>,
    comp: String,
    comp_display: String,
    cands: Vec<String>,
    page: usize,
    pages: usize,
    caps: &'static str,
    /// 九宫格九个格子（左上到右下）里的字，界面直接画
    grid: Vec<String>,
    /// 中文九宫格的拼音栏：定下的音节、可选的音节（选拼音时是当前这页）、是不是正在选拼音
    pinned: Vec<String>,
    py_opts: Vec<String>,
    py_focus: bool,
    py_page: usize,
    py_pages: usize,
}

/// 推给设置窗口的手柄状态
#[derive(Serialize, Clone, PartialEq)]
struct PadDto {
    connected: bool,
    name: String,
    key: String,
    /// "sdl" / "driver" / "none"：有没有认出这是什么手柄
    mapping: &'static str,
    buttons: Vec<ButtonDto>,
    dir: Option<usize>,
    dir_source: &'static str,
    profile: Profile,
    slots: usize,
}

#[derive(Serialize, Clone, PartialEq)]
struct ButtonDto {
    code: u32,
    /// 标准名字（South / LeftTrigger2 …），没认出来是 None
    std_name: Option<String>,
    pressed: bool,
    action: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct Meta {
    actions: Vec<(&'static str, &'static str, &'static str)>,
    presets: Vec<(&'static str, &'static str)>,
    /// (位置 id, 显示名, 通常对应的标准按键名)
    positions: Vec<(&'static str, &'static str, String)>,
    /// (输入层 id, 短名, 说明)，按切换顺序
    layers: Vec<(&'static str, &'static str, &'static str)>,
}

pub fn meta() -> Meta {
    Meta {
        actions: ACTIONS.to_vec(),
        presets: PRESETS.iter().map(|p| (p.0, p.1)).collect(),
        positions: POSITIONS.iter().map(|p| (p.0, p.1, format!("{:?}", p.2))).collect(),
        layers: Layer::ALL
            .iter()
            .map(|&l| {
                let desc = match l {
                    Layer::ChineseT9 => "中文九宫格：只推方向，自动猜拼音和词",
                    Layer::EnglishT9 => "英文联想：只推方向，自动猜单词，选词后自动补空格",
                    Layer::Symbols => "数字：推一下就是一个数字，方向 + 按键打符号",
                    Layer::Pinyin => "中文全拼：方向 + 按键逐个打拼音字母",
                    Layer::Lower => "英文小写：方向 + 按键逐个打字母",
                    Layer::Upper => "英文大写：方向 + 按键逐个打字母",
                };
                (l.id(), l.label(), desc)
            })
            .collect(),
    }
}

/// 这些标准按键位置如果手柄有，就列在设置界面里（不用先按一下）
const STD_BUTTONS: [Button; 15] = [
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::LeftTrigger,
    Button::RightTrigger,
    Button::LeftTrigger2,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
    Button::Mode,
    Button::LeftThumb,
    Button::RightThumb,
    Button::C,
    Button::Z,
];

/// 悬浮窗：换层、开关之后再显示这么久
const TOAST_MS: u64 = 1200;

/// 用户词库现在有多少条，给设置界面显示（输入线程更新）
pub static USER_DICT_LEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

pub fn spawn(
    app: AppHandle,
    rx: Receiver<Cmd>,
    store_path: std::path::PathBuf,
    userdict_path: std::path::PathBuf,
    settings: AppSettings,
    focus: focus::Shared,
    ime: Option<ImeHandle>,
) {
    std::thread::spawn(joyime_core::warm_up);
    std::thread::spawn(move || {
        let mut gilrs = match Gilrs::new() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("读不了手柄：{e}");
                return;
            }
        };
        let mut enigo = Enigo::new(&Settings::default()).ok();
        let mut store = Store::load(store_path);
        let mut machine = Machine::new(settings.layers(), 4);
        machine.set_cn_fullwidth(settings.cn_fullwidth);
        machine.set_learning(settings.learn);
        // 用户词库：启动时读进来，变了过一会儿存回去
        let text = std::fs::read_to_string(&userdict_path).unwrap_or_default();
        machine.set_user_dict(UserDict::parse(&text));
        USER_DICT_LEN.store(machine.user_dict().len(), Ordering::Relaxed);
        let mut userdict_dirty_at: Option<u64> = None;
        // 系统输入法：DLL 装了没有（自动模式靠它决定用不用系统输入法）、当前客户端、发过去的组字串
        let ime_registered = ime_registered(&ime);
        let mut ime_client: Option<u64> = None;
        let mut comp_sent: Option<String> = None;
        let mut active: Option<GamepadId> = None;
        // 状态机按哪个手柄的设置建的（手柄键, 候选键数）；设置改了也要重建
        let mut machine_for: Option<(String, usize)> = None;
        let start = Instant::now();
        let mut last_view: Option<ViewDto> = None;
        let mut last_pad: Option<PadDto> = None;
        let mut last_pad_emit = 0u64;
        let mut toast_until = 0u64;
        let mut settings = settings;
        // 方向提示显示到什么时候；进入输入框、动摇杆都会往后推
        let mut hint_until = 0u64;
        let mut seen_entered = 0u64;
        // 悬浮窗现在的样子和位置，变了才去动窗口
        let mut shown: Option<(Mode, Option<Rect>)> = None;
        let mut last_frame = 0u64;
        let mut script = std::env::var("JOYIME_SCRIPT").ok().and_then(|p| match Script::load(&p) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("{e}");
                None
            }
        });

        loop {
            // 1. 设置界面的请求
            let mut force_refresh = false;
            while let Ok(cmd) = rx.try_recv() {
                let pad = active.and_then(|id| gilrs.connected_gamepad(id));
                match cmd {
                    Cmd::ApplyPreset(p) => {
                        if let Some(pad) = pad {
                            let names = std::mem::take(&mut store.for_pad(&pad).names);
                            *store.for_pad(&pad) = Profile::from_preset(&p, &pad, names);
                            store.save();
                            machine_for = None;
                        }
                    }
                    Cmd::SetBinding(code, action) => {
                        if let Some(pad) = pad {
                            let prof = store.for_pad(&pad);
                            let key = code.to_string();
                            // 一个功能只给一个键：先把别的键上的同一个功能拿掉
                            if let Some(a) = &action {
                                prof.bindings.retain(|k, v| v != a || *k == key);
                            }
                            match action {
                                Some(a) => prof.bindings.insert(key, a),
                                None => prof.bindings.remove(&key),
                            };
                            prof.preset = "custom".into();
                            store.save();
                            machine_for = None;
                        }
                    }
                    Cmd::SetName(code, name) => {
                        if let Some(pad) = pad {
                            // 一个位置只对应一个键
                            let names = &mut store.for_pad(&pad).names;
                            names.retain(|_, v| *v != name);
                            if name.is_empty() {
                                names.remove(&code.to_string());
                            } else {
                                names.insert(code.to_string(), name);
                            }
                            store.save();
                        }
                    }
                    Cmd::SetEnabled(on) => {
                        machine.set_enabled(on);
                        toast_until = elapsed(start) + TOAST_MS;
                    }
                    Cmd::SetSettings(s) => {
                        let slots = machine_for.as_ref().map_or(4, |k| k.1);
                        machine = rebuild(&mut machine, slots, &s);
                        settings = s;
                    }
                    Cmd::ClearUserDict => {
                        machine.user_dict_mut().clear();
                        userdict_dirty_at = Some(0);
                    }
                    Cmd::Refresh => force_refresh = true,
                }
            }

            // 2. 手柄事件：谁动了就用谁
            let mut ev = gilrs.next_event_blocking(Some(Duration::from_millis(5)));
            while let Some(e) = ev {
                match e.event {
                    EventType::Disconnected if active == Some(e.id) => active = None,
                    EventType::ButtonPressed(..) | EventType::AxisChanged(..) => active = Some(e.id),
                    _ => {}
                }
                ev = gilrs.next_event();
            }
            if active.is_none() {
                active = gilrs.gamepads().next().map(|(id, _)| id);
            }
            let now = elapsed(start);
            if now - last_frame > 30 {
                log(&format!("{now}ms：这一帧隔了 {}ms", now - last_frame));
            }
            last_frame = now;
            let pad = active.and_then(|id| gilrs.connected_gamepad(id));

            // 3. 按这个手柄的设置读方向和按键，喂给状态机
            let scripted = script.as_mut().map(|s| s.frame(now));
            if let Some(None) = scripted {
                // 脚本放完了
                script = None;
                if std::env::var_os("JOYIME_EXIT").is_some() {
                    std::thread::sleep(Duration::from_millis(500));
                    app.exit(0);
                }
            }
            let (mut dir, dir_source, mut held) = match (&pad, scripted.flatten()) {
                (_, Some((d, h))) => (d, "脚本", h),
                (Some(p), None) => {
                    let prof = store.for_pad(p);
                    let key = (profile::pad_key(p), prof.slots());
                    if machine_for.as_ref() != Some(&key) {
                        machine = rebuild(&mut machine, key.1, &settings);
                        machine_for = Some(key);
                    }
                    let (d, src) = read_dir(p);
                    (d, src, read_held(p, prof))
                }
                (None, None) => (None, "", Held::default()),
            };
            // 系统输入法模式：只有 JoyIME 是焦点程序的当前输入法时摇杆才打字；
            // 换了程序（或被切走）就丢掉正在打的东西
            let fs = focus.lock().map(|f| f.clone()).unwrap_or_default();
            let use_ime = match settings.output.as_str() {
                "ime" => ime.is_some(),
                "inject" => false,
                _ => ime_registered,
            };
            let snap = ime_snapshot(&ime, fs.fg_pid);
            let target = if use_ime { snap.active } else { None };
            if target != ime_client {
                // 换了程序：旧程序里还挂着的读音清掉，正在打的丢掉
                if let (Some(old), Some(h)) = (ime_client, ime.as_ref()) {
                    if comp_sent.as_deref().is_some_and(|t| !t.is_empty()) {
                        ime_send(h, old, ImeOut::Comp(String::new()));
                    }
                }
                log(&format!("{now}ms：当前客户端 {}", target.map_or("无".into(), |id| ime_describe(&ime, id))));
                machine.reset_input();
                ime_client = target;
                comp_sent = None;
            }
            if use_ime && target.is_none() && !settings_in_front(&fs) {
                dir = None;
                held = Held::default();
            }
            let layer_before = machine.layer();
            let enabled_before = machine.view().enabled;
            let fx = machine.update(now, dir, held);
            if machine.layer() != layer_before || machine.view().enabled != enabled_before {
                toast_until = now + TOAST_MS;
            }
            if dir.is_some() || held != Held::default() {
                hint_until = now.saturating_add(hint_ms(&settings));
            }
            let to_ime = target.and_then(|id| ime.as_ref().map(|h| (h, id)));
            if !fx.is_empty() {
                apply(&app, &mut enigo, fx, settings_in_front(&fs), to_ime);
            }

            // 4. 推状态
            let view = view_dto(&machine.view(), machine.layers());
            if last_view.as_ref().map(|v| &v.comp) != Some(&view.comp) {
                log(&format!("{now}ms：拼写区 {:?}，第一个候选 {:?}", view.comp, view.cands.first()));
            }
            if last_view.as_ref() != Some(&view) || force_refresh {
                let _ = app.emit("view", &view);
                last_view = Some(view.clone());
            }
            // 系统输入法：拼写区显示在文字里（组字串），变了才发
            if let Some((h, id)) = to_ime {
                let text = if view.enabled && !view.comp.is_empty() { view.comp_display.clone() } else { String::new() };
                if comp_sent.as_deref() != Some(text.as_str()) {
                    ime_send(h, id, ImeOut::Comp(text.clone()));
                    comp_sent = Some(text);
                }
            }
            // 悬浮窗：打字时显示候选；换层 / 开关时提示一下；刚进输入框、或者在输入框里动了摇杆，显示方向提示
            let entered = if use_ime { snap.entered } else { fs.entered };
            if entered != seen_entered {
                seen_entered = entered;
                hint_until = now.saturating_add(hint_ms(&settings));
            }
            let in_text_field = if use_ime { target.is_some() } else { fs.editable() };
            // 设置窗口在前台时，练习区自己画候选框，不弹外面的悬浮窗
            let mode = if settings_in_front(&fs) {
                None
            } else if view.enabled && !view.comp.is_empty() {
                Some(Mode::Compose)
            } else if now < toast_until {
                Some(Mode::Toast)
            } else if view.enabled && settings.hint && in_text_field && now < hint_until {
                Some(Mode::Hint)
            } else {
                None
            };
            // 系统输入法报的光标位置最准；没有再用辅助功能接口查到的
            let anchor = if !settings.follow_caret {
                None
            } else if use_ime {
                snap.caret.or(fs.anchor())
            } else {
                fs.anchor()
            };
            let want = mode.map(|m| (m, anchor));
            if want != shown {
                log(&format!("{now}ms：悬浮窗 {want:?}（焦点来自 {:?}）", fs.focus.as_ref().map(|f| f.source)));
                match want {
                    Some((m, a)) => overlay::show(&app, m, a),
                    None => overlay::hide(&app),
                }
                shown = want;
            }
            // 用户词库变了：等 2 秒（期间可能还在连续选词）再存
            if machine.user_dict_mut().take_dirty() && userdict_dirty_at.is_none() {
                userdict_dirty_at = Some(now);
            }
            if userdict_dirty_at.is_some_and(|t| now >= t + 2000) {
                save_user_dict(&userdict_path, machine.user_dict());
                USER_DICT_LEN.store(machine.user_dict().len(), Ordering::Relaxed);
                userdict_dirty_at = None;
            }
            if force_refresh || now - last_pad_emit >= 33 {
                let dto = pad_dto(pad.as_ref(), &mut store, dir, dir_source);
                if force_refresh || last_pad.as_ref() != Some(&dto) {
                    let _ = app.emit_to("settings", "pad", &dto);
                    last_pad = Some(dto);
                }
                last_pad_emit = now;
            }
        }
    });
}

// ---------- 系统输入法（只有 Windows 有；其他平台这些都是空的） ----------

#[cfg(windows)]
pub type ImeHandle = crate::ime::Shared;
#[cfg(not(windows))]
pub type ImeHandle = ();

enum ImeOut {
    Comp(String),
    Commit(String),
}

#[derive(Default)]
struct ImeSnap {
    /// 当前客户端：前台进程里 JoyIME 是当前输入法、焦点在文本上的那个
    active: Option<u64>,
    caret: Option<Rect>,
    /// 当前客户端的焦点序号；变了说明进了一个新的输入框（弹方向提示）
    entered: u64,
}

#[cfg(windows)]
fn ime_snapshot(ime: &Option<ImeHandle>, fg_pid: u32) -> ImeSnap {
    let Some(s) = ime.as_ref().and_then(|h| h.lock().ok()) else { return ImeSnap::default() };
    match s.active_for(fg_pid) {
        Some((id, seq)) => ImeSnap { active: Some(id), caret: s.caret_of(id), entered: seq },
        None => ImeSnap::default(),
    }
}
#[cfg(not(windows))]
fn ime_snapshot(_: &Option<ImeHandle>, _: u32) -> ImeSnap {
    ImeSnap::default()
}

#[cfg(windows)]
fn ime_describe(ime: &Option<ImeHandle>, id: u64) -> String {
    ime.as_ref().and_then(|h| h.lock().ok()).map_or_else(String::new, |s| s.describe(id))
}
#[cfg(not(windows))]
fn ime_describe(_: &Option<ImeHandle>, _: u64) -> String {
    String::new()
}

#[cfg(windows)]
fn ime_registered(ime: &Option<ImeHandle>) -> bool {
    ime.is_some() && crate::ime::tsf_registered()
}
#[cfg(not(windows))]
fn ime_registered(_: &Option<ImeHandle>) -> bool {
    false
}

#[cfg(windows)]
fn ime_send(h: &ImeHandle, id: u64, msg: ImeOut) {
    use crate::ime::Out;
    let out = match msg {
        ImeOut::Comp(text) => Out::Comp { text },
        ImeOut::Commit(text) => Out::Commit { text },
    };
    if let Ok(s) = h.lock() {
        s.send_to(id, &out);
    }
}
#[cfg(not(windows))]
fn ime_send(_: &ImeHandle, _: u64, _: ImeOut) {}

fn save_user_dict(path: &std::path::Path, d: &UserDict) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // 先写临时文件再改名，存到一半断电也不会把原来的词库弄坏
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, d.to_text()).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// 方向提示停留多久；0 秒 = 一直显示
fn hint_ms(s: &AppSettings) -> u64 {
    if s.hint_secs == 0 {
        u64::MAX / 2
    } else {
        s.hint_secs as u64 * 1000
    }
}

fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// 换了手柄（候选键数可能不同）或者改了设置就重建状态机，保留当前层（还启用着的话）、开关和用户词库
fn rebuild(old: &mut Machine, slots: usize, settings: &AppSettings) -> Machine {
    let mut m = Machine::new(settings.layers(), slots);
    m.set_cn_fullwidth(settings.cn_fullwidth);
    m.set_learning(settings.learn);
    m.set_user_dict(old.take_user_dict());
    m.set_layer(old.layer());
    m.set_enabled(old.view().enabled);
    m
}

/// 方向：十字键 → 苦力帽 → 左摇杆，哪个推了用哪个
fn read_dir(p: &Gamepad) -> (Option<Dir>, &'static str) {
    let b = |btn| p.is_pressed(btn) as i32 as f32;
    let (x, y) = (b(Button::DPadRight) - b(Button::DPadLeft), b(Button::DPadDown) - b(Button::DPadUp));
    if x != 0.0 || y != 0.0 {
        return (Dir::from_xy(x, y), "十字键");
    }
    // gilrs 的 Y 轴向上为正
    let (hx, hy) = (p.value(Axis::DPadX), p.value(Axis::DPadY));
    if let Some(d) = Dir::from_xy(hx, -hy) {
        return (Some(d), "苦力帽");
    }
    let (lx, ly) = (p.value(Axis::LeftStickX), p.value(Axis::LeftStickY));
    match Dir::from_xy(lx, -ly) {
        Some(d) => (Some(d), "左摇杆"),
        None => (None, ""),
    }
}

fn read_held(p: &Gamepad, prof: &Profile) -> Held {
    let mut held = Held::default();
    for (code, data) in p.state().buttons() {
        if data.is_pressed() {
            if let Some(a) = prof.action_for(code.into_u32()) {
                held.insert(a);
            }
        }
    }
    held
}

/// 设置窗口是不是在前台，由窗口事件更新（lib.rs）。
/// Windows 上不靠它：隐藏启动时窗口会收到「获得焦点」却永远收不到「失去焦点」，
/// 所以 Windows 上看焦点线程查到的真实前台窗口（FocusState::own_foreground）。
pub static SETTINGS_FOCUSED: AtomicBool = AtomicBool::new(false);

fn settings_in_front(fs: &focus::FocusState) -> bool {
    if cfg!(windows) {
        fs.own_foreground
    } else {
        SETTINGS_FOCUSED.load(Ordering::Relaxed)
    }
}

/// 执行状态机要做的事。设置窗口在前台时不往外打字，只在设置窗口的试打区显示。
/// to_ime：用系统输入法时，上屏交给它（按键还是模拟键入，焦点程序一定能收到）
fn apply(
    app: &AppHandle,
    enigo: &mut Option<Enigo>,
    fx: Vec<Effect>,
    settings_focused: bool,
    to_ime: Option<(&ImeHandle, u64)>,
) {
    for e in fx {
        if settings_focused {
            log(&format!("{e:?} -> 设置窗口在前台，只显示在试打区"));
            let _ = app.emit_to("settings", "typed", effect_text(&e));
            continue;
        }
        if let (Some((h, id)), Effect::Commit(s)) = (to_ime, &e) {
            ime_send(h, id, ImeOut::Commit(s.clone()));
            log(&format!("{e:?} -> 系统输入法"));
            continue;
        }
        let Some(en) = enigo.as_mut() else { continue };
        let result = match &e {
            Effect::Commit(s) => en.text(s),
            Effect::Key(Key::Backspace) => en.key(enigo::Key::Backspace, Direction::Click),
            Effect::Key(Key::Enter) => en.key(enigo::Key::Return, Direction::Click),
            Effect::Miss(_) => Ok(()),
        };
        log(&format!("{e:?} -> {result:?}"));
    }
}

/// 开发用：设了 JOYIME_LOG 就把每个动作记到那个文件里
pub fn log(line: &str) {
    if let Some(path) = std::env::var_os("JOYIME_LOG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// 给设置窗口试打区用：普通文字原样，退格 / 回车用控制字符表示
fn effect_text(e: &Effect) -> String {
    match e {
        Effect::Commit(s) => s.clone(),
        Effect::Key(Key::Backspace) => "\u{8}".into(),
        Effect::Key(Key::Enter) => "\n".into(),
        Effect::Miss(_) => String::new(),
    }
}


fn view_dto(v: &View, layers: &[Layer]) -> ViewDto {
    const GRID: [Option<Dir>; 9] = [
        Some(Dir::UpLeft),
        Some(Dir::Up),
        Some(Dir::UpRight),
        Some(Dir::Left),
        None,
        Some(Dir::Right),
        Some(Dir::DownLeft),
        Some(Dir::Down),
        Some(Dir::DownRight),
    ];
    ViewDto {
        enabled: v.enabled,
        layer: v.layer.label(),
        layers: layers.iter().map(|l| l.label()).collect(),
        dir: v.dir.map(Dir::index),
        comp: v.comp.clone(),
        comp_display: v.comp_display.clone(),
        cands: v.page_cands.clone(),
        page: v.page,
        pages: v.pages,
        caps: match v.caps {
            Caps::Off => "off",
            Caps::Once => "once",
            Caps::Lock => "lock",
        },
        grid: GRID.iter().map(|&d| v.layer.group(d).to_string()).collect(),
        pinned: v.pinned.clone(),
        py_opts: v.py_opts.clone(),
        py_focus: v.py_focus,
        py_page: v.py_page,
        py_pages: v.py_pages,
    }
}

fn pad_dto(
    pad: Option<&Gamepad>,
    store: &mut Store,
    dir: Option<Dir>,
    dir_source: &'static str,
) -> PadDto {
    let Some(p) = pad else {
        return PadDto {
            connected: false,
            name: String::new(),
            key: String::new(),
            mapping: "none",
            buttons: Vec::new(),
            dir: None,
            dir_source: "",
            profile: Profile::default(),
            slots: 4,
        };
    };
    let prof = store.for_pad(p).clone();
    // 列出的按键：标准位置上有的 + 设置里分配过的 + 按过的
    let mut codes: Vec<(u32, Option<String>)> = STD_BUTTONS
        .iter()
        .filter_map(|&b| p.button_code(b).map(|c| (c.into_u32(), Some(format!("{b:?}")))))
        .collect();
    let mut add = |code: u32| {
        if !codes.iter().any(|c| c.0 == code) {
            codes.push((code, None));
        }
    };
    for k in prof.bindings.keys() {
        if let Ok(c) = k.parse() {
            add(c);
        }
    }
    let mut pressed = Vec::new();
    for (code, data) in p.state().buttons() {
        let c = code.into_u32();
        // 十字键是方向，不列成按键
        if matches!(p.axis_or_btn_name(code), Some(gilrs::ev::AxisOrBtn::Btn(
            Button::DPadUp | Button::DPadDown | Button::DPadLeft | Button::DPadRight
        ))) {
            continue;
        }
        add(c);
        if data.is_pressed() {
            pressed.push(c);
        }
    }
    let buttons = codes
        .into_iter()
        .map(|(code, std_name)| ButtonDto {
            code,
            std_name,
            pressed: pressed.contains(&code),
            action: prof.bindings.get(&code.to_string()).cloned(),
        })
        .collect();
    PadDto {
        connected: true,
        name: p.name().to_string(),
        key: profile::pad_key(p),
        mapping: match p.mapping_source() {
            MappingSource::SdlMappings => "sdl",
            MappingSource::Driver => "driver",
            MappingSource::None => "none",
        },
        buttons,
        dir: dir.map(Dir::index),
        dir_source,
        slots: prof.slots(),
        profile: prof,
    }
}

pub type CmdTx = std::sync::Mutex<Sender<Cmd>>;
