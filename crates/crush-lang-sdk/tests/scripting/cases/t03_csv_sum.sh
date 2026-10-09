# stdin: CSV with header (may contain blank lines). Sum amount per team, sorted by team.
tail -n +2 | awk -F, 'NF==3 {s[$2]+=$3} END {for (k in s) print k": "s[k]}' | sort
