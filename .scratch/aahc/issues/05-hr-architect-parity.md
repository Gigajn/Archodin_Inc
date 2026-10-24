# 05: AI HRD — paritas FR-HRD-01/02/03

**What to build:** Dari satu paragraf tujuan bisnis, sistem mencetak divisi kerja lengkap: daftar karyawan dengan prompt dan klaster model, jadwal SOP berantai, dan posisi meja di dalam denah ruangan — lalu menyimpannya ke database. Ditambah pencetakan cepat meja trading per pair.

**Blocked by:** 02, 04

**Status:** ready-for-agent

- [ ] Brief teks → blueprint tervalidasi; payload cacat ditolak, dicoba ulang tepat sekali dengan permintaan perbaikan, lalu gagal dengan kesalahan bertipe (bukan panic).
- [ ] Klaster model yang kosong disimpulkan dari peran; tool di luar daftar izin dibuang; blueprint tanpa karyawan ditolak.
- [ ] Deploy menulis divisi, karyawan, dan langkah workflow dalam satu transaksi; id divisi bentrok diberi akhiran berurutan alih-alih menimpa.
- [ ] Auto-layout: koordinat meja tidak pernah bertumpuk, kolom lorong tetap bebas, dan saat ruangan penuh denah melebar (bukan menumpuk meja di sel yang sama) — terbukti dengan kasus 60 karyawan.
- [ ] Langkah workflow yang merujuk nama karyawan tidak dikenal dilewati dengan peringatan, tidak menggagalkan deploy.
- [ ] Kloning meja trading: satu trader per pair, tepat satu CRO per lantai trading, prompt trader ter-scope ke pair-nya dan melarang eksekusi order langsung, langkah workflow menuju CRO; pemanggilan ulang bersifat idempoten dan melaporkan pair yang dilewati.
- [ ] Rangkaian kasus uji Python Fase 1 untuk modul ini punya padanan yang lulus di Rust.
