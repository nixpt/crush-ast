// A top-level function reads script bindings declared after it.
function report() {
  return `${label}: ${count}`;
}
const label = 'count';
let count = 1;
count = count + 1;
console.log(report());
