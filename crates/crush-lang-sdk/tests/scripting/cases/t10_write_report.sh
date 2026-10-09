# Write a two-line report to out/report.txt, then print it.
mkdir -p out && printf 'rust files: %s\nlogs: %s\n' "$(find src -name '*.rs' | wc -l)" "$(ls logs | wc -l)" > out/report.txt
cat out/report.txt
