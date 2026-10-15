// `push` with several arguments in a loop.
const pairs = [];
for (let i = 0; i < 3; i++) {
  pairs.push(i, i * 10);
}
console.log(pairs, pairs.length);
