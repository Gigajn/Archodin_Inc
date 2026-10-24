# 15: Irisan CRON_BATCH marketing (tren + afiliasi)

**What to build:** Divisi pemasaran bekerja terjadwal tanpa disuruh: riset tren, penyusunan materi promosi, dan penyiapan kandidat produk afiliasi — semuanya berhenti di draf yang menunggu persetujuan manusia sebelum tayang.

**Blocked by:** 06, 07

**Status:** ready-for-agent

- [ ] Scheduler menjalankan batch sesuai jadwal divisi, dan slot yang sudah dijalankan tidak dijalankan ulang (idempoten per slot jadwal).
- [ ] Batch menghasilkan draf yang tersimpan sebagai baris database: topik tren, materi promosi, dan kandidat afiliasi dengan sumbernya.
- [ ] **Tidak ada publikasi eksternal otomatis**: apa pun yang akan tayang di kanal publik berhenti sebagai draf yang butuh persetujuan.
- [ ] Kegagalan satu sumber (mis. API tren tidak dapat dihubungi) tidak menggagalkan seluruh batch; bagian yang gagal dilaporkan per langkah.
- [ ] Batch yang berjalan lama dapat dibatalkan oleh kill-switch dan berhenti pada titik aman.
