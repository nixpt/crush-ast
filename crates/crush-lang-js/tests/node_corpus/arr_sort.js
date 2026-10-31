const a = [10, 9, 1, 2];
a.sort();
console.log(a.join(','));
a.sort((x, y) => x - y);
console.log(a.join(','));
