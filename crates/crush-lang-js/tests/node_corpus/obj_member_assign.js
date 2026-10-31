const o = { n: 1, inner: { v: 2 } }; o.n += 5; o["k"] = 7; o.inner.v = o.inner.v * 10; const a = [1, 2, 3]; a[1] += 40; a[0] = a[2];
console.log(o.n, o.k, o.inner.v, a[0], a[1], a[2]);
