// console.log formats objects like node's util.inspect (CRUSH-zorro-10).
console.log({ a: 1, b: 'two' });
console.log({});
console.log({ nested: { deeper: { deepest: { gone: 1 } } }, list: [1, { k: 'v' }] });
console.log({ 'my-key': 1, _ok: 2, x1: 3, 'with space': 4 });
console.log({ b: 1, 2: 'two', 1: 'one' });
const o = { name: 'crush', version: 1.5 * 4, tags: ['vm', 'aot'], ok: true, none: null };
console.log(o);
o.extra = 'a fairly long string value to push the line over the limit';
console.log(o);
console.log({ quote: "it's", empty: '', arr: [] });
console.log('obj:', { a: [] }, 7);
