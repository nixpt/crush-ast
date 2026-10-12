#include <stdio.h>
int main() { int t = 0; for (int i = 0; i < 4; i++) { switch (i) { case 0: t += 1; break; case 1: t += 10; break; default: t += 100; } } printf("%d\n", t); return 0; }
