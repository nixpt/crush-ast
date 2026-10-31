function h(a, b) { switch (a) { case 0: switch (b) { case 0: return "00"; default: return "0x"; } default: return "x"; } }
console.log(h(0, 0), h(0, 5), h(1, 0));
