function isSafe(board, row, col, n) {
    for (var i = 0; i < col; i = i + 1) {
        if (board[row][i] == 1) return false;
    }
    var r = row; var c = col;
    while (r >= 0 && c >= 0) {
        if (board[r][c] == 1) return false;
        r = r - 1; c = c - 1;
    }
    r = row; c = col;
    while (r < n && c >= 0) {
        if (board[r][c] == 1) return false;
        r = r + 1; c = c - 1;
    }
    return true;
}
function solve(board, col, n) {
    if (col >= n) return true;
    for (var i = 0; i < n; i = i + 1) {
        if (isSafe(board, i, col, n)) {
            board[i][col] = 1;
            if (solve(board, col + 1, n)) return true;
            board[i][col] = 0;
        }
    }
    return false;
}
var n = 12;
var board = [];
for (var i = 0; i < n; i = i + 1) {
    var row = [];
    for (var j = 0; j < n; j = j + 1) row.push(0);
    board.push(row);
}
solve(board, 0, n);
console.log("Solved");
