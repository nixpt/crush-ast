# Print the script arguments, one per line, numbered.
i=1; for a in "$@"; do echo "$i: $a"; i=$((i+1)); done
