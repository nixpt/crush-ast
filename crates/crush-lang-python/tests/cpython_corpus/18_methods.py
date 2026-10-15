s = "a,b,c"
parts = s.split(",")
parts.append("d")
print(s.upper(), "-".join(parts), "  hi ".strip(), s.replace(",", ";"), s.startswith("a"))
print(len(parts), parts.pop(), len(parts), parts)
