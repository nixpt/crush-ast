# stdin: text. Top 3 words by count (ties by word), "count word".
tr ' ' '\n' | grep -v '^$' | sort | uniq -c | sort -k1,1nr -k2,2 | head -3 | awk '{print $1" "$2}'
