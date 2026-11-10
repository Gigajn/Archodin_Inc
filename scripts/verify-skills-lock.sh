#!/usr/bin/env bash
# verify-skills-lock.sh — verifikasi integritas 27 skill Matt Pocock
# terhadap skills-lock.json (manifest integritas).
#
# Pemakaian (dari root repo):
#   bash scripts/verify-skills-lock.sh               # verifikasi (default)
#   bash scripts/verify-skills-lock.sh --check       # sama dengan default
#   bash scripts/verify-skills-lock.sh --regenerate  # tulis ulang computedHash
#                                                      # dari SKILL.md terpasang
#                                                      # (hanya setelah upgrade
#                                                      #  upstream yang disengaja;
#                                                      #  jangan edit lock manual)
#
# Keluar non-zero bila ada hash yang tidak cocok atau file hilang.
# Dijalankan LOKAL (bukan CI GitHub — GitHub Actions tidak tersedia).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK="$ROOT/skills-lock.json"
SKILLS_DIR="$ROOT/.agents/skills"

if [[ ! -f "$LOCK" ]]; then
  echo "skills-lock.json tidak ditemukan: $LOCK" >&2
  exit 1
fi

# Regenerasi: tulis ulang computedHash dari SKILL.md terpasang, lalu selesai.
if [[ "${1:-}" == "--regenerate" ]]; then
  python3 - "$LOCK" "$SKILLS_DIR" <<'PY'
import hashlib, json, sys
lock_path, skills_dir = sys.argv[1], sys.argv[2]
lock = json.load(open(lock_path))
for name, entry in lock["skills"].items():
    data = open(f"{skills_dir}/{name}/SKILL.md", "rb").read()
    entry["computedHash"] = hashlib.sha256(data).hexdigest()
json.dump(lock, open(lock_path, "w"), indent=2, ensure_ascii=False)
open(lock_path, "a").write("\n")
print("skills-lock.json diregenerasi.")
PY
  exit 0
fi

# Verifikasi: bandingkan SHA-256 tiap SKILL.md terpasang dengan computedHash.
mapfile -t ENTRIES < <(python3 - "$LOCK" <<'PY'
import json, sys
lock = json.load(open(sys.argv[1]))
for name in sorted(lock["skills"]):
    print(f"{name} {lock['skills'][name]['computedHash']}")
PY
)

fail=0
checked=0
for entry in "${ENTRIES[@]}"; do
  name="${entry%% *}"
  expected="${entry##* }"
  file="$SKILLS_DIR/$name/SKILL.md"
  if [[ ! -f "$file" ]]; then
    echo "HILANG: $name ($file tidak ada)"
    fail=1
    continue
  fi
  actual="$(sha256sum "$file" | cut -d' ' -f1)"
  checked=$((checked + 1))
  if [[ "$actual" != "$expected" ]]; then
    echo "TIDAK COCOK: $name"
    echo "  lock     : $expected"
    echo "  terpasang: $actual"
    fail=1
  fi
done

if [[ "$fail" -ne 0 ]]; then
  echo "GAGAL: ada hash yang tidak cocok atau file hilang (diperiksa: $checked)."
  exit 1
fi
echo "OK: $checked/27 hash cocok dengan skills-lock.json."
