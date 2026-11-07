# CRUSH-374: `finally` runs exactly once on every path, outside the
# protection of its own `except` clauses.
try:
    print("body")
finally:
    print("finally: normal")

try:
    raise ValueError("a")
except ValueError:
    print("handled")
finally:
    print("finally: handled")

try:
    try:
        raise ValueError("a")
    except KeyError:
        print("wrong handler")
    finally:
        print("finally: no handler matched")
except ValueError:
    print("outer caught it")

try:
    try:
        raise ValueError("a")
    except ValueError:
        raise KeyError("b")
    finally:
        print("finally: handler raised")
except KeyError:
    print("outer caught the handler's error")

try:
    try:
        print("body")
    except ValueError:
        print("own handler must not run")
    finally:
        raise ValueError("from finally")
except ValueError:
    print("outer caught finally's error")

try:
    y = 1
finally:
    print("finally: assigned")
print(y)

for i in [1, 2]:
    try:
        for j in [3, 4]:
            break
        print("loop body")
    finally:
        print("finally: inner break stays inside")

def inner():
    try:
        def nested():
            return 1
        print(nested())
    finally:
        print("finally: nested return stays inside")

inner()
