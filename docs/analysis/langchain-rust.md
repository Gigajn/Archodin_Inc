# Analisis langchain-rust untuk Fork

25 Oktober 2026 · disusun untuk Kudo. Versi dokumen hidup (dengan grafik): https://claude.ai/artifact/3FLKbptRCeo48U9isP5dCm

Dianalisis: [Abraxas-365/langchain-rust](https://github.com/Abraxas-365/langchain-rust) commit `160fc16`, sama dengan fork [Gigajn/langchain-rust](https://github.com/Gigajn/langchain-rust).

## Ringkasan

Fork dan pengembangan lanjutan langchain-rust **boleh**: proyek ini berlisensi MIT, yang mengizinkan penggunaan, modifikasi, distribusi, dan penggunaan komersial asalkan teks lisensi dan copyright "Ryo Kanazawa" tetap disertakan. Ini bukan nasihat hukum.

- **Ukuran:** sekitar 59.600 baris Rust di 290 file, 73 contoh, 778 unit test, versi 7.0.0 di crates.io (sekitar 160 ribu unduhan total).
- **Kondisi:** proyek sempat hampir mati dari akhir 2024 sampai September 2026, lalu bangkit lagi dengan 66 commit di Oktober 2026 dan tiga rilis mayor (v5, v6, v7) dalam sebulan. API masih sering berubah.
- **Celah terbesar untuk proyek multi-agent:** belum ada orkestrasi multi-agent atau graph (setara LangGraph), belum ada klien MCP, belum ada Gemini, dan tipe error tool `Box<dyn Error>` tidak `Send`.
- **Rekomendasi:** jaga kode upstream tetap bisa di-merge (soft fork), dan bangun lapisan multi-agent sebagai crate terpisah di workspace ini. Hard fork baru masuk akal kalau upstream menolak perubahan inti yang dibutuhkan.

## Lisensi dan kewajiban

Kode langchain-rust berlisensi MIT ([LICENSE](../../LICENSE), `license = "MIT"` di Cargo.toml, tidak ada file NOTICE). Fork, modifikasi, rilis ulang dengan nama lain, dan pemakaian komersial atau closed-source semuanya diizinkan.

Kewajiban:

1. Simpan file `LICENSE` asli beserta baris `Copyright (c) Ryo Kanazawa` di semua salinan atau bagian substansial kode.
2. Kode baru boleh memakai lisensi lain, tapi bagian yang berasal dari upstream tetap MIT. Cara paling mudah: tetap MIT dan tambahkan baris copyright sendiri di bawah yang lama.
3. Jangan memakai nama "LangChain" seolah produk resmi LangChain Inc. Nama crate baru sebaiknya berbeda (misalnya `archodin-langchain`), karena nama `langchain-rust` di crates.io milik pengelola upstream.
4. Tidak ada garansi.

Lisensi dependensi (`cargo metadata --all-features`, 1.031 paket):

| Kelompok | Jumlah paket | Catatan |
| --- | --- | --- |
| MIT / Apache-2.0 / BSD / ISC / Zlib / Unicode | sekitar 990 | Longgar, cukup menyertakan teks lisensi saat mendistribusikan binary |
| MPL-2.0 (`cssparser`, `selectors`, `colored`, dll.) | 6 | Copyleft per-file. Aman dipakai; perubahan pada file mereka harus dibuka |
| SurrealDB (`surrealdb-*`, termasuk BUSL-1.1) | sekitar 30 | Hanya lewat feature opsional `surrealdb`. BUSL membatasi pemakaian sebagai layanan database komersial |
| Tanpa metadata lisensi (`ollama-rs`, dll.) | sedikit | Cek manual sebelum rilis komersial |

## Arsitektur dan modul

Satu crate besar yang meniru LangChain Python: setiap konsep adalah trait async (`LLM`, `Embedder`, `VectorStore`, `Tool`, `Agent`, `Chain`, `BaseMemory`, `Loader`, `TextSplitter`, `OutputParser`), dan setiap provider adalah implementasi trait itu di balik feature flag. Instrumentasi lewat `tracing` (span `llm.generate`, `agent.run`, `tool.call`).

| Modul | Baris | Isi utama |
| --- | --- | --- |
| `vectorstore` | 16.670 | 13 store: pgvector, qdrant, chroma, pinecone, weaviate, milvus, mongodb, opensearch, redis, sqlite-vec, sqlite-vss, surrealdb, mariadb |
| `llm` | 8.672 | OpenAI/Azure (via async-openai), Claude, Ollama, DeepSeek, Qwen, Bedrock, cache LLM, parser SSE |
| `chain` | 8.482 | LLMChain, conversational, retrieval QA, map-reduce/refine, router, sequential, SQL, API, constitutional, math, transform |
| `tools` | 5.240 | 12 tool: SerpAPI, DuckDuckGo, Exa, Perplexity, Wikipedia, Wolfram, scraper, SQL, command executor, calculator, text-to-speech, FunctionTool |
| `agent` | 4.038 | `AgentExecutor`, OpenAI tools agent, chat agent, ReAct; streaming event dan cancellation |
| `document_loaders` | 3.751 | teks, CSV, HTML, PDF, pandoc, source code (tree-sitter), git commit, Notion, AssemblyAI |
| `prompt`, `output_parsers`, `schemas` | 5.151 | template, few-shot, parser bertipe JSON ke struct, tipe `Message`/`Document` |
| `embedding` | 1.981 | OpenAI, Ollama, Mistral, FastEmbed (lokal/ONNX), Bedrock, Voyage |
| `memory` | 1.949 | simple, window, token buffer, riwayat SQL/Redis/MongoDB |
| `semantic_router`, `text_splitter`, `language_models` | 2.872 | router berbasis embedding, splitter token/markdown, trait inti LLM |

Alur khas: `Loader` → `TextSplitter` → `Embedder` → `VectorStore` → `Retriever` → `Chain` atau `Agent` (dengan `Tool` dan `Memory`) → `LLM`. `AgentExecutor` mengulang `plan` → panggil tool → `plan` sampai selesai atau `max_iterations` (default 10).

## Dependensi dan feature flag

- **Selalu ikut:** `tokio` (`full`), `reqwest` 0.13 dengan `native-tls`, `async-openai` 0.42, `tiktoken-rs`, `text-splitter`, `scraper`, `readability`, `csv`, `regex`, `secrecy`.
- **Seharusnya dev-dependency:** `mockito` terdaftar sebagai dependensi biasa.
- **TLS:** hanya `native-tls`; belum ada pilihan `rustls`.
- **Feature opsional (22):** `postgres`, `mariadb`, `qdrant`, `chroma`, `pinecone`, `weaviate`, `milvus`, `mongodb`, `opensearch`, `redis`, `sqlite-vec`, `sqlite-vss`, `surrealdb`, `ollama`, `mistralai`, `bedrock`, `fastembed`, `tree-sitter`, `git`, `lopdf`, `pdf-extract`, `html-to-markdown`.
- **Pin rapuh:** `ort = 2.0.0-rc.15` (release candidate) mengikuti `fastembed`.
- **Pembaruan otomatis:** Renovate aktif (63 commit bot).

## Kualitas kode dan pengujian

`cargo test --lib` (feature default, Rust 1.97): **497 lulus, 0 gagal, 47 diabaikan** dalam 16 detik, setelah kompilasi 5,5 menit.

- 778 fungsi test di `src` (termasuk yang butuh feature tertentu); 108 `#[ignore]` karena butuh API key atau layanan nyata; integrasi pakai `testcontainers`.
- Banyak test offline memakai `mockito` dan LLM mock, jadi provider bisa diuji tanpa biaya API.
- `cargo clippy --lib`: 99 peringatan, 0 error (modul bernama sama dengan induknya 19, `Into` alih-alih `From` 9, builder tanpa `Default` 8). Semuanya gaya, bukan bug.
- 29 peringatan kompilasi di build test, sebagian tersembunyi oleh `#![allow(dead_code)]` di level crate.
- Coverage tidak diukur di CI.

## Status pemeliharaan

Aktif lagi, tapi bergantung pada satu orang: dari 66 commit Oktober 2026, 56 dari pengelola utama (Luis Fernando Miranda / Abraxas-365).

| Periode | Commit |
| --- | --- |
| Feb–Apr 2024 | 422 |
| Mei–Des 2024 | 102 |
| Jan–Apr 2025 | 8 |
| Mei 2025 – Sep 2026 | 0 |
| Okt 2026 | 66 |

- **Rilis:** v5.0.0 sampai v7.0.0 semuanya di Oktober 2026, dengan perubahan breaking. Terbaru di crates.io: 7.0.0 (22 Oktober 2026).
- **Komunitas:** sekitar 1.350 bintang, 175 fork.
- **Issue terbuka (8):** Faiss (#301), LanceDB (#133), Mem0 (#229), HuggingFace Inference (#146), dokumentasi (#255, #262), memecah document loaders (#187), Dependency Dashboard (#54).
- **PR terbuka (2):** MiniMax provider (#334), `Option` untuk `add_document` (#300); 294 tertutup.

## Kelemahan dan celah

| Area | Temuan | Dampak |
| --- | --- | --- |
| Multi-agent | Tidak ada graph, state machine, supervisor, atau handoff. `AgentExecutor` hanya loop satu agent | Inti proyek harus dibangun sendiri |
| Protokol tool | Tidak ada klien/server MCP | Tidak bisa memakai ekosistem tool MCP |
| Tipe tool | Input `&str` lalu di-parse ke JSON; error `Box<dyn Error>` (217 kemunculan) tidak `Send + Sync` | Sulit lintas task tokio, schema manual |
| Paralelisme | Tool call satu langkah dieksekusi berurutan | Lambat untuk banyak tool |
| Provider | Belum ada Gemini, Groq, Mistral chat, HuggingFace; MiniMax masih PR | Perlu ditambah |
| Struktur crate | Monolitik, `#![allow(dead_code)]`; issue #187 | Build lambat |
| API publik | Typo jadi API: `PromptFromatter`, modul `sql_datbase` | Perbaikan = breaking change |
| Robustness | Sekitar 700 `unwrap()`/`expect()` di `src` (sebagian di test inline) | Potensi panic |
| CI | `actions-rs` usang, tanpa clippy/cache; pola tag `v0.x.y` tidak cocok dengan `v7.0.0` | Publish manual |
| Dokumentasi | #255, #262 | Kurva belajar tinggi |
| TLS/dependensi | Hanya `native-tls`; `mockito` di runtime | Build statis sulit |

## Strategi dan roadmap

| Pilihan | Kelebihan | Kekurangan |
| --- | --- | --- |
| Soft fork + crate sendiri (disarankan) | Tetap dapat integrasi baru dari upstream | Bergantung pada API upstream yang masih berubah |
| Hard fork | Kontrol penuh | Merawat ~60 ribu baris dan ~1.000 dependensi sendiri |
| Tulis dari nol | Desain bersih | Kehilangan semua integrasi yang sudah jadi |

1. **Fondasi (minggu 1–2):** workspace Cargo, CI dengan `dtolnay/rust-toolchain`, `clippy -D warnings`, cache. `/setup-matt-pocock-skills` lalu `/grill-with-docs` untuk GLOSSARY dan ADR.
2. **Lapisan tool yang aman (minggu 2–4):** trait tool bertipe (input `serde`, schema via `schemars`, error `Send + Sync`), adapter ke `Tool` lama, eksekusi paralel dengan batas konkurensi.
3. **Orkestrasi multi-agent (minggu 4–8):** graph berbasis state, supervisor dan handoff, checkpoint ke Postgres/SQLite, human-in-the-loop, streaming per node.
4. **MCP (minggu 6–9):** klien MCP, opsional server MCP.
5. **Provider dan kontribusi balik (terus):** Gemini, Groq, OpenRouter, perbaikan CI, `rustls`, `mockito` ke dev-dependency.
6. **Kualitas (terus):** evaluasi agent dengan LLM mock, `cargo llvm-cov`, benchmark, contoh multi-agent end-to-end.

## Sinkronisasi dengan upstream

Repo ini memuat riwayat git upstream, jadi pembaruan bisa di-merge langsung:

```bash
git remote add upstream https://github.com/Abraxas-365/langchain-rust
git fetch upstream
git merge upstream/main
```
