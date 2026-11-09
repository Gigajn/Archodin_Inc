# 04: Router provider — paritas FR-RTR-01/02

**What to build:** Mesin perutean model yang berperilaku sama dengan router Python Fase 1: memilih lane sesuai klaster, pindah jalur seketika saat kena limit, dan mengunci jalur yang kena limit sesuai header yang dikirim provider.

**Blocked by:** 02

**Status:** ready-for-agent

*(Batas waktu/anggaran waktu kerja dihapus sesuai arahan: pengerjaan berjalan langsung tanpa batasan waktu kerja).*

- [ ] Empat klaster (CODE, FAST, REASONING, VISION) memilih lane sesuai prioritas urutan, dan lane yang masih dalam masa cooldown dilewati.
- [ ] HTTP 429/503 memindahkan permintaan ke lane berikutnya **tanpa jeda tidur** apa pun; cooldown dihitung dari `Retry-After` (detik maupun format tanggal HTTP); 429 tanpa header memakai default 60 detik, 503 memakai 30 detik.
- [ ] 401/403 mengunci lane 30 menit; 500 dan kegagalan transport **tidak** mengunci lane apa pun.
- [ ] Kesalahan bertipe dan tidak pernah panic: `NO_KEYS`, `ALL_COOLDOWN`, `ALL_FAILED`, `UNKNOWN_CLUSTER` dengan arti yang sama seperti implementasi Python.
- [ ] Cooldown yang sudah kedaluwarsa dibersihkan dan lane dipakai kembali; counter harian bertambah pada pemanggilan sukses dan di-reset saat tanggal UTC berganti.
- [ ] Kunci API hanya dibaca dari environment, nama variabel dilog, nilainya tidak pernah dilog.
- [ ] Seluruh perilaku di atas dibuktikan offline dengan transport yang disuntikkan, mencerminkan 18 kasus uji Python; `cargo test` dan `clippy` hijau.
