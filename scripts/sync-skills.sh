#!/usr/bin/env bash
#
# sync-skills.sh — jaga `.claude/skills/` tetap menjadi cermin symlink
# dari pohon kanonik `.agents/skills/`.
#
#   bash scripts/sync-skills.sh          # perbaiki cermin: buat symlink
#                                          # yang hilang, betulkan target
#                                          # yang salah
#   bash scripts/sync-skills.sh --check  # hanya verifikasi, tanpa mengubah
#
# Skrip ini SENGAJA gagal (exit non-zero) bila menemukan:
#   - direktori nyata di `.claude/skills/` (bukan symlink) — kecuali file
#     `LICENSE-mattpocock-skills` yang memang file biasa;
#   - symlink yang targetnya rusak/hilang (dangling);
#   - entri di `.agents/skills/` tanpa padanan di `.claude/skills/`,
#     atau sebaliknya (entri yatim di cermin).
# Kasus-kasus itu menandakan cermin menyimpang dan harus diperbaiki
# manual, bukan ditimpa otomatis.
#
# GitHub Actions tidak tersedia di repo ini, jadi verifikasi dijalankan
# lokal lewat skrip ini (dan `scripts/verify-skills-lock.sh` setelah
# PR #36 merge).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
AGENTS_DIR="$ROOT/.agents/skills"
CLAUDE_DIR="$ROOT/.claude/skills"

# Satu-satunya file biasa yang diizinkan di `.claude/skills/`.
LICENSE_FILE="LICENSE-mattpocock-skills"

usage() {
  echo "Usage: $0 [--check]" >&2
  echo "  (tanpa argumen)  perbaiki cermin: buat symlink yang hilang," >&2
  echo "                   betulkan target symlink yang salah" >&2
  echo "  --check          hanya verifikasi, tanpa mengubah apa pun" >&2
}

CHECK_ONLY=0
if [[ $# -gt 0 ]]; then
  case "$1" in
    --check) CHECK_ONLY=1 ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "ERROR: argumen tidak dikenal: '$1'" >&2
      usage
      exit 2
      ;;
  esac
fi
if [[ $# -gt 1 ]]; then
  echo "ERROR: terlalu banyak argumen (hanya --check yang didukung)" >&2
  usage
  exit 2
fi

[[ -d "$AGENTS_DIR" ]] || { echo "ERROR: direktori kanonik tidak ditemukan: $AGENTS_DIR" >&2; exit 1; }
[[ -d "$CLAUDE_DIR" ]] || { echo "ERROR: direktori cermin tidak ditemukan: $CLAUDE_DIR" >&2; exit 1; }

shopt -s nullglob

errors=0
fixed=0

# Daftar kanonik (glob sudah terurut) + set untuk pengecekan balik.
declare -a canonical_names=()
declare -A canonical=()
for entry in "$AGENTS_DIR"/*; do
  name="$(basename "$entry")"
  canonical_names+=("$name")
  canonical["$name"]=1
done

# 1) Setiap entri kanonik harus punya symlink cermin yang benar.
for name in "${canonical_names[@]}"; do
  link="$CLAUDE_DIR/$name"
  expected="../../.agents/skills/$name"

  if [[ -L "$link" ]]; then
    current="$(readlink "$link")"
    if [[ "$current" != "$expected" ]]; then
      if [[ "$CHECK_ONLY" -eq 1 ]]; then
        echo "ERROR: $link menunjuk ke target yang salah: '$current' (seharusnya '$expected')" >&2
        errors=$((errors + 1))
      else
        echo "FIX: $link: target salah '$current' -> '$expected'"
        rm "$link"
        ln -s "$expected" "$link"
        fixed=$((fixed + 1))
      fi
    fi
    if [[ ! -e "$link" ]]; then
      echo "ERROR: $link adalah symlink rusak (target tidak ada: '$(readlink "$link")')" >&2
      errors=$((errors + 1))
    fi
  elif [[ -d "$link" ]]; then
    echo "ERROR: $link adalah DIREKTORI NYATA, bukan symlink — cermin menyimpang; hapus/perbaiki manual" >&2
    errors=$((errors + 1))
  elif [[ -e "$link" ]]; then
    echo "ERROR: $link adalah file biasa, bukan symlink — hapus/perbaiki manual" >&2
    errors=$((errors + 1))
  else
    if [[ "$CHECK_ONLY" -eq 1 ]]; then
      echo "ERROR: symlink hilang: $link (seharusnya -> '$expected')" >&2
      errors=$((errors + 1))
    else
      echo "FIX: membuat symlink $link -> '$expected'"
      ln -s "$expected" "$link"
      fixed=$((fixed + 1))
    fi
  fi
done

# 2) Setiap entri cermin harus punya padanan kanonik (kecuali file lisensi).
for entry in "$CLAUDE_DIR"/*; do
  name="$(basename "$entry")"
  if [[ "$name" == "$LICENSE_FILE" ]]; then
    if [[ -d "$entry" && ! -L "$entry" ]]; then
      echo "ERROR: $entry adalah direktori, seharusnya file lisensi biasa" >&2
      errors=$((errors + 1))
    fi
    continue
  fi
  if [[ -z "${canonical[$name]:-}" ]]; then
    if [[ -L "$entry" ]]; then
      echo "ERROR: symlink yatim di cermin: $entry (tidak ada padanan di .agents/skills/)" >&2
    elif [[ -d "$entry" ]]; then
      echo "ERROR: DIREKTORI NYATA di cermin: $entry (tidak ada padanan di .agents/skills/)" >&2
    else
      echo "ERROR: entri tak dikenal di cermin: $entry (tidak ada padanan di .agents/skills/)" >&2
    fi
    errors=$((errors + 1))
  fi
done

if [[ "$errors" -gt 0 ]]; then
  echo "sync-skills: GAGAL — $errors masalah ditemukan." >&2
  if [[ "$CHECK_ONLY" -eq 1 ]]; then
    echo "Jalankan tanpa --check untuk memperbaiki symlink yang hilang/salah target; direktori nyata & entri yatim harus diperbaiki manual." >&2
  fi
  exit 1
fi

if [[ "$CHECK_ONLY" -eq 1 ]]; then
  echo "sync-skills: OK — cermin .claude/skills/ sesuai dengan .agents/skills/ (${#canonical_names[@]} skill)."
else
  echo "sync-skills: OK — cermin sesuai (${#canonical_names[@]} skill, $fixed symlink diperbaiki/dibuat)."
fi
