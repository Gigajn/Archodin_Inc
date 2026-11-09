# 17: Contract — hapus Python beku dan tutup paritas

**What to build:** Akhir dari pola expand–contract: selama ini pohon Python adalah bentuk lama yang menjadi oracle; tiket ini menutup paritas dan menghapusnya — atau, bila paritas inti gagal, menyatakan berhenti dan mengembalikan Python sebagai produksi.

**Blocked by:** 04, 05, 06, 07, 08, 09, 11, 13, 14, 15, 16, 18, 19, 20

Catatan: 10 (spike) sudah terserap ke 11; 12 tetap digerbangi manusia sehingga tidak menghalangi kontrak, tetapi sakelar LIVE harus dalam keadaan mati saat kontrak ditutup.

**Status:** ready-for-agent

*(Batas batal berbasis waktu kerja dihapus sesuai aturan AGENTS.md: paritas tidak digerbangi tenggat waktu; pengerjaan berjalan secepat mungkin. Paritas tetap dibuktikan dengan angka, bukan selera).*

- [ ] Setiap kasus uji Python Fase 1 punya padanan Rust yang lulus, dan pemetaan kasus-per-kasus tercatat (bukan sekadar "jumlahnya sama").
- [ ] Oracle paritas skema melaporkan nol perbedaan **pada skema Python Fase 1**; tabel aditif baru (metrik ML, pesan CS, draf marketing) berada di luar perbandingan dan didokumentasikan sebagai perluasan — bukan divergensi diam-diam. Himpunan kode error identik.
- [ ] Tidak ada pemanggil tersisa pada pohon Python (dibuktikan lewat pencarian referensi), lalu pohon Python dihapus dalam satu perubahan terpisah.
- [ ] Keputusan akhir tercatat di spec: lanjut dengan Rust, atau kembali ke Python — beserta alasan berbasis angka, bukan selera.
- [ ] Dokumen proyek (spec, README perintah, cara menjalankan) menyebut satu stack saja setelah penghapusan, tanpa instruksi yang menyesatkan.
