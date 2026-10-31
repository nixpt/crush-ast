const a = [1, 2, 3, 4];
console.log(a.map(x => x * 2).join(','), a.filter(x => x % 2).join(','), a.reduce((s, x) => s + x, 0));
