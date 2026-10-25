# 19: Sandbox dev terisolasi + siklus self-debug (FR-DEV-01/02)

**What to build:** Karyawan divisi developer mengeksekusi kode yang ia tulis hanya di dalam sandbox terisolasi per proyek, dan bila tester menemukan error, log error kembali ke programmer untuk diperbaiki otomatis — maksimal 5 iterasi sebelum eskalasi ke manusia. Ini menutup FR-DEV-01 (isolasi eksekusi) dan FR-DEV-02 (siklus self-debugging) yang sebelumnya tidak memiliki tiket sama sekali.

**Blocked by:** 18

**Status:** ready-for-agent

- [ ] Eksekusi perintah agen developer dikunci ke folder workspace sandbox per proyek: percobaan menulis/membaca di luar sandbox ditolak dengan kesalahan bertipe (terbukti lewat tes, bukan konvensi).
- [ ] Executor berjalan di dalam kontainer Docker terisolasi; siklus kontainer bersih (tidak ada kontainer yatim setelah sukses, gagal, maupun kill-switch).
- [ ] Bila Docker tidak tersedia di host, jalur fallback (subprocess ber-batas sumber daya seperti divergensi Fase 1) tetap membuat slice ini teruji, dan keterbatasannya dicatat eksplisit.
- [ ] Siklus self-debug terbukti: programmer menghasilkan kode gagal, tester menemukan error, log error kembali ke programmer, perbaikan lolos — dengan penghitung iterasi, maksimal 5, lalu eskalasi bertipe ke manusia.
- [ ] Batas sumber daya sandbox aktif: wall-clock timeout dan ukuran output terbatas sehingga proses liar tidak menggantung kantor 24/7.
- [ ] Kill-switch menghentikan eksekusi sandbox yang sedang berjalan (proses/kontainer mati) dan keadaannya tercatat.
- [ ] Tidak ada jaringan utama yang terekspos dari dalam sandbox tanpa izin eksplisit pada konfigurasi skill-nya.
- [ ] Kontainer sandbox **tidak privileged** dan **tidak menerima mount `/var/run/docker.sock`**. Spesifikasi §7 menyebut mount `docker.sock` pada `aahc-core-backend`, tetapi socket itu adalah jalur escape langsung ke host (mengendalikan daemon = mengendalikan mesin). Yang dibutuhkan backend hanyalah kemampuan memanggil API pembuatan kontainer, jadi opsi yang dipertimbangkan berurutan: (a) socket proxy ber-daftar-izin yang hanya mengizinkan `create`/`start`/`stop`/`remove` pada kontainer ber-label proyek, (b) `docker context` jarak jauh, (c) mount langsung — hanya bisa dipilih bila (a)/(b) terbukti tidak memadai, dengan keputusan tertulis di spec dan dicatat sebagai risiko yang diterima. Pilihan yang dipakai: ____ .
