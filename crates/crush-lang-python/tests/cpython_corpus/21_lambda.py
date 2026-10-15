sq = lambda x: x * x
inc = lambda x: x + 1


def apply(f, v):
    return f(v)


def compose(f, g):
    return lambda x: f(g(x))


def scale(k):
    return lambda x: x * k


print(sq(5), apply(inc, 41), apply(sq, 3))
print(compose(inc, sq)(4), compose(sq, inc)(4))
double = scale(2)
print(double(21), scale(10)(7))
