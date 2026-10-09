# Rename photos/*.jpeg to *.jpg, then list photos/.
for f in photos/*.jpeg; do mv "$f" "${f%.jpeg}.jpg"; done
ls photos
