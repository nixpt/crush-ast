class P {
  constructor(x) {
    this.x = x;
  }
  get2() {
    return this.x * 2;
  }
}
console.log(new P(4).get2());
