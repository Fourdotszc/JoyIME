// 用 Node 跑拼音引擎：node tools/test_pinyin.js [拼音 ...]
// 每个拼音同时测字母模式和九宫格模式（把字母换成键位数字，模拟只推方向不选字母）
const fs = require('fs');
const path = require('path');
const { PinyinEngine, toT9 } = require('../prototype/pinyin.js');

const src = fs.readFileSync(path.join(__dirname, '..', 'prototype', 'pinyin-dict.js'), 'utf8');
const dict = new Function(src + '; return PINYIN_DICT;')();
let t0 = Date.now();
const eng = new PinyinEngine(dict);
console.log(`载入 ${Date.now() - t0}ms`);
t0 = Date.now();
eng.table('t9');
console.log(`建九宫格表 ${Date.now() - t0}ms`);

const inputs = process.argv.slice(2).length ? process.argv.slice(2) : [
  'n', 'ni', 'nih', 'nihao', 'nihaoma', 'woxihuanni', 'jintiantianqihenhao', 'zhongguoren',
  'xian', "xi'an", 'fangan', 'lvse', 'shi', 'woshi', 'zhangsan', 'women', 'shijie', 'diannao',
];
const show = (c, s) => c.slice(0, 6).map(x => x.w + (x.rawLen < s.length ? `(${x.rawLen})` : '')).join(' ');
for (const s of inputs) {
  const d = toT9(s);
  let t = Date.now();
  const a = eng.query(s);
  const ta = Date.now() - t;
  t = Date.now();
  const b = eng.query(d, 't9');
  const tb = Date.now() - t;
  console.log(`${s.padEnd(20)} ${String(ta).padStart(2)}ms ${show(a, s)}`);
  console.log(`${d.padEnd(20)} ${String(tb).padStart(2)}ms ${show(b, d)}   读作 ${b[0] ? b[0].reading : '-'}`);
}
