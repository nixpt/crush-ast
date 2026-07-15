#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
int sieve(int n) {
    bool* prime = malloc((n + 1) * sizeof(bool));
    for (int i = 0; i <= n; i++) prime[i] = true;
    prime[0] = prime[1] = false;
    for (int p = 2; p * p <= n; p++) {
        if (prime[p])
            for (int i = p * p; i <= n; i += p) prime[i] = false;
    }
    int count = 0;
    for (int i = 2; i <= n; i++) if (prime[i]) count++;
    free(prime);
    return count;
}
int main() { printf("%d\n", sieve(1000000)); return 0; }
