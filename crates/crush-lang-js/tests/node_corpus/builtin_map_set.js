const m = new Map();
m.set('a', 1);
const s = new Set([1, 1, 2]);
console.log(m.get('a'), m.size, s.size, s.has(2));
