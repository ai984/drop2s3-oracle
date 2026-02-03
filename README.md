# Drop2S3 ☁️

> Minimalistyczna aplikacja Windows do szybkiego przesyłania plików na Oracle Cloud Object Storage

[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)](https://www.rust-lang.org/)
[![Windows](https://img.shields.io/badge/Windows-10%2B-0078D6?logo=windows)](https://www.microsoft.com/windows)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/ai984/drop2s3-oracle?include_prereleases)](https://github.com/ai984/drop2s3-oracle/releases)

---

## Co to jest?

**Drop2S3** to lekka aplikacja działająca w zasobniku systemowym (system tray), która pozwala błyskawicznie przesyłać pliki do Oracle Cloud Object Storage przez przeciągnięcie i upuszczenie.

```
┌─────────────────────────────────────┐
│                                     │
│         Przeciągnij plik            │
│              tutaj                  │
│                                     │
│            ☁️ ↑                      │
│                                     │
├─────────────────────────────────────┤
│ 📋 Ostatni: faktura.pdf             │
│    https://...eu-fra.../faktura.pdf │
│                        [Kopiuj]     │
├─────────────────────────────────────┤
│ Historia:                           │
│ • screenshot_2026-02-03.png         │
│ • dokument.docx                     │
│ • zdjecie.jpg                       │
└─────────────────────────────────────┘
```

---

## Funkcje

| Funkcja | Opis |
|---------|------|
| 🖱️ **Drag & Drop** | Przeciągnij pliki lub foldery |
| 📋 **Ctrl+V** | Wklej obrazy ze schowka (screenshoty) |
| 🔗 **Szybkie kopiowanie** | Link automatycznie w schowku |
| 📁 **Foldery** | Zachowuje strukturę katalogów |
| 🔒 **Bezpieczne URL** | UUID w ścieżce + noindex |
| ⚡ **Multipart upload** | Szybkie przesyłanie dużych plików |
| 🔄 **Auto-update** | Automatyczne aktualizacje z GitHub |
| 🎨 **Dark/Light mode** | Dopasowuje się do systemu Windows |

---

## Instalacja

### Opcja 1: Pobierz gotowy .exe

1. Przejdź do [Releases](https://github.com/ai984/drop2s3-oracle/releases)
2. Pobierz `Drop2S3.exe`
3. Umieść w dowolnym folderze
4. Uruchom i skonfiguruj

### Opcja 2: Kompilacja ze źródeł

```bash
# Wymagania: Rust 1.75+, Windows 10+
git clone https://github.com/ai984/drop2s3-oracle.git
cd drop2s3-oracle
cargo build --release

# Plik wykonywalny: target/release/Drop2S3.exe
```

---

## Konfiguracja

Przy pierwszym uruchomieniu aplikacja utworzy plik `config.toml` obok `.exe`:

```toml
[oracle]
endpoint = "https://NAMESPACE.compat.objectstorage.REGION.oraclecloud.com"
bucket = "my-bucket"
access_key = "twoj_access_key"
secret_key = "twoj_secret_key"
region = "eu-frankfurt-1"

[app]
auto_copy_link = true    # Automatycznie kopiuj link po uploadzie
auto_start = false       # Uruchamiaj z Windows

[advanced]
parallel_uploads = 3     # Ile plików jednocześnie
multipart_threshold_mb = 5
multipart_chunk_mb = 5
```

### Jak uzyskać credentials Oracle Cloud?

1. Zaloguj się do [Oracle Cloud Console](https://cloud.oracle.com/)
2. Przejdź do **Identity & Security** → **Users** → Twój użytkownik
3. Kliknij **Customer Secret Keys** → **Generate Secret Key**
4. Skopiuj Access Key i Secret Key do `config.toml`

> ⚠️ **Uwaga**: Secret Key jest pokazywany tylko raz! Zapisz go bezpiecznie.

---

## Użycie

### Podstawowe

1. **Kliknij ikonę chmury** w zasobniku systemowym
2. **Przeciągnij plik** do okna Drop Zone
3. **Link skopiowany** do schowka ✓

### Skróty

| Akcja | Jak |
|-------|-----|
| Otwórz okno | Klik w ikonę tray |
| Upload | Przeciągnij plik na okno lub ikonę tray |
| Wklej screenshot | `Ctrl+V` gdy okno aktywne |
| Kopiuj poprzedni link | Klik w element historii |
| Otwórz w przeglądarce | Podwójny klik w historię |

### Menu kontekstowe (prawy klik na tray)

- **Pokaż okno** - otwiera Drop Zone
- **Ustawienia** - edycja konfiguracji
- **Zamknij** - wyłącza aplikację

---

## Bezpieczeństwo

| Zabezpieczenie | Opis |
|----------------|------|
| 🔐 **DPAPI** | Sekrety szyfrowane Windows Data Protection API |
| 🎲 **UUID w URL** | 16-znakowy losowy identyfikator w ścieżce |
| 🤖 **noindex** | Nagłówek X-Robots-Tag zapobiega indeksowaniu |

**Przykładowy URL:**
```
https://bucket.objectstorage.eu-frankfurt-1.oci.customer-oci.com/
  2026-02-03/a1b2c3d4e5f67890/faktura.pdf
  ^^^^^^^^^^ ^^^^^^^^^^^^^^^^ ^^^^^^^^^^^
  data       UUID (trudny     nazwa pliku
             do zgadnięcia)
```

---

## Struktura plików

```
📁 Drop2S3/
├── 📄 Drop2S3.exe      # Aplikacja
├── 📄 config.toml      # Konfiguracja (tworzony automatycznie)
├── 📄 history.json     # Historia uploadów
└── 📁 logs/            # Logi aplikacji
    └── 📄 2026-02-03.log
```

---

## Rozwój

### Wymagania deweloperskie

- Rust 1.75+ (stable)
- Windows 10+ SDK
- Visual Studio Build Tools

### Budowanie

```bash
# Debug
cargo build

# Release (zoptymalizowany)
cargo build --release

# Uruchom
cargo run
```

### Testy

```bash
cargo test
```

---

## Roadmap

- [x] Podstawowy upload drag & drop
- [x] System tray z menu
- [x] Historia plików
- [x] Multipart upload
- [ ] Wklejanie ze schowka (Ctrl+V)
- [ ] Upload folderów z zachowaniem struktury
- [ ] Auto-update z GitHub Releases
- [ ] Obsługa wielu profili/bucketów

---

## Licencja

[MIT](LICENSE) - rób co chcesz, ale bez gwarancji.

---

## Autor

Stworzone z ☕ i 🦀

---

<p align="center">
  <sub>Jeśli Drop2S3 oszczędza Ci czas, zostaw ⭐ na GitHubie!</sub>
</p>
