// `push` with several arguments appends them all, in order. JS evaluates
// every argument before appending any.
const xs = [];
xs.push(1, 2, 3);
console.log(xs);
let k = 0;
const ys = [];
ys.push(k++, k++, k);
console.log(ys, k);
