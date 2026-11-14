# Kunci model selamat dari restart: sealed blob di SQLite + kunci Vault dari env

## Context

Vault hari ini ephemeral secara desain: `assembly.rs:251-271` membuat kunci
Vault acak per proses via `getrandom`, tidak pernah di-persist, dan `App`
membuang Vault setelah startup. Itu benar untuk kredensial login Agora
(password di-hash ulang dari env tiap restart), tetapi berakibat fatal untuk
kunci model: setiap akun yang ditambahkan lewat Agora (ADR 0020) akan hilang
saat restart, dan server kembali ke model chain kosong.

Tiga cacat Vault yang ditemukan Fase 1 ikut menentukan keputusan ini:

- `Vault::seal()` (`vault.rs:163`) mengembalikan `Vec<u8>` dan memakai
  `unwrap_or_default()` — bila serialisasi gagal, ia diam-diam mengembalikan
  byte kosong, dan `unseal` atas byte kosong baru gagal belakangan dengan
  `Malformed`. Kegagalan seal yang sunyi adalah kehilangan kunci yang sunyi.
- Kunci Vault 32-byte tidak di-zeroize: `Vault::new(key: [u8; 32])` memindahkan
  array ke konstruktor dan array itu mati tanpa dihapus. (Nilai secret
  sendiri sudah benar: `Secret(Zeroizing<String>)`.) Kunci yang melindungi
  semua kunci lain adalah yang paling layak di-zeroize.
- Nama secret TERLIHAT di file sealed (JSON nama → byte). Nama
  `model/openai/main` membocorkan provider dan akun apa saja yang ada kepada
  siapa pun yang bisa membaca file — bukan nilai kuncinya, tetapi tetap
  informasi.

Dan satu pola yang tidak boleh ditiru: `Auth::from_vault` adalah SNAPSHOT —
rotasi Vault tidak merotasi login. Kunci model harus memakai pola sebaliknya:
resolve tiap call (sudah ada di `KeySource::Vault`), agar rotasi Vault
langsung berlaku.

## Decision

Keputusan ini adalah hibrida tiga opsi yang dievaluasi: **A + F + B'**.

1. **Bootstrap env tetap jalan (Opsi A).** Kunci dari env di-provision ulang
   tiap startup (ADR 0020). Ini satu-satunya jalan yang didukung penuh kode
   hari ini, dan tetap menjadi jalan yang selalu bekerja — tanpa kunci Vault
   stabil pun, server bisa jalan dengan kunci env.
2. **Akun dari Agora dipersist sebagai sealed blob di SQLite (Opsi F).**
   Satu baris di tabel baru `sealed_vault`, berisi envelope berversi
   `{ "version": 1, "sealed": <bytes dari Vault::seal> }`. SQLite dipilih
   atas file sidecar karena satu file = satu unit backup: operator yang
   mem-backup `basileia.db` otomatis mem-backup kunci ter-seal, dan tidak
   ada file kedua yang bisa ketinggalan atau drift dari DB. Envelope
   berversi wajib: `unseal` menolak versi yang tidak dikenal, agar format
   bisa berubah tanpa membaca blob lama sebagai blob baru.
3. **Kunci Vault stabil datang dari env, bukan dari disk, bukan dari
   password (Opsi B').** Operator menyediakan satu kunci 32-byte lewat
   `BASILEIA_VAULT_KEY` (base64) atau `BASILEIA_VAULT_KEY_FILE` (file 0600
   berisi base64). Kunci ini dibuat sekali oleh operator (perintah
   pembangkit didokumentasikan; Fase 3 menyediakan subcommand
   `basileia generate-vault-key` bila murah, kalau tidak cukup dokumentasi
   `openssl rand -base64 32`). Kunci dipegang dalam `Zeroizing<[u8; 32]>`
   dan di-zeroize setelah `Vault::new`. Kunci ini tidak pernah di-log,
   tidak pernah masuk DB, tidak pernah masuk sealed blob.
4. **Bila kunci Vault tidak ada saat startup, server tetap start —
   degraded secara eksplisit.** Kunci env tetap bekerja; kunci dari Agora
   tidak bisa dibuka dan Registry menandai akun-akun itu `locked`. Startup
   menulis peringatan di log (tanpa materi kunci) dan Agora menampilkan
   status "persisted keys locked". Membuka kunci hanya bisa lewat restart
   dengan env — kunci master TIDAK boleh dikirim lewat API Agora (lihat
   "Alternatives rejected").
5. **Perbaikan Vault yang wajib di Fase 3, sebelum persistensi dipakai:**
   - `seal()` mengembalikan `Result<Vec<u8>, VaultError>` — tidak ada lagi
     `unwrap_or_default()`; gagal seal = gagal persist = error yang
     terlihat, bukan byte kosong.
   - `Vault::new` menerima `Zeroizing<[u8; 32]>`; kunci array di-zeroize.
   - `unseal` memeriksa versi envelope sebelum mendekripsi apa pun.
   - Nama secret dibatasi pada id buram `model/<kind>/<account>`; nama
     tidak boleh mengandung materi kunci atau petunjuk tentang nilai.
     Keterlihatan nama di blob diterima sebagai risiko yang dicatat:
     siapa pun yang bisa membaca `basileia.db` sudah berada di dalam
     batas kepercayaan yang sama dengan file DB itu sendiri; yang ia
     dapatkan hanyalah daftar akun, bukan kunci.

## Consequences

- Restart tanpa operator bisa dilakukan: selama `BASILEIA_VAULT_KEY`
  tersedia di env, semua akun Agora pulih otomatis.
- Keamanan sealed blob berdiri di SATU rahasia: kunci Vault. Siapa pun
  yang memegang DB + kunci Vault memegang semua kunci model. Itu
  konsekuensi yang disadari — dan alasan kunci Vault tidak boleh tinggal
  di samping blob (lihat penolakan Opsi B murni).
- Rotasi kunci model tetap tanpa restart (resolve per call, ADR 0024).
  Rotasi kunci VAULT sendiri adalah operasi langka yang butuh restart
  dan re-seal: baca semua secret dengan kunci lama, buat Vault baru dengan
  kunci baru, `put` ulang, `seal`, simpan. Fase 3 mendokumentasikan
  prosedurnya; tidak ada otomatisasi di v1.
- Server tanpa kunci Vault tidak pernah diam-diam kehilangan akun: status
  `locked` terlihat di log dan di Agora, bukan kegagalan misterius saat
  Job pertama jalan.

## Alternatives rejected

- **Opsi C — KDF dari passphrase / password admin.** DITOLAK. Ini single
  point of failure berlapis: bobol password admin = bobol semua kunci
  model; ganti password admin = enkripsi ulang semua kunci; dan rahasia
  login (yang melindungi Agora) menjadi rahasia master (yang melindungi
  semua kredensial keluar). Dua rahasia dengan dua masa hidup berbeda
  tidak boleh digabung menjadi satu.
- **Opsi B murni — kunci Vault stabil disimpan di disk di samping blob.**
  DITOLAK. Kunci di samping gembok bukan pengamanan, melainkan obfuscation:
  siapa pun yang membaca blob pasti membaca kuncinya. Kunci harus datang
  dari luar artefak yang ia lindungi (env/file operator).
- **Opsi E sebagai utama — input ulang semua kunci via Agora tiap
  restart.** DITOLAK sebagai utama, DITERIMA sebagai fallback. Mengetik
  ulang semua kunci tiap restart adalah beban operator yang menjamin suatu
  hari ada kunci yang lupa dimasukkan — dan server yang start "sehat"
  padahal separuh akunnya hilang. E tetap tersedia: bila kunci Vault
  hilang, operator boleh memasukkan ulang kunci model lewat Agora.
- **File sidecar `vault.sealed` di samping DB.** DITOLAK. File kedua
  adalah satu hal lagi untuk di-backup, di-rotate, dan dilupakan; drift
  antara sidecar dan DB (backup DB lama + sidecar baru, atau sebaliknya)
  menghasilkan akun hantu. Satu file, satu backup.
- **Kunci master dikirim lewat API Agora untuk "unlock" tanpa restart.**
  DITOLAK. Kunci master yang transit lewat HTTP (walau localhost) dan
  diterima dari request web memperluas permukaan serangan ke seluruh
  stack web; unlock hanya lewat env saat startup.
- **Bake-in kunci tetap di source code.** DITOLAK, tanpa pengecualian:
  kunci di repo = kunci yang bocor ke setiap clone. Tidak ada test, contoh,
  atau default yang berisi kunci nyata — hanya placeholder.

## Ditunda

- Rotasi kunci Vault otomatis / tanpa restart — prosedur manual
  terdokumentasi di v1.
- Enkripsi nama secret di dalam sealed blob — diterima terlihat di v1
  (risiko dicatat di atas); bila suatu hari nama dianggap sensitif,
  format envelope v2 yang mengenkripsi nama adalah jalannya.
- Backup terenkripsi terpisah untuk `sealed_vault` — backup DB biasa
  sudah mencakupnya; backup terpisah adalah urusan operator.

## Pertanyaan terbuka (untuk user)

- **Di mana `BASILEIA_VAULT_KEY` tinggal di home server?** Rekomendasi
  grill: file 0600 yang dibaca lewat `BASILEIA_VAULT_KEY_FILE`
  (mis. systemd `EnvironmentFile` atau file di samping DB). Bila setup
  Giga berbeda (mis. semua env di satu tempat lain), beri tahu agar
  dokumentasi dan contoh systemd mengikuti setup yang benar — bukan
  sebaliknya.

## Amandemen 2026-11-14 — file 0600 adalah jalur yang dipilih (jawaban user)

Amandemen ini MENUTUP pertanyaan terbuka di atas dan memperbarui
Decision 3; riwayat keputusan lama di atas TIDAK dihapus.

- **Jawaban user 2026-11-14: IYA.** `BASILEIA_VAULT_KEY` tinggal di
  file 0600 yang dibaca lewat `BASILEIA_VAULT_KEY_FILE` — itu jalur
  yang dipilih untuk home server. `BASILEIA_VAULT_KEY` inline tetap
  didukung sebagai fallback, dan menang bila keduanya diset (dan tidak
  kosong).
- **Subcommand `basileia generate-vault-key` SUDAH ADA** (bukan lagi
  "bila murah"): ia mencetak base64 dari 32 byte acak tanpa membaca
  `Config`, jadi tidak butuh password admin untuk membuat kunci.
  Operator menulis keluarannya ke file 0600, lalu menunjuk
  `BASILEIA_VAULT_KEY_FILE` ke file itu.
- **Perubahan perilaku (A11):** `BASILEIA_VAULT_KEY` yang diset tapi
  kosong / hanya whitespace diperlakukan sebagai ABSEN — server jatuh
  ke `BASILEIA_VAULT_KEY_FILE` bila ada, bukan error keras. Ini selaras
  dengan "file adalah jalur yang dipilih". Bila env DAN file absen,
  perilaku lama berlaku (degraded, kunci acak per proses).
- **Cek izin SEBELUM baca (A1):** izin file diperiksa pada file
  descriptor yang sama yang kemudian dibaca — file yang terlalu longgar
  ditolak sebelum isinya dipercaya.
- **Pesan error ramah operator (A5/A6):** file yang tidak ada menyebut
  `BASILEIA_VAULT_KEY_FILE` dan menyarankan `generate-vault-key`;
  file kosong menyebut kosong dan menyarankan generator yang sama.
  Nilai kunci tidak pernah masuk ke pesan mana pun.
- **Warning startup menyebut `_FILE` (A7):** warning degraded saat ada
  sealed blob, dan warning baru saat tidak ada kunci Vault SAMA SEKALI
  (akun yang ditambahkan di run ini akan terkunci setelah restart),
  keduanya menyebut `BASILEIA_VAULT_KEY_FILE`. Warning ditulis sekali
  saat startup, bukan per request.
