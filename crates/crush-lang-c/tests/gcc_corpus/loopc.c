#include <stdio.h>
int main() { int s = 0; for (int i = 1; i <= 200; i++) { if (i % 2 == 0) continue; s += i; } printf("%d\n", s); return 0; }
