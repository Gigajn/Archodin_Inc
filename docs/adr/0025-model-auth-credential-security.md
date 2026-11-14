# Keamanan kredensial model: larangan bocor, hapus from_llm, semua egress lewat guard

## Context

Pola keamanan yang sudah benar di repo ini — dan menjadi patokan semua
yang baru — ada di `Vault` dan `KeySource`:

- `Vault`'s `Debug` hanya menampilkan NAMA secret (`vault.rs:62-68`),
  tidak pernah nilai.
- `KeySource`'s `Debug` menulis `KeySource::Fixed(..)` /
  `KeySource::Vault(name)` (`agent.rs:177-178`).
- `Vault::redact` mengganti setiap nilai secret yang dikenal dengan
  `[redacted]` sebelum teks mencapai log, event, Briefing, atau Akasha.
- `Config.admin_password` adalah String biasa dengan `Debug` manual.

Tetapi tiga lubang menganga di sebelah pola yang benar itu:

1. **`Provider::from_llm` (`native.rs:112`) mem-bypass auth sepenuhnya.**
   Ia menerima LLM pre-built dengan kunci `Fixed("")`: kredensial dibawa
   LLM itu sendiri, tidak bisa dirotasi (bukan `KeySource::Vault`), tidak
   bisa di-redact (Vault tidak tahu nilainya), dan tidak terlihat di audit.
   Bedrock dan Ollama hari ini dibangun lewat jalur ini.
2. **Client native langchain melewati `http_guard`.** Claude, Qwen, dan
   DeepSeek memakai `reqwest::Client::new()` fresh per request: tanpa
   timeout 30 detik, tanpa cap 2 MiB, tanpa filter SSRF http-acl. Hanya
   Gemini yang lewat `shared_client()`. Satu provider yang menelepon
   tanpa guard membatalkan seluruh janji ADR 0010 ("tidak ada tempat lain
   yang boleh membangun egress client").
3. **Konstruktor yang mengendus env** (`Claude::new` dkk. membaca
   `CLAUDE_API_KEY`/`QWEN_API_KEY`/`DEEPSEEK_API_KEY` dengan
   `unwrap_or_default()`) memberi kunci jalan masuk yang tidak melewati
   Vault, tidak melewati provisioning (ADR 0020), dan gagal diam-diam
   menjadi string kosong.

Ditambah satu risiko struktural: `Provider` yang TIDAK serializable hari
ini adalah kecelakaan yang menguntungkan — tetapi bila seseorang
"memperbaiki" itu agar Provider bisa masuk Store/Checkpoint, kunci akan
ikut ter-persist ke DB. Larangan itu harus ditulis, bukan diasumsikan.

## Decision

### 1. Aturan tidak bocor — berlaku di semua tempat baru, tanpa pengecualian

Nilai kunci (dan setiap string yang bisa mengandungnya) TIDAK BOLEH muncul
di: log, panic message, output `Debug`/`Display`, error yang dikembalikan
(`thiserror` maupun string), respons API (termasuk API Agora), audit,
Briefing, Akasha, Checkpoint, atau Job state.

- Setiap struct baru yang memegang kunci atau `KeySource` WAJIB
  mengimplementasikan `Debug` manual mengikuti pola Vault — `derive(Debug)`
  pada struct semacam itu DILARANG di review.
- Setiap error dari jalur model dilewatkan `Vault::redact` (atau
  `Provider::redact`, `agent.rs:114`) sebelum meninggalkan modul.
- Fase 3 menambahkan test canary: kunci palsu yang mudah dikenali
  (mis. `sk-canary-<random>`) dipakai membangun Provider dan memicu
  error, lalu test menegaskan string canary TIDAK MUNCUL di `Debug`,
  pesan error, maupun output audit. Test ini gagal bila satu jalur baru
  lupa me-redact — bukan dokumentasi yang berharap diingat.

### 2. `Provider::from_llm` DIHAPUS dari API publik di Fase 3

- Konstruktor publik `from_llm` dihapus. Semua provider v1 dibangun lewat
  konstruktor per-provider yang menerima `KeySource` (Vault) dan membangun
  LLM-nya di atas `http_guard::shared_client()`.
- Pengganti untuk test: helper konstruksi hanya di bawah `#[cfg(test)]`.
  Test boleh membangun LLM pre-built; produksi tidak.
- Konsekuensi yang diterima: Bedrock — yang hari ini hanya ada sebagai
  LLM pre-built dari AWS SDK — tidak bisa dibangun sampai ada desain
  kredensial non-string (ADR 0022 menundanya). Itu harga yang benar:
  pola bypass tidak dipertahankan demi satu provider yang ditunda.
- Ollama yang hari ini memakai `from_llm` di-refactor ke konstruktor
  langsung; client-nya adalah client reqwest 0.12 dengan timeout 30 detik
  + loopback-only, BUKAN guarded client — pengecualian tertulis §3a.

### 3. Semua panggilan model WAJIB lewat client yang di-guard — tanpa pengecualian

`http_guard::shared_client()` tetap satu-satunya sumber kebenaran egress
(ADR 0010). Untuk client native langchain yang hari ini membangun
`reqwest::Client::new()` sendiri:

- **Keputusan: patch fork.** Fork langchain-rust adalah fork Archodin
  (ADR 0001: tetap bisa di-merge ke upstream) — Fase 3 menambahkan
  injeksi client (konstruktor/method yang menerima `reqwest::Client`)
  pada client native yang dipakai v1 (Claude; Gemini sudah lewat guard).
- **Garis keras: bila injeksi terbukti tidak mungkin untuk satu provider,
  provider itu TIDAK dikirim di v1.** Tidak ada pengecualian "sementara"
  yang menelepon tanpa guard. Cakupan menyusut; keamanan tidak.
  Satu-satunya pengecualian terhadap garis ini adalah Ollama, dan itu pun
  tertulis dan terbatas di §3a di bawah — bukan "sementara" tanpa batas.
- Konstruktor yang mengendus env (`CLAUDE_API_KEY` dkk. dengan
  `unwrap_or_default()`) DILARANG di jalur produksi. Fase 3 mengaudit
  dengan grep: tidak ada pemanggilan konstruktor itu dari kode produksi;
  semua kunci datang dari Vault lewat provisioning (ADR 0020).

### 3a. Pengecualian TERTULIS dan TERBATAS: Ollama (satu-satunya)

Garis keras §3 di atas berlaku penuh untuk semua provider cloud — tanpa
pengecualian. Satu-satunya pengecualian adalah Ollama, diterima secara
tertulis di sini (keputusan grill 2026-11-14), dan hanya selama
kondisi-kondisi di bawah ini semuanya benar:

- **Alasan teknis, terverifikasi compiler (bukan asumsi):** ollama-rs
  0.3.6 dibangun di atas reqwest 0.12, sedangkan `shared_client()` adalah
  reqwest 0.13 — kedua tipe `Client` itu berbeda, sehingga guarded client
  TIDAK BISA diinjeksikan lewat `Ollama::builder().reqwest_client(...)`
  (builder menerima client eksternal, tetapi hanya yang 0.12). Lihat
  `native.rs` (komentar OPEN RISK di `Provider::ollama`).
- **Kontrol kompensasi 1 — loopback-only:** konstruktor `Provider::ollama`
  menolak URL apa pun yang bukan loopback (`127.0.0.0/8`, `::1`,
  `localhost`). Client yang tidak di-guard itu tidak pernah bisa diarahkan
  ke layanan LAN atau alamat metadata cloud lewat konfigurasi.
- **Kontrol kompensasi 2 — timeout 30 detik (B3):** client 0.12 yang
  dipakai Ollama dibangun di `native.rs` dengan timeout total yang sama
  dengan `shared_client()` (`http_guard::REQUEST_TIMEOUT`), jadi satu
  server Ollama yang menggantung tidak bisa menggantung satu Job
  selamanya.
- **Yang TETAP tidak ada:** client Ollama tidak punya redirect policy,
  filter DNS http-acl, maupun cap 2 MiB seperti guarded client. Itu
  diterima hanya karena loopback-only: permukaannya satu server lokal
  yang dikendalikan operator sendiri.
- **Follow-up, tetap terbuka:** patch ollama-rs ke reqwest 0.13 (atau
  guarded client 0.12), lalu injeksikan `shared_client()` dan HAPUS
  pengecualian ini beserta komentar OPEN RISK-nya. Pengecualian ini BUKAN
  preseden: provider cloud apa pun yang tidak bisa diinjeksikan tetap
  TIDAK dikirim di v1 (garis keras §3).
- **Jangan nonaktifkan feature Ollama** demi menutup celah ini: wadahnya
  harus tetap ada untuk nanti (keputusan user 2026-11-14, ADR 0022).

### 4. Kunci tidak pernah masuk state yang dipersist

`Provider` dan `KeySource` TETAP tidak serializable — dengan sengaja, dan
Fase 3 menambahkan komentar penanda di kedua definisi agar tidak ada yang
"memperbaiki" ini. Checkpoint, Job state, Akasha, dan Briefing hanya boleh
membawa NAMA akun/model, tidak pernah Provider atau kunci. Pelanggaran ini
adalah cacat keamanan, bukan ketidaksesuaian gaya.

### 5. Bila kunci bocor — runbook

Kebocoran tidak bisa di-undo dari sisi server; yang bisa dilakukan adalah
membatasi dan mencatat:

1. **Revoke di server** (ADR 0024) — hentikan pemakaian seketika.
2. **Revoke di dashboard vendor** — hanya vendor yang bisa mematikan
   kunci itu. Server tidak bisa melakukannya untuk operator.
3. **Rotasi** — kunci baru di-provision (env atau Agora), audit mencatat
   `revoke` + `rotate` dengan waktu dan akun.
4. **Audit** — catat apa yang bocor (ke mana, kapan diketahui), tanpa
   pernah menulis nilai kunci yang bocor ke dalam catatan itu sendiri.
5. **Re-seal** — Vault di-seal ulang setelah rotasi (ADR 0021), agar blob
   di SQLite tidak menyimpan ciphertext kunci lama yang sudah dicabut.
6. Bila kebocoran berasal dari log/file yang sudah tertulis, operator
   membersihkan artefak itu di luar server (server tidak menghapus log
   orang lain).

## Consequences

- Menghapus `from_llm` adalah perubahan API publik (breaking) di dalam
  repo — diterima, karena repo ini pre-1.0 dan pola itu adalah lubang
  keamanan, bukan fitur.
- Patch fork langchain-rust menambah pekerjaan Fase 3 yang tidak bisa
  dihindari dengan jalan pintas: tanpa itu, Claude tidak memenuhi janji
  guard. Upstream-mergeability (ADR 0001) dijaga dengan patch sekecil
  mungkin (injeksi client, bukan rewrite client).
- Test canary mengubah "jangan bocor" dari harapan menjadi properti yang
  diuji: setiap jalur baru yang lupa redact akan memerahkan test.

## Alternatives rejected

- **Membungkus `from_llm` dengan validasi ("tetap pakai, tapi periksa
  dulu").** DITOLAK. Validasi di tepi tidak memperbaiki inti: LLM
  pre-built membawa kredensial yang tidak dimiliki Vault, jadi tidak bisa
  dirotasi/di-redact/diaudit. Membungkus lubang bukan menutupnya.
- **Membiarkan client native tanpa guard "untuk v1 saja, diperbaiki
  nanti".** DITOLAK. "Sementara" di jalur keamanan adalah permanen yang
  belum diakui; ADR 0010 sudah memutuskan satu client, dan satu
  pengecualian adalah preseden untuk pengecualian kedua. Satu-satunya
  pengecualian terhadap penolakan ini adalah Ollama, dan itu pun tertulis
  dan terbatas di §3a — bukan "sementara" tanpa batas.
- **Mengirim kunci lewat header yang sama tetapi membiarkan langchain
  membangun client-nya sendiri dengan timeout default.** DITOLAK: timeout
  default reqwest adalah tanpa batas — satu provider lambat dapat
  menggantung satu Job selamanya.
- **Menjadikan `Provider` serializable agar mudah disimpan.** DITOLAK
  (lihat butir 4): itu mengubah kunci menjadi data yang ikut ter-backup,
  ter-checkpoint, dan terbaca siapa saja yang membaca DB.
- **Redact hanya di log, tidak di error/API.** DITOLAK: error dan respons
  API adalah jalur bocor yang paling sering dilupakan, karena dibaca
  manusia dan disalin ke laporan.

## Ditunda

- Deteksi kebocoran otomatis (memindai log/DB untuk pola `sk-...`) —
  berguna, tetapi rentan false positive dan bukan pengganti revoke +
  rotasi; ditunda sampai ada kebutuhan nyata.
- Enkripsi nama secret di sealed blob — lihat ADR 0021.
- Dukungan kunci berumur pendek / token OAuth yang di-refresh otomatis
  (Vertex) — menunggu ADR 0022 membutuhkannya.
