function f(x) { let s = ""; switch (x) { case 1: s += "a"; case 2: s += "b"; break; case 3: s += "c"; default: s += "d"; } return s; }
console.log(f(1), f(2), f(3), f(9));
