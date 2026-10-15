var c = 0; for (var i = 0; i < 4; i++) { for (var j = 0; j < 4; j++) { if (j == 1) { continue; } if (j == 3) { break; } c = c + 1; } } console.log(c);
function f(n) { var r = 1; for (let x = 2; x <= n; x++) { r = r * x; } return r; } console.log(f(10));
var q = 0; for (;;) { q = q + 1; if (q == 7) { break; } } console.log(q);
