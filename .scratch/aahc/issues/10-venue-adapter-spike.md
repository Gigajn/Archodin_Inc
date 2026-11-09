# 10: Spike adapter bursa — satu venue, klines ke candle

**What to build:** Pembuktian bahwa satu venue nyata dapat dialiri data pasar masuk ke program Rust dan diubah menjadi candle yang konsisten — sekaligus keputusan apakah pondasi `NautilusTrader` atau `barter-rs` yang dipakai, sebelum menulis adapter untuk venue lain.

**Blocked by:** 01

**Status:** ready-for-agent

- [ ] Satu venue (publik, tanpa kunci API) mengalirkan kline dan candle terbentuk di memori dengan harga serta waktu yang benar.
- [ ] Putus koneksi ditangani: reconnect otomatis, dan celah data yang hilang dideteksi serta dilaporkan (tidak diam-diam dianggap kontinu).
- [ ] Throughput dan latensi pemrosesan diukur dan dicatat (jumlah pesan/detik, latensi p99) sebagai baseline nyata.
- [ ] Keputusan pondasi tercatat di spec beserta alasan teknisnya, termasuk biaya menulis adapter venue tambahan (target: Binance, Bybit, Indodax, Tokocrypto).
- [ ] Tidak ada kunci API yang dibutuhkan maupun disimpan pada tiket ini.
