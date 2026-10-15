i = 0
s = 0
while True:
    i += 1
    if i % 2 == 0:
        continue
    if i > 9:
        break
    s += i
print(s)
