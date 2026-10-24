# 09: Kantor 2D Phaser membaca database dan bergerak dari stream

**What to build:** Ruang kerja visual: lantai dan meja kerja digambar dari data nyata, karakter karyawan beranimasi mengikuti kejadian yang datang dari stream, dan satu panel inspektur memperlihatkan isi kepala agen saat dipilih.

**Blocked by:** 08

**Status:** ready-for-agent

Catatan lingkup: kanvas browser tetap Phaser.js (JavaScript) sesuai SRS; keputusan ini disengaja dan tidak dibatalkan oleh rewrite Rust.

- [ ] Denah per lantai dan meja digambar dari data API (bukan aset hardcode), termasuk beda tinggi/lebar ruangan saat denah melebar.
- [ ] Karakter berubah animasi saat menerima kejadian: diam, bekerja, berjalan menyerahkan tugas, dan peringatan error/limit.
- [ ] Gerak `HANDOFF` berjalan dari meja asal ke meja tujuan sesuai koordinat yang dikirim.
- [ ] Panel inspektur menampilkan prompt, log pemikiran terbaru, penggunaan token, dan terminal mini untuk meja pengembang.
- [ ] `ERROR`/`LIMIT` menampilkan indikator visual pada karakter yang bersangkutan, dan state-nya bertahan sampai kejadian berikutnya.
- [ ] Dibuktikan dengan tangkapan layar pada dua ukuran viewport (ponsel & desktop) lengkap dengan verdict per kriteria.
