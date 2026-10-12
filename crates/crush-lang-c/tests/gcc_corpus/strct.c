#include <stdio.h>
struct P { int x; int y; };
int main() { struct P p; p.x = 2; p.y = 3; printf("%d\n", p.x * p.y); return 0; }
