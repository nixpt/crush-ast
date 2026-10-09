# Count TODO lines per .rs file under src/, sorted by path, then the total.
total=0
for f in $(find src -name '*.rs' | sort); do n=$(grep -c 'TODO' "$f"); echo "$f: $n"; total=$((total+n)); done
echo "total: $total"
