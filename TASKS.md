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

## Milestone 6 — Paketleme (1 hafta) → PACKAGING.md — TAMAM
- [x] linuxdeploy + appimagetool ile AppImage
- [x] .desktop dosyası + ikon
- [x] GitHub Release CI (x86_64)

## Milestone 7 — Tek ekran toplu dönüşüm (UI yenileme) — TAMAM
- [x] Sekme bazlı dönüşüm ekranlarını kaldır; tek dosya listesi
- [x] Karışık türlerde ortak hedef biçim (kümelerin kesişimi) hesabı
- [x] Sürükle-bırak ile her türden çoklu dosya ekleme
- [x] Kart tabanlı görünüm, tür rozetleri, canlı durum renkleri
- [x] Seçeneklerin yalnızca ilgili türlerde gösterilmesi
- [x] Ayarlar ekranına başlıktan erişim

## Milestone 8 — Masaüstü entegrasyonu ve dil — TAMAM
- [x] KDE Dolphin sağ tık menüsü: MIME başına yalnızca uygun hedef biçimler
- [x] Menüden seçimde pencere açmadan (headless) dönüşüm + masaüstü bildirimi
- [x] Türkçe/İngilizce arayüz, `auto` sistem dili algılama ve eski ayar geçişi
- [x] Kart tabanlı yeni tema, yapışkan dönüştür çubuğu, tür rozetleri
- [x] CLI: `donustur DOSYALAR`, `--convert`, `--to`, `--install-integration`

## Milestone 9 — Kurulum ve oturum açılışı — TAMAM
- [x] `--install`: AppImage'ı `~/.local/bin/Donustur.AppImage` konumuna kopyalar
- [x] Masaüstü girdisi, simge ve sağ tık menülerini kurulu kopyaya yönlendirir
- [x] `~/.config/autostart/donustur.desktop` ile oturum açılışında başlatma
- [x] Ayarlar → Sistem entegrasyonu: kurulum ve otomatik başlatma anahtarı
- [x] İndirilen AppImage ilk açılışta kendini otomatik kurar ve oturum açılışına ekler
- [x] `--uninstall`: uygulama, menüler ve otomatik başlatma kaydını temizler

## Güvenlik denetimi (her milestone sonu)
- [x] SECURITY.md kontrol listesi gözden geçirilir
- [x] cargo audit (CI: rustsec/audit-check)
- [x] Zip bomb / path traversal / argüman enjeksiyonu testleri

## Notlar
- M0: `util/process.rs` tüm harici çağrıları tek noktadan sarar (env temizliği, `kill_on_drop`, zaman aşımı, sınırlı stderr).
- M1: Sihirbaz `pkexec` + sabit paket listesi kullanır; kurulum çıktısı kısaltılarak gösterilir.
- M2: Çıktı adı `-1`, `-2` ile çakışma çözer; nihai yol `create_new` ile ayrılır, üzerine yazma varsayılan kapalı.
- M3: H.264 kodlayıcı `libx264 → libopenh264 → mpeg4` sırasıyla seçilir; FFmpeg < 4 reddedilir.
- M4: Belge hedefleri araç kurulumuna göre filtrelenir; TXT/CSV bellekte UTF-8'e çevrilir (256 MiB sınırı).
- M5: Zip bomb (oran + toplam boyut + girdi sayısı), path traversal ve symlink girdileri reddedilir.
- M6: `scripts/package-appimage.sh` üretimi 4.6 MB AppImage olarak doğrulandı (hedef 10-20 MB'ın altında).
- M7: Tek ekranda karışık dosyalar; hedef listesi tüm dosyaların desteklediği biçimlerin kesişimidir. Arşive paketleme/ayıklama medya araçları gerektirmez.
- M8: Sağ tık menüleri MIME başına üretilir ve 0755 izniyle yazılır (KIO aksi hâlde reddeder). `--to` verilen çağrılar pencere açmadan çalışır; bildirim `notify-send` ile en iyi çaba esaslıdır.
- M9: Kurulum `~/.local/bin/Donustur.AppImage` hedefini kullanır; menüler ve otomatik başlatma bu kalıcı yola bağlanır. Kurulu kopya varsa uygulama menüleri tekrar geçici yola çevirmez.
- Test: 108 birim/uçtan uca test (1 LibreOffice e2e isteğe bağlı); `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check` temiz.

## V2 fikirleri (şimdilik yapma)
- Toplu önizleme, format öneri motoru, sağ tık menüsü entegrasyonu (Nautilus/Dolphin), arşiv desteği genişletme
- AVIF hedefi (`ravif`) ve SVG kaynağı (`resvg`) için yeni bağımlılık önerisi — AGENTS.md gereği ajan tek başına eklemez
