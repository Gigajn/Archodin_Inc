# 01: Bootstrap workspace Rust, git, dan toolchain

**What to build:** Repo ini menjadi proyek Rust yang bisa dibangun dan dijalankan: version control aktif dengan proteksi rahasia, toolchain ter-pin, sebuah workspace cargo, dan satu binary yang hidup menyajikan endpoint kesehatan dengan logging terstruktur dan konfigurasi dari environment.

**Blocked by:** None (bisa mulai segera).

**Status:** done

**Bukti penyelesaian:** `git init` + `.gitignore` + hook pre-commit terpasang; gitleaks terbukti **memblokir** berkas berisi secret dummy (`stripe-access-token`, 2 kebocoran terdeteksi) — catatan: hook gitleaks memindai konten *staged*, jadi bukti pertama yang lolos adalah positif palsu sampai berkas di-`git add`. Toolchain: rustup 1.29.2 dengan rustc 1.99.1 + clippy 0.1.99 + rustfmt 1.10.0, ter-pin lewat `rust-toolchain.toml`. Workspace cargo + crate `aahc-core`; `GET /health` → `{"status":"ok","version":"0.1.0"}` HTTP 200; rute tak dikenal → 404. Jalur gagal-cepat terbukti: tanpa `AAHC_DB_PATH` proses keluar dengan kode 2 dan pesan menyebut nama variabel. `scripts/check.sh` hijau: fmt bersih, clippy tanpa warning, 8 tes lulus. Catatan: `Cargo.lock` sengaja **di-commit** (aplikasi binary, bukan library) — menyimpang dari saran bawaan skill Rust, beralasan reproduktibilitas. `clang` belum terpasang (`sudo` tidak tersedia); belum ada crate bindgen yang membutuhkannya.

- [x] `git status` berjalan; `.gitignore` menutup artefak build dan rahasia; hook pre-commit terpasang dan **terbukti menolak** berkas berisi secret dummy.
- [x] Toolchain Rust ter-pin dan dapat direproduksi; `cargo`, `clippy`, `rustfmt` tersedia; versi tercatat di repo.
- [x] Workspace cargo berisi crate inti; binary tunggal menyajikan endpoint kesehatan yang mengembalikan 200 OK.
- [x] Konfigurasi dibaca dari environment; `.env.example` mendokumentasikan variabel wajib; start ditolak dengan pesan jelas bila variabel kritis hilang.
- [x] Satu perintah menjalankan seluruh gerbang: `cargo test` hijau, clippy tanpa warning, fmt bersih.
- [x] Tidak ada secret di dalam repo.
