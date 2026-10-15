// Top-level functions read and write script variables, call each other and recurse.
let total = 0;
const log = [];
function add(n) {
  total += n;
  log.push(n);
}
function addTwice(n) {
  add(n);
  add(n);
}
function fib(n) {
  return n < 2 ? n : fib(n - 1) + fib(n - 2);
}
addTwice(3);
add(fib(10));
console.log(total, log);
