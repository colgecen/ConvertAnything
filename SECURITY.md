# GÜVENLİK POLİTİKASI

Uygulama yerel çalışır; ağ yüzeyi yoktur. Ana tehditler: dönüştürülen **zararlı girdi dosyaları**, 
**harici araç çağrılarının kötüye kullanımı** ve **kullanıcı verilerinin gizliliği**.

## 1. Girdi güvenliği
- Kullanıcıdan gelen her dosya şüpheyle ele alınır; dosya uzantısına değil **magic bytes** imzasına güvenilir
- Dosya boyutu sınırı (varsayılan 4 GB, ayarlanabilir) — bellek patlamasını önler
- Zararlı formatlar listesi (yürütülebilirler: .exe, .elf, .sh, .so) dönüştürme hedefi/kaynağı yapılamaz
- Arşiv açımında **zip bomb** koruması: oran sınırı (çıktı/girdi > 100 ise reddet), toplam çıktı boyutu sınırı, iç içe arşiv derinlik sınırı (max 3)

## 2. Alt süreç (subprocess) güvenliği — EN KRİTİK
FFmpeg/soffice çağrılarında kabuk kullanılmaz, argümanlar dizi halinde verilir:
```rust
// DOĞRU — shell injection imkânsız
Command::new("ffmpeg").args(["-i", &input, &output])

// YASAK — hiçbir yerde
Command::new("sh").arg("-c").arg(format!("ffmpeg -i {input} ..."))
```
- Girdi/yol argümanlarında `-` ile başlayan değerler reddedilir veya `--` sonrası işaretlenir (argüman enjeksiyonu önlemi)
- Harici araçların çalışma dizini geçici bir klasöre sabitlenir
- Çevre değişkenleri (LD_PRELOAD, PATH) dönüşüm süreçleri için sanitize edilir
- stdout/stderr pipe'ları tıkanmaması için sürekli tüketilir (deadlock önlemi)

## 3. Kaynak sınırları (runaway iş koruması)
- İş başına CPU/zaman sınırı (ör. 30 dk) — aşılırsa iş iptal edilir, araç süreçleri öldürülür
- Paralel iş sayısı = CPU çekirdek sayısı; tek seferde max N job
- Disk doluluğu kontrolü: hedef dizinde gereken alan yoksa iş başlamadan reddedilir

## 4. Dosya sistemi güvenliği
- Çıktı dosyaları asla **mevcut kullanıcı dosyasının üzerine yazılmaz** — çakışmada `-1`, `-2` eklenir (kullanıcı onaylamadıkça)
- Geçici dosyalar 0600 izniyle açılır; iş bitince (başarılı da hatalı da) silinir; panik sonrası kalanlar başlangıçta temizlenir
- Yol çıkarma (path traversal) kontrolü: hedef yol, seçilen çıktı dizininin dışına çıkamaz (canonicalize + starts_with)

## 5. Gizlilik (KVKK/GDPR)
- Uygulama telemetri göndermez, ağ isteği atmaz; tamamen çevrimdışı çalışır
- Kullanıcı verisi yalnızca kullanıcının belirttiği dizinde işlenir; hiçbir log'a dosya içeriği yazılmaz
- Loglarda hassas veri maskelenir; hata raporları yalnızca stderr özeti içerir

## 6. Tedarik zinciri
- `Cargo.lock` commit'lenir; bağımlılıklar `cargo audit` ile CI'da taranır
- `unsafe` kod yasaktır (`#![forbid(unsafe_code)]`)
- Harici araçların sürüm aralığı config'te doğrulanır (çok eski FFmpeg reddedilir)

## 7. Kurulum sihirbazı güvenliği
- Yalnızca sabit paket adları kurulabilir; kullanıcı girdisi paket adına asla eklenmez
- `pkexec` dışında root alma yolu yoktur; `sudo` çağrılmaz
- Kurulum çıktısı loglanır, kullanıcıya özet gösterilir
