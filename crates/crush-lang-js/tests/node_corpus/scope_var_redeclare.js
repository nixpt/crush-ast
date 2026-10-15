// `var x;` again (here, every loop iteration) doesn't reset `x`.
var count = 0;
for (let k = 0; k < 3; k++) {
  var acc;
  if (k === 0) {
    acc = 0;
  }
  acc = acc + k;
  count++;
}
console.log(acc, count);
