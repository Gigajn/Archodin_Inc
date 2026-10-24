# 17: Contract — hapus Python beku dan tutup paritas

**What to build:** Akhir dari pola expand–contract: selama ini pohon Python adalah bentuk lama yang menjadi oracle; tiket ini menutup paritas dan menghapusnya — atau, bila paritas inti gagal, menyatakan berhenti dan mengembalikan Python sebagai produksi.

**Blocked by:** 04, 05, 06, 07, 08, 11

**Status:** ready-for-agent

Batas batal yang telah disepakati dan harus dihormati di tiket ini: bila router dan HRD (modul inti pertama) tidak mencapai paritas dalam **5 hari kerja sejak tiket 04 dimulai**, atau bila total rewrite melewati anggaran waktu yang dicatat di spec, maka rewrite **dibatalkan**: Python tetap menjadi produksi dan pohon Rust disimpan sebagai eksperimen, bukan diadopsi.

- [ ] Setiap kasus uji Python Fase 1 punya padanan Rust yang lulus, dan pemetaan kasus-per-kasus tercatat (bukan sekadar "jumlahnya sama").
- [ ] Oracle paritas skema melaporkan nol perbedaan, dan himpunan kode error identik.
- [ ] Tidak ada pemanggil tersisa pada pohon Python (dibuktikan lewat pencarian referensi), lalu pohon Python dihapus dalam satu perubahan terpisah.
- [ ] Keputusan akhir tercatat di spec: lanjut dengan Rust, atau kembali ke Python — beserta alasan berbasis angka, bukan selera.
- [ ] Dokumen proyek (spec, README perintah, cara menjalankan) menyebut satu stack saja setelah penghapusan, tanpa instruksi yang menyesatkan.
