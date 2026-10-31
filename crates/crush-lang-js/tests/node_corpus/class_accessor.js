class Meter {
  constructor(x) {
    this.x = x;
  }
  get doubled() {
    return this.x * 2;
  }
  set doubled(v) {
    this.x = v / 2;
  }
}
const m = new Meter(21);
console.log(m.doubled);
m.doubled = 100;
console.log(m.doubled);
