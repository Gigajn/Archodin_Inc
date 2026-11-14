# Cakupan provider v1: OpenAI-compatible + Gemini + Claude + Ollama; sisanya ditunda atau ditolak

## Context

Tujuan yang dinyatakan user: server bisa memakai "AI model apa pun".
Fase 1 membuktikan fondasinya sudah separuh ada: `Provider::openai_compatible`
(`agent.rs:39-54`) menutup banyak vendor dengan satu client Bearer, dan
konstruktor native Gemini serta Claude sudah ada dan teruji jalurnya.
Tetapi "apa pun" tidak bisa berarti "semua wrapper native sekaligus di v1":
tiap provider native membawa satu jenis kredensial, satu bentuk auth, satu
cara health check, dan — yang paling berbahaya — satu client HTTP sendiri
yang hari ini melewati `http_guard` (ADR 0025).

Fakta-fakta yang mengikat keputusan ini:

- Client native langchain-rust (Claude/Qwen/DeepSeek) memakai
  `reqwest::Client::new()` fresh per request: tanpa timeout 30 detik, tanpa
  cap 2 MiB, tanpa filter SSRF http-acl. Hanya Gemini yang lewat
  `http_guard::shared_client()`.
- `Claude::new`/`Qwen::new`/`DeepSeek::new` membaca env dengan
  `unwrap_or_default()` — kunci kosong lolos diam-diam.
- DeepSeek dan Qwen ADA di fork langchain-rust tetapi TANPA konstruktor
  `Provider`.
- Nama model di web saling bertentangan (Claude gen 4 vs 5, DeepSeek chat
  vs v4, GPT-5.4/5.5 vs GPT-6). Tidak ada katalog model hardcode yang bisa
  dipercaya; `GET /v1/models` adalah sumber kebenaran runtime.
- Indonesia TERVERIFIKASI didukung OpenAI/Anthropic/Gemini; provider lain
  belum terverifikasi — bukan diblokir, hanya belum dibuktikan.

## Decision

### Diadopsi di v1 (4 kind)

| Kind | Jalur | Catatan |
| --- | --- | --- |
| `openai_compatible` | `Provider::openai_compatible`, Bearer, base_url | Sudah ada; menutup OpenAI, OpenRouter, Groq, DeepSeek, Mistral, Together, xAI, Perplexity, Cohere-compat, Qwen-compat, Azure v1, LM Studio, vLLM. Validasi base_url/model/kunci kosong DITAMBAH di Fase 3 — hari ini tidak ada. |
| `gemini` | native `gemini.rs`, header `x-goog-api-key` | Sudah lewat `shared_client()`. Dipertahankan apa adanya. |
| `claude` | native, `x-api-key` + `anthropic-version: 2023-06-01` | Dipertahankan. WAJIB pindah ke `shared_client()` + `KeySource::Vault` di Fase 3 (ADR 0025). |
| `ollama` | native, tanpa auth | Kind tersedia sebagai wadah/slot opt-in untuk local LLM — feature AKTIF di `basileia/Cargo.toml` — tetapi SEMENTARA TIDAK DIPAKAI (keputusan user 2026-11-14, lihat subbagian Ollama di bawah); model aktif sekarang = Claude, Gemini, ChatGPT (OpenAI), Muse (via OpenRouter). Panic pada URL tidak valid DIHAPUS — menjadi `Result`. Base URL wajib lolos `validate_endpoint` + loopback-only; default `127.0.0.1:11434`. |

### Ollama: wadah opt-in, dicadangkan untuk nanti (keputusan user 2026-11-14)

Ollama / LLM lokal SEMENTARA TIDAK dipakai, tetapi wadahnya dibuat dulu
untuk nanti: kind `ollama` ada di Registry, konstruktor `Provider::ollama`
ada dan teruji, dan feature `ollama` aktif di `basileia/Cargo.toml:14`.
Tidak ada akun Ollama yang terkonfigurasi hari ini, dan tidak ada chain
yang terpasang ke Role — jadi tidak ada jalur produksi yang bisa memanggil
Ollama tanpa akun yang sengaja dikonfigurasi. Model aktif sekarang =
Claude, Gemini, ChatGPT (OpenAI), Muse (via OpenRouter).

**Cara mengaktifkan nanti** (contoh saja, bukan default kode):

```sh
BASILEIA_MODEL_LOCAL_KIND=ollama
BASILEIA_MODEL_LOCAL_MODEL=llama3.2   # contoh model; bukan default kode
# BASILEIA_MODEL_LOCAL_BASE_URL opsional; default http://127.0.0.1:11434
# BASILEIA_MODEL_LOCAL_KEY tidak dipakai: kind ollama tidak memakai kunci
```

**Status akun `"ok"` untuk ollama BUKAN berarti server hidup (B7).**
Daftar akun melaporkan `"ok"` bila akun tidak terkunci — dan karena ollama
tidak punya kunci, tidak ada yang bisa hilang, jadi `"ok"` di sini hanya
berarti "tidak terkunci". Kebenaran apakah server Ollama sedang jalan
hanya datang dari health check manual (`GET /api/tags`, ADR 0024) —
jangan baca `"ok"` sebagai "server hidup".

**Risiko yang diterima (B6):** pemeriksaan loopback berbasis string host
(`127.0.0.0/8`, `::1`, `localhost`), bukan IP hasil resolve. Arah amannya
ketat — nama host yang resolve ke loopback tetapi dieja lain (mis. entri
`/etc/hosts` kustom) ditolak — dan sisi risikonya adalah nama yang diterima
(`localhost`) dipercaya resolve ke loopback tanpa memeriksa hasil
resolve-nya. Itu dinilai rendah di home server, di mana operator
mengendalikan sendiri `/etc/hosts` dan DNS.

### Keputusan auth Claude (menutup temuan "belum terverifikasi")

Dok Anthropic kini menjadikan `Authorization: Bearer` bentuk utama dan
`x-api-key` fallback lama — **keduanya masih diterima**, dan
`anthropic-version: 2023-06-01` masih diterima (versi API terbaru
`2026-10-21` ada, tetapi versi lama tidak dimatikan). Verifikasi 2026-11-12
terhadap tiga sumber sekunder yang konsisten (tidak sempat membaca halaman
dok resmi Anthropic secara langsung — baca langsung tetap menjadi tugas
Fase 3 sebelum rilis).

**Keputusan: PERTAHANKAN `x-api-key` + `2023-06-01` di v1.** Alasan grill:
jalur ini yang sudah teruji di repo; mengganti header auth DAN versi API
sekaligus demi mengikuti dok baru adalah dua perubahan berisiko dengan
nol keuntungan yang terlihat user. Migrasi ke Bearer + versi API baru
adalah perubahan terpisah, teruji, di luar v1 — bukan bagian dari
pekerjaan auth ini.

### Azure: v1 lewat generic, legacy ditolak

Azure OpenAI v1 API (Bearer, `/openai/v1/`, nama DEPLOYMENT sebagai model)
sudah bekerja HARI INI lewat `openai_compatible` — didokumentasikan sebagai
resep, tanpa satu baris kode provider baru. Bentuk legacy (`api-key` +
`api-version` + path deployment) DITOLAK di v1: header non-standar dan query
versi menambah satu bentuk auth untuk nol provider yang membutuhkannya.

## Consequences

- "Apa pun" di v1 berarti: semua vendor dengan endpoint OpenAI-compatible
  + tiga native yang teruji. Itu sudah mencakup hampir semua model yang
  umum dipakai, tanpa satu pun wrapper baru.
- Ollama memberi jalan model lokal tanpa kunci — cocok untuk home server —
  dengan harga validasi endpoint yang ketat (URL Ollama yang menunjuk ke
  mana saja adalah SSRF berkedok konfigurasi).
- Claude v1 mensyaratkan patch fork (client injeksi, ADR 0025). Bila patch
  itu gagal, Claude TIDAK dikirim tanpa guard — cakupan v1 menyusut
  menjadi tiga kind, bukan keamanan yang menyusut.
- Tidak ada kode yang tahu nama model spesifik. Model adalah string
  konfigurasi; kebenarannya ditanyakan ke provider saat runtime.

## Alternatives rejected

- **Bedrock di v1.** DITUNDA, bukan ditolak selamanya. Kredensialnya jenis
  lain (SigV4 via AWS SDK: access key + secret + region + session token,
  bukan satu string Bearer), dan konstruktornya (`from_llm` dengan LLM
  pre-built) adalah pola yang ADR 0025 hapus. Membangun Bedrock berarti
  membangun satu sistem kredensial kedua sebelum sistem pertama terbukti.
- **Vertex AI di v1.** DITUNDA. Butuh OAuth2 + service-account JSON +
  refresh token — jenis kredensial ketiga, dengan mesin refresh-nya
  sendiri. Tidak ada kebutuhan user yang menyebut Google Cloud hari ini.
- **Wrapper native DeepSeek / Qwen.** DITOLAK. Keduanya sudah terjangkau
  lewat `openai_compatible` (DeepSeek dan Qwen menyediakan endpoint
  kompatibel). Wrapper native hanya menambah permukaan: konstruktor yang
  mengendus env + client tanpa guard, tanpa satu kemampuan yang tidak
  dimiliki jalur generic.
- **Wrapper native Mistral chat / Cohere native.** DITOLAK dengan alasan
  yang sama: endpoint OpenAI-compatible tersedia; tidak ada kemampuan
  khusus provider yang hilang.
- **Katalog model hardcode di server.** DITOLAK. Nama model berubah lebih
  cepat dari rilis server; katalog hardcode adalah kebohongan yang
  menunggu tanggal kedaluwarsa. `GET /v1/models` (dan setaranya per
  provider) adalah sumber kebenaran; Agora boleh menampilkannya sebagai
  pilihan, server tidak pernah mengklaim tahu daftar model.
- **Satu `Provider` generik "apa saja" yang menebak-nebak bentuk auth dari
  base_url.** DITOLAK. Menebak auth dari hostname adalah logika rapuh yang
  salah pada kasus pertama yang tidak terduga; kind dinyatakan eksplisit
  di konfigurasi.

## Ditunda

- Bedrock (SigV4 + Bedrock API key mode Bearer `ABSK…`) — setelah pola
  `KeySource` + guard client terbukti di v1, dan setelah diputuskan jenis
  kredensial non-string ditampung bagaimana.
- Vertex AI (OAuth2 + service-account JSON) — sama, menunggu kebutuhan
  nyata.
- Migrasi Claude ke `Authorization: Bearer` + API version `2026-10-21` —
  perubahan teruji terpisah; baca dok resmi Anthropic langsung dulu.
- Bentuk legacy Azure (`api-key` + `api-version`) — hanya bila ada
  deployment yang tidak bisa memakai v1 API.
