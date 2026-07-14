def merge_sort(arr):
    if len(arr) <= 1: return arr
    mid = len(arr) // 2
    left = merge_sort(arr[:mid])
    right = merge_sort(arr[mid:])
    res, i, j = [], 0, 0
    while i < len(left) and j < len(right):
        if left[i] < right[j]: res.append(left[i]); i += 1
        else: res.append(right[j]); j += 1
    res.extend(left[i:])
    res.extend(right[j:])
    return res

data = list(range(5000, 0, -1))
merge_sort(data)
print("Sorted")
