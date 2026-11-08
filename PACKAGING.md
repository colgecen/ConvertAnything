# APPİMAGE PAKETLEME REHBERİ

## Neden AppImage
- Tek indirilebilir dosya, kurulum gerektirmez
- Fedora/Arch/Dağıtımdan bağımsız çalışır (glibc tabanlı)
- Flatpak'tan farklı olarak **sistem araçlarına (ffmpeg vb.) erişim serbest** — kurulum sihirbazımızın çalışması için zorunlu

## Boyut hedefi
10-20 MB (araçlar gömülmediği için). `.desktop`, ikon ve binary dışında bir şey pakete girmez.

## Derleme
```bash
cargo build --release
strip target/release/donustur   # sembolleri sil
```

Önerilen `Cargo.toml` release profili (bkz. ARCHITECTURE):
```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
```

## AppImage dizin yapısı
```
AppDir/
├── AppRun
├── donustur.desktop
├── donustur.png
└── usr/
    └── bin/
        └── donustur
```

donustur.desktop:
```ini
[Desktop Entry]
Name=Donustur
Exec=donustur
Icon=donustur
Type=Application
Categories=Utility;
Comment=Dosya dönüştürücü
```

## Paketleme
```bash
wget https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
chmod +x linuxdeploy-x86_64.AppImage
./linuxdeploy-x86_64.AppImage --appdir AppDir -e target/release/donustur -i donustur.png -d donustur.desktop --output appimage
```

## CI (GitHub Actions özeti)
1. `ubuntu-latest` üzerinde `cargo build --release`
2. linuxdeploy ile AppImage üret
3. Artifact olarak yayınla; tag'de Release'e ekle

## Wayland notları
- eframe/winit Wayland'i native destekler; XWayland gerekmez
- AppImage içinde libwayland-client gerekmez (sistemden yüklenir); 
  statik link `x11` özelliğini kapatmayı gerektirebilir: `eframe` default özelliklerinde Wayland etkin tutulur

## Sınır: AppImage güncelleme
AppImage güncellemesi uygulama içinden yapılmaz; yeni sürüm GitHub Release'den indirilir. (Otomatik güncelleyici ürün kararıdır; AGENTS.md yasağına dikkat.)
