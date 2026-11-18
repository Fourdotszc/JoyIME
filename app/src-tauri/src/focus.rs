//! 后台线程：每 80ms 查一次键盘焦点（caret.rs），结果放在共享状态里给输入线程读。
//! 查焦点要跨进程调用，可能慢，所以不放在输入线程里做。

use crate::caret::{Focus, Rect, Tracker};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct FocusState {
    /// 当前焦点；在 JoyIME 自己的窗口里或者查不到时为 None
    pub focus: Option<Focus>,
    /// 每次「进入了一个新的输入框」加一。输入线程看到它变了就弹方向提示
    pub entered: u64,
    /// 前台是 JoyIME 自己的窗口（设置窗口；悬浮窗永远不会到前台）。
    /// 这时摇杆打的字只进设置窗口的练习区，不往外打
    pub own_foreground: bool,
    /// 前台窗口所属的进程（包括 JoyIME 自己）；系统输入法模式靠它挑当前客户端
    pub fg_pid: u32,
}

impl FocusState {
    pub fn editable(&self) -> bool {
        self.focus.as_ref().is_some_and(|f| f.editable)
    }

    /// 候选窗该贴着哪里：有光标用光标，没有就用输入框本身
    pub fn anchor(&self) -> Option<Rect> {
        let f = self.focus.as_ref().filter(|f| f.editable)?;
        f.caret.or(f.element)
    }
}

pub type Shared = Arc<Mutex<FocusState>>;

pub fn spawn() -> Shared {
    let shared: Shared = Arc::default();
    let out = shared.clone();
    std::thread::spawn(move || {
        let mut tracker = Tracker::new();
        let own = std::process::id();
        // 用来判断是不是换了一个输入框：进程 + 输入框位置（拿不到位置时只看能不能输入）
        let mut last_field: Option<(u32, Option<Rect>)> = None;
        loop {
            let raw = tracker.poll();
            let own_foreground = raw.as_ref().is_some_and(|f| f.process_id == own);
            let fg_pid = raw.as_ref().map_or(0, |f| f.process_id);
            let focus = raw.filter(|f| f.process_id != own);
            let field = focus.as_ref().filter(|f| f.editable).map(|f| (f.process_id, f.element));
            let entered = field.is_some() && field != last_field;
            last_field = field;
            if let Ok(mut s) = out.lock() {
                s.focus = focus;
                s.own_foreground = own_foreground;
                s.fg_pid = fg_pid;
                if entered {
                    s.entered += 1;
                }
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    });
    shared
}
