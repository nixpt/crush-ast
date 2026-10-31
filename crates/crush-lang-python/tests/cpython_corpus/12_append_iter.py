a = []
for i in range(5):
    a.append(i * i)
t = 0
for v in a:
    t += v
print(t, len(a))
