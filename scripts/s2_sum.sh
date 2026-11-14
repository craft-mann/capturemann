#!/usr/bin/env bash
# s2_sum.sh <label>: summary of a BOX listing by group (path prefix before the first "_" or "/"),
# plus the worst 10 boxes.
S=/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/60bc0427-25e0-4ba6-a03e-32e606bb68fa/scratchpad/s2-shots
grep '^BOX' "$S/$1.txt" | awk '
{ f=$2; g=f; sub(/[_\/].*/,"",g); e=$7; sub("err=","",e); kind=$9; busy=$10;
  n[g]++; N++; if (e+0<=6) {p[g]++; P++}
  if (busy=="busy=true") {b[g]++; B++} else if (e+0>6) {sf[g]++; SF++}
  if (kind=="kind=Linear") {l[g]++; L++} }
END { for (g in n) printf "%-8s boxes %4d within6 %4d (%.1f%%) busy %3d silentfail %3d linear %3d\n", g, n[g], p[g], 100*p[g]/n[g], b[g], sf[g], l[g];
  printf "ALL      boxes %4d within6 %4d (%.1f%%) busy %3d silentfail %3d linear %3d\n", N, P, 100*P/N, B, SF, L }'
echo "-- per file"
grep '^BOX' "$S/$1.txt" | awk '{e=$7; sub("err=","",e); n[$2]++; if (e+0<=6) p[$2]++} END {for (f in n) printf "%-40s %4d %5.1f%%\n", f, n[f], 100*p[f]/n[f]}' | sort
echo "-- worst 10"
grep '^BOX' "$S/$1.txt" | sed 's/err=//' | sort -k7 -n -r | head -10
