function id<T>(x: T): T {
  return x;
}
console.log(id<number>(7), id<string>('s'));
