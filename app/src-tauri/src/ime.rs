//! 系统输入法（TSF DLL，crates/joyime-tsf）的管道服务端。协议见 docs/ime-protocol.md。
//!
//! 每个加载了 DLL 的程序（的每个界面线程）连一条管道，各自报告「我的线程里焦点在一个文本框上」。
//! 注意这是线程内的焦点，后台的程序也会报；所以当前客户端 = **前台窗口所属进程**里最近报告获得焦点的那个，
//! 由输入线程按前台进程来挑（active_for），不能简单地取最后一个报告的。

use crate::caret::Rect;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

pub const PIPE: &str = r"\\.\pipe\joyime-ime";
/// 文本服务的 CLSID，和 crates/joyime-tsf 里的一致
const CLSID: &str = "{B547C390-17E8-4D9D-873A-176AEAC628E2}";

/// 发给 DLL 的消息
#[derive(Serialize, Debug)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Out {
    /// 设置组字串，空 = 删掉并结束组字
    Comp { text: String },
    /// 上屏
    Commit { text: String },
}

/// DLL（或者另一个想让我们显示设置窗口的 JoyIME）发来的消息
#[derive(Deserialize, Debug)]
#[serde(tag = "t", rename_all = "lowercase")]
enum In {
    Hello {
        #[serde(default)]
        pid: u32,
        #[serde(default)]
        exe: String,
    },
    Focus {
        on: bool,
    },
    Caret {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
    /// 用户又双击启动了一次 JoyIME：把设置窗口显示出来
    Show,
    /// 新启动的 JoyIME 带了 --replace（更新、测试）：让位，退出
    Quit,
}

struct Client {
    tx: UnboundedSender<String>,
    caret: Option<Rect>,
    exe: String,
    pid: u32,
    /// 这个客户端的线程里焦点在文本上
    focused: bool,
    /// 最近一次报告获得焦点的序号（越大越近）；换了输入框也会再报一次
    focus_seq: u64,
}

#[derive(Default)]
pub struct ImeState {
    clients: HashMap<u64, Client>,
    seq: u64,
}

impl ImeState {
    /// 前台进程 pid 里，焦点在文本上、最近报告的那个客户端：(客户端编号, 它的焦点序号)
    pub fn active_for(&self, pid: u32) -> Option<(u64, u64)> {
        self.clients
            .iter()
            .filter(|(_, c)| c.pid == pid && c.focused)
            .max_by_key(|(_, c)| c.focus_seq)
            .map(|(id, c)| (*id, c.focus_seq))
    }

    pub fn caret_of(&self, id: u64) -> Option<Rect> {
        self.clients.get(&id).and_then(|c| c.caret)
    }

    pub fn describe(&self, id: u64) -> String {
        self.clients.get(&id).map_or_else(String::new, |c| format!("{}（pid {}）", c.exe, c.pid))
    }

    /// 发给某个客户端；它已经断开了返回 false
    pub fn send_to(&self, id: u64, msg: &Out) -> bool {
        let Some(c) = self.clients.get(&id) else { return false };
        match serde_json::to_string(msg) {
            Ok(line) => c.tx.send(line).is_ok(),
            Err(_) => false,
        }
    }
}

pub type Shared = Arc<Mutex<ImeState>>;

/// 启动管道服务端。管道名已经被占了（另一个 JoyIME 在跑）就返回 Err。
/// on_show：收到 show 消息时调用（另一次启动想让我们显示设置窗口）。
pub fn spawn(on_show: impl Fn() + Send + Sync + 'static) -> Result<Shared, ()> {
    let first = tauri::async_runtime::block_on(async { create(true) }).map_err(|_| ())?;
    let state: Shared = Arc::default();
    let st = state.clone();
    let on_show = Arc::new(on_show);
    tauri::async_runtime::spawn(async move {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mut server = first;
        loop {
            if server.connect().await.is_err() {
                // 这个实例坏了，换一个接着等
                match create(false) {
                    Ok(s) => server = s,
                    Err(_) => return,
                }
                continue;
            }
            let conn = server;
            server = match create(false) {
                Ok(s) => s,
                Err(_) => return,
            };
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            tauri::async_runtime::spawn(serve(id, conn, st.clone(), on_show.clone()));
        }
    });
    Ok(state)
}

/// 已经有 JoyIME 在跑时，让它把设置窗口显示出来
pub fn ask_running_instance_to_show() -> bool {
    tell_running_instance(b"{\"t\":\"show\"}\n")
}

/// 已经有 JoyIME 在跑时，让它退出（--replace）
pub fn ask_running_instance_to_quit() -> bool {
    tell_running_instance(b"{\"t\":\"quit\"}\n")
}

fn tell_running_instance(msg: &[u8]) -> bool {
    use std::io::Write;
    match std::fs::OpenOptions::new().write(true).open(PIPE) {
        Ok(mut f) => f.write_all(msg).is_ok(),
        Err(_) => false,
    }
}

/// JoyIME 有没有注册成系统输入法（装过 DLL）
pub fn tsf_registered() -> bool {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, HKEY, HKEY_CLASSES_ROOT, KEY_READ};
    let path = HSTRING::from(format!(r"CLSID\{CLSID}\InprocServer32"));
    let mut key = HKEY::default();
    unsafe {
        let ok = RegOpenKeyExW(HKEY_CLASSES_ROOT, &path, None, KEY_READ, &mut key).is_ok();
        if ok {
            let _ = RegCloseKey(key);
        }
        ok
    }
}

/// 建一个管道实例。安全描述符允许商店应用（AppContainer）和低完整性进程连接，不然新版记事本这类程序连不上。
fn create(first: bool) -> std::io::Result<NamedPipeServer> {
    use windows::core::w;
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

    let mut sd = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w!("D:(A;;GA;;;WD)(A;;GA;;;AC)S:(ML;;NW;;;LW)"),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    let mut sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: false.into(),
    };
    // 安全描述符只在创建时用到；进程里只建少量实例，不释放也无妨（LocalFree 需要再加依赖的 feature）
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(PIPE, &mut sa as *mut _ as *mut std::ffi::c_void)
    }
}

async fn serve(id: u64, conn: NamedPipeServer, state: Shared, on_show: Arc<dyn Fn() + Send + Sync>) {
    let (read, mut write) = tokio::io::split(conn);
    let (tx, mut rx) = unbounded_channel::<String>();
    if let Ok(mut s) = state.lock() {
        s.clients.insert(id, Client { tx, caret: None, exe: String::new(), pid: 0, focused: false, focus_seq: 0 });
    }
    // 写：输入线程往 tx 里放，这里一条条写出去
    let writer = tauri::async_runtime::spawn(async move {
        while let Some(line) = rx.recv().await {
            if write.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let mut lines = BufReader::new(read).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(msg) = serde_json::from_str::<In>(&line) else { continue };
        crate::engine::log(&format!("输入法客户端 {id}：{line}"));
        let Ok(mut s) = state.lock() else { break };
        match msg {
            In::Hello { pid, exe } => {
                if let Some(c) = s.clients.get_mut(&id) {
                    c.pid = pid;
                    c.exe = exe;
                }
            }
            In::Focus { on } => {
                s.seq += 1;
                let seq = s.seq;
                if let Some(c) = s.clients.get_mut(&id) {
                    c.focused = on;
                    if on {
                        c.focus_seq = seq;
                    }
                }
            }
            In::Caret { x, y, w, h } => {
                if let Some(c) = s.clients.get_mut(&id) {
                    c.caret = Some(Rect { x, y, w, h });
                }
            }
            In::Show => on_show(),
            In::Quit => std::process::exit(0),
        }
    }
    if let Ok(mut s) = state.lock() {
        s.clients.remove(&id);
    }
    writer.abort();
}
