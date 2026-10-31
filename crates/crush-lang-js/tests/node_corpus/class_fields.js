class Counter {
  step = 2;
  total = 0;
  constructor(start) {
    this.x = start;
    this.total = this.total + this.step;
  }
  get() {
    return this.x + this.total;
  }
}
console.log(new Counter(40).get());
class Sub extends Counter {
  bonus = 5;
  constructor() {
    super(1);
    this.total = this.total + this.bonus;
  }
}
console.log(new Sub().get());
