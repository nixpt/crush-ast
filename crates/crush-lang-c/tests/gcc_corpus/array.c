#include <stdio.h>
int main() { int a[3] = {4, 5, 6}; a[1] = 50; int s = 0; for (int i = 0; i < 3; i++) s += a[i]; printf("%d\n", s); return 0; }
