// A function declared inside a function sees the enclosing locals, and is
// callable before its declaration (hoisting).
function outer(k) {
  const before = add(1);
  function add(n) {
    return n + k;
  }
  return before + add(5);
}
console.log(outer(2));
