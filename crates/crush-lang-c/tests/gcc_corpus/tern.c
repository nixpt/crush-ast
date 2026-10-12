#include <stdio.h>
int main() { int x = 3; int y = x > 2 ? 10 : 20; int z = !x; printf("%d %d %d\n", y, z, ~x); return 0; }
