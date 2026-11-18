'use strict';

const $ = id => document.getElementById(id);

// 后台告诉悬浮窗现在是哪种样子：compose 打字中 / hint 刚点进输入框的方向提示 / toast 换层、开关的提示
let mode = 'hint';
let lastView = null;

listen('overlay', o => {
  mode = o.mode;
  document.body.classList.toggle('above', !!o.above);
  if (lastView) render(lastView);
});

function render(v) {
  lastView = v;
  const pop = $('pop');
  pop.classList.toggle('paused', !v.enabled);
  pop.classList.toggle('hint', mode === 'hint');

  const dirs = $('dirs');
  dirs.replaceChildren();
  GRID_DIRS.forEach((d, i) => {
    if (d === null) {
      dirs.append(el('span', 'center', v.enabled ? layerShort(v.layer) : t('停')));
      return;
    }
    const cell = el('span', d === v.dir ? 'on' : '');
    cell.append(el('i', '', PHONE_KEYS[d]), document.createTextNode(v.grid[i] || ''));
    dirs.append(cell);
  });

  const composing = mode === 'compose' && v.comp;
  $('reading').textContent = composing ? v.comp_display : '';
  $('keys').textContent = composing && v.comp !== v.comp_display ? v.comp : '';
  const bar = composing ? pinyinBar(v) : null;
  $('py').replaceChildren(...(bar ? [bar] : []));
  pop.classList.toggle('pyfocus', !!(composing && v.py_focus));
  const list = $('cands');
  list.replaceChildren();
  if (composing) {
    v.cands.forEach((w, i) => {
      const li = el('li', '', w);
      li.dataset.n = i + 1;
      list.append(li);
    });
  }
  const toast = $('toast');
  toast.replaceChildren();
  if (mode === 'toast') {
    toast.append(v.enabled ? layerName(v.layer) : t('已暂停'));
    toast.append(el('small', '', v.enabled ? v.layers.map(layerShort).join(' → ') : t('按「开关」键恢复')));
  }
  $('page').innerHTML = composing && v.pages > 1 ? `${v.page + 1}/${v.pages}<br>▸ ◂` : '';
  $('page').hidden = !(composing && v.pages > 1);
  $('main').querySelector('.read').hidden = !composing;
}

LANG_READY.then(() => listen('view', render));

// 开发日志里记下悬浮窗实际有多大（不同缩放比例下布局会不会挤）
window.addEventListener('resize', reportSize);
function reportSize() {
  const r = s => document.querySelector(s).getBoundingClientRect();
  invoke('log_ui', {
    msg: `悬浮窗 ${innerWidth}x${innerHeight} 缩放 ${devicePixelRatio}，面板 ${Math.round(r('.pop').width)}x${Math.round(r('.pop').height)}，模式 ${mode}`,
  });
}
