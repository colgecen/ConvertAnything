# AGENTS — Yapay Zekâ Ajanları İçin Proje Kuralları

Bu dosya, bu depoda çalışan AI ajanlarının (Claude Code, Cursor, Copilot vb.) uyması gereken kuralları tanımlar.

## Proje bağlamı
- Dil: Rust (stable toolchain). GUI: egui/eframe. Async: tokio.
- Hedef: Linux masaüstü (Fedora dnf, Arch pacman), Wayland. Dağıtım: AppImage.
- Dönüşüm motorları (ffmpeg, soffice, pandoc, 7z) **gömülmez**, sistemden `Command` ile çağrılır.
- Daha fazla mimari detay: ARCHITECTURE.md. Güvenlik kuralları: SECURITY.md. İş planı: TASKS.md.

## Zorunlu kurallar
1. `unsafe` yasak — `#![forbid(unsafe_code)]` kaldırılamaz.
2. Harici araç çağrılarında kabuk kullanılmaz (`sh -c` yasak). Argümanlar her zaman dizi olarak.
3. Yeni format desteği eklenecekse önce `registry.rs` destek matrisine işlenir; UI ve converter dokusu ayrı ayrı güncellenir.
4. Panik toleransı: converter'lar `Result` döner; `.unwrap()` yalnızca test kodunda.
5. Kullanıcı arayüzü dili: Türkçe. Kod/yorum dili: İngilizce.
6. Tüm yeni işler `TASKS.md`'deki milestone'a eklenir; bitenler işaretlenir.

## Kodlama standartları
- `cargo clippy -- -D warnings` ve `cargo fmt` CI'da geçmeli.
- Her converter modülü: giriş doğrulama → Command inşası → ilerleme raporlama → temizlik (Drop-safe).
- Dosya I/O'da `tokio::fs`; CPU işi `spawn_blocking` içinde.
- Konfigürasyon şeması değişirse `config.toml` geriye dönük okunabilir kalmalı.

## Ajan görev akışı
1. TASKS.md'den bir görev seç
2. SECURITY.md'i ihlal etmediğinden emin ol
3. İmplemente et + `cargo test`
4. clippy/fmt çalıştır
5. TASKS.md'de işi işaretle, kısa özet not düş

## Yasaklar
- Telemetri / ağ isteği / otomatik güncelleme eklemek (ürün kararı; ajan tek başına ekleyemez)
- Flatpak/snap desteği eklemek (bilinçli olarak dışarıda bırakıldı)
- Yeni runtime bağımlılığı eklemek (öneri olarak raporla, uygulama)
