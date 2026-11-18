'use strict';

// ================= 布局 =================

// 方向编号：0↑ 1↗ 2→ 3↘ 4↓ 5↙ 6← 7↖，CENTER = 中位
const CENTER = -1;
const ARROWS = { [CENTER]: '·', 0: '↑', 1: '↗', 2: '→', 3: '↘', 4: '↓', 5: '↙', 6: '←', 7: '↖' };
// 九宫格从左上到右下依次对应的方向
const GRID = [7, 0, 1, 6, CENTER, 2, 5, 4, 3];
// 每个方向对应手机九宫格的哪个键；5(jkl) 在中位选不到，挪到 ↖
const PHONE_KEY = { 7: '5', 0: '2', 1: '3', 6: '4', 2: '6', 5: '7', 4: '8', 3: '9' };

// 每组 4 个字，对应上排 4 个字母键。3 个字母的组，第 4 格放常用标点。
const ABC = { 7: 'jkl!', 0: 'abc,', 1: 'def.', 6: 'ghi?', 2: "mno'", 5: 'pqrs', 4: 'tuv-', 3: 'wxyz' };
const SYM = {
  7: '1()%', 0: '2+-=', 1: '3*/^',
  6: '4@#&', [CENTER]: '50.,', 2: '6:;"',
  5: '7<>_', 4: '8[]~', 3: '9$\\|',
};
const upper = g => Object.fromEntries(Object.entries(g).map(([k, v]) => [k, v.toUpperCase()]));
// 九宫格层只推方向不选字母，每组只是提示这个键上有哪些字母
const T9 = Object.fromEntries(Object.entries(ABC).map(([k, v]) => [k, v.replace(/[^a-z]/g, '')]));
// 九：推一下方向 = 按一次九宫格键，上排 4 个键随时选候选（默认层，实际试下来比拼更顺手）
// 拼：字母和 abc 层一样，只是打进拼音串；中位 + 字母键选候选
const LAYERS = [
  { label: '九', groups: T9, pinyin: true, t9: true },
  { label: '拼', groups: ABC, pinyin: true },
  { label: 'abc', groups: ABC },
  { label: 'ABC', groups: upper(ABC) },
  { label: '123', groups: SYM },
];
const ABC_LAYER = LAYERS.findIndex(l => l.label === 'abc');
const CN_PUNCT = { ',': '，', '.': '。', '?': '？', '!': '！' };
const PAGE_SIZE = 4;
const MAX_COMP = 40;

const FNS = [
  { id: 'L0', label: '字母 1', slot: 0 },
  { id: 'L1', label: '字母 2', slot: 1 },
  { id: 'L2', label: '字母 3', slot: 2 },
  { id: 'L3', label: '字母 4', slot: 3 },
  { id: 'SPACE', label: '空格' },
  { id: 'BKSP', label: '退格' },
  { id: 'LAYER', label: '切换' },
  { id: 'ENTER', label: '回车' },
];
const SLOT_FNS = FNS.filter(f => f.slot != null);

// 街机摇杆常见的 8 键布局，上排 4 个当字母键、下排 4 个当功能键
// XInput（standard 映射）：上排 X Y RB LB，下排 A B RT LT
const XINPUT_MAP = { L0: 2, L1: 3, L2: 5, L3: 4, SPACE: 0, BKSP: 1, LAYER: 7, ENTER: 6 };
// PS4 式 DirectInput（如 Qanba Drone，按钮 0□ 1× 2○ 3△ 4L1 5R1 6L2 7R2）：上排 □ △ R1 L1，下排 × ○ R2 L2
const DINPUT_MAP = { L0: 0, L1: 3, L2: 5, L3: 4, SPACE: 1, BKSP: 2, LAYER: 7, ENTER: 6 };
const KEY_MAP = { L0: 'KeyJ', L1: 'KeyK', L2: 'KeyL', L3: 'Semicolon', SPACE: 'Space', BKSP: 'Backspace', LAYER: 'Tab', ENTER: 'Enter' };
const KEY_LABEL = { L0: 'J', L1: 'K', L2: 'L', L3: ';', SPACE: 'Space', BKSP: '⌫', LAYER: 'Tab', ENTER: 'Enter' };
const KEY_DIRS = {
  ArrowUp: [0, -1], KeyW: [0, -1], ArrowDown: [0, 1], KeyS: [0, 1],
  ArrowLeft: [-1, 0], KeyA: [-1, 0], ArrowRight: [1, 0], KeyD: [1, 0],
};
const NUMPAD_DIRS = { Numpad8: 0, Numpad9: 1, Numpad6: 2, Numpad3: 3, Numpad2: 4, Numpad1: 5, Numpad4: 6, Numpad7: 7 };

// 输入缓冲：松杆后这么久内按键，仍算刚才的方向；在中位按键后这么久内推杆，算推到的方向
const LOOKBACK_MS = 80;
const LOOKAHEAD_MS = 60;
// 九宫格：回中位时，取最后一个停够这么久的方向（从 ↗ 回中位可能擦过 ↑ 或 →，那一下很短）
const FLICK_STABLE_MS = 40;
const REPEAT_DELAY_MS = 400;
const REPEAT_RATE_MS = 60;
const PAGE_REPEAT_MS = 150; // 按住切换 / 回车时翻页的速度
const IDLE_MS = 3000; // 两次输入间隔超过这个就不计入打字时间

const PHRASES = [
  'hello world',
  'the quick brown fox jumps over the lazy dog',
  'beavers build dams on the river',
  'pack my box with five dozen liquor jugs',
  'sphinx of black quartz, judge my vow.',
  'how vexingly quick daft zebras jump!',
  'arcade sticks can type too',
];

// ================= 状态 =================

const STORE_KEY = 'joystick-ime.padMaps'; // { [手柄 id]: 按键映射 }
const savedMaps = loadSavedMaps();
let padMap = { ...XINPUT_MAP };
let padMapFor; // padMap 属于哪个手柄 id
let layer = 0;
let text = '';
const engine = new PinyinEngine(PINYIN_DICT);
let comp = '';   // 还没上屏的拼音
let cands = [];  // comp 的候选
let page = 0;
let practice = false;
let phraseIdx = 0;
let phraseStart = 0;
const results = [];

let prevPressed = new Set();
let prevRaw = [];
let skipEdges = false;
let lastDir = { dir: CENTER, t: -Infinity };
let pending = [];
let flick = null; // 九宫格：摇杆离开中位到回中位这一下，{ segs: [{ dir, t }], done }
let bkspRepeatAt = 0;
let pageRepeatAt = 0;
let activePadIndex = null;
let calib = null;
const stats = { chars: 0, activeMs: 0, lastT: 0 };
const keysDown = new Set();

function loadSavedMaps() {
  try {
    return JSON.parse(localStorage.getItem(STORE_KEY) || '{}');
  } catch {
    return {};
  }
}

// 换了手柄就换成它的映射：存过的优先，否则按 standard / 非标准挑默认值
function syncPadMap(pad) {
  const id = pad ? pad.id : null;
  if (id === padMapFor) return;
  padMapFor = id;
  const base = pad && pad.mapping !== 'standard' ? DINPUT_MAP : XINPUT_MAP;
  padMap = { ...base, ...(savedMaps[id] || {}) };
  renderLegend();
}

// ================= 读取输入 =================

const isDown = b => !!b && (b.pressed || b.value > 0.5);

function vecToDir(x, y) {
  if (Math.hypot(x, y) < 0.5) return CENTER;
  const a = Math.atan2(x, -y); // 0 = 上，顺时针为正
  return ((Math.round(a / (Math.PI / 4)) % 8) + 8) % 8;
}

function padDir(pad) {
  const b = i => isDown(pad.buttons[i]);
  if (pad.mapping === 'standard') {
    const x = b(15) - b(14), y = b(13) - b(12);
    if (x || y) return { dir: vecToDir(x, y), src: '十字键' };
  }
  if (pad.axes.length >= 2) {
    const d = vecToDir(pad.axes[0], pad.axes[1]);
    if (d !== CENTER) return { dir: d, src: '左摇杆轴' };
  }
  // DirectInput 设备的苦力帽在 Chrome 里通常是 axes[9]：-1 = ↑，顺时针每格 +2/7，中位 > 1
  if (pad.mapping !== 'standard' && pad.axes.length > 9) {
    const v = pad.axes[9];
    if (v >= -1.05 && v <= 1.05) return { dir: Math.round((v + 1) * 3.5) % 8, src: '苦力帽 axes[9]' };
  }
  return { dir: CENTER, src: '' };
}

function pickPad() {
  const pads = navigator.getGamepads ? [...navigator.getGamepads()].filter(Boolean) : [];
  for (const p of pads) {
    if (p.buttons.some(isDown) || padDir(p).dir !== CENTER) activePadIndex = p.index;
  }
  return pads.find(p => p.index === activePadIndex) || pads[0] || null;
}

function keyDir() {
  let x = 0, y = 0;
  for (const [code, [dx, dy]] of Object.entries(KEY_DIRS)) {
    if (keysDown.has(code)) { x += dx; y += dy; }
  }
  const d = vecToDir(Math.sign(x), Math.sign(y));
  if (d !== CENTER) return d;
  for (const [code, nd] of Object.entries(NUMPAD_DIRS)) if (keysDown.has(code)) return nd;
  return CENTER;
}

const HANDLED_CODES = new Set([...Object.keys(KEY_DIRS), ...Object.keys(NUMPAD_DIRS), ...Object.values(KEY_MAP)]);

addEventListener('keydown', e => {
  if (e.code === 'Escape' && calib) { endCalib(false); return; }
  if (!HANDLED_CODES.has(e.code) || e.ctrlKey || e.metaKey || e.altKey) return;
  e.preventDefault();
  keysDown.add(e.code);
});
addEventListener('keyup', e => keysDown.delete(e.code));
addEventListener('blur', () => keysDown.clear());

// ================= 主循环 =================

function frame(now) {
  tick(now);
  requestAnimationFrame(frame);
}

function tick(now) {
  const pad = pickPad();
  if (!calib) syncPadMap(pad);
  let { dir, src } = pad ? padDir(pad) : { dir: CENTER, src: '' };
  const pressed = new Set();
  if (pad) for (const f of FNS) if (isDown(pad.buttons[padMap[f.id]])) pressed.add(f.id);

  if (dir === CENTER) {
    const kd = keyDir();
    if (kd !== CENTER) { dir = kd; src = '键盘'; }
  }
  for (const f of FNS) if (keysDown.has(KEY_MAP[f.id])) pressed.add(f.id);

  const raw = pad ? pad.buttons.map(isDown) : [];
  if (calib) stepCalib(pad, raw);
  else if (skipEdges) skipEdges = false; // 刚改完映射，按住的那个键不算一次输入
  else handleInput(now, dir, pressed);
  prevPressed = pressed;
  prevRaw = raw;

  draw(pad, dir, src, pressed, raw);
}

function handleInput(now, dir, pressed) {
  const t9 = LAYERS[layer].t9;
  if (dir !== CENTER) lastDir = { dir, t: now };
  if (t9) trackFlick(now, dir);

  for (const f of FNS) {
    if (!pressed.has(f.id) || prevPressed.has(f.id)) continue;
    // 九宫格：摇杆还没回中位就按了键，先把这一推记上，再处理按键
    if (t9 && flick && !flick.done) {
      t9Key(resolveFlick(flick, now));
      flick.done = true;
    }
    if (f.slot != null && t9) {
      pickCandidate(page * PAGE_SIZE + f.slot);
    } else if (f.slot != null) {
      if (dir !== CENTER) commit(dir, f.slot);
      else if (now - lastDir.t <= LOOKBACK_MS) commit(lastDir.dir, f.slot);
      else pending.push({ slot: f.slot, t: now });
    } else if (f.id === 'SPACE') {
      if (comp) pickCandidate(page * PAGE_SIZE); else typeChar(' ');
    } else if (f.id === 'BKSP') {
      backspace();
      bkspRepeatAt = now + REPEAT_DELAY_MS;
    } else if (f.id === 'LAYER' || f.id === 'ENTER') {
      // 打字时：切换 = 下一页，回车 = 上一页，按住都能连翻。拼音层没有候选时，回车把字母原样上屏
      if (comp && f.id === 'ENTER' && !cands.length && !t9) {
        commitRaw();
      } else if (comp) {
        flipPage(f.id === 'LAYER' ? 1 : -1);
        pageRepeatAt = now + REPEAT_DELAY_MS;
      } else if (f.id === 'LAYER') {
        setLayer((layer + 1) % LAYERS.length);
      } else if (practice) {
        nextPhrase();
      } else {
        typeChar('\n');
      }
    }
  }

  pending = pending.filter(p => {
    if (dir !== CENTER) { commit(dir, p.slot); return false; }
    if (now - p.t > LOOKAHEAD_MS) { commit(CENTER, p.slot); return false; }
    return true;
  });

  if (pressed.has('BKSP') && prevPressed.has('BKSP') && now >= bkspRepeatAt) {
    backspace();
    bkspRepeatAt = now + REPEAT_RATE_MS;
  }
  // 按住切换 / 回车：连续往后 / 往前翻（只在打字时；两个一起按住就不动）
  const held = id => pressed.has(id) && prevPressed.has(id);
  const step = (held('LAYER') ? 1 : 0) - (held('ENTER') ? 1 : 0);
  if (comp && step && now >= pageRepeatAt) {
    flipPage(step);
    pageRepeatAt = now + PAGE_REPEAT_MS;
  }
}

function trackFlick(now, dir) {
  if (dir !== CENTER) {
    if (!flick) flick = { segs: [], done: false };
    if (flick.segs.at(-1)?.dir !== dir) flick.segs.push({ dir, t: now });
  } else if (flick) {
    if (!flick.done) t9Key(resolveFlick(flick, now));
    flick = null;
  }
}

function resolveFlick(fl, now) {
  let longest = fl.segs[0], longestDur = 0;
  for (let k = fl.segs.length - 1; k >= 0; k--) {
    const dur = (k + 1 < fl.segs.length ? fl.segs[k + 1].t : now) - fl.segs[k].t;
    if (dur >= FLICK_STABLE_MS) return fl.segs[k].dir;
    if (dur > longestDur) { longestDur = dur; longest = fl.segs[k]; }
  }
  return longest.dir;
}

function t9Key(dir) {
  pulse(cellEl(dir), 'tap');
  if (comp.length < MAX_COMP) { comp += PHONE_KEY[dir]; updateComp(); }
}

// ================= 编辑 =================

function commit(dir, slot) {
  const L = LAYERS[layer];
  if (L.pinyin && dir === CENTER && comp) {
    pulse(charEl(dir, slot), 'flash');
    pickCandidate(page * PAGE_SIZE + slot);
    return;
  }
  const ch = L.groups[dir]?.[slot];
  if (!ch) { pulse(cellEl(dir), 'miss'); return; }
  pulse(charEl(dir, slot), 'flash');
  if (L.pinyin) pinyinKey(ch); else typeChar(ch);
}

function typeChar(ch) {
  const now = performance.now();
  if (stats.lastT && now - stats.lastT < IDLE_MS) stats.activeMs += now - stats.lastT;
  stats.lastT = now;
  stats.chars += [...ch].length;
  if (practice && text.length === 0) phraseStart = now;
  text += ch;
  if (practice && text === PHRASES[phraseIdx]) finishPhrase(now);
  renderText();
}

function backspace() {
  if (comp) {
    comp = comp.slice(0, -1);
    updateComp();
  } else if (text) {
    text = [...text].slice(0, -1).join('');
    renderText();
  }
}

// ================= 拼音 =================

function pinyinKey(ch) {
  if (/[a-z]/.test(ch)) {
    if (comp.length < MAX_COMP) { comp += ch; updateComp(); }
  } else if (ch === "'" && comp) {
    if (!comp.endsWith("'")) { comp += ch; updateComp(); } // 音节分隔：xi'an
  } else {
    commitAll(); // 标点：先把拼音整段上屏
    typeChar(CN_PUNCT[ch] || ch);
  }
}

function updateComp() {
  cands = comp ? engine.query(comp, LAYERS[layer].t9 ? 't9' : 'letters') : [];
  page = 0;
  renderGrid();
  renderText();
}

function pickCandidate(i) {
  const c = cands[i];
  if (!c) { pulse(cellEl(CENTER), 'miss'); return; }
  comp = comp.slice(c.rawLen);
  typeChar(c.w);
  updateComp();
}

// 依次选第一个候选，直到拼音串打完；查不到的部分按字母原样上屏
function commitAll() {
  for (let guard = 0; comp && guard < MAX_COMP; guard++) {
    if (!cands.length) break;
    pickCandidate(0);
  }
  if (comp) commitRaw();
}

// 把拼音串按字母原样上屏（拼音层查不到候选时）
function commitRaw() {
  const raw = compText().replace(/'/g, '');
  comp = '';
  if (raw) typeChar(raw);
  updateComp();
}

// 翻候选：step = 1 下一页，-1 上一页，两头循环（第 1 页往前翻到最后一页）
function flipPage(step) {
  const pages = Math.max(1, Math.ceil(cands.length / PAGE_SIZE));
  page = (((page + step) % pages) + pages) % pages;
  renderGrid();
  renderText();
}

const pageCands = () => cands.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);

// 显示用的拼音串：九宫格模式下是数字，换成第一个候选的读音，没覆盖到的部分仍显示数字
function compText() {
  if (!LAYERS[layer].t9 || !cands.length) return comp;
  const c = cands[0];
  return c.reading + (c.rawLen < comp.length ? "'" + comp.slice(c.rawLen) : '');
}

function setLayer(i) {
  layer = i;
  flick = null;
  $('#layer').textContent = LAYERS[layer].label;
  renderGrid();
}

function finishPhrase(now) {
  const secs = (now - phraseStart) / 1000;
  results.unshift({ wpm: text.length / 5 / (secs / 60), secs });
  nextPhrase();
}

function nextPhrase() {
  phraseIdx = (phraseIdx + 1) % PHRASES.length;
  resetText(ABC_LAYER);
}

function resetText(toLayer) {
  text = '';
  comp = '';
  setLayer(toLayer);
  updateComp();
}

// ================= 按键设置 =================

function startCalib(list) {
  calib = { list, step: 0, backup: { ...padMap }, msg: '' };
  renderLegend();
}

function stepCalib(pad, raw) {
  if (!pad) return;
  const fn = calib.list[calib.step];
  for (let i = 0; i < raw.length; i++) {
    if (!raw[i] || prevRaw[i]) continue;
    if (pad.mapping === 'standard' && i >= 12 && i <= 15) continue; // 十字键是方向
    const other = FNS.find(f => f !== fn && padMap[f.id] === i);
    if (other && calib.list.slice(0, calib.step).includes(other)) {
      calib.msg = `#${i} 刚设成了「${other.label}」，换一个`;
      return;
    }
    if (other) padMap[other.id] = padMap[fn.id]; // 和原来占着这个按钮的功能对调
    padMap[fn.id] = i;
    calib.msg = '';
    calib.step++;
    if (calib.step >= calib.list.length) endCalib(true);
    else renderLegend();
    return;
  }
}

function endCalib(save) {
  if (save && padMapFor) {
    savedMaps[padMapFor] = padMap;
    localStorage.setItem(STORE_KEY, JSON.stringify(savedMaps));
  } else {
    padMap = calib.backup;
  }
  calib = null;
  skipEdges = true;
  renderLegend();
}

// ================= 练习提示 =================

function findChar(ch) {
  const order = [layer, ...LAYERS.keys()];
  for (const li of order) {
    if (LAYERS[li].pinyin) continue; // 练习的是英文，拼音层打不出字母
    for (const [d, g] of Object.entries(LAYERS[li].groups)) {
      const s = g.indexOf(ch);
      if (s >= 0) return { layer: li, dir: +d, slot: s };
    }
  }
  return null;
}

function currentHint() {
  if (!practice) return null;
  const target = PHRASES[phraseIdx];
  let i = 0;
  while (i < text.length && text[i] === target[i]) i++;
  if (i < text.length) return { fn: 'BKSP' };
  const ch = target[i];
  if (ch == null) return null;
  if (ch === ' ') return { fn: 'SPACE' };
  const loc = findChar(ch);
  if (!loc) return null;
  if (loc.layer !== layer) return { fn: 'LAYER' };
  return { fn: SLOT_FNS[loc.slot].id, dir: loc.dir, slot: loc.slot };
}

// ================= 渲染 =================

const $ = s => document.querySelector(s);
const gridEl = $('#grid');
const legendEl = $('#legend');

function el(tag, cls, txt) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (txt != null) e.textContent = txt;
  return e;
}

function pulse(e, cls) {
  if (!e) return;
  e.classList.remove(cls);
  void e.offsetWidth; // 重新触发动画
  e.classList.add(cls);
}

const cellEl = d => gridEl.querySelector(`.cell[data-dir="${d}"]`);
const charEl = (d, s) => cellEl(d)?.querySelector(`.ch[data-slot="${s}"]`);

function renderGrid() {
  const L = LAYERS[layer];
  gridEl.replaceChildren();
  for (const d of GRID) {
    // 拼音层打字时，中位那格放当前这页候选
    const words = L.pinyin && d === CENTER && comp ? pageCands().map(c => c.w) : null;
    const g = words || (L.groups[d] ? [...L.groups[d]] : null);
    const cell = el('div', 'cell');
    cell.dataset.dir = d;
    if (d === CENTER) cell.classList.add('center');
    if (g) cell.classList.add('has-chars');
    if (!L.groups[CENTER] && PHONE_KEY[d]) cell.append(el('span', 'key', PHONE_KEY[d]));
    const arrow = el('div', 'arrow', ARROWS[d]);
    if (d === CENTER) arrow.id = 'live-arrow';
    cell.append(arrow);
    const chars = el('div', words ? 'chars words' : 'chars');
    if (g) {
      g.forEach((c, s) => {
        // 九宫格层的字母不用按键选，不标按键颜色
        const sp = el('span', `ch ${L.t9 && !words ? 'plain' : 's' + s}${words ? ' word' : ''}`, c);
        sp.dataset.slot = s;
        chars.append(sp);
      });
    }
    cell.append(chars);
    gridEl.append(cell);
  }
}

function renderLegend() {
  legendEl.replaceChildren();
  for (const f of FNS) {
    const b = el('button', `btn ${f.slot != null ? 's' + f.slot : 'fn'}`);
    b.dataset.fn = f.id;
    b.title = '点击单独重设这个按键对应的手柄按钮';
    b.append(el('b', '', f.label), el('small', '', `手柄 #${padMap[f.id] ?? '—'} · 键盘 ${KEY_LABEL[f.id]}`));
    b.addEventListener('click', () => startCalib([f]));
    legendEl.append(b);
  }
}

function renderText() {
  const out = $('#output');
  out.replaceChildren(document.createTextNode(text), el('span', 'comp', compText()), el('span', 'cursor'));
  renderCands();

  if (!practice) {
    const mins = stats.activeMs / 60000;
    $('#stats').textContent = stats.chars >= 5 && mins > 0
      ? `已输入 ${stats.chars} 字 · 约 ${Math.round(stats.chars / mins)} 字/分钟`
      : `已输入 ${stats.chars} 字`;
    return;
  }

  const target = PHRASES[phraseIdx];
  const tEl = $('#target');
  tEl.replaceChildren();
  let ok = true;
  [...target].forEach((c, i) => {
    let cls = 'todo';
    if (i < text.length) {
      ok = ok && text[i] === c;
      cls = ok ? 'ok' : 'bad';
    } else if (i === text.length) {
      cls = 'cur';
    }
    tEl.append(el('span', cls, c));
  });
  $('#practice-progress').textContent = `第 ${phraseIdx + 1} / ${PHRASES.length} 句 · Enter 跳过`;
  $('#results').innerHTML = results.length
    ? '最近：' + results.slice(0, 5).map(r => `<b>${r.wpm.toFixed(1)}</b> WPM (${r.secs.toFixed(1)}s)`).join(' · ')
    : '打完一句会在这里显示速度';
  $('#stats').textContent = '';
}

function renderCands() {
  const box = $('#ime');
  box.hidden = !comp;
  if (!comp) return;
  const t9 = LAYERS[layer].t9;
  const pages = Math.max(1, Math.ceil(cands.length / PAGE_SIZE));
  $('#comp').textContent = t9 ? `${compText()}　键 ${comp}` : comp;
  $('#page').textContent = cands.length
    ? `${page + 1} / ${pages} 页 · 切换下一页 · 回车上一页`
    : (t9 ? '没有候选' : '没有候选，回车按字母上屏');
  const list = $('#cands');
  list.replaceChildren();
  pageCands().forEach((c, s) => {
    const item = el('span', `cand s${s}`);
    item.append(el('i', '', String(s + 1)), document.createTextNode(c.w));
    list.append(item);
  });
}

// 拼音层打字时，功能键的含义会变
function fnLabel(f) {
  const t9 = LAYERS[layer].t9;
  if (t9 && f.slot != null) return `候选 ${f.slot + 1}`;
  if (!comp) return f.label;
  if (f.id === 'ENTER' && !cands.length && !t9) return '字母上屏';
  return {
    SPACE: '选第 1 个',
    BKSP: t9 ? '删键' : '删拼音',
    LAYER: '下一页',
    ENTER: '上一页',
  }[f.id] || `${f.label} / 候选 ${f.slot + 1}`;
}

let lastStatus = '';
function setStatus(msg, cls) {
  if (msg === lastStatus) return;
  lastStatus = msg;
  const s = $('#status');
  s.textContent = msg;
  s.className = cls || '';
}

function draw(pad, dir, src, pressed, raw) {
  const hint = currentHint();

  for (const cell of gridEl.children) {
    const d = +cell.dataset.dir;
    cell.classList.toggle('active', d === dir);
    cell.classList.toggle('hint', !!hint && hint.dir === d);
    for (const ch of cell.querySelectorAll('.ch')) {
      ch.classList.toggle('hint', !!hint && hint.dir === d && hint.slot === +ch.dataset.slot);
    }
  }
  $('#live-arrow').textContent = ARROWS[dir];

  for (const b of legendEl.children) {
    const id = b.dataset.fn;
    b.classList.toggle('down', pressed.has(id));
    b.classList.toggle('hint', !!hint && hint.fn === id);
    b.classList.toggle('calib', !!calib && calib.list[calib.step]?.id === id);
    const label = fnLabel(FNS.find(f => f.id === id));
    if (b.firstChild.textContent !== label) b.firstChild.textContent = label;
  }

  if (calib) {
    const fn = calib.list[calib.step];
    setStatus(pad
      ? `请按手柄上要当作「${fn.label}」的按钮（${calib.step + 1}/${calib.list.length}），Esc 取消${calib.msg ? ' · ' + calib.msg : ''}`
      : '没有检测到手柄，无法设置按键（Esc 取消）', 'calib');
  } else if (pad) {
    setStatus(`已连接：${pad.id}${pad.mapping === 'standard' ? '' : '（非标准映射）'}`, 'ok');
  } else {
    setStatus('未检测到手柄：先按一下摇杆上的任意按钮（浏览器要有一次按键才会识别手柄）；也可以用键盘试');
  }

  if ($('#debug').open && pad) {
    const btns = raw.map((d, i) => (d ? i : null)).filter(i => i !== null);
    $('#raw').textContent = [
      `id: ${pad.id}`,
      `mapping: ${pad.mapping || '（非标准）'}`,
      `axes: ${pad.axes.map((v, i) => `[${i}] ${v.toFixed(2)}`).join('  ')}`,
      `按下的按钮: ${btns.join(', ') || '无'}`,
      `方向: ${ARROWS[dir]} ${src}`,
    ].join('\n');
  }
}

// ================= 启动 =================

$('#btn-practice').addEventListener('click', e => {
  practice = !practice;
  e.currentTarget.classList.toggle('on', practice);
  $('#practice').hidden = !practice;
  resetText(practice ? ABC_LAYER : 0);
});
$('#btn-calib').addEventListener('click', () => startCalib(FNS));
$('#btn-copy').addEventListener('click', () => navigator.clipboard.writeText(text));
$('#btn-clear').addEventListener('click', () => resetText(layer));
// 点完按钮把焦点拿走，免得空格/回车又触发它
document.addEventListener('click', e => { if (e.target.closest('button')) document.activeElement.blur(); });

renderGrid();
renderLegend();
renderText();
requestAnimationFrame(frame);
