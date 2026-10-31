class Engine {
  constructor({ width = 960, height = 540 } = {}) {
    this.width = width;
    this.height = height;
  }
  area() {
    return this.width * this.height;
  }
}
console.log(new Engine().area());
console.log(new Engine({ width: 100 }).area());
console.log(new Engine({ width: 2, height: 3 }).area());
class Box {
  volume({ w: width, h, d = h }) {
    return width * h * d;
  }
}
console.log(new Box().volume({ w: 2, h: 3 }));
function head([first, , third = 40]) {
  return first + third;
}
console.log(head([1, 2]));
console.log(head([1, 2, 3]));
function deep({ point: { x, y } }) {
  return x + y;
}
console.log(deep({ point: { x: 20, y: 22 } }));
function pair([a, b] = [30, 12]) {
  return a + b;
}
console.log(pair());
console.log(pair([1, 2]));
