# 12: Sakelar LIVE trading (digerbangi manusia)

**What to build:** Kemampuan berpindah ke uang asli, dirancang agar tidak pernah menyala karena kelalaian: hanya bisa dinyalakan manusia, dengan kunci yang izin penarikannya dimatikan, di bawah batas risiko, dan dengan jejak audit.

**Blocked by:** 11

**Status:** ready-for-human

Tiket ini sengaja ditandai `ready-for-human`: memerlukan penyiapan kunci bursa oleh manusia dan keputusan sadar untuk berisiko uang nyata. Agen tidak boleh menyelesaikan tiket ini sendiri.

- [ ] Mode LIVE memerlukan konfirmasi eksplisit manusia; tidak ada agen, konfigurasi bawaan, atau endpoint yang bisa menyalakannya sendiri.
- [ ] Sistem menolak start mode LIVE bila kunci bursa terdeteksi memiliki izin penarikan dana, dan pesan penolakannya jelas.
- [ ] Batas risiko wajib terpasang (risiko maksimum per trade, jumlah order harian, batas kerugian beruntun) sebelum order pertama dikirim.
- [ ] Setiap order LIVE punya jejak audit lengkap: siapa mengusulkan, siapa menyetujui, parameter, dan hasilnya.
- [ ] Kill-switch menutup koneksi LIVE dan membatalkan order terbuka dengan aman.
- [ ] Ada jalur kembali: mematikan LIVE langsung mengembalikan seluruh meja ke paper tanpa me-restart.
