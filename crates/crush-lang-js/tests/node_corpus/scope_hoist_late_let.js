// A hoisted nested function reads a `let` declared after it.
function outer() {
  function show() {
    return x;
  }
  let x = 5;
  return show();
}
console.log(outer());
