s = 0
i = 0
while i < __N__:
    s = (s + i * i) % 1000003
    i = i + 1
print(s)
