# Follow-up model auth: vault key file, wadah Ollama, dan resep Muse/OpenRouter

## Context

PR model auth (branch `model-auth`, ADR 0020–0025) meninggalkan empat
keputusan terbuka untuk user (2026-11-14 21:02 WIB, user menjawab dua di
antaranya) dan satu keputusan desain yang masih harus diputuskan di sisi
Muse:

1. **Di mana `BASILEIA_VAULT_KEY` tinggal di home server?** ADR 0021
   §"Pertanyaan terbuka" bertanya ini. User menjawab: **IYA — kunci
   tinggal di file 0600** yang dibaca lewat `BASILEIA_VAULT_KEY_FILE`
   (sudah diimplementasikan di PR ini: file harus mode 0600, ditolak bila
   lebih longgar, hanya dibaca bila `BASILEIA_VAULT_KEY` inline absen).
2. **Ollama / LLM lokal: dipakai sekarang atau dicadangkan?** User
   menjawab: **SEMENTARA TIDAK dipakai, tapi buatkan tempat/wadahnya
   dulu untuk nanti**; sementara pakai model cloud — Claude, Gemini,
   ChatGPT, dan Muse AI ini — untuk diintegrasikan.
3. **Bagaimana Muse (dan OpenRouter) dikonfigurasi?** PR ini tidak punya
   preset Muse/OpenRouter: Muse harus lewat kind `openai_compatible` +
   `BASE_URL` OpenRouter + `MODEL` `meta/muse-spark-open` (nama model
   tidak pernah di-hardcode, ADR 0022/0023). Pertanyaannya: apakah perlu
   kind baru (`muse` / `openrouter`), atau cukup resep env?
4. **HTTP 402 dari provider** (no credits, khas OpenRouter) saat itu
   masuk `ModelError::ProviderUnavailable` (transien) — sehingga bisa
   di-retry / fallback sia-sia — dan health check memetakannya ke
   `unreachable`, yang menyesatkan operator ke arah jaringan.

Dua fakta terverifikasi yang mengikat keputusan (3):

- ADR 0022 baris 36 sudah memutuskan OpenRouter tercakup
  `openai_compatible` — tabel kind v1 menyebut OpenRouter di baris
  `openai_compatible`.
- `default_base_url` diduplikasi di `config.rs:87` dan `model_ops.rs:64`
  tanpa test kesepakatan; follow-up ini menambahkan test literal di
  kedua sisi agar drift terdeteksi.

## Decision

### 1. Vault key file 0600 adalah jalur yang dipilih

- `BASILEIA_VAULT_KEY_FILE` menunjuk file berisi satu baris base64
  kunci Vault 32-byte. File **harus mode 0600**; file yang lebih longgar
  ditolak saat startup sebelum isinya dibaca (cek dilakukan pada file
  descriptor yang sudah dibuka — ADR 0021 amandemen 2026-11-14).
- `BASILEIA_VAULT_KEY` inline hanya **fallback**: dibaca dulu bila ada
  dan tidak kosong; bila absen/kosong, barulah file dibaca. Urutan ini
  disengaja — file adalah jalur yang dipilih user, inline hanya jalan
  darurat (mis. systemd `Environment=` sesaat atau test).
- Runbook pembuatan file ada di `crates/basileia/README.md`
  ("Model accounts"): `basileia generate-vault-key` (atau
  `openssl rand -base64 32`) → tulis ke file → `chmod 0600` (atau buat
  dengan `umask 077` / `install -m 0600` agar tidak pernah 0644) → set
  `BASILEIA_VAULT_KEY_FILE`.
- Perilaku bila kunci Vault absen tidak berubah (ADR 0021 §Decision 4):
  server tetap start dalam mode **degraded** — kunci env tetap bekerja,
  akun Agora yang tersegel dilaporkan `locked`, unlock hanya lewat
  restart dengan kunci. Lihat entri "degraded / locked" di `GLOSSARY.md`.

### 2. Ollama adalah wadah local LLM yang dicadangkan untuk nanti

- Kind `ollama` tetap ada sebagai **slot opt-in, loopback-only**,
  dicadangkan untuk nanti — **tidak aktif sekarang**. Akun Ollama boleh
  didaftarkan (tanpa kunci, health check `GET /api/tags`), tetapi tidak
  ada model chain v1 yang menggunakannya sampai user memutuskan
  mengaktifkan model lokal.
- Penyimpangan guard **diterima tertulis** (amandemen ADR 0022/0025):
  `ollama-rs` 0.3.6 memakai reqwest 0.12.28, sehingga injeksi
  `shared_client()` (reqwest 0.13) ditolak compiler. Ollama v1 =
  loopback-only + client tidak di-guard; sebagai gantinya URL
  non-loopback ditolak saat validasi, jadi client tanpa guard tidak
  pernah menjangkau alamat di luar mesin. Follow-up yang dicatat (bukan
  v1): patch `ollama-rs` ke reqwest 0.13 agar guard bisa diinjeksikan.
- Tidak ada perubahan perilaku Ollama di follow-up ini — hanya
  dokumentasi statusnya (glossary + README) agar tidak ada yang
  mengira model lokal sudah berjalan.

### 3. Muse/OpenRouter: resep env `openai_compatible` (opsi c) — tanpa kind baru

Tiga opsi dipertimbangkan (grill 2026-11-14):

- **(a) Kind `muse` baru — DITOLAK eksplisit.** Kind adalah bentuk API,
  bukan nama model (semangat ADR 0022). Kind `muse` membuka preseden
  katalog kind: tiap nama model baru menuntut kind baru, dan nama
  "Muse" sendiri ambigu (keluarga model Meta — Spark, dan kelak yang
  lain — bukan satu endpoint). Penolakan ini final untuk v1; bila suatu
  hari Muse punya bentuk API yang tidak tercakup
  `openai_compatible`, itu ADR baru dengan bukti bentuk API-nya, bukan
  sekadar nama.
- **(b) Kind `openrouter` baru — DITUNDA sebagai ADR masa depan.**
  Tanpa header atribusi (`HTTP-Referer` / `X-Title`) dan tanpa pemetaan
  402, kind `openrouter` hanyalah alias kosong dari
  `openai_compatible` — tidak ada perilaku yang berubah. Kind ini baru
  bernilai bila salah satu diputuskan: header atribusi dikirim (butuh
  Config kustom / patch fork — lihat C7 di bawah), dan/atau pemetaan
  402 (sudah dikerjakan di follow-up ini, lihat §4).
- **(c) Resep env `openai_compatible` — DIPILIH.** Muse dipakai lewat
  OpenRouter dengan kind yang sudah ada:

  ```sh
  BASILEIA_MODEL_muse_KEY='sk-canary-...'
  BASILEIA_MODEL_muse_KIND='openai_compatible'
  BASILEIA_MODEL_muse_BASE_URL='https://openrouter.ai/api/v1'
  BASILEIA_MODEL_muse_MODEL='meta/muse-spark-open'
  ```

  `MODEL` di atas adalah **contoh konfigurasi, bukan default kode**:
  nama model tidak pernah punya default di mana pun (ADR 0022/0023) —
  `MODEL` wajib ditulis eksplisit. Resep lengkap untuk keempat model
  cloud (Claude, Gemini, ChatGPT, Muse) ada di
  `crates/basileia/README.md` ("Model accounts"). Pesan error kind
  tidak dikenal di `model_ops.rs` kini menyebut petunjuk ini
  (`"(OpenRouter/Muse use openai_compatible with BASE_URL)"`).

### 4. HTTP 402 menjadi error tersendiri, bukan transien

- `ModelError::from_status` memetakan **402 →
  `ProviderPaymentRequired`** (varian baru, konstruktor
  `ModelError::payment_required`). 401/403/404 dan 429/5xx tidak berubah.
  Alasan: 402 berarti akun kehabisan kredit — retry atau fallback ke
  pasangan chain berikutnya hanya membakar panggilan sampai akun di-top
  up, jadi chain harus berhenti di sini dengan pesan yang jelas
  ("no credits left — top up, then retry").
- Health check memetakan **402 → `HealthStatus::PaymentRequired`**
  (`"payment_required"`, bukan `"unreachable"`): jaringan baik-baik
  saja, yang habis adalah saldo. Satu-satunya `match` eksaustif di luar
  `archodin-graph` yang menyentuh status ini ada di
  `archodin-console/src/model_api.rs` (audit hasil health check) — ia
  **harus** mendapat satu lengan baru
  (`PaymentRequired → (AuditAction::HealthCheck, "payment_required: …")`)
  sebelum kode ini di-merge, kalau tidak crate itu tidak kompilasi.
  Itu di luar cakupan file follow-up ini; koordinator yang
  menerapkannya.
- Test: 402 → varian baru (bukan `ProviderUnavailable`), health 402 →
  `payment_required`, 401/403/404 tidak berubah.

## Catatan verifikasi dan keterbatasan (C4, C5, C7)

- **C4 — Health check belum pernah diuji live.** Seluruh pemetaan health
  check (termasuk 402 dan akun OpenRouter di atas) diuji lewat mockito
  di `model_ops.rs`, bukan terhadap provider sungguhan. Verifikasi live
  (`GET /api/v1/models` OpenRouter dengan kunci asli, dan satu health
  check tiap kind) adalah **verifikasi manual di tangan user** sebelum
  rilis — bukan gate merge otomatis di sini.
- **C5 — Health check OpenRouter bisa positif palsu.** `GET /v1/models`
  OpenRouter bersifat publik dan tidak memvalidasi kunci (catatan grill,
  **belum terverifikasi live**): health check bisa melaporkan `ok`
  padahal kunci salah atau dicabut. Jangan jadikan `ok` di sini sebagai
  bukti kunci valid. Calon perbaikan masa depan: endpoint yang
  memvalidasi kunci, mis. `GET /api/v1/key` — perlu verifikasi live
  dulu apakah endpoint itu ada dan apa yang dikembalikannya.
- **C7 — Header atribusi OpenRouter tidak bisa dikirim lewat jalur
  sekarang.** `HTTP-Referer` / `X-Title` (yang dipakai OpenRouter untuk
  atribusi aplikasi) tidak ada jalurnya di `Provider::openai_compatible`
  hari ini; mengirimnya butuh Config kustom atau patch fork. Ditunda
  bersama kind `openrouter` (opsi b di atas): tanpa keputusan bahwa
  header itu bernilai, tidak ada yang dibangun.

## Risiko dan temuan minor yang diterima/ditunda

- **B6 — Pemeriksaan loopback berbasis string host, diterima sebagai risiko.** `is_loopback_url` (`native.rs`) hanya memeriksa host literal (`127.0.0.0/8`, `::1`, `localhost`); nama host yang me-resolve ke loopback lewat konfigurasi DNS/`/etc/hosts` tidak dicek pada saat request (client reqwest 0.12 memakai resolver sistem). Dinilai rendah di home server karena operator mengendalikan `/etc/hosts`/DNS sendiri. Diterima tanpa perubahan kode.
- **B8 — Konstruktor Ollama publik, ditunda.** `Provider::ollama`/`ollama_default` bersifat publik dan secara teknis bisa dipanggil tanpa Registry (melewati akun/chain/audit). Hari ini tidak ada pemanggil seperti itu di produksi — satu-satunya jalur Ollama yang hidup adalah health check on-demand yang mensyaratkan akun terdaftar. Menjadikannya `pub(crate)` atau mewajibkan konstruksi lewat Registry adalah pekerjaan masa depan, bukan follow-up ini.
- **B9 — Kosmetik, ditunda.** `pair().account()` untuk Ollama berisi base URL (`http://127.0.0.1:11434`), bukan nama akun Registry; dan `ModelAccount::new` untuk ollama tetap menurunkan nama secret `model/ollama/<nama>` yang tidak pernah dipakai. Tidak ada perubahan perilaku; bila salah satunya menuntut perubahan kode, ia masuk sebagai follow-up sendiri dengan ADR-nya, bukan diselipkan di sini.
- Health check `GET /v1/models` OpenRouter yang publik (C5) diterima
  sebagai keterbatasan yang didokumentasikan, bukan sebagai bug yang
  harus diperbaiki sebelum merge.
- Duplikasi `default_base_url` (`config.rs` vs `model_ops.rs`)
  dipertahankan — sekarang dijaga oleh test kesepakatan literal di kedua
  sisi (C1), bukan dihapus: masing-masing sisi milik crate yang berbeda
  dan pemakaian bersama akan menciptakan dependensi yang tidak perlu.

## Consequences

- Operator mengonfigurasi Muse persis seperti vendor OpenAI-compatible
  lainnya — tidak ada yang perlu dipelajari selain satu resep env.
- Akun OpenRouter yang kehabisan kredit kini gagal dengan pesan yang
  menunjuk ke saldo, bukan ke jaringan dan bukan ke retry yang sia-sia.
- Vault key file adalah satu-satunya jalur kunci Vault yang
  didokumentasikan sebagai "yang dipilih"; inline tetap bekerja sebagai
  fallback dan tidak akan dihapus diam-diam.
- Ollama tetap bisa diaktifkan nanti tanpa perubahan skema — slotnya
  sudah ada (kind, health check, validasi loopback) — tetapi tidak ada
  yang mengklaim model lokal sudah berjalan hari ini.

## Alternatives rejected

- Kind `muse` (lihat §3 opsi a) — DITOLAK: melanggar "kind = bentuk
  API", preseden katalog kind, nama ambigu.
- Kind `openrouter` sekarang (lihat §3 opsi b) — DITUNDA: alias kosong
  tanpa header atribusi / pemetaan 402.
- 402 tetap transien — DITOLAK: retry/fallback atas akun tanpa kredit
  adalah pemborosan yang terlihat seperti bug jaringan.
- 402 dipetakan ke `AuthFailed` — DITOLAK: kunci tidak salah, saldonya
  yang habis; operator yang membaca `auth_failed` akan me-rotasi kunci
  yang sebenarnya sehat.

## Ditunda

- Kind `openrouter` — ADR masa depan bila header atribusi (C7) dan/atau
  pemetaan 402 diputuskan bernilai (402 sudah dipetakan di sini).
- Verifikasi live health check (C4) dan endpoint validasi kunci
  OpenRouter `GET /api/v1/key` (C5) — manual, di tangan user.
- Header atribusi OpenRouter `HTTP-Referer` / `X-Title` (C7) — butuh
  Config kustom / patch fork.
- Patch `ollama-rs` ke reqwest 0.13 agar guard bisa diinjeksikan
  (penyimpangan Ollama v1 diterima tertulis sampai saat itu).
