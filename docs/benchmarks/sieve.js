function sieve(n) {
    const prime = new Uint8Array(n + 1).fill(1);
    let p = 2;
    while (p * p <= n) {
        if (prime[p] === 1) {
            for (let i = p * p; i <= n; i += p) prime[i] = 0;
        }
        p++;
    }
    let count = 0;
    for (let i = 2; i <= n; i++) if (prime[i] === 1) count++;
    return count;
}
console.log(sieve(1000000));
