#!/usr/bin/env bash
# A realistic, multi-construct script that the bash and zsh walkers must
# reproduce exactly. Kept free of anything they refuse on purpose: no
# pipelines, redirections, command substitution, external commands or globs.
version=1.10
name="crush shell"

echo "== $name v$version =="

greet() {
  local who=${1:-world}
  echo "hello, $who"
}
greet
greet "a b"
greet bob alice

fruits=(apple banana cherry)
echo "count: ${#fruits[@]}"
for f in "${fruits[@]}"; do
  echo "- $f"
done
fruits+=(date)
fruits[1]=BANANA
echo "all: ${fruits[*]}"
echo "first: $fruits"
echo "last: ${fruits[-1]}"

for f in "${fruits[@]}"; do
  case $f in
    apple) echo "red" ;;
    *AN*) echo "shouted" ;;
    [dc]*) echo "d-or-c" ;;
    *) echo "other" ;;
  esac
done

path=/usr/local/bin/tool.tar.gz
if [[ $path == *.tar.gz ]]; then
  base=${path##*/}
  echo "dir: ${path%/*}"
  echo "base: $base"
  echo "stem: ${base%%.tar.gz}"
fi

n=5
if [[ $n -gt 3 && $n -lt 10 ]]; then
  echo "in range"
fi
if [[ ! -z $name && -z "" ]]; then
  echo "logic ok"
fi

echo "hex: $((0x10)) oct: $((010)) pow: $((2**8)) neg: $((-2**2))"
total=0
for ((i = 1; i <= 5; i++)); do
  total=$((total + i))
done
echo "sum: $total"
((total > 10)) && echo "total is big"

i=0
until [ $i -ge 3 ]; do
  i=$((i + 1))
done
echo "i=$i"

printf '%-8s|%8s|\n' left right
printf '%05d %.2s %x\n' 42 abcdef 255
printf '[%s=%d]\n' answer 42
printf 'pi=%.2f e=%.3e big=%g\n' 3.14159 27182.818 0.000012345
printf '[%08.2f][%-7.1f][%+.0f]\n' -3.5 2.25 2.5

unset maybe
echo "default: ${maybe:-fallback}"
word=a-b-c
echo "swapped: ${word//-/+}"
echo "slice: ${word:2:3}"
echo "strip: ${word#*-} / ${word%-*}"
file=report-2026.tar.gz
echo "glob sub: ${file//[0-9]/.} ${file/report-*/X}"
echo "anchored: ${file/#report-/draft-} ${file/%.gz/.zip} ${file/#out/X} ${file/%zip/X}"

false
echo "status: $?"
true
echo "status: $?"
exit 0
