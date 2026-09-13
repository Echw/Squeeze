# Squeeze

Squeeze kompresuje obrazy JPEG i PNG lokalnie w przeglądarce. Może zachować
format źródłowy albo przekonwertować wynik do WebP. Pliki, nazwy i piksele nie
są wysyłane na serwer.

## Co działa

- pojedyncze obrazy i całe paczki;
- profile jakości z kontrolowaną kompresją stratną;
- bezstratna i paletowa optymalizacja PNG;
- lokalna kompresja JPEG i PNG przez Rust oraz WebAssembly;
- lokalna konwersja do WebP przez `@jsquash/webp`;
- kolejka z autostartem, pauzą, anulowaniem i ponownym przeliczeniem;
- porównanie przed i po z suwakiem, zoomem oraz skalą 1:1;
- pobieranie pojedynczego wyniku lub paczki ZIP;
- działanie offline po zapisaniu potrzebnych zasobów przez przeglądarkę.

Limity wejścia to 100 MB i 24 MP na plik. Obsługiwane są JPEG i PNG. AVIF oraz
przetwarzanie na serwerze pozostają poza obecnym zakresem.

Nowa instalacja używa profilu `Mocna kompresja`, zachowuje format i rozpoczyna
pracę automatycznie. Dla kwalifikującego się nieprzezroczystego PNG 8-bit szybka
automatyka tworzy jeden wariant: paleta 256 kolorów bez ditheringu. Zawsze
zachowuje format i wymiary, a plik większy od wejścia zostaje odrzucony. Pełne
porównanie wariantów, dithering, bezstratna ścieżka i ręczna paleta pozostają w
zwiniętych ustawieniach zaawansowanych.

## Technologie

- Vite i vanilla TypeScript dla interfejsu;
- Rust dla kompresji JPEG i PNG;
- WebAssembly i Web Worker dla pracy poza głównym wątkiem;
- `@jsquash/webp` z libwebp dla konwersji do WebP;
- Vitest dla testów logiki frontendu.

## Wymagania

- Node.js `20.19+` albo `>=22.12`;
- npm;
- Rust `1.90+` zainstalowany przez [rustup](https://rustup.rs/);
- target Rust `wasm32-unknown-unknown`;
- `wasm-bindgen-cli` w wersji `0.2.128`.

### Windows

`npm run dev`, Vite i skrypty budujące WASM obsługują Windows bez powłoki Unix.
Do pracy nad aplikacją webową wystarczą Node.js, Rust oraz target
`wasm32-unknown-unknown`. Do uruchamiania natywnego CLI na Windows potrzebne są
dodatkowo **Visual Studio Build Tools** z workloadem „Desktop development with
C++” — zależność `libdeflate` kompiluje kod C dla lokalnego optymalizatora PNG.
W PowerShell katalog na fixture podawaj jako `$env:TEMP\\squeeze-fixtures`.

Wersje możesz sprawdzić poleceniami:

```sh
node --version
npm --version
rustc --version
cargo --version
```

## Szybki start

Wszystkie polecenia uruchamiaj w głównym katalogu repozytorium.

1. Przygotuj narzędzia Rust i WebAssembly:

   ```sh
   rustup target add wasm32-unknown-unknown
   cargo install wasm-bindgen-cli --version 0.2.128 --locked
   ```

2. Zainstaluj zależności:

   ```sh
   npm ci
   ```

3. Uruchom aplikację:

   ```sh
   npm run dev
   ```

Vite wypisze adres aplikacji, zwykle <http://localhost:5173/>. Proces zatrzymasz
przez `Ctrl+C`.

Repozytorium korzysta z npm workspaces i jednego pliku `package-lock.json` w
głównym katalogu. Zależności instaluj z tego miejsca. Nie uruchamiaj osobnego
`npm install` w katalogu `web/`.

## Development

`npm run dev` przygotowuje brakujący lub nieaktualny moduł WASM, uruchamia Vite
i obserwuje kod Rust. Zmiana w `optimizer-core` lub `optimizer-wasm` przebudowuje
silnik bez restartowania całego środowiska. Vite obsługuje zmiany w TypeScript,
HTML i SCSS.

Do pracy wyłącznie nad interfejsem użyj:

```sh
npm run dev:web
```

Ten tryb korzysta z istniejącego artefaktu WASM. Na świeżym klonie przygotuj go
najpierw przez `npm run build:wasm`.

### Dostępne polecenia

| Polecenie | Działanie |
| --- | --- |
| `npm run dev` | Uruchamia Vite i automatyczną przebudowę Rust/WASM |
| `npm run dev:web` | Uruchamia tylko Vite |
| `npm run build:wasm` | Buduje produkcyjny moduł Rust/WASM |
| `npm run build` | Buduje WASM i aplikację do `web/dist/` |
| `npm run test` | Uruchamia testy Vitest i wszystkich pakietów Rust |
| `npm run check:web` | Sprawdza testy UI, TypeScript, build Vite i licencje npm |
| `npm run check` | Sprawdza format Rust, testy Rust oraz pełne `check:web` |

### Walidacja przed wysłaniem zmian

Uruchom kolejno:

```sh
npm run check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
npm run build
```

`cargo deny` wymaga jednorazowej instalacji:

```sh
cargo install cargo-deny --locked
```

GitHub Actions wykonuje te same kontrole na Node.js 20 i Rust 1.90.

### Aktualizacja zależności

- zależności npm dodawaj i aktualizuj z głównego katalogu repozytorium;
- zależności Rust definiuj wspólnie w sekcji `workspace.dependencies`;
- commituj właściwy lockfile razem ze zmianą zależności;
- po aktualizacji uruchom `npm run check` i `cargo deny check`;
- wersję `wasm-bindgen-cli` utrzymuj zgodną z `wasm-bindgen` w `Cargo.lock`.

## Build produkcyjny

```sh
npm run build
```

Gotowe pliki trafiają do `web/dist/`. Build zawiera aplikację, Workery, moduł
Rust/WASM oraz lokalny enkoder i dekoder WebP. Service Worker przechowuje zasoby
aplikacji i usuwa starsze wersje cache.

## CLI i benchmarki

CLI korzysta z tego samego rdzenia Rust co aplikacja:

```sh
cargo run -p optimizer-cli -- optimize photo.jpg --profile balanced --json
cargo run -p optimizer-cli -- benchmark ./corpus --profile maximum-compression --method auto --search-effort auto --warmup 1 --runs 5 --json
```

Format lokalnego corpusu oraz deterministyczny generator legalnych fixture’ów
opisuje [corpus/README.md](corpus/README.md). Obrazy testowe nie są częścią
repozytorium. Manifest corpusu powinien zawierać ich pochodzenie i licencję.

## Struktura repozytorium

```text
crates/optimizer-core/   kompresja JPEG i PNG, metryki i limity zasobów
crates/optimizer-wasm/   API łączące Rust z Workerem przeglądarkowym
crates/optimizer-cli/    CLI oraz benchmark corpusu
web/                     interfejs, kolejka, Workery i pobieranie wyników
scripts/                 build WASM, tryb developerski i kontrole projektu
```

Decyzje produktowe opisuje [PRODUCT.md](PRODUCT.md), a zasady interfejsu
[DESIGN.md](DESIGN.md).

## Rozwiązywanie problemów

### Brak targetu WebAssembly

```sh
rustup target add wasm32-unknown-unknown
npm run build:wasm
```

### Brak lub zła wersja `wasm-bindgen`

```sh
cargo install wasm-bindgen-cli --version 0.2.128 --locked --force
npm run build:wasm
```

Skrypt użyje `wasm-pack` jako mechanizmu zapasowego, jeśli jest dostępny.

### UI działa, ale kompresja zachowująca format jest niedostępna

Przyczyną jest zwykle uruchomienie `npm run dev:web` bez gotowego modułu WASM:

```sh
npm run build:wasm
npm run dev:web
```

Po przebudowaniu odśwież stronę.

### Port 5173 jest zajęty

Vite wybierze kolejny wolny port. Użyj adresu wyświetlonego po `Local:`.

## Prywatność

Obrazy trafiają bezpośrednio do lokalnego Workera. Kod aplikacji nie wysyła ich
nazw ani pikseli. Service Worker obsługuje tylko zasoby aplikacji z tego samego
źródła.

## Licencja

Kod Squeeze jest dostępny na licencji [MIT](LICENSE).

Aktualne zależności npm i Cargo korzystają z licencji pozwalających na darmowe
użycie, modyfikację i dystrybucję, także w projektach komercyjnych. Kontrole
projektu odrzucają między innymi GPL, AGPL, LGPL oraz licencje komercyjne.
Licencje MIT, Apache 2.0 i BSD wymagają zachowania właściwych informacji
licencyjnych podczas dystrybucji.
