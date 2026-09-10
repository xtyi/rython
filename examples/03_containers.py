first = 1
second = 2

values = [first, second, 3]
values[1] = 9
selected = values[1]

pair = (first, second)
tuple_item = pair[0]

mapping = {"a": first}
mapping["b"] = second
mapped = mapping["b"]

items = {first, second, first}

left_text = "x"
right_text = "y"
words = left_text + right_text
repeated = words * 3

print(values, selected, pair, tuple_item, mapping, mapped, items)
print(words, repeated)
