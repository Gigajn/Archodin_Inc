# 16: Pengerasan 24/7 dan audit sumber daya vs baseline Python

**What to build:** Kantor yang benar-benar tahan dinyalakan berbulan-bulan: hidup kembali setelah crash atau listrik mati, dapat dihentikan dengan aman, dan — yang paling penting — klaim penghematan sumber daya diukur, bukan diklaim.

**Blocked by:** 08

**Status:** ready-for-agent

Baseline terukur dari implementasi Python (diukur di mesin ini): seluruh stack Fase 1 menganggur = **45,7 MB RSS**, CPU saat menganggur ≈ **0,0021 detik per 3 detik** (≈0,07% satu core). Angka inilah pembanding yang harus dikalahkan.

Data pembanding dari sisi Rust (diukur saat tiket 01): binary rilis yang hanya menyajikan `/health` menganggur = **4,1 MB RSS**. Jadi klaim penghematan besar benar untuk *proses layanan*, tetapi harus diuji ulang pada beban kerja nyata (agen aktif, stream pasar) — bukan disimpulkan dari endpoint kosong.

- [ ] Layanan hidup kembali otomatis setelah proses dibunuh paksa dan setelah mesin dinyalakan ulang (restart policy terdokumentasi dan terbukti).
- [ ] Penghentian normal menutup koneksi database, stream, dan child process tanpa sisa proses yatim.
- [ ] Kill-switch diuji ulang pada kantor yang sedang bekerja: seluruh pekerjaan berhenti dan keadaan tercatat.
- [ ] RSS dan CPU stack Rust diukur pada beban yang sama dengan baseline Python, dan hasilnya **dipublikasikan apa adanya** — termasuk bila penghematannya kecil atau tidak ada.
- [ ] Jejak audit tersedia untuk pertanyaan "siapa/kapan mematikan kantor".
- [ ] Log terstruktur dan dapat diputar; berkas log tidak tumbuh tanpa batas.
