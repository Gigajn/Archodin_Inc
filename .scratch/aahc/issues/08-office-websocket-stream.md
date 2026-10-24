# 08: Office WebSocket stream

**What to build:** Saluran real-time yang membuat kantor 2D hidup: setiap perubahan keadaan agen dipancarkan ke klien dengan kontrak yang sudah ditetapkan, dan transisinya berasal dari pekerjaan nyata — bukan animasi rekaan.

**Blocked by:** 07

**Status:** ready-for-agent

- [ ] Payload mengikuti kontrak SRS §6.2 persis: timestamp, jenis kejadian, lantai, id divisi, dan payload berisi id karyawan, keadaan baru, meja asal, meja tujuan, ikon balon, node router aktif, dan cuplikan log.
- [ ] Keadaan `IDLE`, `WORKING`, `HANDOFF`, `ERROR` muncul dari baris database dan kejadian eksekusi nyata; satu siklus deploy + satu siklus kerja menghasilkan urutan kejadian yang diharapkan.
- [ ] Klien lambat tidak memblokir kantor: buffer terbatas, klien yang tertinggal di-drop atau dilewati dengan catatan, bukan menahan seluruh broadcast.
- [ ] Klien yang baru tersambung menerima snapshot keadaan terkini lebih dulu, lalu lanjutan kejadian.
- [ ] Kill-switch menutup seluruh koneksi stream; tidak ada task yang bocor setelah shutdown.
