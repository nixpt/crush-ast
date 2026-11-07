# CRUSH-372: every link of a comparison chain counts, and the chain
# short-circuits, so a later operand isn't evaluated once a link is false.
def later():
    print("evaluated")
    return 10

x = 5
print(0 <= x < 3)
print(0 <= x < 9)
print(1 < 2 < 3 < 4)
print(1 < 2 < 3 < 3)
print(1 == 1 == 2)
print(1 != 2 != 1)
if 0 <= x < 3:
    print("in range")
else:
    print("out of range")
print(9 < x < later())
print(0 < x < later())
print(0 <= x + 1 < 9)
values = [1, 2, 3]
print(0 < values[1] < 2)
print(0 < x < 9 and x != 5)
