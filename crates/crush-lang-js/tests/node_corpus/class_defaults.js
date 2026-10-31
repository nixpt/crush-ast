class Acc {
  constructor(start = 40) {
    this.x = start;
  }
  add(by = 1, extra = by) {
    this.x = this.x + extra;
    return this.x;
  }
  sum(first, ...rest) {
    return first + rest[0] + rest[1];
  }
}
const a = new Acc();
console.log(a.add());
console.log(a.add(5, 100));
console.log(new Acc(1).sum(1, 20, 21));
class Base {
  constructor(x = 7) {
    this.x = x;
  }
  get() {
    return this.x;
  }
  greet(sep = "B") {
    return sep;
  }
}
class Mid extends Base {
  greet() {
    return super.greet() + "M";
  }
}
class Leaf extends Mid {}
console.log(new Leaf().get());
console.log(new Leaf(9).get());
console.log(new Mid(1).greet());
class Util {
  static pick(v = 42) {
    return v;
  }
}
console.log(Util.pick());
console.log(Util.pick(1));
