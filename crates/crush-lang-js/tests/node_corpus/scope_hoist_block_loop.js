// A function hoisted in a loop body sees that iteration's later `const`.
const fs = [];
for (let i = 0; i < 3; i++) {
  function get() {
    return v;
  }
  const v = i * 10;
  fs.push(get);
}
const a = fs[0], c = fs[2];
console.log(a(), c());
