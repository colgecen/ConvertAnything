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

- Sürükle-bırak ve çoklu dosya; tek ekranda karışık türler
- Toplu dönüşüm: hedef biçim listesi tüm dosyaların ortak desteklediği biçimlerden oluşur
- KDE Dolphin sağ tık menüsü: dosyaya uygun hedefler listelenir, seçince pencere açılmadan dönüştürür
- Türkçe/İngilizce arayüz; sistem dilini otomatik algılar
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

## Kurulum
AppImage'ı indirdikten sonra tek komutla kullanıcı hesabına kur:

```bash
chmod +x Donustur-*.AppImage
./Donustur-*.AppImage --install
```

Bu komut:
- AppImage'ı `~/.local/bin/Donustur.AppImage` konumuna kopyalar,
- masaüstü girdisini, simgeyi ve sağ tık menülerini kurar,
- oturum açılışında otomatik başlatmayı etkinleştirir (`~/.config/autostart/donustur.desktop`).

Kaldırmak için:
```bash
~/.local/bin/Donustur.AppImage --uninstall
```

Kurulumu ve otomatik başlatmayı arayüzden de yönetebilirsin:
**Ayarlar → Sistem entegrasyonu**.

## Çalıştırma
```bash
cargo run --release
```

Kullanım:
```
donustur [DOSYALAR…]                  # ana ekranı dosyalarla aç
donustur --convert DOSYALAR…          # hedef biçim seçiciyi aç
donustur --convert DOSYALAR… --to png # pencere açmadan dönüştür
donustur --install                    # kur + oturum açılışında başlat
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
