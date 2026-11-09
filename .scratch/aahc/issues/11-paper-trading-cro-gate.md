# 11: Irisan paper trading dengan gerbang CRO

**What to build:** Meja trading bekerja dalam mode simulasi: tiap trader menganalisis satu pair dan mengusulkan order, CRO memvalidasi ukuran posisi, dan hasilnya tercatat di buku besar — tanpa satu rupiah pun bergerak.

**Blocked by:** 05, 10

**Status:** ready-for-agent

- [ ] Satu trader melayani tepat satu pair dan keluarannya berupa proposal order terstruktur; trader tidak pernah memanggil eksekusi bursa.
- [ ] CRO memvalidasi ukuran posisi dengan rumus yang dispesifikasikan dan menolak proposal yang melampaui batas risiko, disertai alasan penolakan.
- [ ] Hanya jalur kode CRO yang boleh memanggil klien eksekusi; ini dibuktikan lewat tes, bukan konvensi.
- [ ] Setiap trade tersimpan di buku besar dengan mode `PAPER`, status persetujuan CRO, ukuran posisi, dan kaitan ke karyawan pelakunya.
- [ ] Proposal yang ditolak CRO tidak pernah sampai ke sisi eksekusi.
- [ ] Mode default adalah paper; tidak ada jalur yang menyalakan uang asli pada tiket ini.
