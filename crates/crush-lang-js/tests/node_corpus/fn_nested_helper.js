// A nested function that uses only its own parameters (no captures).
function outer() {
  function sq(x) {
    return x * x;
  }
  return sq(3) + sq(4);
}
console.log(outer());
