function countPrimes(n) {
  let flags = [];
  let i = 0;
  while (i <= n) { flags.push(1); i = i + 1; }
  let count = 0;
  let p = 2;
  while (p <= n) {
    if (flags[p] == 1) {
      count = count + 1;
      let m = p * p;
      while (m <= n) { flags[m] = 0; m = m + p; }
    }
    p = p + 1;
  }
  return count;
}
console.log(countPrimes(__N__));
