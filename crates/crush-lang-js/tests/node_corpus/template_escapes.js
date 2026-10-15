// Template literals use the cooked text and JS ToString (CRUSH-zorro-9).
const x = 1.5 * 4;
console.log(`value ${x}`);
console.log(`a\nb`);
console.log(`tab\there A\x42 \\ \``);
console.log(`${0.1 + 0.2} ${1e21} ${-0} ${2 / 4}`);
const name = 'crush';
console.log(`hello ${name}, ${true} ${null}!`);
console.log(`${1}${2}${'3'}`);
