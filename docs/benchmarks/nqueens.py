def is_safe(board, row, col, n):
    for i in range(col):
        if board[row][i] == 1: return False
    i = row; j = col
    while i >= 0 and j >= 0:
        if board[i][j] == 1: return False
        i = i - 1; j = j - 1
    i = row; j = col
    while i < n and j >= 0:
        if board[i][j] == 1: return False
        i = i + 1; j = j - 1
    return True

def solve(board, col, n):
    if col >= n: return True
    for i in range(n):
        if is_safe(board, i, col, n):
            board[i][col] = 1
            if solve(board, col + 1, n): return True
            board[i][col] = 0
    return False

n = 12
board = []
for _ in range(n):
    row = []
    for _ in range(n):
        row.append(0)
    board.append(row)
solve(board, 0, n)
print("Solved")
