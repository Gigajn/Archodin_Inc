# 06: Skill vault + Skill Hunter + Toolsmith — paritas

**What to build:** Gudang kemampuan perusahaan: menemukan skill yang belum dimiliki, mengambilnya hanya dari sumber yang diizinkan, memasangnya ke vault, membuktikan ia layak pakai lewat uji asap di sandbox, dan — bila tidak ada di mana pun — menulis skill baru dari nol.

**Blocked by:** 02, 03, 04

**Status:** ready-for-agent

- [ ] Format vault (satu folder per skill berisi instruksi, executor yang dapat dijalankan, dan daftar dependensi) dibaca dan didaftarkan otomatis; metadata dibaca hanya dari jendela header yang disepakati.
- [ ] Urutan pencarian lokal → MCP → GitHub → PyPI, berhenti di sumber pertama yang menghasilkan hit; payload tiap registry diparse; status non-200 ditoleransi tanpa gagal.
- [ ] Skill yang sudah ada di vault dilaporkan apa adanya tanpa menulis berkas baru.
- [ ] Pemasangan menulis berkas skill, mencatat baris registry dalam keadaan **belum terverifikasi**, dan status verifikasi hanya berubah bila uji asap sandbox keluar dengan kode 0.
- [ ] **Tidak ada pemasangan paket yang dieksekusi otomatis**: perintah pemasangan hanya dicatat untuk dijalankan sandbox.
- [ ] Jalur Toolsmith: bila tidak ada hit di sumber mana pun dan ada router, sistem menghasilkan executor yang bisa dijalankan dan lolos uji asap; bila tidak ada router, hasilnya jelas "tidak ditemukan".
- [ ] Kegagalan verifikasi tidak pernah melempar exception keluar; alasannya tercatat.
- [ ] Rangkaian kasus uji Python Fase 1 untuk modul ini punya padanan yang lulus di Rust.
