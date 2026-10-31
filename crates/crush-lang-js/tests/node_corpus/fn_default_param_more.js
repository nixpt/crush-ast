// Defaults run only when the argument is missing (not for null or 0), in
// parameter order, and may use earlier parameters.
function scale(x, by = 2, label = `x${by}`) {
  return label + ':' + x * by;
}
console.log(scale(3), scale(3, 5), scale(3, 5, 'five'));
function pick(v = 'dflt') {
  return v;
}
console.log(pick(), pick(null), pick(0));
let calls = 0;
function count(n = ++calls) {
  return n;
}
console.log(count(), count(10), count(), calls);
function sumTo(n, acc = 0) {
  return n === 0 ? acc : sumTo(n - 1, acc + n);
}
console.log(sumTo(4));
