def merge(arr, left, mid, right):
    n1 = mid - left + 1
    n2 = right - mid
    L = [0] * n1
    R = [0] * n2
    i = 0
    while i < n1:
        L[i] = arr[left + i]
        i = i + 1
    i = 0
    while i < n2:
        R[i] = arr[mid + 1 + i]
        i = i + 1
    i = 0
    j = 0
    k = left
    while i < n1 and j < n2:
        if L[i] <= R[j]:
            arr[k] = L[i]
            i = i + 1
        else:
            arr[k] = R[j]
            j = j + 1
        k = k + 1
    while i < n1:
        arr[k] = L[i]
        i = i + 1
        k = k + 1
    while j < n2:
        arr[k] = R[j]
        j = j + 1
        k = k + 1

def sort(arr, left, right):
    if left < right:
        mid = left + (right - left) / 2
        sort(arr, left, mid)
        sort(arr, mid + 1, right)
        merge(arr, left, mid, right)

data = [5, 2, 8, 1, 9, 3]
sort(data, 0, 5)
print("Sorted")
