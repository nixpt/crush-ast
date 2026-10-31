const o = {
  v: 0,
  set x(n) {
    this.v = n;
  },
};
o.x = 5;
console.log(o.v);
