outer: for (let i = 0; i < 3; i++) {
  for (let j = 0; j < 3; j++) {
    if (j == 1) continue outer;
    if (i == 2) break outer;
    console.log(i, j);
  }
}
