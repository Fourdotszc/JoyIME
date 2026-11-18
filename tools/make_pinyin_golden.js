// 用网页原型的拼音引擎生成 Rust 版的对照数据：node tools/make_pinyin_golden.js
// 写到 crates/joyime-pinyin/tests/golden.json，Rust 版对同样的输入必须给出完全一样的结果。
const fs = require('fs');
const path = require('path');
const { PinyinEngine, toT9 } = require('../prototype/pinyin.js');

const root = path.join(__dirname, '..');
const src = fs.readFileSync(path.join(root, 'prototype', 'pinyin-dict.js'), 'utf8');
const eng = new PinyinEngine(new Function(src + '; return PINYIN_DICT;')());

const letters = [
  'n', 'ni', 'nih', 'nihao', 'nihaoma', 'woxihuanni', 'jintiantianqihenhao', 'zhongguoren',
  'xian', "xi'an", 'fangan', "fang'an", 'nv', 'lvse', 'shi', 'woshi', 'zhangsan', 'zhen', "zhen'",
  'nh', 'wxhn', 'wxh', 'zg', 'zgr', 'bj', 'xx', 'nhao', 'jtq', 'dd', "n'h", 'a', 'e', 'o', 'ang',
  'qxjkq', 'diannao', 'yu', "yu'", 'women', 'shijie',
];
const cases = [];
for (const s of letters) cases.push({ mode: 'letters', raw: s });
for (const s of letters) if (!s.includes("'")) cases.push({ mode: 't9', raw: toT9(s) });
cases.push({ mode: 't9', raw: "94'26" });

const out = cases.map(({ mode, raw }) => {
  const c = eng.query(raw, mode);
  return {
    mode,
    raw,
    count: c.length,
    top: c.slice(0, 15).map(x => ({ w: x.w, rawLen: x.rawLen, reading: x.reading })),
    // 生僻字要能出现在列表里（甄在 zhen 里排二十几）
    contains: ['甄', '毓', '鑫'].filter(ch => c.some(x => x.w === ch)),
  };
});
const dst = path.join(root, 'crates', 'joyime-pinyin', 'tests', 'golden.json');
fs.writeFileSync(dst, JSON.stringify(out, null, 1));
console.log(`${out.length} 组 -> ${dst}`);
