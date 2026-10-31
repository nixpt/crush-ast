const n = 7; const label = n > 5 ? (n > 6 ? "big" : "mid") : "small"; let calls = 0; function side() { calls++; return 1; } const z = true ? 0 : side(); console.log(label, z, calls);
