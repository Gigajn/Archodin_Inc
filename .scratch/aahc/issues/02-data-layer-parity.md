# 02: Lapisan data — skema SQLite dengan oracle paritas Python

**What to build:** Program Rust mampu membuat dan memigrasikan database perusahaan yang **struktur-identik** dengan yang dibuat Python Fase 1, dan ada alat otomatis yang membuktikan kesamaan itu (bukan sekadar klaim). Oracle ini menjadi alat pembukti paritas untuk seluruh tiket porting berikutnya.

**Blocked by:** 01

**Status:** done

**Bukti penyelesaian:** `schema.sql` disalin byte-identik dari `backend/schema.sql` (sha256 sama) dan dieksekusi langsung dengan `sqlx::raw_sql` (bukan migrasi sqlx, agar tidak ada tabel buku catatan `sqlite_`/`sqlx` yang mengacaukan oracle). `db::init_database` membuat ke-6 tabel, menyemai 11 lane, dan idempoten (jalankan kedua → 0 baris baru). Oracle paritas membandingkan dump terstruktur (tabel, kolom, tipe, NOT NULL, default, PK, FK, indeks, kolom indeks, keberadaan CHECK/REFERENCES/NOT NULL) DB Rust vs DB Python Fase 1 dan melaporkan **nol perbedaan**; baris `router_api_pool` identik. `Cargo.lock` mengunci versi sqlx. Gerbang `scripts/check.sh` hijau: fmt bersih, clippy tanpa warning, 14 tes (8 sebelumnya + 6 data) lulus. CLI `init-db --db <path>` terbukti membuat DB yang sama via verifikasi independen Python `sqlite3`.

Bentuk data yang dipertahankan persis (dari prototipe Python yang kini dibekukan): tabel `divisions`, `employees`, `workflow_steps`, `router_api_pool`, `trading_ledger`, `skills_registry`; indeks unik lane router pada `(provider_name, cluster_type, model_identifier)`; 11 lane gratis ter-seed (CLUSTER_CODE 3, CLUSTER_FAST 3, CLUSTER_REASONING 3, CLUSTER_VISION 2).

- [x] Perintah inisialisasi membuat seluruh tabel, kolom, tipe, constraint CHECK, foreign key, dan indeks; menjalankannya dua kali bersifat idempoten (tidak menggandakan lane).
- [x] Oracle paritas membandingkan dump struktur database buatan Rust dengan database buatan Python Fase 1 dan melaporkan **nol perbedaan** (nama tabel, kolom, tipe, urutan, index, FK).
- [x] 11 lane router ter-seed dengan nilai `provider_name`/`cluster_type`/`model_identifier`/`env_key_name`/`rpm_limit` yang identik dengan milik Python.
- [x] Verifikasi berjalan offline (tanpa jaringan) dan menjadi bagian dari gerbang CI yang sama dengan tiket 01.
