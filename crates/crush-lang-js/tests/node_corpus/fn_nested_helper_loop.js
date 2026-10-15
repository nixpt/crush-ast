// A nested helper called in a loop, declared after its use (hoisting).
function sum(xs) {
  let total = 0;
  for (const x of xs) {
    total = add(total, x);
  }
  function add(a, b) {
    return a + b;
  }
  return total;
}
console.log(sum([1, 2, 3]));
