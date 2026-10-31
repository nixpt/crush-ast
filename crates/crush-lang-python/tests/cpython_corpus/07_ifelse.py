def sign(n):
    if n > 0:
        return 1
    elif n < 0:
        return -1
    else:
        return 0
print(sign(5), sign(-3), sign(0))
