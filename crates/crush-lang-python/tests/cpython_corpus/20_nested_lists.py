grid = [[1, 2], [3, 4]]
grid[1][0] = 30
total = 0
for row in grid:
    for v in row:
        total += v
print(grid, total)
