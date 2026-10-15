function g(a: number, b?: number): number {
  return b === undefined ? a : a + b;
}
console.log(g(1), g(1, 2));
