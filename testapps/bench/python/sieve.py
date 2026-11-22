n = 2000000
flags = [1] * (n + 1)
p = 2
while p * p <= n:
    if flags[p] == 1:
        m = p * p
        while m <= n:
            flags[m] = 0
            m = m + p
    p = p + 1
count = 0
i = 2
while i <= n:
    if flags[i] == 1:
        count = count + 1
    i = i + 1
print(count)
