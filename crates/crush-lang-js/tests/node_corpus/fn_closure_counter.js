function mk() {
  let c = 0;
  return () => ++c;
}
const k = mk();
k();
console.log(k());
