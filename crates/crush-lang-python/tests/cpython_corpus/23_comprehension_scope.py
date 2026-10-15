def f():
    n = 10
    xs = [n for n in range(3)]
    return [n, xs]


n = 10
ys = [n * 2 for n in range(3)]
print(n, ys)
print(f())
m = 3
zs = [m + k for m in range(m) for k in range(2)]
print(m, zs)
