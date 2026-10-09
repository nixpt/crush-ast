# Every .md and .rs file, sorted.
find . \( -name '*.md' -o -name '*.rs' \) | sed 's|^\./||' | sort
