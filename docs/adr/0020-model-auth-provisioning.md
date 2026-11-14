# Provisioning kunci model: env saat startup + API Agora, Vault sumber kebenaran

## Context

Produksi hari ini tidak membangun satu pun Provider: semua Role punya model
chain kosong (`assembly.rs:89,196-197`), dan `config.rs` tidak mengenal satu
pun env var kunci model — hanya `BASILEIA_BIND`/`BASILEIA_DB`/`ADMIN_USER`/
`ADMIN_PASSWORD`. Satu-satunya jalan kunci masuk ke proses adalah konstruktor
`Provider` yang dipanggil kode, dengan `KeySource::Fixed(String)` atau
`KeySource::Vault(Vault, secret)` (`agent.rs:168-171`).

Tiga fakta memaksa keputusan ini dibuat sebelum satu baris implementasi:

- `KeySource::Vault` me-resolve tiap call, jadi rotasi tanpa restart sudah
  didukung arsitektur — asal kunci memang hidup di Vault, bukan di `Fixed`.
- Sebaliknya, konstruktor native di fork langchain-rust (`Claude::new`,
  `Qwen::new`, `DeepSeek::new`) diam-diam membaca env
  (`CLAUDE_API_KEY`/`QWEN_API_KEY`/`DEEPSEEK_API_KEY`) dengan
  `unwrap_or_default()`: kunci kosong menjadi string kosong yang lolos
  diam-diam, dan sumber kunci tersebar di luar kendali server.
- User menyiapkan agen/LLM wiring-nya sendiri. Desain ini hanya mengatur
  bagaimana kredensial masuk, disimpan, dan dipakai — bukan wiring agen.

Pertanyaan yang dijawab ADR ini: dari mana kunci berasal, apa sumber
kebenaran di dalam proses, dan apakah menambah provider butuh restart.

## Decision

1. **Sumber kebenaran di dalam proses adalah Vault, dan hanya Vault.**
   Setiap Provider produksi membawa `KeySource::Vault`. `KeySource::Fixed`
   hanya untuk test dan untuk bootstrap sesaat di startup (lihat butir 4) —
   tidak pernah untuk Provider yang melayani Job.
2. **Provisioning bootstrap: env var saat startup.** Satu akun model = satu
   kelompok env var dengan awalan `BASILEIA_MODEL_`:
   - `BASILEIA_MODEL_<ACCOUNT>_KEY` — wajib bila akun itu didefinisikan;
     nilai tidak pernah di-log, termasuk saat kosong (pesan error menyebut
     nama var, bukan nilainya).
   - `BASILEIA_MODEL_<ACCOUNT>_KIND` — `openai_compatible`, `gemini`,
     `claude`, atau `ollama` (cakupan v1, ADR 0022). Wajib.
   - `BASILEIA_MODEL_<ACCOUNT>_BASE_URL` — opsional; bila kosong dipakai
     default per kind. Wajib divalidasi `http_guard::validate_endpoint`
     bila diisi.
   - `BASILEIA_MODEL_<ACCOUNT>_MODEL` — opsional; model default akun itu.
     Boleh kosong bila model ditetapkan per Role (ADR 0023).
   - `<ACCOUNT>` adalah slug huruf kecil (`main`, `groq_cepat`), bukan nama
     provider — satu provider boleh punya beberapa akun.
   Env dibaca sekali saat startup. Mengubah env butuh restart; itu diterima
   (lihat butir 5 untuk jalan tanpa restart).
3. **Provisioning runtime: API Agora, tanpa restart.** Endpoint Agora
   (auth admin yang sama dengan login, ADR 0007/KR-07) untuk menambah,
   mengubah, dan menghapus akun model. Setiap penulisan masuk ke Vault,
   lalu dipersist sebagai sealed blob (ADR 0021) dan didaftarkan di Model
   Registry (ADR 0023). **Menambah provider baru TIDAK butuh restart.**
   Server yang sedang menjalankan Job tidak boleh dimatikan hanya karena
   operator menambah kunci.
4. **Jalur env ke Vault harus singkat dan terlihat.** Saat startup, kunci env
   langsung `put` ke Vault di bawah nama `model/<kind>/<account>`, lalu
   string env dibiarkan mati bersama `Config` startup — tidak disimpan di
   `App`, tidak disalin ke struktur lain. Tidak ada struct yang menyimpan
   kunci plaintext di luar Vault.
5. **Validasi saat provisioning adalah validasi bentuk, bukan jaringan.**
   Kunci tidak boleh kosong, `base_url` harus lolos `validate_endpoint`,
   nama model tidak boleh kosong bila diisi. TIDAK ada panggilan jaringan
   saat startup: server harus bisa start saat provider mati atau internet
   putus. Kesehatan jaringan diuji on-demand (ADR 0024).

## Consequences

- Ada dua jalan masuk kunci (env + Agora) tetapi satu tempat hidup
  (Vault): semua aturan rotasi, revoke, redact, dan audit (ADR 0024,
  0025) cukup ditulis sekali, terhadap Vault.
- Operator yang hanya memakai env mendapat perilaku paling sederhana:
  restart = re-provision, tanpa file rahasia baru untuk dijaga.
- Operator yang memakai Agora mendapat penambahan tanpa restart, dengan
  harga satu kunci Vault stabil (ADR 0021) — tanpa kunci itu, akun yang
  ditambahkan lewat Agora hilang saat restart dan server start dalam
  keadaan degraded yang eksplisit (peringatan di log, status di Agora).
- Skema penamaan env baru ini sengaja TIDAK memakai nama env bawaan
  tiap SDK (`ANTHROPIC_API_KEY`, dsb.): satu skema untuk semua provider
  berarti satu tempat untuk diaudit, dan konstruktor yang mengendus env
  sendiri menjadi tidak terpanggil.

## Alternatives rejected

- **File config (TOML/YAML/JSON) sebagai sumber kebenaran.** Ditolak:
  file config mudah ikut ter-commit, mudah di-copy ke mesin lain, dan
  isinya drift dari Vault tanpa ada yang tahu. Nama model dan base URL
  boleh hidup di Registry (bukan rahasia, ADR 0023); kunci tidak.
- **Satu env var generik berisi semua kunci (mis. JSON di satu var).**
  Ditolak: satu var bocor = semua kunci bocor sekaligus; rotasi satu akun
  memaksa menulis ulang semuanya; error "var tidak ada" tidak menunjuk
  akun mana.
- **Membiarkan tiap konstruktor native membaca env-nya sendiri
  (`CLAUDE_API_KEY`, dst.).** Ditolak: sumber kunci tersebar, kunci kosong
  lolos diam-diam via `unwrap_or_default()`, dan tidak ada jalan rotasi
  tanpa restart. Konstruktor semacam itu dilarang di jalur produksi
  (ADR 0025).
- **Provisioning lewat Skill atau konten tidak tepercaya.** Ditolak tanpa
  diskusi: kunci hanya masuk lewat env (operator) atau Agora (admin
  login). Skill, Job, dan jawaban model tidak pernah boleh menulis kunci.
- **Provisioning hanya lewat Agora (tanpa env).** Ditolak: server fresh
  install tidak punya apa-apa sebelum admin sempat login dan mengisi;
  env adalah jalan bootstrap yang bekerja sebelum Agora ada isinya.
- **Provisioning hanya lewat env (tanpa Agora).** Ditolak: setiap tambah
  provider butuh restart, dan restart membunuh Job yang sedang jalan.
  Itu harga yang tidak perlu dibayar.

## Ditunda

- Impor kunci massal (bulk import file) — operator menambah akun satu per
  satu lewat Agora di v1.
- Integrasi secret manager eksternal (1Password, Vault HashiCorp, dsb.) —
  env + Agora cukup untuk satu home server; secret manager adalah
  pengganti env, bukan tambahan, bila suatu hari dibutuhkan.
- Dukungan multi-tenant / kunci per pengguna Agora — v1 hanya punya satu
  admin (KR-07).
