#include <stdio.h>
int main() { int n = 0; do { n++; if (n == 2) continue; } while (n < 5); printf("%d\n", n); return 0; }
