'use strict';

// 界面语言。中文直接写在页面和代码里；英文在这里查：
//   页面上带 data-t="key" 的元素换成 EN[key]（data-t-title 换 title）
//   代码里的 t('中文 {0}', x) 用中文原文当 key 查 EN，{0} {1} 换成参数
//   后台给的 meta（功能、预设、输入方式、按键位置）按 id 查 EN_META
// LANG 由后台决定（设置里选的，或跟随 Windows 显示语言）；换语言时后台发 lang 事件，页面整个重新加载。

let LANG = 'zh';

const EN = {
  // ---- 设置窗口：页面上的固定文字（data-t） ----
  'tab.controller': 'Controller',
  'tab.input': 'Input',
  'tab.practice': 'Practice',
  'tab.about': 'About',
  'power.title': 'Pause / resume controller input',
  'tb.layout': 'Layout',
  'tb.detect': 'Detect buttons…',

  'h.ui': 'Interface',
  'lang.desc': 'Language of this window, the candidate window and the tray menu.',
  'lang.auto': 'Same as Windows',

  'h.output': 'Output',
  'output.name': 'How text gets into apps',
  'output.desc': 'System IME: works only when JoyIME is selected in the input method list (Win + Space); the reading shows inline while you type, like any other IME.<br>Simulated typing: nothing to install — the controller types straight into any app.',
  'output.auto': 'Auto',
  'output.ime': 'System IME',
  'output.inject': 'Simulated typing',

  'h.hint': 'Direction hints',
  'hint.name': 'Show direction hints when entering a text field',
  'hint.desc': 'When you click into a place where you can type, a small grid next to the cursor shows which letters each of the eight directions gives.',
  'hintsecs.name': 'Hide after inactivity',
  'hintsecs.desc': 'Moving the stick or pressing a button brings it back; it stays up while you type.',
  'secs.2': '2 seconds',
  'secs.4': '4 seconds',
  'secs.6': '6 seconds',
  'secs.10': '10 seconds',
  'secs.0': 'Always show',

  'h.cands': 'Candidate window',
  'follow.name': 'Position',
  'follow.desc': 'When following the cursor, apps that don’t report a cursor position fall back to the bottom of the screen.',
  'follow.caret': 'Follow the text cursor',
  'follow.bottom': 'Bottom of the screen',

  'h.layers': 'Input modes',
  'order.name': 'Switch order',
  'order.desc': 'When you’re not typing, Switch cycles through the modes turned on above.',

  'h.learn': 'Learning',
  'learn.name': 'Remember the words you use',
  'learn.desc': 'Words you pick move up next time. Words you built from several parts and whole phrases are remembered too, and get suggested halfway through. Stored only on this computer.',
  'userdict.name': 'Remembered words',
  'btn.clear': 'Clear',

  'h.punct': 'Punctuation',
  'punct.name': 'Punctuation in Chinese modes',
  'punct.desc': 'The comma, period, question mark and exclamation mark typed in Chinese T9 and Chinese pinyin. English and numbers always use half-width.',
  'punct.full': 'Full-width ，。？！',
  'punct.half': 'Half-width , . ? !',

  'practice.area': 'Practice area',
  'practice.hint': 'While this window is in front, whatever you type with the controller shows up only here, not in other apps.',

  'about.intro': '<b>JoyIME</b> 0.1.0 — type with an arcade stick or gamepad.',
  'about.controls': 'Controls',
  'about.cn9': 'CN · Chinese T9',
  'about.cn9.d': 'Push a direction and let it return to center = one phone-key press. The top-row buttons pick candidates. When you’re not typing, the four top buttons type ，。？！ directly.',
  'about.en9': 'EN · English T9',
  'about.en9.d': 'Same idea — directions only, and the word is guessed for you. A space is added after the word you pick; typing punctuation right after removes it.',
  'about.num': '123 · Numbers',
  'about.num.d': 'One push = the digit in that direction, laid out like a phone keypad: ↖1 ↑2 ↗3 ←4 →6 ↙7 ↓8 ↘9. With the stick centered the four top buttons are 5 0 . , — hold a direction and press a button for symbols (+ - = @ # …).',
  'about.letters': 'PY / abc / ABC',
  'about.letters.d': 'Push a direction to choose a group of letters; the Nth button types the Nth letter.',
  'about.pinyin': 'Picking pinyin',
  'about.pinyin.d': 'While typing in Chinese T9, hold Space and the top-row buttons pick the pinyin instead of a candidate — e.g. yo rather than wo for 96.',
  'about.switch': 'Switch · Enter',
  'about.switch.d': 'When you’re not typing: change mode, Enter. While typing: next page, previous page; hold to keep flipping.',
  'about.initials': 'Initials',
  'about.initials.d': 'Typing just the initials works too: nh → 你好, zgr → 中国人.',
  'about.third': 'Third-party data',
  'about.dict': 'Chinese dictionary',
  'about.dict.d': 'Google Pinyin dictionary (AOSP PinyinIME), Apache License 2.0',
  'about.freq': 'English word frequencies',
  'about.freq.d': 'FrequencyWords (Hermit Dave, from OpenSubtitles), MIT License',
  'about.pads': 'Controller detection',
  'about.pads.d': 'gilrs and its bundled SDL game controller database',

  'wz.title': 'Detect buttons',
  'wz.sub': 'Press the button at that spot on your controller. If it doesn’t have one, click Skip.',
  'wz.cancel': 'Cancel',
  'wz.skip': 'Skip',

  // ---- 代码里的字（t('中文')） ----
  '未分配': 'Unassigned',
  '按键 #{0}': 'Button #{0}',
  '没有摇杆': 'No controller',
  '插上摇杆后按一下任意键。': 'Plug in a controller and press any button.',
  '← 全部按键': '← All buttons',
  '不分配': 'None',
  '这个键什么都不做': 'This button does nothing',
  '现在在{0}': 'now on {0}',
  '按键分配': 'Button mapping',
  '点图上的按键或下面一行，给它换功能。': 'Click a button in the picture, or a row below, to change what it does.',
  '其他按键': 'Other buttons',
  '未连接摇杆': 'No controller connected',
  '已自定义': 'Customized',
  '已识别型号': 'Known model',
  '系统驱动映射': 'Driver mapping',
  '未识别型号，建议先「检测按键」': 'Unknown model — try Detect buttons first',
  '{0} 个按键': '{0} buttons',
  '检测过 {0} 个位置': '{0} positions detected',
  '每页 {0} 个候选': '{0} candidates per page',
  '方向': 'Direction',
  '中位': 'center',
  '十字键': 'D-pad',
  '苦力帽': 'hat',
  '左摇杆': 'left stick',
  '脚本': 'script',
  '已开启': 'On',
  '已暂停': 'Paused',
  '第 {0} / {1} 页 · 切换下一页 · 回车上一页': 'Page {0} / {1} · Switch: next · Enter: previous',
  '{0} 条': '{0} entries',
  '还没有': 'None yet',
  '清除': 'Clear',
  '再点一次确认清除': 'Click again to clear',
  '完成': 'Done',
  '取消': 'Cancel',
  '记下了 <em>{0}</em> 个按键的位置。要套用哪种布局？': 'Recorded <em>{0}</em> button positions. Which layout do you want?',
  '按「<em>{0}</em>」': 'Press <em>{0}</em>',
  '按住空格选': 'hold Space',
  '停': 'Off',
  '按「开关」键恢复': 'Press On/Off to resume',
  '8 键街机摇杆（演示）': 'Arcade stick, 8 buttons (demo)',
};

// 后台 meta 里的字，按 id 查
const EN_META = {
  actions: {
    slot0: ['Letter / candidate 1', 'In letter modes, the 1st letter of the group; while typing, the 1st candidate; Chinese T9 when idle: ，'],
    slot1: ['Letter / candidate 2', '2nd letter / candidate; Chinese T9 when idle: 。'],
    slot2: ['Letter / candidate 3', '3rd letter / candidate; Chinese T9 when idle: ？'],
    slot3: ['Letter / candidate 4', '4th letter / candidate; Chinese T9 when idle: ！'],
    space: ['Space', 'Space; while typing, picks the 1st candidate; in Chinese T9 hold it + a top button to pick pinyin'],
    backspace: ['Backspace', 'Backspace; while typing, deletes one key'],
    enter: ['Enter / previous page', 'Enter; while typing, goes back a page (hold to keep flipping)'],
    cycle_layer: ['Switch / next page', 'Switches input mode; while typing, goes forward a page (hold to keep flipping)'],
    next_page: ['Next page', 'Only flips pages, never switches mode'],
    prev_page: ['Previous page', 'Only flips pages, never switches mode'],
    escape: ['Clear', 'Throws away the pinyin / keys being typed'],
    shift: ['Shift', 'English: capitalize the next word; press again for caps lock'],
    toggle: ['On / Off', 'Pause / resume (for when you’re playing games)'],
    pinyin: ['Pick pinyin', 'In Chinese T9, makes the top buttons pick pinyin; press again to go back to candidates (or skip it: hold Space + a top button)'],
  },
  presets: { arcade8: 'Arcade stick · 8 buttons', arcade6: 'Arcade stick · 6 buttons', gamepad: 'Gamepad' },
  layers: {
    cn9: 'Directions only; the pinyin and words are guessed for you',
    en9: 'Directions only; words are guessed and a space follows each one',
    num: 'One push is one digit; direction + button for symbols',
    pinyin: 'Spell the pinyin letter by letter with direction + button',
    lower: 'Type letters one at a time with direction + button',
    upper: 'Type capital letters one at a time with direction + button',
  },
  positions: {
    top1: 'Top 1', top2: 'Top 2', top3: 'Top 3', top4: 'Top 4',
    bot1: 'Bottom 1', bot2: 'Bottom 2', bot3: 'Bottom 3', bot4: 'Bottom 4',
  },
};

// 按键图上写的短名字（圆键里只放得下几个字）
const SHORT = {
  zh: {
    slot0: '候选 1', slot1: '候选 2', slot2: '候选 3', slot3: '候选 4',
    space: '空格', backspace: '退格', enter: '回车', cycle_layer: '切换',
    next_page: '下一页', prev_page: '上一页', escape: '清空', shift: '大写', toggle: '开关', pinyin: '选拼音',
  },
  en: {
    slot0: 'Cand 1', slot1: 'Cand 2', slot2: 'Cand 3', slot3: 'Cand 4',
    space: 'Space', backspace: 'Bksp', enter: 'Enter', cycle_layer: 'Switch',
    next_page: 'Next pg', prev_page: 'Prev pg', escape: 'Clear', shift: 'Shift', toggle: 'On/Off', pinyin: 'Pinyin',
  },
};
const shortName = id => SHORT[LANG][id];

// 输入层：后台给的短名 → [英文短名, 中文全名, 英文全名]
const LAYERS = {
  '九': ['CN', '中文九宫格', 'Chinese T9'],
  '拼': ['PY', '中文全拼', 'Chinese pinyin'],
  '英': ['EN', '英文联想', 'English T9'],
  'abc': ['abc', '小写字母', 'Lowercase'],
  'ABC': ['ABC', '大写字母', 'Uppercase'],
  '123': ['123', '数字符号', 'Numbers & symbols'],
};
const layerShort = l => (LANG === 'en' && LAYERS[l] ? LAYERS[l][0] : l);
const layerName = l => (LAYERS[l] ? LAYERS[l][LANG === 'en' ? 2 : 1] : l);

function t(zh, ...args) {
  const s = LANG === 'en' && EN[zh] != null ? EN[zh] : zh;
  return s.replace(/\{(\d)\}/g, (_, i) => args[i]);
}

/** 后台 meta 换成当前语言（中文就原样返回） */
function localizeMeta(m) {
  if (LANG !== 'en') return m;
  return {
    actions: m.actions.map(([id, name, desc]) => [id, ...(EN_META.actions[id] || [name, desc])]),
    presets: m.presets.map(([id, name]) => [id, EN_META.presets[id] || name]),
    layers: m.layers.map(([id, label, desc]) => [id, label, EN_META.layers[id] || desc]),
    positions: m.positions.map(([id, name, std]) => [id, EN_META.positions[id] || name, std]),
  };
}

function translatePage() {
  document.documentElement.lang = LANG === 'en' ? 'en' : 'zh-CN';
  if (LANG !== 'en') return;
  for (const e of document.querySelectorAll('[data-t]')) if (EN[e.dataset.t] != null) e.innerHTML = EN[e.dataset.t];
  for (const e of document.querySelectorAll('[data-t-title]')) if (EN[e.dataset.tTitle] != null) e.title = EN[e.dataset.tTitle];
}

/** 页面脚本先等它：拿到语言、翻译好页面上的固定文字 */
const LANG_READY = Promise.resolve(invoke('get_lang')).then(l => {
  LANG = l === 'en' ? 'en' : 'zh';
  translatePage();
});
listen('lang', () => location.reload());
