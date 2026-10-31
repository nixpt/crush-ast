// `+` with a string uses JS ToString for the number (CRUSH-zorro-9).
console.log('x' + 1.5 * 4);
console.log(1.5 * 4 + 'x');
console.log('a' + 1e21);
console.log('a' + 0.1 * 3);
console.log('a' + -0);
console.log('a' + 1 / 3);
console.log('n=' + (4 / 2) + ', h=' + 7 / 2);
let s = 'total: ';
let t = 0.5;
t = t + 1.5;
s += t;
console.log(s);
console.log(1 + 2 + 'x', 'x' + 1 + 2);
console.log('flag ' + true + ' ' + null);
let n = 2;
console.log(n + 3, n + 0.5);
console.log(true + 1, null + 1);
