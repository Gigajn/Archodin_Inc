# 03: Spike klien MCP (rmcp + npx)

**What to build:** Pembuktian bahwa program Rust dapat menjalankan server MCP yang didistribusikan lewat `npx` sebagai child process, berbicara lewat stdio, dan memanggil satu tool dengan hasil yang benar. Ini risiko lingkungan terbesar dari seluruh rewrite: kalau gagal, jalur Skill Hunter harus didesain ulang — jadi dibuktikan lebih dulu, bukan di akhir.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] Sebuah binary/driver Rust men-spawn satu server MCP berbasis `npx`, melakukan handshake, mendaftar tool, dan berhasil memanggil satu tool dengan hasil yang tervalidasi.
- [ ] Kegagalan bersifat bertipe, bukan panic: `npx` tidak ada, server gagal start, server mati di tengah sesi, dan tool mengembalikan error.
- [ ] Tidak ada child process yang tersisa (orphan) setelah shutdown normal maupun setelah kegagalan.
- [ ] Ada batas waktu pada handshake dan pemanggilan tool sehingga server yang menggantung tidak memblokir proses 24/7.
- [ ] Keputusan tercatat di spec: pakai `rmcp`, atau fallback apa, beserta alasannya.
