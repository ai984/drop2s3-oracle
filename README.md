# Drop2S3 ☁️

> Lekka aplikacja Windows do przesylania plikow na Oracle Cloud Object Storage

[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)](https://www.rust-lang.org/)
[![Windows](https://img.shields.io/badge/Windows-10%2B-0078D6?logo=windows)](https://www.microsoft.com/windows)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![GitHub release](https://img.shields.io/github/v/release/ai984/drop2s3-oracle?include_prereleases)](https://github.com/ai984/drop2s3-oracle/releases)

---

## Co to jest?

**Drop2S3** to lekka aplikacja dzialajaca w zasobniku systemowym (system tray), ktora pozwala blyskawicznie przesylac pliki do Oracle Cloud Object Storage przez przeciagniecie i upuszczenie.

<img src="/assets/nJsenesuVL._com.webp" width="400">

---

## Funkcje

| Funkcja | Opis |
|---------|------|
| 🖱️ **Drag & Drop** | Przeciagnij pliki lub foldery |
| 📋 **Ctrl+V** | Wklej obrazy ze schowka (screenshoty) |
| 🔗 **Szybkie kopiowanie** | Link automatycznie w schowku |
| 📁 **Foldery** | Zachowuje strukture katalogow |
| 🔒 **Bezpieczne URL** | UUID w sciezce + noindex |
| ⚡ **Multipart upload** | Szybkie przesylanie duzych plikow |
| 🔄 **Auto-update** | Automatyczne aktualizacje z GitHub |
| 🎨 **Dark/Light mode** | Dopasowuje sie do systemu Windows |

---

## Instalacja

### Dla administratora (pierwsza konfiguracja)

1. Pobierz `Drop2S3.exe` z [Releases](https://github.com/ai984/drop2s3-oracle/releases)
2. Zaszyfruj credentials (patrz sekcja ponizej)
3. Utworz `config.toml` z zaszyfrowanymi danymi
4. Rozdystrybuuj `Drop2S3.exe` + `config.toml` do uzytkownikow

### Dla uzytkownika

1. Otrzymaj od administratora: `Drop2S3.exe` + `config.toml`
2. Umiesc oba pliki w tym samym folderze
3. Uruchom `Drop2S3.exe`
4. Gotowe - aplikacja dziala w zasobniku systemowym

---

## Pierwsza konfiguracja (Administrator)

### Krok 1: Uzyskaj credentials Oracle Cloud

1. Zaloguj sie do [Oracle Cloud Console](https://cloud.oracle.com/)
2. Przejdz do **Identity & Security** → **Users** → Twoj uzytkownik
3. Kliknij **Customer Secret Keys** → **Generate Secret Key**
4. Zapisz **Access Key** i **Secret Key** (Secret Key pokazywany tylko raz!)

### Krok 2: Zaszyfruj credentials

Uruchom w konsoli:

```cmd
Drop2S3.exe --encrypt
```

Program zapyta o Access Key i Secret Key, a nastepnie wygeneruje zaszyfrowane dane:

```
Drop2S3 Credential Encryption Tool
===================================

Access Key: AKIAIOSFODNN7EXAMPLE
Secret Key: wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

Add this to your config.toml:
------------------------------
[credentials]
version = 2
data = "base64_zaszyfrowane_dane..."
```

### Krok 3: Utworz config.toml

```toml
[oracle]
endpoint = "https://NAMESPACE.compat.objectstorage.REGION.oraclecloud.com"
bucket = "nazwa-bucketa"
namespace = "twoj-namespace"
region = "eu-frankfurt-1"

[credentials]
version = 2
data = "TUTAJ_WKLEJ_ZASZYFROWANE_DANE_Z_KROKU_2"

[app]
auto_copy_link = true
auto_start = false

[advanced]
parallel_uploads = 3
multipart_threshold_mb = 5
multipart_chunk_mb = 5
```

### Krok 4: Dystrybucja

Przekaz uzytkownikom tylko dwa pliki:
- `Drop2S3.exe`
- `config.toml`

> **Uwaga**: Credentials sa zaszyfrowane - nawet jesli ktos otworzy `config.toml`, nie zobaczy kluczy w postaci jawnej.

---

## Uzycie

### Podstawowe

1. **Kliknij ikone chmury** w zasobniku systemowym
2. **Przeciagnij plik** do okna Drop Zone
3. **Link skopiowany** do schowka ✓

### Skroty

| Akcja | Jak |
|-------|-----|
| Otworz okno | Klik w ikone tray |
| Upload | Przeciagnij plik na okno |
| Wklej screenshot | `Ctrl+V` gdy okno aktywne |
| Kopiuj poprzedni link | Klik w element historii |
| Otworz w przegladarce | Podwojny klik w historie |

### Menu kontekstowe (prawy klik na tray)

- **Pokaz okno** - otwiera Drop Zone
- **Zamknij** - wylacza aplikacje

---

## Bezpieczenstwo

| Zabezpieczenie | Opis |
|----------------|------|
| 🔐 **Szyfrowanie credentials** | XChaCha20-Poly1305 - credentials zaszyfrowane w config.toml |
| 🎲 **UUID w URL** | 16-znakowy losowy identyfikator w sciezce |
| 🤖 **robots.txt** | Plik robots.txt w buckecie zapobiega indeksowaniu (`--init-robots`) |
| 📦 **Portable** | Ikony zaszyte w exe - tylko 2 pliki do dystrybucji |

**Przykladowy URL:**
```
https://bucket.objectstorage.eu-frankfurt-1.oci.customer-oci.com/
  2026-02-03/faktura_a1b2c3d4e5f67890.pdf
  ^^^^^^^^^^ ^^^^^^^^ ^^^^^^^^^^^^^^^^ ^^^^
  data       nazwa    UUID (trudny     rozszerzenie
             pliku    do zgadniecia)
```

---

## Struktura plikow

```
📁 Drop2S3/
├── 📄 Drop2S3.exe      # Aplikacja (ikony zaszyte w srodku)
├── 📄 config.toml      # Konfiguracja z zaszyfrowanymi credentials
├── 📄 history.json     # Historia uploadow (tworzony automatycznie)
└── 📁 logs/            # Logi aplikacji (tworzony automatycznie)
    └── 📄 drop2s3.log.2026-02-03
```

---

## Rozwoj

### Wymagania deweloperskie

- Rust 1.75+ (stable)
- **Windows**: Windows 10+ SDK + Visual Studio Build Tools
- **Linux (cross-compilation)**: xwin toolchain + lld-link-19

### Budowanie

```bash
# Debug (wymaga --target na Linux)
cargo build --target x86_64-pc-windows-msvc

# Release (zoptymalizowany, ~2-3 MB exe)
cargo build --release --target x86_64-pc-windows-msvc
```

### Testy

```bash
cargo test --target x86_64-pc-windows-msvc
```

---

## Roadmap

- [x] Podstawowy upload drag & drop
- [x] System tray z menu
- [x] Historia plikow
- [x] Multipart upload
- [x] Wklejanie ze schowka (Ctrl+V)
- [x] Szyfrowanie credentials (portable)
- [x] Ikony zaszyte w exe
- [x] Auto-update z GitHub Releases
- [ ] Upload folderow z zachowaniem struktury
- [ ] Obsluga wielu profili/bucketow

---

## Kompilacja ze zrodel

```bash
# Wymagania: Rust 1.75+, Windows 10+ SDK (lub xwin na Linux)
git clone https://github.com/ai984/drop2s3-oracle.git
cd drop2s3-oracle
cargo build --release --target x86_64-pc-windows-msvc

# Plik wykonywalny: target/x86_64-pc-windows-msvc/release/drop2s3.exe
```

---

## Licencja

[MIT](LICENSE) - rob co chcesz, ale bez gwarancji.

---

## Autor

Stworzone z ☕ i 🦀

---

<p align="center">
  <sub>Jesli Drop2S3 oszczedza Ci czas, zostaw ⭐ na GitHubie!</sub>
</p>
