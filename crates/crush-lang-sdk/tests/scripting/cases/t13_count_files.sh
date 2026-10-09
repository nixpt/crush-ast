# Count regular files (not directories) under the current directory.
echo "files: $(find . -type f | wc -l)"
