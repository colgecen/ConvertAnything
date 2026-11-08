# İŞ LİSTESİ

## Milestone 0 — İskelet (1. hafta)
- [ ] cargo projesi, eframe hello-world, clippy/fmt CI
- [ ] `config.toml` okuma/yazma + `setup_done` bayrağı
- [ ] `detector.rs`: tool_exists, pkg manager tespiti (dnf/pacman)
- [ ] `installer.rs`: pkexec ile kurulum + dağıtım-paket haritası

## Milestone 1 — Kurulum Sihirbazı (1. hafta)
- [ ] Eksik araç listesi ekranı (ffmpeg kritik, soffice/pandoc opsiyonel)
- [ ] [Kur] akışı: polkit penceresi, ilerleme, hata gösterimi
- [ ] [Atla] akışı: ilgili sekmeleri kilitleme
- [ ] Tekrar göstermeme: setup_done bayrağı

## Milestone 2 — Görsel Dönüşüm (1-2 hafta)
- [ ] `registry.rs` format tanımları + destek matrisi
- [ ] Sürükle-bırak + çoklu dosya
- [ ] image crate: jpg↔png↔webp↔bmp↔tiff, kalite ayarı
- [ ] İş kuyruğu (tokio), ilerleme çubuğu, üzerine yazma koruması
- [ ] Çıktı `-1`/`-2` çakışma çözümü

## Milestone 3 — Ses/Video FFmpeg (2 hafta)
- [ ] `av.rs`: güvenli Command inşası, argüman enjeksiyonu koruması
- [ ] Codec/bitrate/çözünürlük seçenekleri
- [ ] stderr parse ile ilerleme yüzdesi
- [ ] Zaman sınırı + süreç öldürme
- [ ] Video→GIF

## Milestone 4 — Belge (1-2 hafta)
- [ ] `document.rs`: soffice --headless --convert-to (pdf hedefi)
- [ ] pandoc: md↔html↔docx↔epub
- [ ] TXT/CSV kodlama dönüşümü (UTF-8)

## Milestone 5 — Arşiv (opsiyonel, 1 hafta)
- [ ] zip/tar.gz oluşturma ve açma (zip bomb koruması ile)

## Milestone 6 — Paketleme (1 hafta) → PACKAGING.md
- [ ] linuxdeploy + appimagetool ile AppImage
- [ ] .desktop dosyası + ikon
- [ ] GitHub Release CI (x86_64)

## Güvenlik denetimi (her milestone sonu)
- [ ] SECURITY.md kontrol listesi gözden geçirilir
- [ ] cargo audit
- [ ] Zip bomb / path traversal / argüman enjeksiyonu testleri

## V2 fikirleri (şimdilik yapma)
- Toplu önizleme, format öneri motoru, sağ tık menüsü entegrasyonu (Nautilus/Dolphin), arşiv desteği genişletme
