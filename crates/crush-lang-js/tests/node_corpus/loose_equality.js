// == and != follow JS abstract equality for primitives (CRUSH-zorro-22).
console.log('1' == 1, 1 == '1.0', '' == 0, ' \t\n' == 0);
console.log('0x1f' == 31, '1e3' == 1000, 'abc' == 0);
console.log(true == 1, true == '1', false == '', false == 0);
console.log(null == 0, null == '', null == false, null == null);
console.log(2 == 2.0, 0 == -0, 'a' == 'a', 'a' != 'b');
console.log('1' != 1, 1 != 2, null != null);
const a = [1];
const b = a;
console.log(a == b, a == [1], a != b);
const x = 3;
if (x == '3') { console.log('coerced'); }
