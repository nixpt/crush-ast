function first(xs: number[]): number {
  const v: number | undefined = xs[0];
  return v! * 2;
}
console.log(first([21]));
