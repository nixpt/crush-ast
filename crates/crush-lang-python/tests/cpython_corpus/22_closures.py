def make_adder(a):
    def add(b):
        return a + b
    return add


def make_counter():
    count = 0

    def tick():
        nonlocal count
        count += 1
        return count
    return tick


def local_assignment():
    x = 1

    def inner():
        x = 2
        return x
    r = inner()
    return x * 10 + r


def first():
    def helper():
        return "first"
    return helper()


def second():
    def helper():
        return "second"
    return helper()


def factorial(n):
    def fact(k):
        if k <= 1:
            return 1
        return k * fact(k - 1)
    return fact(n)


add3 = make_adder(3)
print(add3(4), make_adder(10)(5))
a = make_counter()
b = make_counter()
print(a(), a(), b(), a())
print(local_assignment())
print(first(), second())
print(factorial(6))
fs = []
for i in range(3):
    fs.append(lambda: i * 10)
print(fs[0](), fs[1](), fs[2]())
