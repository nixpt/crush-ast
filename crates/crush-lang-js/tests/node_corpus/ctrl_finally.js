function f() {
  try {
    return 1;
  } finally {
    console.log('fin');
  }
}
console.log(f());
