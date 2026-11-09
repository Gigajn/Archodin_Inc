# SRS & Cetak Biru Infrastruktur — Autonomous AI Holding Company (AAHC)

## Catatan Rekonsiliasi Milestone 0

AAHC telah berganti nama menjadi **Basileia**. Dokumen ini dipertahankan sebagai baseline kebutuhan dan riwayat implementasi lama; path dan isi historis belum dimigrasikan seluruhnya. Batas Archodin–Basileia mengikuti `docs/adr/0001-archodin-basileia-boundary.md`; rencana pemisahan repository mengikuti ADR 0002. Kepemilikan informasi diatur dalam `docs/agents/domain.md`.

Keputusan yang telah dikonfirmasi dalam `Goals.md`, bagian **Confirmed Product Scope**, menggantikan asumsi berikut pada isi lama di bawah:

- Empat divisi bukan struktur wajib; masing-masing merupakan contoh atau kandidat pengembangan.
- Agora operasional awal berupa dashboard. Kantor visual 2D menjadi target lanjutan, bukan syarat operasi pertama.
- Free tier dan model lokal merupakan pilihan optimasi, bukan jaminan biaya mendekati nol atau failover tanpa gangguan. Perpindahan ke layanan berbayar tetap memerlukan otorisasi sesuai kebijakan Budget.

Rancangan FastAPI/LangGraph, pilihan dependency, dan klaim status implementasi di bawah perlu direkonsiliasi dengan target Rust dan bukti kode/pengujian saat ini. Ini tidak berarti seluruh kebutuhan lama dibuang atau seluruh fiturnya sudah terverifikasi. Ticket lama belum otomatis dinilai ulang melalui perubahan catatan ini.

**Versi Dokumen:** 2.0 (Enterprise Local-First Edition)
**Arsitektur Target:** Localhost Workstation / Mini-Server 24/7 dengan Antarmuka Visual 2D Pixel-Art
**Status:** Fase 1 (fondasi) diimplementasikan — lihat catatan status di akhir dokumen.

---

## 1. Pendahuluan & Ruang Lingkup Sistem

**Autonomous AI Holding Company (AAHC)** adalah platform orkestrasi *multi-agent* otonom yang dirancang untuk menjalankan operasional berbagai lini bisnis digital secara simultan di infrastruktur lokal dengan biaya penggunaan model bahasa (LLM) mendekati nol rupiah melalui mekanisme *smart free-tier routing*. Sistem ini menggabungkan mesin eksekusi *backend* dengan antarmuka visual ruang kerja 2D tempat setiap agen direpresentasikan sebagai karyawan virtual yang memiliki meja kerja, perangkat komputer, dan tugas spesifik.

### 1.1 Empat Pilar Operasional Utama

1. **Divisi R&D dan Software House:** Pengembangan aplikasi *Web* (Backend/Frontend), *Mobile Apps*, *Machine Learning Text* (NLP/RAG/Dataset), dan *Machine Learning Image* (Computer Vision/Generative Pipeline) secara iteratif di dalam lingkungan *sandbox* terisolasi.
2. **Divisi Quant & Automated Trading:** Pengelolaan perdagangan aset (Crypto/Forex/Saham) dengan arsitektur **1 Karyawan = 1 Pair** yang diawasi oleh agen *Chief Risk Officer* (CRO) sebagai pemegang otoritas eksekusi.
3. **Divisi Media & Affiliate Marketing:** Otomasi riset tren (seperti *GitHub Trending*), pembuatan *copywriting landing page* untuk monetisasi Facebook Pro, serta kurasi produk afiliasi Shopee dan TikTok.
4. **Divisi Omnichannel Customer Service (CS):** Layanan pelanggan reaktif 24/7 yang terhubung dengan *knowledge base* perusahaan untuk menjawab pesan masuk dan melakukan *closing* penjualan.

---

## 2. Arsitektur Infrastruktur Lokal 24/7 (Infrastructure Blueprint)

Agar sistem dapat berjalan 24 jam penuh di komputer lokal tanpa mengorbankan keamanan sistem operasi utama (terutama saat agen *developer* mengeksekusi kode buatan sendiri), infrastruktur dibangun di atas arsitektur **Micro-Containerized Local Stack**.

### 2.1 Topologi Jaringan & Kontainerisasi

```text
[ Internet / API Eksternal ]
   ├── LLM Gratis (Groq, OpenRouter, Gemini, GitHub Models, Cerebras)
   ├── Bursa & Market Data (Binance WebSocket / CCXT / TradingView)
   └── Saluran Publikasi & CS (FB Graph API, GitHub API, Telegram/WA Webhook)
          │
          ▼ (Melalui Cloudflare Tunnel / Ngrok untuk Webhook Masuk)
================================================================================
[ HOST MACHINE (PC Lokal / Home Server - Berjalan 24/7) ]
   │
   ├── 1. FRONTEND ENGINE (Port 3000)
   │      └── Phaser.js 2D Virtual Office + Xterm.js (Live Code Terminal)
   │
   ├── 2. CORE ORCHESTRATOR - FastAPI & WebSocket Hub (Port 8000)
   │      ├── AI HRD Architect (Pencetak Divisi & Karyawan Dinamis)
   │      ├── Smart 9Router Engine (LiteLLM + Cooldown State Manager)
   │      └── 4 Execution Engines (LangGraph, QuantStream, WebhookCS, CronBatch)
   │
   ├── 3. DATA & MESSAGE BROKER LAYER
   │      ├── SQLite (WAL Mode) / PostgreSQL : Penyimpanan Konfigurasi & Ledger
   │      ├── Redis (Port 6379)              : Antrean Tugas & Pub/Sub Status UI
   │      └── ChromaDB (Port 8001)           : Memori Jangka Panjang & SOP CS
   │
   ├── 4. ISOLATED DEV SANDBOX (Docker-in-Docker / Restricted Containers)
   │      ├── Container Worker: Web & App Builder (Python, Node, PHP, Flutter)
   │      └── Container Worker: ML Lab (PyTorch, Scikit-Learn, ComfyUI API)
   │
   └── 5. LOCAL LLM FALLBACK (Port 11434)
          └── Ollama Server (Qwen2.5-Coder:7b / Llama3.2:3b) - Cadangan Mati Internet/Limit
================================================================================
```

### 2.2 Spesifikasi Perangkat Keras (Hardware Requirements)

| Komponen | Spesifikasi Minimum (Fokus Cloud Free-Tier API) | Spesifikasi Rekomendasi (Full Local ML & Dev Sandbox) |
| --- | --- | --- |
| **Prosesor (CPU)** | Intel Core i5 / AMD Ryzen 5 (4 Core / 8 Thread) | Intel Core i7 / AMD Ryzen 7 (8 Core / 16 Thread) |
| **Memori (RAM)** | 16 GB DDR4 (Cukup untuk 30+ agen berbasis API) | 32 GB DDR4/DDR5 (Untuk *compile* App & Ollama Lokal) |
| **Kartu Grafis (GPU)** | Integrated GPU (Beban AI ditanggung 9Router Cloud) | NVIDIA RTX 3060 / 4060 (VRAM 8GB–12GB untuk ML Image/Text) |
| **Penyimpanan** | 256 GB NVMe SSD | 1 TB NVMe SSD (Untuk *dataset* ML, *docker image*, & *log*) |
| **Daya & Jaringan** | UPS *Backup* + Koneksi Internet Stabil 20 Mbps | Smart Plug (Auto-boot BIOS saat listrik nyala) + Dual ISP |

---

## 3. Kebutuhan Fungsional (Functional Requirements - SRS)

### 3.1 Modul AI HRD & Manajemen Organisasi Dinamis (`FR-HRD`)

* **FR-HRD-01 (Auto-Provisioning Divisi):** Sistem harus mampu menerima *input* teks tujuan bisnis dari pengguna, lalu menghasilkan struktur JSON berisi jumlah agen optimal, deskripsi peran, *system prompt*, jadwal kerja, dan kebutuhan *tools*.
* **FR-HRD-02 (Auto-Layout Ruangan 2D):** Setiap kali divisi atau karyawan baru disimpan ke database, sistem harus otomatis menghitung koordinat kosong pada peta berlantai kayu dan memunculkan aset meja, komputer, kursi, serta karakter karyawan di layar.
* **FR-HRD-03 (Kloning Spesialis Cepat):** Sistem harus menyediakan fitur duplikasi instan (khususnya untuk *Trading Desk*), di mana pengguna cukup memasukkan daftar *pair* (misal: `["BTC/USDT", "ETH/USDT", "SOL/USDT"]`) dan sistem langsung mencetak 3 agen identik dengan penugasan *pair* yang berbeda.

### 3.2 Modul Smart 9Router & Manajemen Kuota (`FR-RTR`)

* **FR-RTR-01 (Klasifikasi Tugas Otomatis):** Router wajib mengarahkan permintaan agen ke klaster model yang sesuai:
  * `CLUSTER_CODE`: Qwen-2.5-Coder-32B (OpenRouter Free) → Gemini 2.5 Pro/Flash → Ollama Lokal.
  * `CLUSTER_FAST`: Groq (Llama-3.3-70B) → Cerebras → Groq Akun Ke-2/Ke-3.
  * `CLUSTER_REASONING`: DeepSeek-R1 (OpenRouter/Groq Free) → Gemini 2.5 Flash Thinking.
  * `CLUSTER_VISION`: Gemini 2.5 Flash → Llama-3.2-11B-Vision.
* **FR-RTR-02 (Zero-Downtime Failover):** Apabila sebuah *endpoint* API mengembalikan kode HTTP `429` (*Rate Limit*) atau `503`, router harus memindahkan *request* ke jalur berikutnya dalam waktu < 200 ms dan mengunci jalur yang terkena limit ke status *cooldown* sesuai header `Retry-After`.

### 3.3 Modul Software House & ML Engineering (`FR-DEV`)

* **FR-DEV-01 (Eksekusi Kode Terisolasi):** Agen *developer* (Web, App, ML Text, ML Image) tidak boleh mengeksekusi perintah `bash` atau menulis *file* langsung di sistem operasi utama, melainkan harus di dalam folder `/workspace_sandboxes/{nama_proyek}` yang terhubung ke kontainer Docker terisolasi.
* **FR-DEV-02 (Siklus Self-Debugging):** Jika agen Tester menemukan *error* pada output terminal saat menjalankan kode yang ditulis agen Programmer, sistem harus mengembalikan *log error* tersebut kepada agen Programmer untuk diperbaiki secara otomatis (maksimal 5 iterasi sebelum meminta bantuan manusia).
* **FR-DEV-03 (Pipeline ML Otomatis):** Agen ML harus memiliki akses ke *tool* pengunduh dataset, skrip *preprocessing*, pelatihan model, dan pencatatan metrik evaluasi (Loss/Accuracy/F1-Score) ke dashboard UI.

### 3.4 Modul Quant Trading Per-Pair (`FR-TRD`)

* **FR-TRD-01 (Dedikasi 1 Karyawan = 1 Pair):** Setiap agen *trader* hanya berlangganan (*subscribe*) pada satu kanal *stream* data pasar (contoh: `btcusdt@kline_15m`) agar konteks analisisnya tidak tercampur dengan aset lain.
* **FR-TRD-02 (Otorisasi Risk Gatekeeper):** Agen *pair* hanya dapat mengeluarkan objek `OrderProposal`. Eksekusi ke API bursa hanya dapat dilakukan oleh **Agen Risk Manager** setelah memvalidasi rumus ukuran posisi (*position sizing*):

  Position Size = (Total Equity × Max Risk Per Trade %) / |Entry Price − Stop Loss Price|

* **FR-TRD-03 (Paper vs Live Switch):** Setiap meja *trading* harus memiliki sakelar status `PAPER_TRADING` (simulasi saldo lokal) dan `LIVE_TRADING` (uang asli via API Key bursa).

### 3.5 Modul Visualisasi Kantor 2D & Kontrol Interaktif (`FR-UI`)

* **FR-UI-01 (Pemetaan Status Visual ke Sprite):** Kanvas Phaser.js harus merender ruangan kantor dengan detail meja kerja, monitor komputer, kursi, tanaman hias, rak buku, dan tumpukan kardus, serta mengubah animasi karakter berdasarkan pesan WebSocket:
  * `IDLE`: Karakter duduk diam di kursi menghadap meja atau berdiri di lorong.
  * `WORKING`: Animasi tangan mengetik di depan komputer/laptop dengan indikator layar menyala.
  * `HANDOFF`: Karakter berjalan melintasi lantai kayu dari mejanya menuju meja agen berikutnya untuk menyerahkan data.
  * `ERROR / LIMIT`: Muncul balon ikon peringatan di atas kepala karakter.
* **FR-UI-02 (Live Inspector Panel):** Ketika pengguna mengklik salah satu karakter atau meja di layar, UI harus menampilkan panel samping berisi: konfigurasi *prompt*, *log* pikiran agen (*Chain-of-Thought*) secara *real-time*, penggunaan token, dan layar terminal mini (khusus divisi Developer).

---

## 4. Kebutuhan Non-Fungsional (Non-Functional Requirements - NFR)

1. **NFR-SEC-01 (Keamanan Kunci Finansial):** Kunci API bursa *trading* wajib disimpan dalam variabel lingkungan terenkripsi dan **dilarang keras** memiliki izin penarikan dana (*Withdrawal Permission Disabled* pada pengaturan API bursa).
2. **NFR-AVL-01 (Pemulihan Otomatis 24/7):** Seluruh proses *backend* harus dijalankan dengan kebijakan `restart: always` pada Docker atau dikelola oleh *process manager* (`PM2` / `systemd`), sehingga jika terjadi *crash* atau komputer baru menyala kembali, seluruh lantai kantor langsung beroperasi kembali.
3. **NFR-PRF-01 (Efisiensi Memori Agen):** Agen tidak boleh disimpan sebagai proses terpisah yang memakan RAM saat menganggur. Agen harus disimpan sebagai baris konfigurasi di database dan hanya diinstansiasi ke dalam memori (*stateless execution*) saat gilirannya bekerja.

---

## 5. Skema Database Inti (SQLite / PostgreSQL DDL)

File migrasi awal: `backend/schema.sql` (implementasi Fase 1). Tabel: `divisions`, `employees`, `workflow_steps`, `router_api_pool`, `trading_ledger`, `skills_registry`.

```sql
-- 1. Tabel Lantai & Divisi Perusahaan
CREATE TABLE divisions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    floor_number INTEGER NOT NULL,
    execution_mode TEXT CHECK(execution_mode IN ('DAG_DEV', 'STREAM_TRADE', 'WEBHOOK_CS', 'CRON_BATCH')),
    cron_schedule TEXT,
    is_active BOOLEAN DEFAULT 1,
    room_grid_bounds TEXT -- JSON: {"x": 0, "y": 0, "w": 12, "h": 10}
);

-- 2. Tabel Karyawan (Agen AI)
CREATE TABLE employees (
    id TEXT PRIMARY KEY,
    division_id TEXT REFERENCES divisions(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    role_title TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    model_cluster TEXT CHECK(model_cluster IN ('CLUSTER_CODE', 'CLUSTER_FAST', 'CLUSTER_REASONING', 'CLUSTER_VISION')),
    allowed_tools TEXT NOT NULL, -- JSON Array: ["github_search", "docker_bash"]
    assigned_pair TEXT,          -- Khusus divisi trading, misal: "BTC/USDT"
    sprite_asset TEXT NOT NULL,  -- Contoh: "char_brown_hair_white_shirt"
    desk_coord_x INTEGER NOT NULL,
    desk_coord_y INTEGER NOT NULL,
    current_state TEXT DEFAULT 'IDLE'
);

-- 3. Tabel Alur Kerja (SOP Berantai)
CREATE TABLE workflow_steps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    division_id TEXT REFERENCES divisions(id) ON DELETE CASCADE,
    step_order INTEGER NOT NULL,
    from_employee_id TEXT REFERENCES employees(id),
    to_employee_id TEXT REFERENCES employees(id),
    validation_rule TEXT -- Syarat agar tugas bisa lanjut ke meja berikutnya
);

-- 4. Tabel Pool API Gratis (9Router State)
CREATE TABLE router_api_pool (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_name TEXT NOT NULL, -- "groq_1", "openrouter_1", "gemini_1", "ollama_local"
    cluster_type TEXT NOT NULL,
    model_identifier TEXT NOT NULL,
    env_key_name TEXT NOT NULL,
    rpm_limit INTEGER DEFAULT 30,
    is_cooldown BOOLEAN DEFAULT 0,
    cooldown_until DATETIME,
    total_calls_today INTEGER DEFAULT 0
);

-- 5. Tabel Buku Besar Trading (Audit Finansial)
CREATE TABLE trading_ledger (
    trade_id TEXT PRIMARY KEY,
    employee_id TEXT REFERENCES employees(id),
    pair TEXT NOT NULL,
    mode TEXT CHECK(mode IN ('PAPER', 'LIVE')),
    side TEXT CHECK(side IN ('BUY', 'SELL')),
    entry_price REAL NOT NULL,
    stop_loss REAL NOT NULL,
    take_profit REAL NOT NULL,
    position_size REAL NOT NULL,
    risk_manager_approved BOOLEAN DEFAULT 0,
    pnl_realized REAL DEFAULT 0.0,
    status TEXT DEFAULT 'OPEN',
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 6. Tabel Gudang Skill Perusahaan (Skill Vault)
CREATE TABLE skills_registry (
    skill_id TEXT PRIMARY KEY,          -- Contoh: "crawl4ai_scraper"
    skill_name TEXT NOT NULL,
    category TEXT NOT NULL,             -- "DEV", "TRADING", "MARKETING", "CS", "CORE"
    source_type TEXT CHECK(source_type IN ('LOCAL_PYTHON', 'MCP_SERVER', 'API_WRAPPER')),
    install_command TEXT,               -- Contoh: "pip install crawl4ai"
    skill_md_path TEXT NOT NULL,        -- Lokasi file instruksi SKILL.md
    is_verified BOOLEAN DEFAULT 0,      -- Wajib lolos uji sandbox sebelum dipakai karyawan
    created_by TEXT DEFAULT 'SYSTEM'    -- 'SYSTEM' atau 'SKILL_HUNTER_AGENT'
);
```

---

## 6. Spesifikasi Protokol Komunikasi (REST API & WebSocket)

### 6.1 Endpoint REST API (Manajemen Kantor)

* `POST /api/v1/hr/generate-division`: Mengirim deskripsi bisnis baru ke AI HRD, mengembalikan *preview* daftar agen dan tata letak meja.
* `POST /api/v1/hr/deploy-division`: Menyimpan *blueprint* divisi ke database, mengaktifkan mesin pekerja, dan menggambar ruangan baru di UI.
* `PATCH /api/v1/employees/{id}/config`: Mengubah *prompt*, mengganti klaster model, atau mencabut izin *tools* karyawan secara langsung saat kantor sedang berjalan.
* `POST /api/v1/emergency/kill-switch`: Menghentikan seluruh eksekusi agen dan menutup paksa semua koneksi *trading* dalam keadaan darurat.

### 6.2 Kontrak Payload WebSocket (`ws://localhost:8000/ws/office-stream`)

```json
{
  "timestamp": "2026-10-24T17:55:10Z",
  "event_type": "AGENT_STATE_CHANGE",
  "floor": 2,
  "division_id": "div_software_web",
  "payload": {
    "employee_id": "emp_dev_01",
    "new_state": "HANDOFF",
    "source_desk": {"x": 4, "y": 5},
    "target_desk": {"x": 8, "y": 5},
    "bubble_icon": "icon_code_file",
    "active_router_node": "openrouter/qwen-2.5-coder-32b:free",
    "live_log_snippet": "Berhasil membuat endpoint API autentikasi. Menyerahkan ke meja QA Tester..."
  }
}
```

---

## 7. Cetak Biru Infrastruktur Deployment (`docker-compose.yml`)

Belum dibuat (Fase 4). Komponen yang dispesifikasikan: `aahc-core-backend` (8000), `aahc-office-ui` (3000), `redis-broker` (6379), `chromadb-memory` (8001), `ollama-fallback` (11434), seluruhnya `restart: always`. `aahc-core-backend` membutuhkan mount `/var/run/docker.sock` untuk memanggil sandbox container Divisi Dev.

---

## 8. Urutan Tahapan Implementasi (Roadmap Eksekusi)

1. **Fase 1 (Pondasi & Router):** database SQLite (`schema.sql`), modul `smart_9router.py` untuk rotasi API gratis, `hr_architect.py` pencetak karyawan, dan `skill_hunter.py` (Skill Vault).
2. **Fase 2 (UI Kantor 2D & WebSocket):** kanvas Phaser.js di `localhost:3000` yang membaca tabel `employees`, merender lantai kayu beserta meja-meja kerja, dan menggerakkan animasi karakter melalui sinyal WebSocket FastAPI.
3. **Fase 3 (Mesin Bisnis Pertama - FB Pro & Affiliate):** mesin `CRON_BATCH` untuk divisi pencari tren GitHub dan afiliasi Shopee/TikTok hingga menghasilkan postingan otomatis.
4. **Fase 4 (Ekspansi Divisi Berat - Dev Sandbox, Trading, & CS):** kontainer *sandbox* divisi *developer*/ML, WebSocket harga *market* untuk meja *trading per pair* (mode *Paper Trade* dulu), *webhook* agen *Customer Service*.

---

## 9. Ekosistem Skill Modular (MCP, Function Calling Tools, `SKILL.md`)

### 9.1 Arsitektur AI Skill Hunter & Toolsmith

```text
[AI HRD: "Butuh karyawan dengan skill 'olx_scraper', tapi belum ada di gudang lokal"]
       │
       ▼
[Agen Skill Hunter (Meja R&D)] ──► 1. Cari di Registry Lokal (/skills_vault)
       │ (Tidak ditemukan)
       ▼
2. Cari ke Direktori Eksternal (GitHub API / Smithery MCP Registry / PyPI)
       │
       ├──► [Jika Ada Library/MCP Resmi]: Unduh ke Sandbox -> Uji Keamanan -> Pasang ke Karyawan
       │
       └──► [Jika Tidak Ada]: Oper ke "Agen Toolsmith" -> Tulis kode Python/Playwright baru
                              -> Buat file SKILL.md -> Uji di Docker -> Simpan ke Gudang Skill Permanen
```

Struktur folder skill lokal (`backend/skills_vault/<skill_id>/`): `SKILL.md` (aturan pakai, batasan parameter, contoh prompt), `tool_executor.py` (kode eksekutor), `requirements.txt` (dependensi khusus skill).

### 9.2 Whitelist Sumber Pencarian Skill (anti-malware)

1. Smithery.ai & Official MCP Registry (`modelcontextprotocol/servers`)
2. CrewAI Tools & LangChain Community Tools
3. ClawHub / Awesome-Agent-Skills GitHub Topics
4. PyPI (Python Package Index)

### 9.3 Skill Fondasi yang Wajib Tersedia

**Kelompok "Pembangun Sistem":** Context7 MCP, Sequential Thinking, Python FastAPI & Async Rules, Phaser 3 Tilemap & Sprite Architecture.
**Kelompok "Mata & Tangan Digital":** Crawl4AI, Playwright Stealth Browser, GitHub Official MCP, DuckDuckGo/SearXNG Local Search.
**Kelompok "Mesin Uang Spesialis":** `ccxt-universal-exchange`, `pandas-ta-quant`, `docker-sandbox-runner`, `huggingface-dataset-hub`, `chroma-rag-memory`.

### 9.4 Visualisasi Skill Hunter di Kantor 2D

Agen Skill Hunter ditempatkan di meja dekat rak buku. AI HRD berjalan menghampirinya saat ada skill yang belum dimiliki; balon kaca pembesar (🔍) muncul saat mencari, ikon kunci inggris (🔧) saat menguji di Docker Sandbox, lalu ia menyerahkan `SKILL.md` ke karyawan baru.

---

## Catatan Status Implementasi (Fase 1)

Diimplementasikan pada `backend/`:

| Berkas | Peran |
| --- | --- |
| `backend/schema.sql` | 6 tabel DDL dari §5 + indeks unik lane router |
| `backend/db.py` | Pembantu koneksi SQLite (WAL, foreign_keys, busy_timeout) |
| `backend/init_db.py` | Penerap skema + seed 11 lane router gratis + scan skill vault |
| `backend/smart_9router.py` | FR-RTR-01/02: klaster, failover tanpa tidur, cooldown `Retry-After` |
| `backend/hr_architect.py` | FR-HRD-01/02/03: generate blueprint, deploy + auto-layout meja, kloning trading desk |
| `backend/skill_hunter.py` | Pencarian whitelist, install ke vault, verifikasi sandbox, Toolsmith |
| `backend/skills_vault/` | Format vault + skill `web_search` |
| `backend/smoke_demo.py` | Demo end-to-end offline (tanpa API key, tanpa jaringan) |
| `backend/tests/` | 55 tes offline (router, HRD, skill hunter) |

### Cara menjalankan

```bash
cd backend
python3 -m venv .venv && .venv/bin/pip install -r requirements.txt   # sekali saja
.venv/bin/python init_db.py          # buat/perbarui backend/data/aahc.db
.venv/bin/python smoke_demo.py       # uji end-to-end offline
.venv/bin/python -m pytest tests -q  # 55 tes
```

Hasil demo terverifikasi: 11 lane router (CODE 3, FAST 3, REASONING 3, VISION 2),
lantai trading `div_quant_trading_floor` berisi CRO + 3 trader (BTC/ETH/SOL) dengan
meja tidak bertumpuk, dan `web_search` lolos verifikasi sandbox.

### Belum termasuk Fase 1 (sesuai roadmap)

FastAPI/WebSocket hub, kanvas Phaser.js, Redis, ChromaDB, Docker sandbox nyata,
`docker-compose.yml`, mesin eksekusi `CRON_BATCH`/`STREAM_TRADE`/`WEBHOOK_CS`/`DAG_DEV`,
`trading_ledger` (baru tabelnya), serta pembatasan `rpm_limit` (kolom tersedia, belum
ditegakkan oleh router).

### Divergensi dari SRS

- SRS menyebut **LiteLLM** sebagai lapisan abstraksi router. Fase 1 memakai HTTP langsung
  ke endpoint OpenAI-compatible (kelima provider yang dispesifikasikan kompatibel) agar
  tanpa dependensi berat dan bisa diuji offline. LiteLLM dapat dipasang sebagai transport
  alternatif pada Fase 2 tanpa mengubah antarmuka `SmartRouter.chat()`.
- `init_db.py` menambahkan satu **indeks unik** `(provider_name, cluster_type, model_identifier)`
  pada `router_api_pool` supaya seeding idempoten; kolom dan tabel lain persis seperti §5.
- Verifikasi sandbox skill pada Fase 1 memakai subprocess ber-`timeout` (bukan Docker);
  isolasi kontainer menyusul di Fase 4 sesuai FR-DEV-01.

---

## Keputusan: Rewrite Total ke Rust (disetujui)

Pemilik proyek memutuskan **rewrite total** backend ke Rust. Keputusan ini menggantikan
stack Python, tetapi **tidak** membatalkan SRS: perilaku yang dispesifikasikan tetap sama,
hanya bahasanya yang berubah. Pohon Python Fase 1 **dibekukan sebagai oracle paritas**
(55 tes) dan baru dihapus di tiket terakhir.

### Keputusan yang mengikat

| # | Keputusan |
| --- | --- |
| 1 | Cakupan "semuanya Rust" = **kode aplikasi kita**. Server pihak ketiga (Ollama, Redis, ChromaDB, kontainer sandbox) tetap seperti apa adanya. |
| 2 | Data pasar: **tidak ada CCXT di Rust.** Pondasi `NautilusTrader`/`barter-rs` + adapter ditulis hanya untuk venue yang dipakai (Binance, Bybit, Indodax, Tokocrypto — ≤6). Cakupan 100+ bursa CCXT memang hilang; itu cakupan yang tidak terpakai. |
| 3 | ML: `candle` untuk inference + pipeline dataset; **pelatihan berat boleh didelegasikan ke tool trainer eksternal**. Ini berarti "nol Python" tidak mutlak di jalur pelatihan. |
| 4 | Orkestrasi agen: **engine sendiri** (state row + antrean + state machine per `execution_mode`). Tidak ada LangGraph di Rust. |
| 5 | UI 2D tetap **Phaser.js** di browser; rewrite Rust tidak menyentuh kanvas browser. |
| 6 | Skill Hunter tetap memakai **MCP** (`rmcp`), dan lewat tiket 03 kelayakannya dibuktikan lebih dulu. |
| 7 | Prasyarat: `git init`, `rustup` (toolchain ter-pin), `clang` (untuk crate yang butuh bindgen). |
| 8 | Urutan porting: skema → router → HRD → skill hunter → hub → trading → ML. |

### Batas batal (dihapus)

> Bila router + HRD belum mencapai paritas dalam 5 hari kerja sejak tiket 04 dimulai, atau
> total rewrite melewati anggaran waktu yang dicatat, maka rewrite dibatalkan; Python tetap
> menjadi produksi, pohon Rust disimpan sebagai eksperimen.
>
> **Dihapus sesuai aturan AGENTS.md (Working rules §1):** batas waktu kerja yang tidak
> berhubungan dengan perilaku produk diabaikan. Paritas dan keputusan lanjut/kembali tetap
> dituntaskan di tiket 17 dengan angka (termasuk pengukuran RSS/CPU tiket 16 yang dibandingkan
> dengan baseline Python **45,7 MB / ~0,07% core saat menganggur**), tetapi *tanpa* tenggat waktu.

### Rencana tiket

20 tiket tracer-bullet tersimpan di `.scratch/aahc/issues/` (satu berkas per tiket).
Kurva expand→contract: pohon Python hidup sampai tiket 17.

### Audit rencana tiket (25 Oktober 2026)

Audit terhadap SRS menemukan tiga kebutuhan yang belum bertiket dan beberapa gerbang blokir yang keliru. Perubahan yang diterapkan:

- **Tiket baru 18** — engine eksekusi agen inti (state machine per `execution_mode`). Tanpa ini, tiket 07/08/09/13/14/15 mengandaikan "pekerjaan nyata" yang tidak pernah dibangun.
- **Tiket baru 19** — sandbox dev terisolasi + siklus self-debug (FR-DEV-01/02), termasuk batas 5 iterasi dan eskalasi ke manusia.
- **Tiket baru 20** — paket deployment 24/7 (compose/systemd, `restart: always`, Redis/ChromaDB/Ollama) yang menjadi bukti nyata NFR-AVL-01; tiket 16 kini bergantung padanya.
- **Gerbang diperbaiki** — 07 bergantung pada 18 (kill-switch butuh pekerjaan nyata); 13/14/15 tidak lagi dipaksa menunggu REST 07 karena irisannya diuji sebagai pipeline/layanan dengan harness, integrasi kantor lewat 18; 17 kini digerbangi seluruh tiket paritas + 18/19/20.
- **Oracle paritas** — tabel aditif baru (metrik ML, pesan CS, draf marketing) dinyatakan di luar perbandingan skema Fase 1; paritas berarti "nol perbedaan pada skema Fase 1", bukan "tidak boleh ada tabel baru".
- **Batas batal** — tanggal mulai 5 hari kerja dicatat di tiket 04 saat status berubah; tanpa catatan itu batas tidak dapat dieksekusi.
- **Penyimpangan penomoran** — 18–20 ditambahkan setelah rencana awal; relasi blokir mengikuti daftar `Blocked by`, bukan urutan nomor.

Kebutuhan skill agen yang belum tertutup paket skill di repo: konvensi Rust (tipe error, higiene tokio untuk proses 24/7), operasi deployment (compose/systemd, pengukuran RSS/CPU), dan daftar periksa keamanan sandbox. Ketiganya lebih tepat ditulis sebagai skill proyek saat tiket terkait dikerjakan, bukan dipasang di muka.
