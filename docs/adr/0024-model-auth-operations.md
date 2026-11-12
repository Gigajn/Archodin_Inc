# Operasional kunci model: rotasi, revoke, health check, audit, dan Budget

## Context

Arsitektur hari ini sudah memberi hadiah yang jarang: `KeySource::Vault`
me-resolve tiap call, jadi mengganti nilai secret di Vault langsung berlaku
untuk call berikutnya tanpa restart. Tetapi tidak ada satu pun prosedur di
atasnya — tidak ada endpoint rotasi, tidak ada revoke, tidak ada health
check, tidak ada audit, dan tidak ada yang menghubungkan pemakaian model
dengan Budget meter yang sudah ada. Operator yang kuncinya bocor hari ini
tidak punya tombol apa pun untuk ditekan.

Health check juga tidak bisa disamaratakan begitu saja: tiap provider
menguji dirinya dengan cara berbeda, dan "ping" yang salah (mis. memanggil
endpoint inference untuk sekadar test) membakar token dan uang.

## Decision

### Rotasi — atomic swap, tanpa restart, tanpa dual-key

- Rotasi = `Vault::put` dengan nilai baru di bawah nama secret yang sama
  (sudah ada, `vault.rs:123`), lewat API Agora (admin) atau re-provision
  env + restart. Karena resolve terjadi tiap call, call berikutnya memakai
  kunci baru. **Tidak ada masa tenggang dual-key di v1**: satu nama, satu
  nilai, satu saat pergantian. Call yang sedang berjalan menyelesaikan
  dirinya dengan kunci lama; call baru memakai kunci baru.
- Setelah rotasi, Vault di-seal ulang dan blob di SQLite ditulis ulang
  (ADR 0021) — bila seal/persist gagal, rotasi DILAPORKAN gagal (jangan
  pernah dalam keadaan Vault baru tetapi disk masih menyimpan kunci lama).
- Rotasi dicatat di audit (lihat di bawah) dengan nama akun dan waktu,
  tanpa nilai lama maupun baru.

### Revoke — hapus, maka call baru gagal dengan error bertipe

- Revoke = `Vault::remove` + tandai akun `revoked` di Registry. Call baru
  ke akun itu gagal dengan `ModelKeyMissing` (ADR 0023) — bukan hang,
  bukan retry tanpa akhir, bukan fallback diam-diam ke akun lain.
- Revoke di server TIDAK mencabut kunci di sisi vendor: operator tetap
  harus me-revoke di dashboard provider (runbook ADR 0025). Server hanya
  berhenti memakai kunci itu.
- Menghapus akun dari Registry tanpa me-revoke kuncinya adalah operasi
  yang ditolak: hapus akun = revoke kunci. Tidak ada kunci yatim.

### Health check — on-demand, per provider, tanpa inference

Satu endpoint Agora (admin) + satu pemicu manual; TIDAK ADA health check
otomatis berkala di v1 (tidak ada background job yang menelepon provider
tanpa diminta — ia membakar kuota dan menulis log atas nama operator).

| Kind | Cara test | Keterangan |
| --- | --- | --- |
| `openai_compatible` | `GET /v1/models` | Daftar model; 401 = kunci salah. |
| `gemini` | `GET /v1beta/models` | Dengan header `x-goog-api-key`. |
| `claude` | `GET /v1/models` | Dengan `x-api-key` + `anthropic-version`. |
| `ollama` | `GET /api/tags` | Tanpa auth; gagal = server Ollama mati. |

- Semua health check lewat `shared_client()` (timeout 30 detik, cap
  2 MiB, guard SSRF). Hasilnya: status (ok / auth gagal / tidak
  terjangkau / model tidak ditemukan bila model default dicek), latency,
  waktu — dan TIDAK PERNAH body respons mentah (di-redact dulu).
- Health check TIDAK memblokir startup dan TIDAK dijalankan otomatis
  setelah provisioning: operator yang baru menambah akun menekannya
  sendiri. Satu tombol, satu hasil yang jujur.
- Hasil health check tidak di-cache di v1: setiap penekanan adalah
  panggilan live. Cache hanya menyembunyikan keadaan yang sudah berubah.

### Audit — siapa memakai kunci apa, kapan; nilai kunci tidak pernah tercatat

- Tabel append-only `model_audit`: waktu, aktor (admin Agora), aksi
  (`provision`, `rotate`, `revoke`, `health_check`, `auth_failed`,
  `call_failed`), kind provider, nama akun, nama model, hasil.
- Larangan keras: nilai kunci, sebagian nilai kunci, dan body error
  upstream mentah TIDAK PERNAH masuk audit. Semua teks yang bisa
  mengandung kunci dilewatkan `Vault::redact` dulu (ADR 0025).
- Audit pemakaian per-call (setiap Job mencatat setiap panggilan model)
  DITOLAK di v1 — terlalu berisik dan tidak menjawab pertanyaan operator
  ("kunci mana yang bocor / kapan terakhir dirotasi"). Yang dicatat adalah
  peristiwa lifecycle kunci dan kegagalan auth, bukan lalu lintas normal.

### Budget — pakai meter yang sudah ada, per akun provider

- Batas biaya per provider/kunci = Budget meter yang sudah ada (glossary:
  "Budget: token atau uang maksimum sebelum berhenti"). Budget diterapkan
  PER AKUN PROVIDER di jalur pemanggilan model, dihitung dari pemakaian
  yang dilaporkan provider / dihitung lokal dari token bila tersedia.
- Tidak ada sistem metering baru. Bila meter tidak bisa mengukur satu
  provider (respons tanpa pemakaian token), Budget akun itu dinyatakan
  "tidak terukur" secara eksplisit di Agora — bukan diam-diam dianggap nol.
- Melewati Budget = Job berhenti dan menunggu keputusan orang, sama
  seperti Budget Agen/Job yang sudah ada. Tidak ada auto-top-up, tidak ada
  pemotongan diam-diam.

## Consequences

- Operator punya empat tombol yang hari ini tidak ada: tambah, rotasi,
  revoke, test — semuanya tanpa restart, semuanya tercatat.
- Rotasi darurat saat kebocoran menjadi operasi satu langkah (ganti di
  vendor, rotasi di Agora), bukan operasi restart server di tengah Job.
- Health check memberi jawaban atas pertanyaan "apakah provider X
  benar-benar bisa dipakai dari Indonesia" dengan bukti (hasil live),
  bukan dengan klaim dokumentasi.

## Alternatives rejected

- **Dual-key / masa tenggang (kunci lama tetap berlaku N menit).**
  DITOLAK di v1: dua kunci aktif berarti dua permukaan bocor, dan logika
  "coba kunci baru, bila gagal pakai lama" menyembunyikan rotasi yang
  gagal. Atomic swap lebih sederhana dan kegagalannya terlihat.
- **Health check otomatis berkala.** DITOLAK: panggilan tanpa diminta
  membakar kuota/uang, dan kegagalan berkala yang ditulis ke log akan
  menenggelamkan sinyal yang penting. Operator menekan tombol saat ia
  butuh jawaban.
- **Health check dengan memanggil inference ("tanya 'hai' ke model").**
  DITOLAK: membakar token untuk pertanyaan yang tidak dijawab siapa pun;
  endpoint list-models/tag sudah cukup membuktikan auth + jangkauan.
- **Health check yang memblokir startup.** DITOLAK (alasan ADR 0020):
  server harus start saat provider mati.
- **Audit per-call / audit body respons lengkap.** DITOLAK: volume tanpa
  nilai; dan body respons adalah tempat kunci bocor paling sering
  bersembunyi (echo error dari provider). Audit mencatat peristiwa,
  bukan lalu lintas.
- **Sistem metering/biaya baru per provider.** DITOLAK: Budget sudah ada,
  sudah dipahami, dan sudah punya alur persetujuan manusia. Menduplikasi
  itu berarti dua definisi "batas" yang bisa bertentangan.

## Ditunda

- Rotasi otomatis terjadwal / rotasi paksa saat kunci berumur N hari —
  butuh kebijakan umur kunci yang belum diputuskan; rotasi manual +
  audit di v1.
- Notifikasi saat kunci mendekati kedaluwarsa (bila provider memberi
  tanggal kedaluwarsa) — menunggu satu provider yang benar-benar
  menyediakannya.
- Cache hasil health check — hanya bila frekuensi pengecekan menjadi
  masalah, yang tidak terjadi pada pemicu manual.
