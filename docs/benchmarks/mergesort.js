function mergeSort(arr) {
    if (arr.length <= 1) return arr;
    const mid = Math.floor(arr.length / 2);
    const left = mergeSort(arr.slice(0, mid));
    const right = mergeSort(arr.slice(mid));
    const res = [];
    let i = 0, j = 0;
    while (i < left.length && j < right.length) {
        if (left[i] < right[j]) res.push(left[i++]);
        else res.push(right[j++]);
    }
    return res.concat(left.slice(i)).concat(right.slice(j));
}
const data = Array.from({length: 5000}, (_, i) => 5000 - i);
mergeSort(data);
console.log("Sorted");
