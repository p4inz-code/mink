s = 0
i = 0
while i < 30000000:
    s = s + i * i % 7
    i = i + 1
print(s)
