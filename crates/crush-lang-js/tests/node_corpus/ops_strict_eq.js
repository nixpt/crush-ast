// === / !== are strict (CRUSH-290): no coercion, one number type, objects by
// identity.
console.log(1 === 1, 1 === 2, 'a' !== 'b');
console.log(1 === '1', 1 === 1.0, '' === 0, '1' === 1);
console.log(true === 1, false === 0, true === true, 'a' === 'a');
console.log(null === null, null === 0, null === '', null === false);
console.log(0 === -0, 1 !== 1, 'x' !== 'x');
const o = {};
const p = o;
console.log(o === p, {} === {}, o === [], o !== p);
