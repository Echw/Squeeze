# Squeeze — lokalna inteligentna kompresja obrazów

Squeeze zachowuje format wejściowy (JPEG → JPEG, PNG → PNG), działa lokalnie w
przeglądarce i nie wysyła plików ani ich nazw do sieci. Repozytorium zawiera jeden
pipeline używany przez CLI i WebAssembly.

## Stan v1

- JPEG: adaptacyjne przeszukiwanie jakości i podpróbkowania przez `mozjpeg-rs`,
  SSIMULACRA2 oraz Butteraugli dla finalistów.
- PNG: OxiPNG lossless w CLI; quantette dla nieprzezroczystych PNG 8-bit w CLI/WASM,
  z bezpiecznym passthrough lossless dla alpha i PNG 16-bit w przeglądarce.
- Przeglądarka: pojedynczy Worker, transferables, anulowanie przez restart Workera,
  kolejka wielu plików, pobieranie, ZIP, porównanie before/after i raport JSON.
- Prywatność: po załadowaniu aplikacja nie potrzebuje sieci. CSP blokuje połączenia.

Jpegli nie jest zależnością produkcyjną. Dostępny wrapper Rust ma licencję AGPL/
commercial, więc nie spełnia allowlisty. Natywny Jpegli można później dodać wyłącznie
jako oddzielny adapter benchmarkowy na licencji BSD, jeśli przejdzie bramkę WASM.

## Instalacja i uruchomienie aplikacji w przeglądarce

Poniższe kroki przygotowują **pełną aplikację**: interfejs oraz lokalny silnik
kompresji WebAssembly. Wszystko wykonuj w katalogu głównym repozytorium, czyli
tam, gdzie znajduje się ten plik `README.md`.

### 1. Zainstaluj wymagane narzędzia (jednorazowo)

Potrzebujesz:

- Node.js 20 lub nowszego (razem z `npm`),
- Rust 1.90 lub nowszego (`rustc` i `cargo`),
- `wasm-bindgen-cli` w wersji zgodnej z zależnością `wasm-bindgen` projektu.

Najprościej zainstalować Rust przez [rustup](https://rustup.rs/). Po instalacji
otwórz nowe okno terminala i sprawdź, czy narzędzia są dostępne:

```sh
node --version
npm --version
rustc --version
cargo --version
```

Następnie zainstaluj narzędzie wiążące moduł Rust z przeglądarką:

```sh
cargo install wasm-bindgen-cli --version 0.2.128 --locked
wasm-bindgen --version
```

`wasm-pack` jest obsługiwany jako zapasowy mechanizm budowania, ale nie jest
potrzebny w standardowym środowisku projektu.

Jeżeli któreś z poleceń zwraca komunikat „command not found”, zamknij i otwórz
terminal ponownie. Na Windows warto upewnić się, że terminal został otwarty po
instalacji Rust; instalator dodaje katalog z narzędziami do `PATH`.

### 2. Pobierz zależności frontendu

W katalogu projektu uruchom:

```sh
npm --prefix web ci
```

Polecenie korzysta z zapisanego lockfile, więc instaluje dokładnie zestaw
zależności przewidziany dla projektu. Wykonuj je po świeżym sklonowaniu projektu
albo gdy zmieni się `web/package-lock.json`.

### 3. Uruchom aplikację

```sh
npm run dev
```

Pierwsze uruchomienie buduje silnik WASM. Potem skrypt wykrywa zmiany w `crates/`
i manifestach Cargo, przebudowuje tylko wtedy, gdy jest to potrzebne, a Vite
odświeża aplikację. Terminal pokaże lokalny adres, zwykle
`http://localhost:5173/`. Otwórz go w przeglądarce. Pozostaw ten terminal otwarty
podczas pracy; aby zatrzymać serwer, naciśnij `Ctrl+C`.

Jeśli aplikacja po wybraniu obrazu zgłasza „Nie znaleziono silnika WASM”, wróć do
kroku 3, sprawdź komunikat budowania WASM i odśwież stronę. Sam serwer frontendu
może się uruchomić bez Rust/WASM, ale kompresja obrazów wtedy nie działa.

### Szybka lista poleceń

Gdy środowisko jest już przygotowane, przy kolejnych uruchomieniach wystarczy:

```sh
npm run dev
```

Do jednorazowego ręcznego zbudowania silnika użyj `npm run build:wasm`.

CLI:

CLI działa niezależnie od przeglądarki, ale również wymaga Rusta. Przykłady:

```sh
cargo run -p optimizer-cli -- optimize photo.jpg --profile balanced --json
cargo run -p optimizer-cli -- benchmark ./corpus --csv
```

## Sprawdzenie projektu

Przed zmianą kodu lub przed przekazaniem zmian możesz uruchomić pełną walidację:

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo deny check
npm --prefix web ci
npm --prefix web run check
```

Jeśli firmowa polityka Windows Application Control blokuje `rustc.exe`, kompilację
Rust/WASM trzeba uruchomić w zatwierdzonym środowisku CI albo poprosić administratora
o dopuszczenie oficjalnego toolchaina Rust. Sam frontend nadal można budować.

## Licencje

Kod projektu: MIT. CI dopuszcza wyłącznie MIT, Apache-2.0, BSD-2-Clause,
BSD-3-Clause, ISC i Zlib. GPL, AGPL, biblioteki komercyjne oraz `libimagequant` są
jawnie blokowane.
