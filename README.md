# Dönüştür — Dosya Dönüştürücü

Rust + egui ile yazılmış, Linux (Fedora/Arch, Wayland) hedefli masaüstü dosya
dönüştürme uygulaması. Dağıtım: **AppImage** (~10-20 MB). Dönüşüm motorları
(FFmpeg, LibreOffice, Pandoc) sisteme kurulur, uygulamaya gömülmez.

## Özellikler

| Tür | Kaynak → Hedef |
|---|---|
| Görsel | jpg, png, webp, bmp, tiff, gif, ico (kendi aralarında) |
| Ses | mp3, wav, flac, ogg, opus, aac, m4a |
| Video | mp4, mkv, webm, avi, mov → birbirleri ve gif |
| Belge | docx, odt, md, html, epub, txt, csv → pdf dahil |
| Arşiv | herhangi bir dosya → zip/tar.gz; zip/tar.gz → ayıklama |

- Sürükle-bırak ve çoklu dosya
- Tokio tabanlı paralel iş kuyruğu, canlı ilerleme
- Çakışmada `-1`, `-2` ekleyerek **üzerine yazmama**
- JPEG kalitesi, ses/video bit hızı ve çözünürlük seçenekleri
- Zip bomb / path traversal / argüman enjeksiyonu korumaları
- Tamamen çevrimdışı; telemetri ve otomatik güncelleme yok

AVIF ve SVG, saf Rust bağımlılıklarıyla güvenilir biçimde kodlanamadığı için
destek matrisinde yer almaz. 7z desteği V2'ye bırakılmıştır.

## Neden araçlar gömülü değil?
Uygulama boyutunu 10-20 MB'ta tutmak için motorlar sistem paketidir.
İlk açılışta kurulum sihirbazı eksik araçları `pkexec` + `dnf`/`pacman` ile
kurar (bir kez, yalnızca sabit paket adlarıyla). Sonrasında uygulama araçları
`std::process::Command` ile **kabuk kullanmadan** çağırır.

## Çalıştırma
```bash
cargo run --release
```

Geliştirme için:
```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check
```

## Paketleme
```bash
./scripts/package-appimage.sh
```
Ayrıntılar: [PACKAGING.md](PACKAGING.md)

## Dokümanlar
- [ARCHITECTURE.md](ARCHITECTURE.md) — mimari ve modül yapısı
- [SECURITY.md](SECURITY.md) — güvenlik politikası ve kontroller
- [AGENTS.md](AGENTS.md) — AI ajanları için proje kuralları
- [TASKS.md](TASKS.md) — iş listesi ve kilometre taşları
- [PACKAGING.md](PACKAGING.md) — AppImage paketleme rehberi
