// CRUSH-251 step 4b: `let` / `const` are a fresh binding each time they run;
// `for (let ..)` gets one per iteration (copied before the update); `var` is
// one function-scoped binding.
const byLet = [];
for (let i = 0; i < 3; i++) {
  byLet.push(() => i);
}
const byVar = [];
for (var v = 0; v < 3; v++) {
  byVar.push(() => v);
}
const skipped = [];
for (let s = 0; s < 5; s++) {
  if (s % 2 === 0) continue;
  skipped.push(() => s);
}
const inBody = [];
let k = 0;
while (k < 3) {
  const j = k * 10;
  inBody.push(() => j);
  k++;
}
const l0 = byLet[0], l2 = byLet[2], v0 = byVar[0], s0 = skipped[0], s1 = skipped[1];
const b0 = inBody[0], b2 = inBody[2];
console.log(l0(), l2(), v0(), s0(), s1(), b0(), b2());
const ofConst = [];
for (const x of [1, 2, 3]) {
  ofConst.push(() => x);
}
const inLet = [];
for (let key in { a: 1, b: 2 }) {
  inLet.push(() => key);
}
const ofVar = [];
for (var y of [7, 8]) {
  ofVar.push(() => y);
}
const o0 = ofConst[0], o2 = ofConst[2], i0 = inLet[0], i1 = inLet[1], y0 = ofVar[0];
console.log(o0(), o2(), i0(), i1(), y0());
