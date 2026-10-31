class Util {
  static factor = 3;
  static scale(x) {
    return x * Util.factor;
  }
}
console.log(Util.scale(14));
Util.factor = 2;
console.log(Util.scale(14));
