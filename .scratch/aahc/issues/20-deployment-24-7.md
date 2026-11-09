# 20: Paket deployment 24/7 (compose/systemd) sesuai §7 dan NFR-AVL-01

**What to build:** Satu perintah membawa kantor dari nol ke jalan di host ini: topologi §7 (backend, office-ui, redis, chromadb, ollama) sebagai docker-compose dengan `restart: always` — atau unit systemd bila Docker tidak tersedia — sehingga kantor hidup kembali sendiri setelah crash maupun mesin dinyalakan ulang. Ini bahan bukti yang sebelumnya hanya dijanjikan tiket 16 tanpa artefak pembawanya.

**Blocked by:** 08, 18

**Status:** ready-for-agent

- [ ] Salah satu artefak deployment (docker-compose per §7, atau unit systemd) terpasang dan dijalankan di host ini; layanan inti merespons `/health` dari proses terpisah di host.
- [ ] `restart: always` terbukti: proses dibunuh paksa tombol `kill -9` dan layanan hidup kembali sendiri; bukti tercatat (log/waktu).
- [ ] Pemulihan setelah reboot mesin terbukti atau, bila tidak bisa diuji, prosedur pemulihannya terdokumentasi dan simulasi (stop semua → start semua) lolos.
- [ ] Default bind tetap localhost; pembukaan ke jaringan publik hanya lewat keputusan eksplisit (konsisten dengan prinsip bind tiket 07).
- [ ] Layanan pendukung (redis, chromadb, ollama) didefinisikan; engine (18) dan CS (14) toleran ketika chromadb/ollama belum dipakai, dengan status yang jelas bukan crash.
- [ ] Log punya strategi rotasi/pembatasan ukuran sejak artefak ini dibuat, bukan ditunda ke tiket 16.
- [ ] Satu dokumen singkat cara menjalankan/menghentikan/memulihkan kantor lewat artefak ini tersimpan dan sesuai dengan yang benar-benar dijalankan.
