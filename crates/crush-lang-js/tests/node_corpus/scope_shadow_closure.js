// A closure captures the binding it sees, not another of the same name.
let x = 'outer';
const fs = [];
{
  let x = 'block';
  fs.push(() => x);
}
fs.push(() => x);
const a = fs[0], b = fs[1];
console.log(a(), b());
