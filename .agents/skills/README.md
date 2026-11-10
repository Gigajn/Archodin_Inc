# Skills — pohon kanonik & cermin

## Pohon kanonik

`.agents/skills/` adalah **pohon kanonik**: satu-satunya tempat isi skill
disimpan dan diedit. Setiap subdirektori di sini adalah satu skill.

`.claude/skills/` adalah **cermin symlink** dari pohon kanonik: setiap
entri di dalamnya adalah symlink ke `../../.agents/skills/<nama-skill>`.
Claude Code membaca skill dari `.claude/skills/`; isinya selalu sama
dengan `.agents/skills/` karena symlink, bukan salinan.

Satu-satunya pengecualian adalah file `LICENSE-mattpocock-skills` di
`.claude/skills/` — file lisensi untuk skill Matt Pocock, bukan sebuah
skill.

> Jangan pernah mengedit file di bawah `.claude/skills/` — perubahan di
> sana akan hilang atau, lebih buruk, membuat cermin menyimpang dari
> pohon kanonik. Edit selalu di `.agents/skills/`.

## Menambah skill baru

1. Tambahkan direktori skill baru di `.agents/skills/<nama-skill>/`
   (lengkap dengan `SKILL.md`).
2. Jalankan `bash scripts/sync-skills.sh` dari root repo — skrip ini
   membuat symlink cermin di `.claude/skills/` secara otomatis.
3. Verifikasi dengan `bash scripts/sync-skills.sh --check` (harus lolos,
   exit 0).
4. Commit `.agents/skills/<nama-skill>/` dan symlink baru di
   `.claude/skills/` bersama-sama.

Menghapus skill: hapus direktorinya dari `.agents/skills/`, lalu hapus
symlink yatimnya di `.claude/skills/<nama-skill>/` secara manual
(`rm .claude/skills/<nama-skill>`), lalu verifikasi dengan
`bash scripts/sync-skills.sh --check`. `sync-skills.sh` sengaja tidak
menghapus entri cermin secara otomatis — penghapusan harus eksplisit.

## Verifikasi lokal

GitHub Actions tidak tersedia di repo ini (limit), jadi semua verifikasi
dijalankan lokal:

```sh
bash scripts/sync-skills.sh --check   # hanya memeriksa, tanpa mengubah
bash scripts/verify-skills-lock.sh    # memeriksa integritas skills-lock.json
```

Catatan: `scripts/verify-skills-lock.sh` dibuat di PR #36 yang belum
merge saat README ini ditulis. Setelah PR #36 merge, skrip itu tersedia
di `main`; sampai saat itu, jalankan `sync-skills.sh --check` saja dan
lewati langkah `verify-skills-lock.sh`.

`sync-skills.sh` tanpa `--check` memperbaiki cermin: membuat symlink
yang hilang dan membetulkan target yang salah. Skrip ini SENGAJA gagal
(exit non-zero) bila menemukan direktori nyata di `.claude/skills/`
(bukan symlink), symlink yang targetnya rusak/hilang, atau entri yang
tidak punya padanan di sisi lain — kasus-kasus itu menandakan cermin
menyimpang dan harus diperbaiki manual, bukan ditimpa otomatis.

## Caveat Windows

Symlink di Git hanya bekerja benar di Windows bila `core.symlinks=true`
saat checkout. Checkout di Windows tanpa pengaturan itu mengubah symlink
menjadi file teks biasa berisi path target — `.claude/skills/` jadi
rusak dan `sync-skills.sh --check` akan gagal. Aktifkan dulu:

```sh
git config --global core.symlinks true
```

lalu clone/checkout ulang. (Pengguna utama repo ini memakai Mac/Linux,
jadi ini hanya catatan untuk kontributor Windows.)
