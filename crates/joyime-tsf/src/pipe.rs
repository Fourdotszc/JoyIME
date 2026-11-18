//! 和后台程序（joyime-app.exe）之间的管道，全部在一个后台线程里做，绝不卡宿主程序的界面线程。
//!
//! - 界面线程要发的消息（focus / caret）放进 channel，后台线程连上了就写出去；
//!   断线期间只记住最后的 focus 和 caret，重连后补发。
//! - 收到的 comp / commit 放进队列，再 PostMessage 给界面线程上的消息窗口，由它去请求 TSF 编辑会话
//!   （TSF 的对象只能在界面线程上碰）。
//! - 读用 PeekNamedPipe 轮询：同一个同步句柄上，一个线程阻塞在 ReadFile 时另一个线程的 WriteFile 也会被卡住，
//!   所以不阻塞读，每 8ms 看一眼有没有数据。
//! - 连不上（后台没在跑）而且焦点在我们这里时，启动后台程序 `--service`，之后每 200ms 重试。

use crate::log::log;
use serde::Deserialize;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Pipes::PeekNamedPipe;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const PIPE: &str = r"\\.\pipe\joyime-ime";
/// 收到后台消息后发给消息窗口的消息号
pub const WM_JOYIME: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 0x4a;

/// 后台程序发来的指令
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Incoming {
    /// 设置组字串（空 = 删掉并结束组字）
    Comp { text: String },
    /// 上屏
    Commit { text: String },
}

pub type Queue = Arc<Mutex<VecDeque<Incoming>>>;

pub struct Link {
    out: Sender<String>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Link {
    /// hwnd：界面线程上的消息窗口，收到消息后 PostMessage 给它
    pub fn start(hwnd: HWND, queue: Queue) -> Link {
        let (out, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let st = stop.clone();
        let hwnd = hwnd.0 as isize;
        let thread = std::thread::Builder::new()
            .name("joyime-pipe".into())
            .spawn(move || {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(hwnd, rx, st, queue)));
            })
            .ok();
        Link { out, stop, thread }
    }

    pub fn send(&self, line: String) {
        let _ = self.out.send(line);
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            // 后台线程最多睡 200ms 就会看到 stop；等它结束，DLL 卸载时不能还有线程在跑这里的代码
            let _ = t.join();
        }
    }
}

/// 断线期间记住的最新状态，重连后补发
#[derive(Default)]
struct Pending {
    focus: Option<String>,
    caret: Option<String>,
}

impl Pending {
    fn remember(&mut self, line: &str) {
        if line.contains(r#""t":"focus""#) {
            self.focus = Some(line.to_string());
        } else if line.contains(r#""t":"caret""#) {
            self.caret = Some(line.to_string());
        }
    }

    fn wants_service(&self) -> bool {
        self.focus.as_deref().is_some_and(|f| f.contains(r#""on":true"#))
    }
}

fn run(hwnd: isize, rx: Receiver<String>, stop: Arc<AtomicBool>, queue: Queue) {
    let mut pending = Pending::default();
    let mut launched_at: Option<Instant> = None;
    while !stop.load(Ordering::Relaxed) {
        // 没连上：收下界面线程发来的东西（只记最新状态），试着连
        loop {
            match rx.try_recv() {
                Ok(line) => pending.remember(&line),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        let pipe = match std::fs::OpenOptions::new().read(true).write(true).open(PIPE) {
            Ok(f) => f,
            Err(_) => {
                // 焦点在我们这里才值得把后台拉起来；拉过一次 10 秒内不再拉
                if pending.wants_service() && launched_at.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
                    launch_service();
                    launched_at = Some(Instant::now());
                }
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
        };
        log("连上后台程序");
        if serve(hwnd, pipe, &rx, &stop, &queue, &mut pending).is_err() {
            log("和后台程序断开");
        }
    }
}

/// 连上之后：先打招呼、补发状态，然后收发
fn serve(
    hwnd: isize,
    mut pipe: File,
    rx: &Receiver<String>,
    stop: &AtomicBool,
    queue: &Queue,
    pending: &mut Pending,
) -> std::io::Result<()> {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let hello = format!(r#"{{"t":"hello","v":1,"pid":{},"exe":{}}}"#, std::process::id(), json_string(&exe));
    writeln!(pipe, "{hello}")?;
    for line in [pending.focus.clone(), pending.caret.clone()].into_iter().flatten() {
        writeln!(pipe, "{line}")?;
    }
    let handle = HANDLE(pipe.as_raw_handle());
    let mut buf: Vec<u8> = Vec::new();
    while !stop.load(Ordering::Relaxed) {
        loop {
            match rx.try_recv() {
                Ok(line) => {
                    pending.remember(&line);
                    writeln!(pipe, "{line}")?;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        let mut avail = 0u32;
        unsafe { PeekNamedPipe(handle, None, 0, None, Some(&mut avail), None) }
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        if avail > 0 {
            let mut chunk = vec![0u8; avail as usize];
            let n = pipe.read(&mut chunk)?;
            buf.extend_from_slice(&chunk[..n]);
            let mut got = false;
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                match serde_json::from_slice::<Incoming>(&line) {
                    Ok(msg) => {
                        if let Ok(mut q) = queue.lock() {
                            q.push_back(msg);
                            got = true;
                        }
                    }
                    Err(_) => log(&format!("看不懂的消息：{}", String::from_utf8_lossy(&line))),
                }
            }
            if got {
                unsafe {
                    let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_JOYIME, WPARAM(0), LPARAM(0));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    Ok(())
}

/// 启动后台程序：先看 HKCU，再看 HKLM 里登记的路径，都没有就找 DLL 旁边的 joyime-app.exe
fn launch_service() {
    let path = crate::register::read_string(true, r"Software\JoyIME", "ServicePath")
        .or_else(|| crate::register::read_string(false, r"SOFTWARE\JoyIME", "ServicePath"))
        .or_else(|| crate::register::find_near("joyime-app.exe").map(|p| p.to_string_lossy().into_owned()));
    let Some(path) = path else {
        log("找不到后台程序 joyime-app.exe");
        return;
    };
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    match std::process::Command::new(&path)
        .arg("--service")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn()
    {
        Ok(_) => log(&format!("启动后台程序 {path}")),
        Err(e) => log(&format!("启动后台程序失败 {path}：{e}")),
    }
}

pub fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}
