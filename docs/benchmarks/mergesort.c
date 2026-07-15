#include <stdio.h>
#include <stdlib.h>
#include <string.h>
void merge(int* arr, int* tmp, int l, int m, int r) {
    memcpy(tmp + l, arr + l, (r - l + 1) * sizeof(int));
    int i = l, j = m + 1, k = l;
    while (i <= m && j <= r) arr[k++] = (tmp[i] < tmp[j]) ? tmp[i++] : tmp[j++];
    while (i <= m) arr[k++] = tmp[i++];
    while (j <= r) arr[k++] = tmp[j++];
}
void merge_sort_r(int* arr, int* tmp, int l, int r) {
    if (l >= r) return;
    int m = l + (r - l) / 2;
    merge_sort_r(arr, tmp, l, m);
    merge_sort_r(arr, tmp, m + 1, r);
    merge(arr, tmp, l, m, r);
}
int main() {
    int n = 5000;
    int* arr = malloc(n * sizeof(int));
    int* tmp = malloc(n * sizeof(int));
    for (int i = 0; i < n; i++) arr[i] = n - i;
    merge_sort_r(arr, tmp, 0, n - 1);
    printf("Sorted\n");
    free(arr); free(tmp);
    return 0;
}
