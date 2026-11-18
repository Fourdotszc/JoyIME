'use strict';

// 两个窗口共用：输入层的全名、功能的颜色分类、Tauri 接口（在普通浏览器里预览时没有 Tauri，用演示数据）

const LAYER_NAMES = {
  '九': '中文九宫格',
  '拼': '中文全拼',
  '英': '英文联想',
  'abc': '小写字母',
  'ABC': '大写字母',
  '123': '数字符号',
};

// 九宫格九个格子对应的方向编号（左上到右下，中间是中位），和后台的 Dir 顺序一致：0↑ 1↗ 2→ 3↘ 4↓ 5↙ 6← 7↖
const GRID_DIRS = [7, 0, 1, 6, null, 2, 5, 4, 3];
const PHONE_KEYS = { 7: '5', 0: '2', 1: '3', 6: '4', 2: '6', 5: '7', 4: '8', 3: '9' };

const TAURI = window.__TAURI__ || null;

function invoke(cmd, args) {
  return TAURI ? TAURI.core.invoke(cmd, args) : Promise.resolve(DEMO.invoke(cmd, args));
}

function listen(event, fn) {
  if (TAURI) return TAURI.event.listen(event, e => fn(e.payload));
  DEMO.listeners.push([event, fn]);
}

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
}

// ---------- 浏览器预览用的演示数据（打包后的程序里不会用到） ----------
const DEMO = {
  listeners: [],
  emit(event, payload) {
    for (const [e, fn] of this.listeners) if (e === event) fn(payload);
  },
  invoke(cmd, args) {
    if (cmd === 'get_meta') return DEMO_META;
    if (cmd === 'get_settings') return DEMO_SETTINGS;
    if (cmd === 'userdict_len') return 128;
    if (cmd === 'set_settings') Object.assign(DEMO_SETTINGS, args.settings);
    if (cmd === 'set_binding') {
      for (const [k, v] of Object.entries(DEMO_PAD.profile.bindings)) if (v === args.action) delete DEMO_PAD.profile.bindings[k];
      if (args.action) DEMO_PAD.profile.bindings[args.code] = args.action;
      else delete DEMO_PAD.profile.bindings[args.code];
      DEMO_PAD.profile.preset = 'custom';
      DEMO_PAD.buttons.forEach(b => (b.action = DEMO_PAD.profile.bindings[b.code] || null));
      setTimeout(() => DEMO.emit('pad', DEMO_PAD));
    }
    if (cmd === 'refresh') setTimeout(() => { DEMO.emit('pad', DEMO_PAD); DEMO.emit('view', DEMO_VIEW); });
    return null;
  },
};

const DEMO_SETTINGS = { hint: true, hint_secs: 4, follow_caret: true, layers: ['cn9', 'en9', 'num', 'lower', 'upper'], cn_fullwidth: true, output: 'auto', learn: true };

const DEMO_META = {
  actions: [
    ['slot0', '字母 / 候选 1', ''], ['slot1', '字母 / 候选 2', ''], ['slot2', '字母 / 候选 3', ''], ['slot3', '字母 / 候选 4', ''],
    ['space', '空格', ''], ['backspace', '退格', ''], ['enter', '回车 / 上一页', ''], ['cycle_layer', '切换 / 下一页', ''],
    ['next_page', '下一页', ''], ['prev_page', '上一页', ''], ['escape', '清空', ''], ['shift', '大写', ''], ['toggle', '开关输入法', ''], ['pinyin', '选拼音', ''],
  ],
  presets: [['arcade8', '街机摇杆 · 8 键'], ['arcade6', '街机摇杆 · 6 键'], ['gamepad', '游戏手柄']],
  layers: [
    ['cn9', '九', '中文九宫格：只推方向，自动猜拼音和词'], ['en9', '英', '英文联想：只推方向，自动猜单词，选词后自动补空格'],
    ['num', '123', '数字：推一下就是一个数字，方向 + 按键打符号'], ['pinyin', '拼', '中文全拼：方向 + 按键逐个打拼音字母'],
    ['lower', 'abc', '英文小写：方向 + 按键逐个打字母'], ['upper', 'ABC', '英文大写：方向 + 按键逐个打字母'],
  ],
  positions: [
    ['top1', '上排 1', 'West'], ['top2', '上排 2', 'North'], ['top3', '上排 3', 'RightTrigger'], ['top4', '上排 4', 'LeftTrigger'],
    ['bot1', '下排 1', 'South'], ['bot2', '下排 2', 'East'], ['bot3', '下排 3', 'RightTrigger2'], ['bot4', '下排 4', 'LeftTrigger2'],
    ['select', 'Select / Back', 'Select'], ['start', 'Start', 'Start'], ['home', 'Home', 'Mode'],
  ],
};

const DEMO_PAD = (() => {
  const std = ['West', 'North', 'RightTrigger', 'LeftTrigger', 'South', 'East', 'RightTrigger2', 'LeftTrigger2', 'Select', 'Start', 'Mode'];
  const act = ['slot0', 'slot1', 'slot2', 'slot3', 'space', 'backspace', 'cycle_layer', 'enter', 'escape', 'pinyin', 'toggle'];
  const bindings = {};
  std.forEach((_, i) => act[i] && (bindings[i] = act[i]));
  return {
    connected: true,
    name: '8 键街机摇杆（演示）',
    key: '030000002c2200000020000000000000',
    mapping: 'sdl',
    buttons: std.map((s, i) => ({ code: i, std_name: s, pressed: false, action: act[i] })),
    dir: null,
    dir_source: '苦力帽',
    profile: { preset: 'arcade8', bindings, names: {} },
    slots: 4,
  };
})();

const DEMO_VIEW = {
  enabled: true, layer: '九', layers: ['九', '英', '拼', 'abc', 'ABC', '123'], dir: 2,
  comp: '64426', comp_display: "ni'hao", cands: ['你好', '你敢', '米高', '你好啊'], page: 0, pages: 9, caps: 'off',
  grid: ['jkl', 'abc', 'def', 'ghi', '', 'mno', 'pqrs', 'tuv', 'wxyz'],
  pinned: [], py_opts: ['ni', 'mi', 'oh'], py_focus: false, py_page: 0, py_pages: 1,
};
// 截图用：?demo=pinyin 演示按住空格选拼音（9426 → 拼音栏亮着）
if (!TAURI && new URLSearchParams(location.search).get('demo') === 'pinyin') {
  Object.assign(DEMO_VIEW, {
    dir: null, comp: '9426', comp_display: 'xian', cands: ['先', '线', '现在', '县'], pages: 44,
    pinned: ['xian'], py_opts: [], py_focus: false,
  });
}
if (!TAURI && new URLSearchParams(location.search).get('demo') === 'pyfocus') {
  Object.assign(DEMO_VIEW, {
    dir: null, comp: '9426', comp_display: 'xiao', cands: ['小', '先', '找', '线'], pages: 250,
    py_opts: ['xian', 'xiao', 'zhao', 'zhan'], py_focus: true, py_page: 0, py_pages: 3,
  });
}

/**
 * 中文九宫格的拼音栏（悬浮窗和练习区共用）：定下的音节 + 可选的音节。
 * 编号对应上排键：按住空格 + 上排键选拼音。选拼音时（按住空格一会儿、或按了选拼音键）高亮。没有就返回 null
 */
function pinyinBar(v) {
  const pinned = v.pinned || [];
  const opts = v.py_opts || [];
  if (!pinned.length && !opts.length) return null;
  const bar = el('div', 'pybar' + (v.py_focus ? ' focus' : ''));
  pinned.forEach(s => bar.append(el('b', '', s)));
  const list = el('ol');
  opts.forEach((s, i) => {
    const li = el('li', '', s);
    li.dataset.n = i + 1;
    list.append(li);
  });
  if (opts.length) bar.append(list);
  if (v.py_focus && v.py_pages > 1) bar.append(el('small', '', `${v.py_page + 1}/${v.py_pages}`));
  else if (!v.py_focus && opts.length) bar.append(el('small', '', '按住空格选'));
  return bar;
}

if (!TAURI) {
  // 演示：方向和按键自己动起来
  let t = 0;
  setInterval(() => {
    t++;
    DEMO_PAD.dir = [2, 6, 6, 0, 2, null, null, null][t % 8];
    DEMO_PAD.buttons.forEach((b, i) => (b.pressed = t % 8 === 6 && i === 0));
    DEMO_VIEW.dir = DEMO_PAD.dir;
    DEMO.emit('pad', DEMO_PAD);
    DEMO.emit('overlay', { mode: 'compose', above: false });
    DEMO.emit('view', DEMO_VIEW);
  }, 600);
}
