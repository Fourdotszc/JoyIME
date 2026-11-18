//! TSF 文本服务对象：一个程序的一个界面线程里，JoyIME 被激活时 TSF 会建一个。
//!
//! TSF 的规矩：
//! - 所有 TSF 对象只能在这个界面线程上用（单线程套间）。管道线程收到后台的指令后，
//!   PostMessage 给本线程的消息窗口，在窗口过程里再去请求编辑会话。
//! - 改文字必须在编辑会话（ITfEditSession::DoEditSession）里做，拿到的 ec 只在这一次回调里有效。
//! - 组字串（composition）：开始组字后，在它的 range 上改文字，最后 EndComposition 上屏。

use crate::pipe::{Incoming, Link, Queue, WM_JOYIME};
use crate::{guarded, log::log, Live, GUID_DISPLAY_ATTRIBUTE};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::rc::Rc;
use windows::core::{implement, w, Interface, Ref, Result, BSTR, GUID};
use windows_core::IUnknownImpl;
use windows::Win32::Foundation::{E_INVALIDARG, E_NOTIMPL, HWND, LPARAM, LRESULT, POINT, RECT, S_FALSE, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::HiDpi::{
    GetAwarenessFromDpiAwarenessContext, GetThreadDpiAwarenessContext, LogicalToPhysicalPointForPerMonitorDPI,
    DPI_AWARENESS_PER_MONITOR_AWARE,
};
use windows::Win32::UI::TextServices::*;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
    WNDCLASSW,
};

thread_local! {
    /// 本线程当前激活的文本服务。消息窗口的窗口过程靠它找到要操作的对象
    static CURRENT: RefCell<Option<Rc<Inner>>> = const { RefCell::new(None) };
}

#[implement(
    ITfTextInputProcessorEx,
    ITfThreadMgrEventSink,
    ITfTextEditSink,
    ITfCompositionSink,
    ITfDisplayAttributeProvider
)]
pub struct TextService {
    inner: Rc<Inner>,
    _live: Live,
}

impl TextService {
    pub fn new() -> TextService {
        TextService { inner: Rc::new(Inner::default()), _live: Live::new() }
    }
}

/// 文本服务的状态。TextService（COM 对象）和消息窗口共用它
#[derive(Default)]
struct Inner {
    state: RefCell<State>,
    queue: Queue,
    active: Cell<bool>,
}

struct State {
    thread_mgr: Option<ITfThreadMgr>,
    tid: u32,
    tm_cookie: u32,
    /// 有焦点的文本上下文
    context: Option<ITfContext>,
    edit_cookie: u32,
    composition: Option<ITfComposition>,
    /// 我们自己的 sink 接口。开始组字、挂编辑通知时要交给 TSF；停用时清掉，免得循环引用
    composition_sink: Option<ITfCompositionSink>,
    edit_sink: Option<ITfTextEditSink>,
    /// 收后台指令的消息窗口
    hwnd: HWND,
    link: Option<Link>,
    /// 组字串显示属性登记后的编号
    attr_atom: Option<u32>,
    last_caret: Option<RECT>,
}

impl Default for State {
    fn default() -> Self {
        State {
            thread_mgr: None,
            tid: 0,
            tm_cookie: TF_INVALID_COOKIE,
            context: None,
            edit_cookie: TF_INVALID_COOKIE,
            composition: None,
            composition_sink: None,
            edit_sink: None,
            hwnd: HWND::default(),
            link: None,
            attr_atom: None,
            last_caret: None,
        }
    }
}

impl Inner {
    fn send(&self, line: String) {
        if let Some(l) = &self.state.borrow().link {
            l.send(line);
        }
    }

    fn send_focus(&self, on: bool) {
        self.send(format!(r#"{{"t":"focus","on":{on}}}"#));
    }

    /// 换焦点：拆掉旧上下文上的编辑通知，挂到新的上面，告诉后台
    fn set_focus(&self, doc: Option<&ITfDocumentMgr>) -> Result<()> {
        {
            let mut s = self.state.borrow_mut();
            if let Some(old) = s.context.take() {
                if s.edit_cookie != TF_INVALID_COOKIE {
                    if let Ok(src) = old.cast::<ITfSource>() {
                        unsafe {
                            let _ = src.UnadviseSink(s.edit_cookie);
                        }
                    }
                }
            }
            s.edit_cookie = TF_INVALID_COOKIE;
            s.composition = None;
            s.last_caret = None;
        }
        let context = match doc {
            Some(d) => unsafe { d.GetTop().ok() },
            None => None,
        };
        let Some(context) = context else {
            self.send_focus(false);
            return Ok(());
        };
        {
            let mut s = self.state.borrow_mut();
            if let (Ok(src), Some(sink)) = (context.cast::<ITfSource>(), s.edit_sink.clone()) {
                if let Ok(cookie) = unsafe { src.AdviseSink(&ITfTextEditSink::IID, &sink) } {
                    s.edit_cookie = cookie;
                }
            }
            s.context = Some(context.clone());
        }
        self.send_focus(true);
        // 量一下光标位置（只读的编辑会话）
        self.request(&context, Action::Measure, TF_ES_READ);
        Ok(())
    }

    fn request(&self, context: &ITfContext, action: Action, access: TF_CONTEXT_EDIT_CONTEXT_FLAGS) {
        let Some(me) = CURRENT.with(|c| c.borrow().clone()) else { return };
        let tid = self.state.borrow().tid;
        let session: ITfEditSession = Session { inner: me, action, _live: Live::new() }.into();
        unsafe {
            let _ = context.RequestEditSession(tid, &session, TF_ES_ASYNCDONTCARE | access);
        }
    }

    /// 消息窗口收到通知：把队列里后台的指令逐条交给编辑会话
    fn drain(&self) {
        let items: VecDeque<Incoming> = match self.queue.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => return,
        };
        let Some(context) = self.state.borrow().context.clone() else { return };
        for msg in items {
            self.request(&context, Action::Apply(msg), TF_ES_READWRITE);
        }
    }

    /// 编辑会话里：执行后台的指令
    fn apply(&self, ec: u32, msg: &Incoming) -> Result<()> {
        let (context, sink, existing) = {
            let s = self.state.borrow();
            (s.context.clone(), s.composition_sink.clone(), s.composition.clone())
        };
        let Some(context) = context else { return Ok(()) };
        match msg {
            Incoming::Comp { text } if text.is_empty() => {
                // 删掉组字串，结束组字，什么都不上屏
                if let Some(c) = existing {
                    unsafe {
                        if let Ok(r) = c.GetRange() {
                            let _ = r.SetText(ec, 0, &[]);
                        }
                        let _ = c.EndComposition(ec);
                    }
                    self.state.borrow_mut().composition = None;
                }
            }
            Incoming::Comp { text } => {
                let comp = match existing {
                    Some(c) => c,
                    None => {
                        let Some(sink) = sink else { return Ok(()) };
                        unsafe {
                            // 在当前选区开始组字
                            let ins: ITfInsertAtSelection = context.cast()?;
                            let range = ins.InsertTextAtSelection(ec, TF_IAS_QUERYONLY, &[])?;
                            let cc: ITfContextComposition = context.cast()?;
                            let c = cc.StartComposition(ec, &range, &sink)?;
                            self.state.borrow_mut().composition = Some(c.clone());
                            c
                        }
                    }
                };
                unsafe {
                    let range = comp.GetRange()?;
                    range.SetText(ec, 0, &wide(text))?;
                    self.set_attribute(ec, &context, &range);
                    select_end(ec, &context, &range)?;
                }
            }
            Incoming::Commit { text } => unsafe {
                if let Some(c) = existing {
                    let range = c.GetRange()?;
                    range.SetText(ec, 0, &wide(text))?;
                    if let Ok(prop) = context.GetProperty(&GUID_PROP_ATTRIBUTE) {
                        let _ = prop.Clear(ec, &range);
                    }
                    select_end(ec, &context, &range)?;
                    c.EndComposition(ec)?;
                    self.state.borrow_mut().composition = None;
                } else {
                    let ins: ITfInsertAtSelection = context.cast()?;
                    let range = ins.InsertTextAtSelection(ec, INSERT_TEXT_AT_SELECTION_FLAGS(0), &wide(text))?;
                    select_end(ec, &context, &range)?;
                }
            },
        }
        Ok(())
    }

    /// 给组字串设上虚线下划线
    fn set_attribute(&self, ec: u32, context: &ITfContext, range: &ITfRange) {
        let atom = {
            let mut s = self.state.borrow_mut();
            if s.attr_atom.is_none() {
                s.attr_atom = unsafe {
                    CoCreateInstance::<_, ITfCategoryMgr>(&CLSID_TF_CategoryMgr, None, CLSCTX_INPROC_SERVER)
                        .and_then(|m| m.RegisterGUID(&GUID_DISPLAY_ATTRIBUTE))
                        .ok()
                };
            }
            s.attr_atom
        };
        let Some(atom) = atom else { return };
        unsafe {
            if let Ok(prop) = context.GetProperty(&GUID_PROP_ATTRIBUTE) {
                let v = VARIANT::from(atom as i32);
                let _ = prop.SetValue(ec, range, &v);
            }
        }
    }

    /// 量光标（组字时是组字串末尾）在屏幕上的位置，变了就告诉后台
    fn measure(&self, ec: u32) {
        let (context, comp) = {
            let s = self.state.borrow();
            (s.context.clone(), s.composition.clone())
        };
        let Some(context) = context else { return };
        let rect = unsafe { caret_rect(ec, &context, comp.as_ref()) };
        let Some(rect) = rect else { return };
        let changed = {
            let mut s = self.state.borrow_mut();
            let changed = s.last_caret.as_ref().is_none_or(|r| !same(r, &rect));
            s.last_caret = Some(rect);
            changed
        };
        if changed {
            self.send(format!(
                r#"{{"t":"caret","x":{},"y":{},"w":{},"h":{}}}"#,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top
            ));
        }
    }
}

fn same(a: &RECT, b: &RECT) -> bool {
    a.left == b.left && a.top == b.top && a.right == b.right && a.bottom == b.bottom
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// 把选区放到 range 末尾（光标跟着组字串 / 上屏的字走）
unsafe fn select_end(ec: u32, context: &ITfContext, range: &ITfRange) -> Result<()> {
    let end = range.Clone()?;
    end.Collapse(ec, TF_ANCHOR_END)?;
    let mut sel = TF_SELECTION {
        range: ManuallyDrop::new(Some(end)),
        style: TF_SELECTIONSTYLE { ase: TF_AE_NONE, fInterimChar: false.into() },
    };
    let r = context.SetSelection(ec, std::slice::from_ref(&sel));
    ManuallyDrop::drop(&mut sel.range);
    r
}

/// 光标的屏幕矩形，物理像素。组字时取组字串末尾，否则取选区末尾
unsafe fn caret_rect(ec: u32, context: &ITfContext, comp: Option<&ITfComposition>) -> Option<RECT> {
    let range = match comp {
        Some(c) => c.GetRange().ok()?,
        None => {
            let mut sel = [TF_SELECTION::default()];
            let mut fetched = 0u32;
            context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut sel, &mut fetched).ok()?;
            if fetched == 0 {
                return None;
            }
            let r = ManuallyDrop::take(&mut sel[0].range)?;
            r
        }
    };
    let end = range.Clone().ok()?;
    end.Collapse(ec, TF_ANCHOR_END).ok()?;
    let view = context.GetActiveView().ok()?;
    let mut rc = RECT::default();
    let mut clipped = Default::default();
    view.GetTextExt(ec, &end, &mut rc, &mut clipped).ok()?;
    if rc.bottom <= rc.top {
        return None;
    }
    // 宿主程序不是 per-monitor DPI aware 时，GetTextExt 给的是逻辑坐标，换成物理像素
    let aware = GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext());
    if aware != DPI_AWARENESS_PER_MONITOR_AWARE {
        if let Ok(hwnd) = view.GetWnd() {
            let mut a = POINT { x: rc.left, y: rc.top };
            let mut b = POINT { x: rc.right, y: rc.bottom };
            let _ = LogicalToPhysicalPointForPerMonitorDPI(Some(hwnd), &mut a);
            let _ = LogicalToPhysicalPointForPerMonitorDPI(Some(hwnd), &mut b);
            rc = RECT { left: a.x, top: a.y, right: b.x, bottom: b.y };
        }
    }
    Some(rc)
}

// ---------- 编辑会话 ----------

enum Action {
    /// 执行后台的 comp / commit，之后量一下光标
    Apply(Incoming),
    /// 只量光标
    Measure,
}

#[implement(ITfEditSession)]
struct Session {
    inner: Rc<Inner>,
    action: Action,
    _live: Live,
}

impl ITfEditSession_Impl for Session_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        guarded(|| {
            if let Action::Apply(msg) = &self.action {
                if let Err(e) = self.inner.apply(ec, msg) {
                    log(&format!("执行 {msg:?} 失败：{e}"));
                }
            }
            self.inner.measure(ec);
            Ok(())
        })
    }
}

// ---------- 消息窗口 ----------

const WINDOW_CLASS: windows::core::PCWSTR = w!("JoyIMEMessageWindow");

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_JOYIME {
        let _ = std::panic::catch_unwind(|| {
            if let Some(inner) = CURRENT.with(|c| c.borrow().clone()) {
                inner.drain();
            }
        });
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn create_message_window() -> Option<HWND> {
    unsafe {
        static REGISTER: std::sync::Once = std::sync::Once::new();
        REGISTER.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: crate::dll_instance(),
                lpszClassName: WINDOW_CLASS,
                ..Default::default()
            };
            RegisterClassW(&wc);
        });
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            WINDOW_CLASS,
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(crate::dll_instance()),
            None,
        )
        .ok()
    }
}

// ---------- TSF 接口 ----------

impl TextService_Impl {
    fn activate(&self, tm: Option<&ITfThreadMgr>, tid: u32) -> Result<()> {
        guarded(|| {
            let inner = &self.inner;
            if inner.active.get() {
                return Ok(());
            }
            let tm = tm.cloned().ok_or_else(|| windows::core::Error::from(E_INVALIDARG))?;
            log("激活");
            let me_tm: ITfThreadMgrEventSink = self.to_interface();
            let cookie = unsafe { tm.cast::<ITfSource>()?.AdviseSink(&ITfThreadMgrEventSink::IID, &me_tm)? };
            let hwnd = create_message_window().unwrap_or_default();
            {
                let mut s = inner.state.borrow_mut();
                s.thread_mgr = Some(tm.clone());
                s.tid = tid;
                s.tm_cookie = cookie;
                s.composition_sink = Some(self.to_interface());
                s.edit_sink = Some(self.to_interface());
                s.hwnd = hwnd;
                s.link = Some(Link::start(hwnd, inner.queue.clone()));
            }
            CURRENT.with(|c| *c.borrow_mut() = Some(inner.clone()));
            inner.active.set(true);
            // 激活时焦点可能已经在某个文档上了
            if let Ok(doc) = unsafe { tm.GetFocus() } {
                inner.set_focus(Some(&doc))?;
            }
            Ok(())
        })
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, tm: Ref<'_, ITfThreadMgr>, tid: u32) -> Result<()> {
        self.activate(tm.as_ref(), tid)
    }

    fn Deactivate(&self) -> Result<()> {
        guarded(|| {
            let inner = &self.inner;
            if !inner.active.get() {
                return Ok(());
            }
            log("停用");
            inner.send_focus(false);
            let _ = inner.set_focus(None);
            let (tm, cookie, hwnd, link) = {
                let mut s = inner.state.borrow_mut();
                s.composition_sink = None;
                s.edit_sink = None;
                (s.thread_mgr.take(), s.tm_cookie, s.hwnd, s.link.take())
            };
            if let (Some(tm), true) = (tm, cookie != TF_INVALID_COOKIE) {
                if let Ok(src) = tm.cast::<ITfSource>() {
                    unsafe {
                        let _ = src.UnadviseSink(cookie);
                    }
                }
            }
            if let Some(l) = link {
                l.stop();
            }
            if !hwnd.is_invalid() {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            CURRENT.with(|c| *c.borrow_mut() = None);
            inner.active.set(false);
            Ok(())
        })
    }
}

impl ITfTextInputProcessorEx_Impl for TextService_Impl {
    fn ActivateEx(&self, tm: Ref<'_, ITfThreadMgr>, tid: u32, _flags: u32) -> Result<()> {
        self.activate(tm.as_ref(), tid)
    }
}

impl ITfThreadMgrEventSink_Impl for TextService_Impl {
    fn OnInitDocumentMgr(&self, _: Ref<'_, ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }
    fn OnUninitDocumentMgr(&self, _: Ref<'_, ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }
    fn OnSetFocus(&self, focus: Ref<'_, ITfDocumentMgr>, _prev: Ref<'_, ITfDocumentMgr>) -> Result<()> {
        guarded(|| self.inner.set_focus(focus.as_ref()))
    }
    fn OnPushContext(&self, _: Ref<'_, ITfContext>) -> Result<()> {
        Ok(())
    }
    fn OnPopContext(&self, _: Ref<'_, ITfContext>) -> Result<()> {
        Ok(())
    }
}

impl ITfTextEditSink_Impl for TextService_Impl {
    /// 文字或选区变了（用户打字、点了别处、我们自己改的）：重新量光标
    fn OnEndEdit(&self, _context: Ref<'_, ITfContext>, ec_read_only: u32, _record: Ref<'_, ITfEditRecord>) -> Result<()> {
        guarded(|| {
            self.inner.measure(ec_read_only);
            Ok(())
        })
    }
}

impl ITfCompositionSink_Impl for TextService_Impl {
    /// 组字被别人终止了（用户点了别处、程序自己结束的）
    fn OnCompositionTerminated(&self, _ec: u32, _comp: Ref<'_, ITfComposition>) -> Result<()> {
        guarded(|| {
            self.inner.state.borrow_mut().composition = None;
            Ok(())
        })
    }
}

// ---------- 组字串的显示属性：虚线下划线 ----------

impl ITfDisplayAttributeProvider_Impl for TextService_Impl {
    fn EnumDisplayAttributeInfo(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        guarded(|| Ok(AttributeEnum { done: Cell::new(false), _live: Live::new() }.into()))
    }

    fn GetDisplayAttributeInfo(&self, guid: *const GUID) -> Result<ITfDisplayAttributeInfo> {
        guarded(|| unsafe {
            if !guid.is_null() && *guid == GUID_DISPLAY_ATTRIBUTE {
                Ok(DisplayAttribute { _live: Live::new() }.into())
            } else {
                Err(E_INVALIDARG.into())
            }
        })
    }
}

#[implement(ITfDisplayAttributeInfo)]
struct DisplayAttribute {
    _live: Live,
}

impl ITfDisplayAttributeInfo_Impl for DisplayAttribute_Impl {
    fn GetGUID(&self) -> Result<GUID> {
        Ok(GUID_DISPLAY_ATTRIBUTE)
    }
    fn GetDescription(&self) -> Result<BSTR> {
        Ok(BSTR::from("JoyIME 组字串"))
    }
    fn GetAttributeInfo(&self, pda: *mut TF_DISPLAYATTRIBUTE) -> Result<()> {
        guarded(|| unsafe {
            if pda.is_null() {
                return Err(E_INVALIDARG.into());
            }
            *pda = TF_DISPLAYATTRIBUTE { lsStyle: TF_LS_DOT, bAttr: TF_ATTR_INPUT, ..Default::default() };
            Ok(())
        })
    }
    fn SetAttributeInfo(&self, _: *const TF_DISPLAYATTRIBUTE) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn Reset(&self) -> Result<()> {
        Ok(())
    }
}

/// 只有一个显示属性的枚举器
#[implement(IEnumTfDisplayAttributeInfo)]
struct AttributeEnum {
    done: Cell<bool>,
    _live: Live,
}

impl IEnumTfDisplayAttributeInfo_Impl for AttributeEnum_Impl {
    fn Clone(&self) -> Result<IEnumTfDisplayAttributeInfo> {
        Ok(AttributeEnum { done: Cell::new(self.done.get()), _live: Live::new() }.into())
    }
    fn Next(&self, count: u32, out: *mut Option<ITfDisplayAttributeInfo>, fetched: *mut u32) -> Result<()> {
        let give = count > 0 && !self.done.get() && !out.is_null();
        unsafe {
            if give {
                *out = Some(DisplayAttribute { _live: Live::new() }.into());
                self.done.set(true);
            }
            if !fetched.is_null() {
                *fetched = give as u32;
            }
        }
        // 给的比要的少要返回 S_FALSE（它是成功码，放在 Err 里也会原样返回给调用方）
        if give && count == 1 {
            Ok(())
        } else {
            Err(windows::core::Error::from_hresult(S_FALSE))
        }
    }
    fn Reset(&self) -> Result<()> {
        self.done.set(false);
        Ok(())
    }
    fn Skip(&self, count: u32) -> Result<()> {
        if count > 0 {
            self.done.set(true);
        }
        Ok(())
    }
}

