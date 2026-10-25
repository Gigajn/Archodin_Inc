# 14: Mesin CS (WEBHOOK_CS) dengan memori RAG

**What to build:** Layanan pelanggan yang menjawab pesan masuk berdasarkan pengetahuan perusahaan sendiri: katalog produk, harga, dan SOP balasan disimpan sebagai memori vektor, dan jawaban dibuat dari dokumen yang benar-benar ditemukan.

**Blocked by:** 02

Catatan: webhook diuji sebagai irisan layanan; integrasi ke engine (`WEBHOOK_CS`) lewat tiket 18. Tabel pesan/pengetahuan baru bersifat aditif terhadap oracle paritas.

**Status:** ready-for-agent

- [ ] Ada jalur pemasukan pengetahuan (katalog/SOP) ke memori vektor, dan jalur pembaruannya saat dokumen berubah.
- [ ] Webhook pesan masuk memicu pencarian yang relevan dan sebuah balasan yang tercatat, lengkap dengan dokumen sumber yang dirujuk.
- [ ] Jawaban yang menyebut harga atau kebijakan **wajib** merujuk dokumen hasil retrieval; bila tidak ada dokumen relevan, sistem menjawab tidak tahu alih-alih mengarang.
- [ ] Anggaran latensi per balasan diukur dan dicatat pada beban wajar.
- [ ] Kegagalan memori vektor (server mati) tidak menjatuhkan kantor: pesan masuk gagal dengan status jelas dan bisa dicoba ulang.
