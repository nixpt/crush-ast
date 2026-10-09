# Run commands, report each exit status.
sh -c 'exit 0'; echo "true-ish: $?"
sh -c 'exit 3'; echo "fails: $?"
out=$(echo hello); echo "captured: $out"
