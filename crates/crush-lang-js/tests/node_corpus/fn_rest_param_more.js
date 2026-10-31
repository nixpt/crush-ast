// Rest parameters collect the remaining arguments (possibly none); extra
// arguments to a plain function are ignored.
function tagged(tag, ...items) {
  return [tag, items];
}
console.log(tagged('a'), tagged('b', 1, 2, 3));
function outer() {
  function inner(...rest) {
    return rest.length;
  }
  return inner() + inner(1, 2);
}
console.log(outer());
function two(a, b) {
  return a + b;
}
console.log(two(1, 2, 3));
