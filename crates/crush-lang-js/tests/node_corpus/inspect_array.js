// console.log formats arrays like node's util.inspect (CRUSH-zorro-10).
console.log([1, 'a', true, null], [1.5 * 4]);
console.log([]);
console.log([[1, 2], [3, [4, [5, [6]]]]]);
console.log(['it\'s', 'say "hi"', 'tab\tnewline\n', 'back\\slash']);
console.log([0.1 + 0.2, 1e21, 1 / 3, 2 / 4, -1.5]);
const nums = [];
for (let i = 0; i < 30; i++) { nums.push(i * 7); }
console.log(nums);
const words = [];
for (let i = 0; i < 12; i++) { words.push('w' + i); }
console.log(words);
const big = [];
for (let i = 0; i < 120; i++) { big.push(i); }
console.log(big);
console.log(['a long string that goes on for a while', 'another long string here', 'and one more']);
console.log('label', [1, 2], 'tail');
