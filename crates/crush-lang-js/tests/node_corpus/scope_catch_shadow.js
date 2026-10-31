// A `catch` parameter is block-scoped: it doesn't overwrite an outer `e`.
let e = 'kept';
try {
  throw 'thrown';
} catch (e) {
  console.log('caught', e);
}
console.log(e);
