count = 3
total = 0

while count > 0:
    total += count
    count -= 1

if total == 6:
    result = "ok"
else:
    result = "bad"

print(count, total, result)
