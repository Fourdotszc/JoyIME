'use strict';

const $ = id => document.getElementById(id);
const SVGNS = 'http://www.w3.org/2000/svg';

// 按键上写的短名字
const SHORT = {
  slot0: '候选 1', slot1: '候选 2', slot2: '候选 3', slot3: '候选 4',
  space: '空格', backspace: '退格', enter: '回车', cycle_layer: '切换',
  next_page: '下一页', prev_page: '上一页', escape: '清空', shift: '大写', toggle: '开关', pinyin: '选拼音',
};
// 方向编号 → 单位向量（右、下为正）
const DIR_VEC = [[0, -1], [0.71, -0.71], [1, 0], [0.71, 0.71], [0, 1], [-0.71, 0.71], [-1, 0], [-0.71, -0.71]];

// 线稿里每个位置的形状和坐标（viewBox 760×400）。街机按 Vewlix 排布：上排、下排各 4 个，下排往左错开
const LAYOUT = {
  top1: { c: [390, 190] }, top2: { c: [468, 172] }, top3: { c: [548, 172] }, top4: { c: [626, 186] },
  bot1: { c: [372, 270] }, bot2: { c: [450, 252] }, bot3: { c: [530, 252] }, bot4: { c: [608, 266] },
  select: { r: [478, 66, 56, 20] }, start: { r: [546, 66, 56, 20] }, home: { c: [640, 76], small: true },
};
const GATE = [180, 222];

let meta = null;
let pad = null;
let view = null;
let selCode = null;
let builtFor = '';
let typed = '';

const actionName = id => (meta.actions.find(a => a[0] === id) || [id, id])[1];
const posLabel = pos => (meta.positions.find(p => p[0] === pos) || [pos, pos])[1];

/** 按键在摇杆上的位置：检测过的优先，否则按标准名字推断 */
function positionOf(b) {
  const learned = pad.profile.names[b.code];
  if (learned) return learned;
  const p = b.std_name && meta.positions.find(p => p[2] === b.std_name);
  return p ? p[0] : null;
}
const buttonAt = pos => pad.buttons.find(b => positionOf(b) === pos) || null;
const keyLabel = b => {
  const pos = positionOf(b);
  return pos ? posLabel(pos) : `按键 #${b.code}`;
};

function svg(tag, attrs, text) {
  const e = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs || {})) e.setAttribute(k, v);
  if (text != null) e.textContent = text;
  return e;
}

// ---------- 页签 ----------
$('tabs').addEventListener('click', e => {
  const b = e.target.closest('button');
  if (!b) return;
  for (const x of $('tabs').children) x.classList.toggle('on', x === b);
  for (const p of document.querySelectorAll('.page')) p.classList.toggle('on', p.id === 'page-' + b.dataset.page);
});

// 地址里带 #input 之类就直接打开那一页（浏览器预览、截图用）
if (location.hash) document.querySelector(`#tabs [data-page="${location.hash.slice(1)}"]`)?.click();

$('power').addEventListener('change', e => invoke('set_enabled', { on: e.target.checked }));

// ---------- 摇杆线稿 ----------
/** 画整张摇杆。opts.target：向导里正在等的位置；opts.interactive：能不能点 */
function drawStick(root, opts = {}) {
  root.replaceChildren();
  root.append(svg('rect', { class: 'body', x: 30, y: 46, width: 700, height: 318, rx: 26 }));

  // 摇杆：八角限位 + 方向上的字母 + 球
  const [gx, gy] = GATE;
  const oct = [];
  for (let k = 0; k < 8; k++) {
    const a = (Math.PI / 8) * (2 * k + 1);
    oct.push(`${gx + 74 * Math.cos(a)},${gy + 74 * Math.sin(a)}`);
  }
  root.append(svg('polygon', { class: 'gate', points: oct.join(' ') }));
  for (let d = 0; d < 8; d++) {
    const [x, y] = DIR_VEC[d];
    root.append(svg('text', { class: 'gate-dir', 'data-dir': d, x: gx + x * 100, y: gy + y * 100 }));
  }
  const ball = svg('g', { id: opts.interactive ? 'ball' : '' });
  ball.append(svg('circle', { class: 'ball', cx: gx, cy: gy, r: 30 }), svg('circle', { class: 'ball-dot', cx: gx, cy: gy, r: 4 }));
  root.append(ball);

  for (const [pos, g] of Object.entries(LAYOUT)) {
    const b = buttonAt(pos);
    const k = svg('g', { class: 'key' });
    k.dataset.pos = pos;
    if (!b) k.classList.add('missing');
    else {
      k.dataset.code = b.code;
      if (!b.action) k.classList.add('none');
      if (b.code === selCode) k.classList.add('sel');
    }
    if (pos === opts.target) k.classList.add('target');
    const label = b ? SHORT[b.action] || '未分配' : '';
    if (g.c) {
      const [cx, cy] = g.c;
      const r = g.small ? 12 : 32;
      k.append(svg('circle', { cx, cy, r }));
      if (g.small) {
        k.append(svg('text', { class: 'pos', x: cx, y: cy - 22 }, 'Home'), svg('text', { x: cx, y: cy + 26 }, b ? label : ''));
      } else {
        k.append(svg('text', { x: cx, y: cy }, label));
        // 上排的位置名写在上面，下排写在下面，免得和另一排挤在一起
        const below = pos.startsWith('bot');
        k.append(svg('text', { class: 'pos', x: cx, y: below ? cy + 46 : cy - 46 }, posLabel(pos)));
      }
    } else {
      const [x, y, w, h] = g.r;
      k.append(svg('rect', { x, y, width: w, height: h, rx: 10 }));
      k.append(svg('text', { class: 'pos', x: x + w / 2, y: y + h / 2 }, pos === 'select' ? 'Select' : 'Start'));
      k.append(svg('text', { x: x + w / 2, y: y + h + 16 }, b ? label : ''));
    }
    if (opts.interactive && b) k.addEventListener('click', () => select(b.code));
    root.append(k);
  }
}

/** 只更新会动的部分：方向、球、按下的键 */
function updateStick(root) {
  const down = new Set(pad.buttons.filter(b => b.pressed).map(b => String(b.code)));
  for (const k of root.querySelectorAll('.key[data-code]')) k.classList.toggle('down', down.has(k.dataset.code));
  for (const t of root.querySelectorAll('.gate-dir')) {
    const d = +t.dataset.dir;
    const i = GRID_DIRS.indexOf(d);
    t.textContent = (view && view.grid[i]) || PHONE_KEYS[d];
    t.classList.toggle('on', pad.dir === d);
  }
  const ball = root.querySelector('#ball');
  if (ball) {
    const [x, y] = pad.dir == null ? [0, 0] : DIR_VEC[pad.dir];
    ball.setAttribute('transform', `translate(${x * 40} ${y * 40})`);
  }
}

// ---------- 右侧检查器 ----------
function renderInspector() {
  const box = $('inspector');
  box.replaceChildren();
  if (!pad.connected) {
    const h = el('div', 'ins-head');
    h.append(el('b', '', '没有摇杆'), el('p', '', '插上摇杆后按一下任意键。'));
    box.append(h);
    return;
  }
  const b = pad.buttons.find(x => x.code === selCode);
  if (!b) return renderOverview(box);

  const head = el('div', 'ins-head');
  const back = el('button', 'link', '← 全部按键');
  back.onclick = () => select(null);
  head.append(back, el('b', '', keyLabel(b)), el('span', '', `#${b.code}${b.std_name ? ' · ' + b.std_name : ''}`));
  box.append(head);

  const owner = id => pad.buttons.find(x => x.action === id && x.code !== b.code);
  const options = [...meta.actions, ['', '不分配', '这个键什么都不做']];
  for (const [id, name, desc] of options) {
    const row = el('label', 'opt');
    const r = el('input');
    r.type = 'radio';
    r.name = 'action';
    r.checked = (b.action || '') === id;
    r.onchange = () => invoke('set_binding', { code: b.code, action: id || null });
    const o = id && owner(id);
    const t = el('b', '', name);
    if (o) {
      row.classList.add('taken');
      t.dataset.taken = `现在在${keyLabel(o)}`;
    }
    row.append(r, t, el('small', '', desc));
    box.append(row);
  }
}

function renderOverview(box) {
  const head = el('div', 'ins-head');
  head.append(el('b', '', '按键分配'), el('p', '', '点图上的按键或下面一行，给它换功能。'));
  box.append(head);
  const list = el('div', 'overview');
  const placed = new Set();
  for (const [pos] of meta.positions) {
    const b = buttonAt(pos);
    if (!b) continue;
    placed.add(b.code);
    list.append(overviewRow(b, posLabel(pos)));
  }
  const others = pad.buttons.filter(b => !placed.has(b.code));
  if (others.length) {
    list.append(el('div', 'ov-sec', '其他按键'));
    for (const b of others) list.append(overviewRow(b, `#${b.code}${b.std_name ? ' ' + b.std_name : ''}`));
  }
  box.append(list);
}

function overviewRow(b, label) {
  const r = el('div', 'ov-row');
  r.dataset.code = b.code;
  r.append(el('span', '', label), el('span', b.action ? '' : 'unset', b.action ? actionName(b.action) : '未分配'));
  r.onclick = () => select(b.code);
  return r;
}

function select(code) {
  selCode = code;
  renderInspector();
  for (const k of $('stick').querySelectorAll('.key')) k.classList.toggle('sel', k.dataset.code === String(code));
}

// ---------- 顶栏、工具栏 ----------
function renderChrome() {
  $('device').classList.toggle('on', pad.connected);
  $('dev-name').textContent = pad.connected ? pad.name : '未连接摇杆';
  $('btn-detect').disabled = !pad.connected;

  const seg = $('presets');
  seg.replaceChildren();
  for (const [id, name] of meta.presets) {
    const b = el('button', pad.profile.preset === id ? 'on' : '', name.replace('街机摇杆 · ', '街机 '));
    b.disabled = !pad.connected;
    b.onclick = () => invoke('apply_preset', { preset: id });
    seg.append(b);
  }
  if (pad.profile.preset === 'custom') seg.append(el('span', 'custom', '已自定义'));

  const n = pad.buttons.length;
  const learned = Object.keys(pad.profile.names).length;
  const how = { sdl: '已识别型号', driver: '系统驱动映射', none: '未识别型号，建议先「检测按键」' }[pad.mapping];
  $('canvas-note').textContent = pad.connected ? `${how} · ${n} 个按键${learned ? ` · 检测过 ${learned} 个位置` : ''} · 每页 ${pad.slots} 个候选` : '';
}

function onPad(p) {
  pad = p;
  const sig = JSON.stringify([p.connected, p.key, p.profile, p.buttons.map(b => [b.code, b.action, b.std_name]), p.slots]);
  if (sig !== builtFor) {
    builtFor = sig;
    renderChrome();
    drawStick($('stick'), { interactive: true });
    renderInspector();
  }
  updateStick($('stick'));
  const down = new Set(p.buttons.filter(b => b.pressed).map(b => String(b.code)));
  for (const r of document.querySelectorAll('.ov-row')) r.classList.toggle('down', down.has(r.dataset.code));
  $('dir-status').textContent = p.connected
    ? `方向 ${p.dir == null ? '中位' : '↑↗→↘↓↙←↖'[p.dir]}${p.dir_source ? ' · ' + p.dir_source : ''}`
    : '方向 —';
  wizardStep();
}

// ---------- 练习页 + 顶栏开关 ----------
function onView(v) {
  view = v;
  $('power').checked = v.enabled;
  $('power-label').textContent = v.enabled ? '已开启' : '已暂停';
  $('layer-order').textContent = v.layers.join(' → ');

  const grid = $('pgrid');
  grid.replaceChildren();
  GRID_DIRS.forEach((d, i) => {
    if (d === null) {
      grid.append(el('div', 'pcell center', LAYER_NAMES[v.layer] || v.layer));
      return;
    }
    const c = el('div', 'pcell' + (d === v.dir ? ' on' : ''));
    c.append(el('i', '', PHONE_KEYS[d]), document.createTextNode(v.grid[i] || ''));
    grid.append(c);
  });

  $('pmeta').textContent = v.comp && v.pages > 1 ? `第 ${v.page + 1} / ${v.pages} 页 · 切换下一页 · 回车上一页` : '';
  renderPractice();
  if (pad) updateStick($('stick'));
}

/** 练习区：打出的字 + 正在打的读音（下划线）+ 光标，候选框贴在光标下面，和在别的程序里一样 */
function renderPractice() {
  const box = $('ptext');
  box.replaceChildren(document.createTextNode(typed));
  const composing = view && view.enabled && view.comp;
  if (composing) box.append(el('span', 'pcompose', view.comp_display));
  const caret = el('span', 'pcaret');
  box.append(caret);
  if (!composing) return;
  const pop = el('div', 'ppop' + (view.py_focus ? ' pyfocus' : ''));
  const bar = pinyinBar(view);
  if (bar) pop.append(bar);
  const list = el('ol');
  view.cands.forEach((w, k) => {
    const li = el('li', '', w);
    li.dataset.n = k + 1;
    list.append(li);
  });
  pop.append(list);
  box.append(pop);
  // 放在光标下方；右边放不下就往左挪
  const left = Math.min(caret.offsetLeft, box.clientWidth - pop.offsetWidth - 8);
  pop.style.left = `${Math.max(8, left)}px`;
  pop.style.top = `${caret.offsetTop + caret.offsetHeight + 4}px`;
}

listen('typed', s => {
  for (const ch of s) typed = ch === '\b' ? [...typed].slice(0, -1).join('') : typed + ch;
  renderPractice();
});
$('pclear').onclick = () => { typed = ''; renderPractice(); };

// ---------- 输入设置 ----------
let settings = null;

async function loadSettings() {
  settings = await invoke('get_settings');
  $('set-hint').checked = settings.hint;
  $('set-hint-secs').value = String(settings.hint_secs);
  $('set-follow').value = settings.follow_caret ? 'caret' : 'bottom';
  $('set-hint-secs').disabled = !settings.hint;
  $('set-punct').value = settings.cn_fullwidth ? 'full' : 'half';
  $('set-output').value = settings.output;
  $('set-learn').checked = settings.learn;
  refreshUserDictCount();
  renderLayerRows();
}

// 每种输入方式一行开关；至少要留一个
function renderLayerRows() {
  const box = $('layer-rows');
  box.replaceChildren();
  for (const [id, label, desc] of meta.layers) {
    const row = el('div', 'row');
    const text = el('div');
    text.append(el('b', '', `${label}　${LAYER_NAMES[label] || ''}`), el('p', '', desc));
    const sw = el('label', 'switch-row');
    const cb = el('input');
    cb.type = 'checkbox';
    cb.checked = settings.layers.includes(id);
    cb.onchange = () => {
      const on = meta.layers.map(l => l[0]).filter(x => (x === id ? cb.checked : settings.layers.includes(x)));
      if (!on.length) {
        cb.checked = true; // 一个都不开就没法打字了
        return;
      }
      settings.layers = on;
      saveSettings();
    };
    sw.append(cb, el('i', 'switch'));
    row.append(text, sw);
    box.append(row);
  }
}

function saveSettings() {
  $('set-hint-secs').disabled = !$('set-hint').checked;
  Object.assign(settings, {
    hint: $('set-hint').checked,
    hint_secs: +$('set-hint-secs').value,
    follow_caret: $('set-follow').value === 'caret',
    cn_fullwidth: $('set-punct').value === 'full',
    output: $('set-output').value,
    learn: $('set-learn').checked,
  });
  invoke('set_settings', { settings });
}
for (const id of ['set-hint', 'set-hint-secs', 'set-follow', 'set-punct', 'set-output', 'set-learn']) $(id).addEventListener('change', saveSettings);

async function refreshUserDictCount() {
  const n = await invoke('userdict_len');
  $('userdict-count').textContent = n ? `${n} 条` : '还没有';
  $('btn-clear-userdict').disabled = !n;
}

// 清除是删用户的数据：点第一下变成「再点一次确认」，3 秒内再点才真的清
let clearArmed = null;
$('btn-clear-userdict').onclick = () => {
  const b = $('btn-clear-userdict');
  if (!clearArmed) {
    b.textContent = '再点一次确认清除';
    clearArmed = setTimeout(() => { b.textContent = '清除'; clearArmed = null; }, 3000);
    return;
  }
  clearTimeout(clearArmed);
  clearArmed = null;
  b.textContent = '清除';
  invoke('clear_userdict');
  // 后台存盘有 2 秒延迟，等它存完再刷新数字
  setTimeout(refreshUserDictCount, 2500);
};
// 切到「输入」页时刷新一下数字
$('tabs').addEventListener('click', e => { if (e.target.closest('[data-page="input"]')) refreshUserDictCount(); });

// ---------- 检测按键 ----------
let wz = null; // { i, prev: Set, found }

$('btn-detect').onclick = () => {
  wz = { i: 0, prev: new Set(pad.buttons.filter(b => b.pressed).map(b => b.code)), found: 0 };
  $('wizard').hidden = false;
  wizardRender();
};
$('wz-cancel').onclick = () => { wz = null; $('wizard').hidden = true; };
$('wz-skip').onclick = () => { if (wz) { wz.i++; wizardRender(); } };

function wizardRender() {
  const total = meta.positions.length;
  const done = wz.i >= total;
  $('wz-step').textContent = done ? '完成' : `${wz.i + 1} / ${total}`;
  $('wz-skip').hidden = done;
  $('wz-presets').hidden = !done;
  $('wz-sub').hidden = done;
  if (done) {
    $('wz-ask').innerHTML = `记下了 <em>${wz.found}</em> 个按键的位置。要套用哪种布局？`;
    $('wz-cancel').textContent = '完成';
    drawStick($('wz-stick'));
    const box = $('wz-presets');
    box.replaceChildren();
    for (const [id, name] of meta.presets) {
      const b = el('button', 'btn', name);
      b.onclick = () => { invoke('apply_preset', { preset: id }); $('wz-cancel').click(); };
      box.append(b);
    }
    return;
  }
  $('wz-cancel').textContent = '取消';
  const pos = meta.positions[wz.i][0];
  $('wz-ask').innerHTML = `按「<em>${posLabel(pos)}</em>」`;
  drawStick($('wz-stick'), { target: pos });
}

function wizardStep() {
  if (!wz || wz.i >= meta.positions.length) return;
  const now = new Set(pad.buttons.filter(b => b.pressed).map(b => b.code));
  const fresh = [...now].find(c => !wz.prev.has(c));
  wz.prev = now;
  if (fresh == null) return;
  const pos = meta.positions[wz.i][0];
  invoke('set_button_name', { code: fresh, name: pos });
  // 新名字要等后台推回来才生效，先在本地记上，向导里的图马上就对
  for (const k of Object.keys(pad.profile.names)) if (pad.profile.names[k] === pos) delete pad.profile.names[k];
  pad.profile.names[fresh] = pos;
  wz.found++;
  wz.i++;
  wizardRender();
}

// ---------- 启动 ----------
(async () => {
  meta = await invoke('get_meta');
  listen('pad', onPad);
  listen('view', onView);
  loadSettings();
  invoke('refresh');
})();
