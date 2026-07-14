#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
bool is_safe(int** board, int row, int col, int n) {
    for (int i = 0; i < col; i++) if (board[row][i] == 1) return false;
    for (int i = row, j = col; i >= 0 && j >= 0; i--, j--) if (board[i][j] == 1) return false;
    for (int i = row, j = col; i < n && j >= 0; i++, j--) if (board[i][j] == 1) return false;
    return true;
}
bool solve(int** board, int col, int n) {
    if (col >= n) return true;
    for (int i = 0; i < n; i++) {
        if (is_safe(board, i, col, n)) {
            board[i][col] = 1;
            if (solve(board, col + 1, n)) return true;
            board[i][col] = 0;
        }
    }
    return false;
}
int main() {
    int n = 12;
    int** board = malloc(n * sizeof(int*));
    for (int i = 0; i < n; i++) { board[i] = calloc(n, sizeof(int)); }
    int r = solve(board, 0, n);
    printf("%s\n", r ? "Solved" : "No Solution");
    for (int i = 0; i < n; i++) free(board[i]);
    free(board);
    return 0;
}
