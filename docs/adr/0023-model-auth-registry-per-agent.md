# Pemilihan model per agen: Model Registry, default global + override per Role/Agen

## Context

Hari ini tidak ada yang memilih model untuk siapa pun: semua Role punya
model chain kosong, dan `Role`/`Roster`/`Store` tidak bisa mem-persist
Provider (tidak serializable — dan itu benar, lihat ADR 0025). Yang ada
hanya konsepnya di glossary: "Model chain: ordered list of Provider account
and model pairs … Set per Role, overridable per Agent."

Tiga lubang harus ditutup sekaligus oleh keputusan ini:

- **Tidak ada tempat menyimpan pilihan.** Nama model dan base URL bukan
  rahasia, tetapi hari ini tidak ada tabel/struktur yang memilikinya.
- **Error yang tidak jelas.** Bila kunci/model tidak tersedia, yang ada
  hari ini adalah string HTTP generik — atau lebih buruk, panic
  (`Provider::ollama` panic bila URL tidak valid).
- **Godaan fallback diam-diam.** Bila model A gagal, memakai model B
  mana saja yang "tersedia" terdengar membantu, tetapi itu berarti Job
  berjalan pada model yang tidak pernah disetujui operator, dengan harga
  dan perilaku yang berbeda — dan tagihannya masuk ke akun yang salah.

## Decision

1. **Model Registry adalah satu-satunya tempat pilihan model hidup.**
   Registry in-process, dibangun saat startup dari env (ADR 0020) +
   sealed vault (ADR 0021) + tabel Registry, dan diubah runtime lewat API
   Agora. Isinya dua hal:
   - **Akun model**: `kind`, `base_url`, model default, referensi kunci
     (`model/<kind>/<account>` di Vault). Nama akun, kind, base_url, dan
     nama model adalah data NON-rahasia dan disimpan plaintext di tabel
     SQLite `model_accounts`. Kunci TIDAK PERNAH masuk tabel ini.
   - **Chain per Role dan override per Agen**: daftar berurutan pasangan
     (akun, model). Model di chain boleh berbeda dari model default akun.
2. **Urutan resolusi, tanpa pengecualian diam-diam:**
   override per Agen → chain per Role → default global.
   Default global adalah satu chain yang ditetapkan operator (boleh kosong
   saat fresh install — server start, tetapi Job yang butuh model gagal
   dengan error yang jelas, bukan dengan model tebak-tebakan).
3. **Fallback hanya di dalam chain yang dikonfigurasi.** Bila entri pertama
   gagal (error, rate limit, timeout), chain lanjut ke entri berikut —
   persis definisi "Model chain" yang sudah ada. TIDAK ADA fallback lintas
   provider yang tidak tertulis di chain. Chain kosong atau semua entri
   gagal = Job gagal dengan error, bukan Job jalan dengan model lain.
4. **Error bertipe, bukan panic, bukan string generik.** Fase 3
   memperkenalkan error bertipe di jalur resolusi/pemanggilan:
   `ModelNotConfigured` (tidak ada chain), `ModelKeyMissing` (akun ada,
   kunci tidak ada — mis. revoked atau vault terkunci), `ProviderAuthFailed`
   (401/403 — kunci salah/dicabut di sisi vendor), `ModelNotFound`
   (404 — nama model salah), `ProviderUnavailable` (jaringan/timeout).
   Setiap error membawa provider kind, nama akun, dan nama model — TIDAK
   PERNAH nilai kunci, dan TIDAK PERNAH body error upstream mentah
   (di-redact dulu, ADR 0025). `Provider::ollama` yang panic diubah
   menjadi `Result`.
5. **Validasi startup hanya bentuk, bukan jaringan.** Registry menolak saat
   startup: akun tanpa kind, base_url yang tidak lolos `validate_endpoint`,
   nama model kosong, chain yang merujuk akun yang tidak ada. TIDAK ada
   panggilan jaringan saat startup (alasan sama seperti ADR 0020).
6. **Kebenaran nama model ditanyakan ke provider, bukan dihafal server.**
   Registry menyimpan nama model sebagai string konfigurasi. Agora boleh
   memanggil `GET /v1/models` (dan setaranya, ADR 0024) untuk menampilkan
   daftar model yang benar-benar tersedia sebagai pilihan — tetapi server
   tidak pernah memvalidasi "model ini ada" dari katalog sendiri, dan
   tidak pernah meng-hardcode nama model apa pun.
7. **Provider tetap tidak serializable — dengan sengaja.** Registry
   menyimpan *referensi* (nama akun + nama model), bukan Provider. Provider
   dibangun sesaat dari Registry + Vault tiap kali dibutuhkan. Inilah yang
   membuat rotasi tanpa restart dan revoke langsung berlaku (ADR 0024):
   tidak ada Provider beku yang menyimpan kunci lama di dalam Job,
   Checkpoint, atau Store.

## Consequences

- Operator dapat menjawab "agen X memakai model apa" dengan membaca
  Registry — bukan dengan membaca kode atau menebak env.
- Agen baru dari Role yang sama otomatis mewarisi chain Role; override
  per Agen hanya untuk pengecualian yang disengaja, bukan jalan default.
- Job yang gagal karena model memberi pesan yang menunjuk penyebab
  (akun mana, model mana, kunci hilang vs auth ditolak vs model tidak
  ada), sehingga operator tahu apakah yang harus dirotasi, diperbaiki
  namanya, atau diganti providernya.
- Harga dari "tidak ada fallback diam-diam": Job bisa gagal total padahal
  ada provider lain yang sehat — tetapi itu kegagalan yang jujur dan
  terlihat, dan operator yang menginginkannya dapat menulis chain
  lintas-provider secara eksplisit.

## Alternatives rejected

- **Katalog model hardcode di server.** DITOLAK (alasan lengkap ADR 0022):
  nama model berubah lebih cepat dari rilis; katalog adalah kebohongan
  yang menunggu kedaluwarsa.
- **Env var per agen (`BASILEIA_MODEL_STRATEGOS_...`).** DITOLAK: env
  adalah provisioning akun, bukan konfigurasi routing; mencampur keduanya
  membuat env meledak dan routing tidak bisa diubah tanpa restart.
- **Fallback implisit ke provider mana saja yang sehat.** DITOLAK: Job
  berjalan pada model yang tidak disetujui, tagihan masuk akun yang salah,
  dan hasil tidak dapat direproduksi. Fallback adalah keputusan operator
  yang tertulis di chain, bukan keputusan darurat server.
- **Menyimpan Provider (dengan kunci) di Store/Checkpoint agar Job bisa
  resume.** DITOLAK: kunci di state yang dipersist adalah kunci yang
  bocor ke file DB, ke backup, dan ke semua pembaca checkpoint. Provider
  dibangun ulang dari Registry + Vault saat resume.
- **Model default diam-diam (mis. "bila tidak dikonfigurasi, pakai
  model X").** DITOLAK: default yang tidak dinyatakan adalah perilaku
  yang tidak bisa diaudit. Default global harus ditetapkan eksplisit oleh
  operator; bila kosong, error `ModelNotConfigured` yang berbicara.
- **Validasi jaringan saat startup ("ping semua provider sebelum
  listen").** DITOLAK: server menjadi tidak bisa start saat internet
  putus; kesehatan adalah urusan ADR 0024, on-demand.

## Ditunda

- Routing berdasarkan biaya/latensi otomatis (memilih model termurah
  tercepat saat runtime) — butuh data harga/latensi yang hari ini tidak
  ada; chain eksplisit cukup untuk v1.
- Chain kondisional (model berbeda untuk Job besar vs kecil) — menunggu
  kebutuhan nyata; override per Agen menutup kasus paling umum.
- Menyimpan riwayat perubahan Registry (audit trail konfigurasi) —
  audit aksi kunci ada di ADR 0024; versi lengkap Registry ditunda.
