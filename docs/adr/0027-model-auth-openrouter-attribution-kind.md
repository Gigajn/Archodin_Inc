# Follow-up model auth: kind `openrouter` + header atribusi OpenRouter

## Context

ADR 0026 §3 menunda kind `openrouter` sebagai "ADR masa depan" dengan
syarat: kind itu baru bernilai bila header atribusi dikirim dan/atau
pemetaan 402 diputuskan. Pemetaan 402 sudah dikerjakan di follow-up itu.
Header atribusi (C7) tertunda karena premisnya: mengirim `HTTP-Referer` /
`X-Title` "butuh Config kustom atau patch fork".

User menjawab 2026-11-14 21:39 WIB:

1. **Header atribusi: "IYA PERLU".** Keputusan desain yang ditunggu
   C7 kini ada — header dibangun di ADR ini.
2. **C4/C5: "okee".** Verifikasi live health check (C4) dan potensi
   positif palsu health check OpenRouter (C5) diterima sebagai
   keterbatasan yang didokumentasikan, bukan gate rilis — dicatat di
   sini sebagai konteks, tanpa perubahan perilaku health check.

## Koreksi premis ADR 0026 (C7)

Premis "butuh Config kustom atau patch fork" **salah untuk header**:
`OpenAIConfig::with_header` adalah API publik async-openai 0.42.3
(`config.rs:168`, diverifikasi di source crate) — header kustom
di-merge dengan prioritas menang dan ikut di semua request. Tidak ada
patch fork yang diperlukan untuk header atribusi. (Patch fork yang ada
di ADR 0025 adalah soal injeksi `shared_client()` yang di-guard —
hal yang berbeda — dan tidak berubah di sini.)

## Decision

### D1 — Mekanisme: `OpenAIConfig::with_header`, tanpa patch fork

Header dikirim lewat Config kustom `OpenAIConfig::with_header` (API
publik async-openai 0.42.3). Titik injeksi tunggal:
`Provider::llm()` (`crates/archodin-graph/src/agent.rs`) saat
membangun config — setelah `with_api_base` / `with_api_key`, bila
`attribution` ada, kedua header ditambahkan via `with_header`.
**Tanpa patch fork** — koreksi atas C7 di atas.

### D2 — Nilai: konstanta, tanpa env var baru

- `HTTP-Referer` = `https://github.com/Gigajn/Archodin_Inc`
- `X-OpenRouter-Title` = `Basileia` (nama modern; `X-Title` hanya
  alias lama — **jangan** kirim aliasnya)

Nilai adalah konstanta v1, tanpa env var baru. Validasi di
`Attribution::new`: referer harus URL http(s) (ditolak bila kosong
atau bukan URL), title tidak boleh kosong / whitespace-only; keduanya
melewati `HeaderValue::from_str` (menolak karakter kontrol). Error
menyebut nama field, bukan nilai.

### D3 — Kind `openrouter` DIBUAT; kind `muse` TETAP DITOLAK

Syarat ADR 0026 §3(b) terpenuhi karena header diputuskan perlu — kind
`openrouter` kini ada sebagai kind kelima (`registry.rs`:
`ModelKind::OpenRouter`, ejaan `"openrouter"`, default base URL
`https://openrouter.ai/api/v1` di kedua `default_base_url`).
Kind `muse` **tetap ditolak** eksplisit: kind adalah bentuk API,
bukan nama model — Muse tetap nama model (`MODEL=meta/muse-spark-open`)
di akun kind `openrouter`.

Resep env kini:

```sh
BASILEIA_MODEL_MUSE_KEY='sk-canary-…'
BASILEIA_MODEL_MUSE_KIND='openrouter'
BASILEIA_MODEL_MUSE_MODEL='meta/muse-spark-open'
```

`BASE_URL` opsional (default per kind). Resep lama
(`openai_compatible` + `BASE_URL=https://openrouter.ai/api/v1`)
**tetap bekerja** — akun itu hanya tidak mengirim header atribusi;
operator yang mau atribusi memindahkan `KIND` ke `openrouter`.
Pesan error kind tak dikenal di `model_ops.rs` dan `config.rs` kini
menyebut lima kind dan mengarahkan pengguna OpenRouter ke kind
`openrouter` (bukan lagi ke `openai_compatible` + `BASE_URL`).

### D4 — Cakupan: HANYA kind `openrouter`

- Health check **tidak** mengirim header atribusi: health check
  memakai Bearer saja (`model_ops.rs: health_check`), dan test
  mockito menegaskan kedua header absen di sana.
- Gemini / Claude / Ollama tidak dapat header.
- Jalur embedding tidak disentuh.
- `X-OpenRouter-Categories` / `X-OpenRouter-App-Visibility` tidak
  dikirim — di luar yang disetujui user.

### D5 — Keamanan

- Nilai header **bukan rahasia** (URL repo publik + nama app):
  boleh muncul di `Debug` dan teks error; jangan log header
  wholesale (aturan umum, bukan karena rahasia).
- **Fail-closed**: bila `with_header` gagal, `llm()` mengembalikan
  error — jangan pernah kirim request OpenRouter tanpa atribusi.
- Jangan tulis kunci asli di mana pun — hanya `sk-canary-*`
  (aturan yang sudah berlaku, ADR 0021/0025).

### D6 — Seam: struct `Attribution` + field di `Provider`

- Struct baru `Attribution` (`crates/archodin-graph/src/attribution.rs`):
  `referer` + `title` (`String`), `Attribution::default()` (nilai D2),
  `Attribution::new(referer, title) -> Result<Self, AttributionError>`
  (validasi D2), `headers(&self) -> [(HeaderName, HeaderValue); 2]`.
- Field `attribution: Option<Attribution>` di `Provider`;
  konstruktor publik baru `Provider::openrouter` (+
  `Provider::try_openrouter`, cermin `openai_compatible` /
  `try_openai_compatible`): kind `Some(ModelKind::OpenRouter)`,
  `key_required: true`, `native: None`,
  `attribution: Some(Attribution::default())`. Semua konstruktor lain
  mengisi `None`.
- `llm()` mengirim header bila `attribution.is_some()` (D1).
- `ModelAccount` tidak berubah (tidak ada field attribution).
- `Provider` sengaja NOT serializable — tidak berubah (ADR 0025 §4).

### D7 — Tanpa perubahan skema DB dan tanpa perubahan API Agora

Akun `openrouter` mengalir lewat jalur provisioning / Registry /
health check yang sudah ada (`validate_provision`,
`parse_model_accounts`, `ModelAccount`). Tidak ada tabel baru,
tidak ada kolom baru, tidak ada endpoint Agora baru. Satu-satunya
penambahan lintas crate adalah lengan `OpenRouter` di kedua
`default_base_url` (dijaga test kesepakatan literal seperti biasa,
C1) dan lengan bearer di `health_check`.

## Catatan verifikasi dan keterbatasan

- Semua verifikasi lewat mockito, bukan terhadap OpenRouter sungguhan
  (C4 tetap terbuka): test wire menegaskan kedua header ada dan
  nilainya benar pada chat completion kind `openrouter`, dan absen
  pada kind `openai_compatible` (negative test terhadap bocor ke
  vendor lain) serta absen pada health check kind `openrouter`.
- Test baru: 5 unit `Attribution` (`default`, `new` terima/tolak),
  2 test wire (`openrouter_attribution.rs`), 1 unit
  `try_openrouter`, 1 test provision kind `openrouter`, 1 test pesan
  error kind, 1 test URL health check kind `openrouter` (satu segmen
  `/v1`); 3 test diperbarui (round-trip kind, anti-drift
  `default_base_url` di kedua crate, pesan error kind di kedua crate).
  Baseline 416 test tidak ada yang dihapus — yang berubah hanya
  perilakunya.
- Health check OpenRouter masih bisa positif palsu (C5, user menjawab
  "okee"): `GET /v1/models` bersifat publik dan tidak memvalidasi
  kunci. `ok` di sana bukan bukti kunci valid.

## Risiko dan temuan minor yang diterima/ditunda

- **Header `http-referer` ditulis lowercase di kode.**
  `HeaderName::from_static("http-referer")` dan
  `HeaderName::from_static("x-openrouter-title")` adalah ejaan
  kanonis HTTP (nama header case-insensitive); OpenRouter menerima
  `HTTP-Referer` / `X-OpenRouter-Title` dalam bentuk apa pun.
  Test wire memakai ejaan yang sama dengan yang dikirim.
- **Nilai atribusi tidak bisa diubah per akun di v1.** Operator yang
  butuh referer/title berbeda per akun harus menunggu env override
  (lihat "Ditunda"). Default `Basileia` adalah satu-satunya nilai
  sampai saat itu.
- **Test `sessions_expire` flaky di bawah beban paralel.**
  Ditemukan saat verifikasi ADR ini (bukan oleh perubahan ini —
  tidak ada kode sesi/auth yang disentuh): lifetime 50 ms vs
  `sleep` 80 ms bisa kalah oleh penjadwalan saat 48 suite berjalan
  paralel di VM berbagi CPU. Lolos 3/3 saat dijalankan sendiri.
  Ditunda sebagai perbaikan test, bukan gate ADR ini.

## Consequences

- Akun OpenRouter kini punya kind sendiri dengan perilaku nyata
  (header atribusi), bukan lagi alias kosong dari
  `openai_compatible` — premis penundaan ADR 0026 §3(b) tidak
  berlaku lagi.
- Operator mengonfigurasi Muse lewat `KIND=openrouter` tanpa
  `BASE_URL`; resep lama tetap jalan tanpa atribusi.
- Akun `openrouter` yang kehabisan kredit tetap gagal sebagai
  `payment_required` (402, ADR 0026 §4) — tidak berubah.

## Alternatives rejected

- **Patch fork async-openai untuk header** — DITOLAK: `with_header`
  API publik sudah cukup (koreksi C7). Patch fork menambah beban
  merge upstream tanpa perilaku tambahan.
- **Header di `shared_client()` (guarded client)** — DITOLAK:
  client itu dipakai semua jalur HTTP keluar (Ronin, MCP, health
  check) — header atribusi akan bocor ke vendor lain dan ke
  health check, melanggar D4.
- **Menebak OpenRouter dari `base_url`** — DITOLAK: melanggar ADR
  0022 (jangan menebak dari URL); kind adalah fakta konfigurasi,
  bukan tebakan.
- **Kirim header ke semua `openai_compatible`** — DITOLAK: identitas
  Basileia akan bocor ke vendor lain (Groq, DeepSeek, …) yang
  tidak memintanya; atribusi hanya bermakna untuk OpenRouter.
- **Env var untuk nilai atribusi** — DITOLAK untuk v1: nilai
  konstanta sudah benar (repo URL + nama app); env menambah
  permukaan konfigurasi tanpa kebutuhan.
- **`X-OpenRouter-Categories` / `X-OpenRouter-App-Visibility`** —
  DITOLAK untuk v1: di luar yang disetujui user; kategori/visibilitas
  aplikasi bukan bagian dari keputusan header.

## Ditunda

- Endpoint validasi kunci OpenRouter `/api/v1/key` untuk health
  check (C5) — perlu verifikasi live dulu apakah endpoint itu ada
  dan apa yang dikembalikannya; manual, di tangan user.
- Jalur embedding: atribusi di sana belum diputuskan.
- Env override nilai atribusi (referer/title per akun).
- Perbaikan flakiness test `sessions_expire` (lihat di atas).
