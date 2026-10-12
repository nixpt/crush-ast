def count_primes(n):
    flags = []
    i = 0
    while i <= n:
        flags.append(1)
        i = i + 1
    count = 0
    p = 2
    while p <= n:
        if flags[p] == 1:
            count = count + 1
            m = p * p
            while m <= n:
                flags[m] = 0
                m = m + p
        p = p + 1
    return count

print(count_primes(__N__))
