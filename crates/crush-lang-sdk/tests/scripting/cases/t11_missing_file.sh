# Read a missing file; on failure print a message and continue.
if ! c=$(cat nope.txt 2>/dev/null); then echo "nope.txt missing, using default"; fi
echo "continued"
