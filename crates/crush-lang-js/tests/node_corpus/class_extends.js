class A {
  hi() {
    return 'A';
  }
}
class B extends A {
  hi() {
    return 'B' + super.hi();
  }
}
console.log(new B().hi());
