#include <stdio.h>
int sq(int n) { return n * n; }
int main() { int t = 0; int i = 0; while (i < 5) { t = t + sq(i); i = i + 1; } printf("%d\n", t); return 0; }
