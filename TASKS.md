# İŞ LİSTESİ

## Milestone 0 — İskelet (1. hafta) — TAMAM
- [x] cargo projesi, eframe hello-world, clippy/fmt CI
- [x] `config.toml` okuma/yazma + `setup_done` bayrağı
- [x] `detector.rs`: tool_exists, pkg manager tespiti (dnf/pacman)
- [x] `installer.rs`: pkexec ile kurulum + dağıtım-paket haritası

## Milestone 1 — Kurulum Sihirbazı (1. hafta) — TAMAM
- [x] Eksik araç listesi ekranı (ffmpeg kritik, soffice/pandoc opsiyonel)
- [x] [Kur] akışı: polkit penceresi, ilerleme, hata gösterimi
- [x] [Atla] akışı: ilgili sekmeleri kilitleme
- [x] Tekrar göstermeme: setup_done bayrağı

## Milestone 2 — Görsel Dönüşüm (1-2 hafta) — TAMAM
- [x] `registry.rs` format tanımları + destek matrisi
- [x] Sürükle-bırak + çoklu dosya
- [x] image crate: jpg↔png↔webp↔bmp↔tiff, kalite ayarı
- [x] İş kuyruğu (tokio), ilerleme çubuğu, üzerine yazma koruması
- [x] Çıktı `-1`/`-2` çakışma çözümü

## Milestone 3 — Ses/Video FFmpeg (2 hafta) — TAMAM
- [x] `av.rs`: güvenli Command inşası, argüman enjeksiyonu koruması
- [x] Codec/bitrate/çözünürlük seçenekleri
- [x] stderr parse ile ilerleme yüzdesi
- [x] Zaman sınırı + süreç öldürme
- [x] Video→GIF

## Milestone 4 — Belge (1-2 hafta) — TAMAM
- [x] `document.rs`: soffice --headless --convert-to (pdf hedefi)
- [x] pandoc: md↔html↔docx↔epub
- [x] TXT/CSV kodlama dönüşümü (UTF-8)

## Milestone 5 — Arşiv (opsiyonel, 1 hafta) — TAMAM
- [x] zip/tar.gz oluşturma ve açma (zip bomb koruması ile)

## Milestone 6 — Paketleme (1 hafta) → PACKAGING.md
- [ ] linuxdeploy + appimagetool ile AppImage
- [ ] .desktop dosyası + ikon
- [ ] GitHub Release CI (x86_64)

## Güvenlik denetimi (her milestone sonu)
- [ ] SECURITY.md kontrol listesi gözden geçirilir
- [ ] cargo audit
- [x] Zip bomb / path traversal / argüman enjeksiyonu testleri

## V2 fikirleri (şimdilik yapma)
- Toplu önizleme, format öneri motoru, sağ tık menüsü entegrasyonu (Nautilus/Dolphin), arşiv desteği genişletme
