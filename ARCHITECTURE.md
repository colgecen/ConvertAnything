# MİMARİ

## Teknik yığın
| Katman | Seçim | Neden |
|---|---|---|
| Dil | Rust (stable) | Boyut, hız, güvenlik |
| GUI | egui/eframe (winit) | Tek binary, native Wayland, küçük |
| Async | tokio | Dönüşüm işlerini bloklamadan yürütmek |
| Video/Ses | FFmpeg (harici binary) | Standart; sıfırdan codec yazılmaz |
| Belge | LibreOffice headless + pandoc (harici) | docx/odt/pdf dönüşüm standardı |
| Görsel | image crate | jpg/png/webp/bmp/tiff/gif |
| Yapılandırma | serde + toml | ~/.config/donustur/config.toml |
| Log | tracing | Hata ayıklama ve kullanıcı raporları |

## Modül yapısı
```
src/
├── main.rs            # Giriş noktası, CLI, eframe/headless başlatma
├── app.rs             # Uygulama durumu (egui App trait)
├── i18n.rs            # Türkçe/İngilizce, sistem dilini algılama
├── integration.rs     # .desktop + KDE servis menüsü kurulumu
├── ui/
│   ├── mod.rs         # Ekran yönlendirme
│   ├── theme.rs       # Renkler, kartlar, düğme stilleri
│   ├── setup_wizard.rs# İlk kurulum sihirbazı
│   ├── convert_view.rs# Tek ekran: dosya listesi + ortak hedef + ilerleme
│   ├── quick_convert.rs# CLI hedef seçici (pencere açmadan dönüşüm yolu)
│   └── settings.rs    # Ayarlar
├── core/
│   ├── detector.rs    # tool_exists(), paket yöneticisi tespiti
│   ├── installer.rs   # pkexec + dnf/pacman kurulumu
│   ├── registry.rs    # Format tanımı ve destek matrisi
│   └── job.rs         # Dönüşüm işi modeli (kuyruk)
├── converters/
│   ├── image.rs       # image crate
│   ├── av.rs          # FFmpeg çağrıları
│   ├── document.rs    # soffice/pandoc çağrıları
│   └── archive.rs     # zip/tar(.gz) + güvenli ayıklama
├── state/
│   └── config.rs      # config.toml okuma/yazma, setup_done bayrağı
└── util/
    ├── paths.rs       # XDG dizinleri, güvenli çıktı adları, çakışma çözümü
    └── process.rs     # güvenli Command, env temizliği, zaman aşımı, stderr

assets/                # donustur.desktop, donustur.png, ikon üretici
scripts/               # AppImage paketleme betiği
.github/workflows/     # CI ve Release (AppImage) iş akışları
```

## Akışlar

### İlk açılış (kurulum sihirbazı)
1. `detector.rs`: ffmpeg / soffice / pandoc varlık kontrolü
2. Eksikler listelenir; kullanıcı [Kur] / [Atla] seçer
3. [Kur] → `installer.rs`:
   - `detect_pkg_manager()`: `/usr/bin/dnf|dnf5` → Dnf, `/usr/bin/pacman` → Pacman
   - `pkexec dnf install -y ffmpeg` veya `pkexec pacman -S --noconfirm ffmpeg`
   - Paket adları dağıtıma göre haritalanır (bkz. installer.rs)
4. Başarılıysa `~/.config/donustur/config.toml` içine `setup_done = true` yazılır
5. Sonraki açılışlarda sadece sessiz `tool_exists()` kontrolü; eksik araç gerektiren hedefler hedef listesinden çıkarılır

### Dönüşüm
1. Kullanıcı bir veya çok dosyayı (karışık türler dahil) sürükler → `registry.rs` kaynak formatı tanır
2. Hedef format, tüm dosyaların desteklediği biçimlerin kesişiminden seçilir ve kurulu araçlarla doğrulanır
3. `job.rs` kuyruğa iş ekler; tokio semaforu ile sınırlı paralellikte çalışır
4. İlgili converter harici aracı `util/process.rs` üzerinden kabuksuz çağırır
5. İlerleme stderr/`-progress` parse ile güncellenir; sonuç bildirimi gösterilir
6. Çıktı önce geçici dosyaya yazılır, sonra `create_new` ile ayrılmış nihai
   yola taşınır; mevcut dosya asla ezilmez
7. Geçici dizinler `~/.cache/donustur/tmp` altında, iş bitince (hata dahil) silinir

### Sağ tık menüsü (KDE)
1. Uygulama açılışta `integration.rs` ile `~/.local/share` altına masaüstü girdisini, simgeyi ve MIME başına servis menülerini yazar (idempotent).
2. Menüler kullanıcı tarafından çalıştırılabilir (0755) olur; aksi hâlde KIO erişimi reddeder.
3. Dosyanın MIME türüne göre yalnızca desteklenen hedefler listelenir; örneğin PNG menüsünde JPEG, WebP, BMP, TIFF, GIF, ICO, ZIP ve TAR.GZ bulunur.
4. Seçim, `donustur --convert %F --to FORMAT` çağırır. Bu yol **pencere açmaz**: dönüşüm arka planda yapılır, sonuçta masaüstü bildirimi gönderilir.

### Dil
- `config.toml` içindeki `language` değeri `auto`, `tr` veya `en` olabilir.
- `auto` iken `LC_ALL`, `LC_MESSAGES`, `LANGUAGE`, `LANG` sırasıyla denenir; `tr*` Türkçe, `en*` İngilizce seçer.
- Eski sürümlerden gelen `language = "tr"` değeri, kullanıcı Ayarlar'dan açıkça seçmediyse otomatik algılamaya taşınır.

## Kritik ilkeler
- **Araçlar asla gömülmez** — Command ile çağrılır; AppImage 10-20 MB kalır
- **Kurulum idempotent** — zaten kuruluysa tekrar kurulmaz
- **Opsiyonel araçlar uygulamayı bloklamaz** — yoksa o araca bağlı hedefler listelenmez
- **Flatpak desteklenmez** (sistem paketlerine erişim engeli); dağıtım yalnız AppImage
