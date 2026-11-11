#!/bin/sh
B=https://github.com/craft-mann/capturemann/releases
for u in latest/download/SHA256SUMS.txt download/v0.3.1/SHA256SUMS.txt download/apt/Release; do
  echo "== $u"; curl -sSI -o /dev/null -w "%{http_code} -> %{redirect_url}\n" "$B/$u"
done
echo "== API latest"; curl -sS https://api.github.com/repos/craft-mann/capturemann/releases/latest | head -c 300; echo
echo "== API list (tag, prerelease, draft)"; curl -sS "https://api.github.com/repos/craft-mann/capturemann/releases?per_page=6" | python3 -I -c "import sys,json; [print(r['tag_name'], r['prerelease'], r['draft']) for r in json.load(sys.stdin)]"
