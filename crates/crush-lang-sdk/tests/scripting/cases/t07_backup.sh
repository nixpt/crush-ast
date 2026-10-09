# Copy docs/notes.md into backup/notes.md.bak (creating backup/), report.
mkdir -p backup && cp docs/notes.md backup/notes.md.bak
[ -f backup/notes.md.bak ] && echo "backed up: $(wc -l < backup/notes.md.bak) lines"
