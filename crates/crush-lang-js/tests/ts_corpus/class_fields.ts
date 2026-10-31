class P {
  x: number;
  constructor(x: number) {
    this.x = x;
  }
  get2(): number {
    return this.x * 2;
  }
}
console.log(new P(4).get2());
