# APPIMAGE PAKETLEME REHBERİ

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

Release profili `Cargo.toml` içinde tanımlıdır (opt-level z, LTO, strip).
Tek komutla AppImage üretmek için:

```bash
./scripts/package-appimage.sh            # sürüm Cargo.toml'dan okunur
./scripts/package-appimage.sh 0.1.0      # sürümü elle ver
```

Betik ilk çalıştırmada `linuxdeploy` ve `appimagetool` araçlarını `.tools/`
dizinine indirir (yalnızca derleme zamanında ağ kullanılır), `AppDir` oluşturur
ve `dist/Donustur-<sürüm>-x86_64.AppImage` dosyasını üretir.

## AppImage dizin yapısı
```
AppDir/
├── AppRun
├── donustur.desktop
├── donustur.png
└── usr/
    ├── bin/
    │   └── donustur
    └── share/
        ├── applications/donustur.desktop
        └── icons/hicolor/256x256/apps/donustur.png
```

`assets/donustur.desktop`:
```ini
[Desktop Entry]
Name=Dönüştür
Exec=donustur
Icon=donustur
Type=Application
Categories=Utility;Office;Graphics;AudioVideo;
Comment=Görsel, ses, video, belge ve arşiv dönüştürücü
```

Simge `assets/generate-icon.py` (Pillow) ile üretilir; PNG depoda hazır tutulur.

## Elle paketleme
```bash
curl -fL -o linuxdeploy-x86_64.AppImage \
  https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
curl -fL -o appimagetool-x86_64.AppImage \
  https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x linuxdeploy-x86_64.AppImage appimagetool-x86_64.AppImage

APPIMAGE_EXTRACT_AND_RUN=1 ./linuxdeploy-x86_64.AppImage \
  --appdir AppDir -e target/release/donustur \
  -i assets/donustur.png -d assets/donustur.desktop
APPIMAGE_EXTRACT_AND_RUN=1 ./appimagetool-x86_64.AppImage AppDir dist/Donustur.AppImage
```

## CI (GitHub Actions)
- `ci.yml`: her push/PR'da `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, release derlemesi
- `release.yml`: `v*` etiketinde AppImage üretir, artifact ve GitHub Release'e ekler
- Paketleme `ubuntu-22.04` üzerinde yapılır; daha eski glibc sayesinde
  yeni dağıtımlarda da çalışır

## Wayland notları
- eframe/winit Wayland'i native destekler; XWayland gerekmez
- AppImage içinde libwayland-client gerekmez (sistemden yüklenir)
- `eframe` özelliklerinde hem `wayland` hem `x11` açık tutulur

## Sınır: AppImage güncelleme
AppImage güncellemesi uygulama içinden yapılmaz; yeni sürüm GitHub Release'den indirilir.
(Otomatik güncelleyici ürün kararıdır; AGENTS.md yasağına dikkat.)
