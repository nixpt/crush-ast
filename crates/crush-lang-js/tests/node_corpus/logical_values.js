// && and || evaluate to the deciding operand (CRUSH-zorro-22).
console.log(0 || 'dflt', 'a' && 'b');
console.log('' || 0 || null, 1 && 0, null && 'never');
console.log(5 || 'unused', 0 && 'unused');
const arr = [] || 'not reached';
console.log(arr.length);
const obj = {} && { ok: 1 };
console.log(obj.ok);
// The right side runs only when the left side doesn't decide.
function shout(v) { console.log('evaluated', v); return v; }
const a = 1 || shout('a');
const b = 0 && shout('b');
const c = 0 || shout('c');
const d = 'x' && shout('d');
console.log(a, b, c, d);
const port = 0;
console.log(port || 8080, 0 / 1 || 'zero');
if ('' || 0) { console.log('wrong'); } else { console.log('falsy'); }
if ('x' && 'y') { console.log('truthy'); }
