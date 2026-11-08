# Donustur — Dosya Dönüştürücü

Rust + egui ile yazılmış, Linux (Fedora/Arch, Wayland) hedefli masaüstü dosya dönüştürme uygulaması.
Dağıtım: **AppImage** (~10-20 MB). Dönüşüm motorları (FFmpeg, LibreOffice vb.) sisteme kurulur, uygulamaya gömülmez.

## Hedefler
- Görsel: jpg, png, webp, bmp, tiff, gif, ico, avif, svg (kayıpsız formatlar hariç dönüştürme)
- Ses: mp3, wav, flac, ogg, opus, aac, m4a
- Video: mp4, mkv, webm, avi, mov, gif
- Belge: pdf, docx, odt, md, html, txt, csv (LibreOffice + pandoc)
- Arşiv: zip, tar, gz, 7z (opsiyonel)

## Neden araçlar gömülü değil?
Uygulama boyutunu 10-20 MB'ta tutmak için motorlar sistem paketidir.
İlk açılışta kurulum sihirbazı eksik araçları `pkexec` + `dnf`/`pacman` ile kurar (bir kez).
Sonrasında uygulama araçları `std::process::Command` ile çağırır.

## Çalıştırma
```bash
cargo run --release
```

## Dokümanlar
- [ARCHITECTURE.md](ARCHITECTURE.md) — mimari ve modül yapısı
- [SECURITY.md](SECURITY.md) — güvenlik politikası ve kontroller
- [AGENTS.md](AGENTS.md) — AI ajanları için proje kuralları
- [TASKS.md](TASKS.md) — iş listesi ve kilometre taşları
- [PACKAGING.md](PACKAGING.md) — AppImage paketleme rehberi
