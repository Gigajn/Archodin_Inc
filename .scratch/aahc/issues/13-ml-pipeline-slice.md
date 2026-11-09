# 13: Irisan pipeline ML (dataset → latih → metrik)

**What to build:** Divisi ML bisa menjalankan siklus penuh untuk model kecil: mengambil dataset, menyiapkannya, melatih, dan melaporkan mutu model ke database sehingga terlihat di dashboard.

**Blocked by:** 02

Catatan: irisan ini diuji sebagai pipeline dengan harness; integrasi ke kantor (menjalankan divisi lewat engine) menyusul di tiket 18. Tabel metrik baru bersifat aditif — oracle paritas tiket 17 membandingkan skema Fase 1 saja.

**Status:** ready-for-agent

- [ ] Dataset kecil diunduh dan disiapkan lewat jalur yang terdokumentasi, dengan jumlah contoh dan bentuk data tercatat.
- [ ] Model mainan dilatih sampai selesai dengan seed tetap sehingga hasilnya dapat direproduksi (dua kali jalan menghasilkan metrik yang sama).
- [ ] Metrik evaluasi (mis. loss dan F1) tersimpan sebagai baris database yang dapat dibaca dashboard, bukan hanya tercetak di terminal.
- [ ] Pelatihan berat boleh didelegasikan ke tool trainer eksternal; keputusan ini tercatat, termasuk apa yang hilang dibanding ekosistem Python.
- [ ] Kegagalan pelatihan (dataset kosong, memori kurang, proses dimatikan di tengah) menghasilkan status gagal yang jelas beserta alasannya, tanpa mengunci proses utama.
