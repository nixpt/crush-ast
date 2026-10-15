// A loop variable, a block `let` and a nested block may reuse an outer name.
let i = 100;
let total = 0;
for (let i = 0; i < 3; i++) {
  let total = i * 10;
  {
    let i = -1;
    console.log('inner', i, total);
  }
  console.log('loop', i, total);
}
console.log('outer', i, total);
