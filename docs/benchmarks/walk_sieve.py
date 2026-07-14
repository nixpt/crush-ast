def sieve(n):
    prime = []
    i = 0
    while i <= n:
        prime.append(1)
        i = i + 1
    prime[0] = 0
    prime[1] = 0
    p = 2
    while p * p <= n:
        if prime[p] == 1:
            i = p * p
            while i <= n:
                prime[i] = 0
                i = i + p
        p = p + 1
    count = 0
    i = 2
    while i <= n:
        if prime[i] == 1:
            count = count + 1
        i = i + 1
    return count

print(sieve(10000))
