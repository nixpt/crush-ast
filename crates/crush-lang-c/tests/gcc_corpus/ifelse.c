#include <stdio.h>
int classify(int n) { if (n < 0) { return -1; } else if (n == 0) { return 0; } else { return 1; } }
int main() { int s = 0; for (int i = -3; i <= 3; i++) { if (i % 2) s += 10; else s += 1; } printf("%d %d %d %d\n", classify(-5), classify(0), classify(9), s); return 0; }
