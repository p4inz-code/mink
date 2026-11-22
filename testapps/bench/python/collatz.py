best = 0
best_start = 0
start = 1
while start < 100000:
    n = start
    steps = 0
    while n != 1:
        if n % 2 == 0:
            n = n // 2
        else:
            n = 3 * n + 1
        steps = steps + 1
    if steps > best:
        best = steps
        best_start = start
    start = start + 1
print(best_start)
print(best)
