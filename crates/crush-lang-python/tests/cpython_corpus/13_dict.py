d = {"a": 1, "b": 2}
d["c"] = 3
t = 0
for k in d:
    t += d[k]
print(t, d["c"])
