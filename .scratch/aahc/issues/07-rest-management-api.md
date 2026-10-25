# 07: REST API manajemen kantor

**What to build:** Antarmuka kendali kantor: menghasilkan rancangan divisi dari deskripsi bisnis, menyimpan dan mengaktifkan divisi, mengubah konfigurasi karyawan saat kantor sedang berjalan, dan menghentikan seluruh eksekusi dalam keadaan darurat.

**Blocked by:** 05, 18

Catatan: aktivasi divisi dan kill-switch hanya dapat dibuktikan terhadap pekerjaan nyata, karena itu engine (18) menjadi gerbang; 18 mengimplisitkan 02 dan 04.

**Status:** ready-for-agent

- [ ] Empat operasi dari SRS §6.1 tersedia dan berperilaku sesuai kontraknya; setiap permintaan mengubah baris database nyata yang dapat diperiksa ulang lewat pembacaan.
- [ ] Perubahan konfigurasi karyawan (prompt, klaster model, cabut izin tool) berlaku tanpa me-restart layanan.
- [ ] Kill-switch menghentikan pekerjaan yang sedang berjalan dan menutup koneksi stream; setelah ditekan, tidak ada pekerjaan baru yang dimulai, dan keadaannya terlihat di endpoint status.
- [ ] Kesalahan divalidasi dan dikembalikan bertipe (permintaan cacat, id tidak dikenal, payload blueprint tidak valid) — tidak ada panic yang menjadi 500 mentah.
- [ ] Layanan **bind ke localhost secara default**; membukanya ke jaringan harus keputusan eksplisit, karena kill-switch tanpa autentikasi tidak boleh terekspos.
- [ ] Transkrip curl untuk setiap operasi (sukses dan gagal) tercatat sebagai bukti penerimaan.
