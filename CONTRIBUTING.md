# Katkıda Bulunma ve HANCORE Mimari Standartları (CONTRIBUTING.md)

Bu proje (`omarchy-omaprot` / **OmaProton**), Omarchy Linux (`linux-omarchy`) için CachyOS seviyesinde çekirdek (BORE + ADIOS + NTSYNC) ve işlemci mikromimarisi (`x86-64-v3`, `x86-64-v4`, `znver4`, `alderlake`) optimizasyonlu Proton katmanı ve Quickshell kontrol merkezidir.

## 1. HANCORE Güvenlik Kuralları
- **Süreç İzolasyonu:** Tüm alt süreçler `process_group(0)` ile ayrı süreç grubunda başlatılır ve `ProcessGroupGuard` (RAII) ile temizlenir (`src/subproc.rs`).
- **Dosya Okuma Tavanı:** Tüm dosya okumalarında 1 MiB (`take(cap + 1)`) sınırı uygulanır (`src/securefs.rs`).
- **Atomik Yazım ve Symlink Koruması:** Tüm yapılandırma ve Steam uyumluluk dosyaları `O_NOFOLLOW` ve `symlink_metadata` kontrolünden geçerek `.tmp_*` geçici dosyaları üzerinden atomik `rename` ile (`0700` dizin, `0600` yapılandırma) yazılır.
- **Argüman Enjeksiyonu Savunması:** Harici süreç çağrılarında `--` bayrak sonlandırıcı ve karakter beyaz listesi zorunludur.

## 2. Tipografi ve Görünüm Standartları
- Tüm Quickshell QML bileşenlerinde (`Panel.qml`, `qml/MainWindow.qml`, `qml/theme/Theme.qml`) varsayılan yazı tipi `"JetBrainsMono Nerd Font, JetBrains Mono, monospace"` olarak tanımlanmıştır.
- Sıfır Emoji Politikası geçerlidir; yalnızca Nerd Font v3 glifleri kullanılır.

## 3. Derleme ve Doğrulama
```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```
