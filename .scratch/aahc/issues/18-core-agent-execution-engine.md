# 18: Engine eksekusi agen inti (state machine per execution_mode)

**What to build:** Dari satu divisi yang sudah di-deploy, karyawan benar-benar bekerja: baris database dimuat menjadi pekerja, panggilan LLM lewat router, izin tool ditegakkan, keadaan tertulis kembali ke database, dan setiap transisi dipancarkan sebagai kejadian yang akan dikonsumsi stream kantor (tiket 08). Ini komponen yang hilang dari rencana sebelumnya: REST (07), WS (08), UI (09), ML (13), CS (14), marketing (15) semuanya menganggap "pekerjaan nyata" sudah ada — tiket inilah penyedianya.

**Blocked by:** 05 (mengimplisitkan 02 dan 04). Penomoran 18–20 ditambahkan belakangan; lihat catatan audit di spec — relasi blokir mengikuti daftar di bawah, bukan urutan nomor.

**Status:** ready-for-agent

Catatan urutan: tiket ini dan 07 saling membutuhkan (kill-switch 07 hanya bermakna terhadap pekerjaan nyata; pekerjaan nyata butuh pembatalan). Urutan pengerjaannya: 18 dibangun dulu dengan **seam pembatalan internalnya sendiri** (token pembatalan yang dibagikan ke task turunan), diuji lewat tes yang memanggil seam itu langsung; 07 kemudian hanya menyambungkan endpoint kill-switch ke seam yang sudah ada. Jadi tidak ada yang menunggu yang lain selesai.

- [ ] Satu langkah workflow DAG_DEV dua meja selesai ujung ke ujung: state `IDLE → WORKING → HANDOFF → IDLE` tercatat dari baris database nyata, dengan meja asal–tujuan dari data.
- [ ] NFR-PRF-01 terbukti: karyawan yang menganggur adalah baris DB, bukan proses/thread hidup — tes menghitung pekerja aktif = 0 saat kantor sibuk-di-deploy tapi tidak ada tugas.
- [ ] `allowed_tools` ditegakkan: tool di luar daftar izin ditolak dengan kesalahan bertipe, bukan dibungkam.
- [ ] Router (04) dipanggil lewat seam yang dapat disuntikkan; seluruh kriteria diuji offline dengan transport palsu, dan satu jalur integrasi membuktikan router nyata dipakai.
- [ ] Kontrak payload §6.2 terpenuhi di sisi produsen: event keadaan berisi semua field yang diperlukan stream (id karyawan, state, meja asal/tujuan, ikon balon, node router aktif, cuplikan log).
- [ ] Kill-switch (07) menghentikan pekerjaan di titik aman: task dibatalkan tanpa kebocoran, keadaan terakhir tercatat, tidak ada pekerjaan baru dimulai.
- [ ] Kegagalan LLM/tool menghasilkan state `ERROR` + event ikon peringatan, bukan panic dan bukan diam.
- [ ] Pekerjaan yang selesai meninggalkan pekerja kembali menganggur (stateless) — tidak ada task tokio yang tersisa setelah satu siklus.
