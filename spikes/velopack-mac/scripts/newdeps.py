import re, subprocess, sys
repo = "/Users/trungtran/source/repos/capturemann/.claude/worktrees/agent-a5b37077b3611f8ef"
spike = repo + "/spikes/velopack-mac"
out = subprocess.run(["cargo", "tree", "--manifest-path", spike + "/Cargo.toml", "-e", "normal", "--prefix", "none", "--target", "aarch64-apple-darwin"],
                     capture_output=True, text=True).stdout
pk = set()
for l in out.splitlines():
    m = re.match(r"(\S+) v(\S+)", l)
    if m:
        pk.add((m.group(1), m.group(2)))
lock = open(repo + "/Cargo.lock").read()
repo_names = set(re.findall(r'^name = "([^"]+)"', lock, re.M))
names = sorted({n for n, _ in pk if n not in ("velo-spike",)})
new = [n for n in names if n not in repo_names]
print("macOS arm64 dependency packages of velopack (excluding scratch crate):", len(names))
print("not already in the repo Cargo.lock:", len(new))
print(", ".join(new))
