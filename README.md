# Squeeze

Squeeze to lokalna aplikacja do kompresji JPEG i PNG oraz opcjonalnej konwersji
do WebP. Interfejs działa w przeglądarce, a obrazy, ich nazwy i piksele nie są
wysyłane na serwer.

Projekt łączy:

- Vite i vanilla TypeScript dla interfejsu;
- Rust kompilowany do WebAssembly dla JPEG i PNG;
- dostarczany z aplikacją `@jsquash/webp` dla WebP;
- pojedynczy Worker do ograniczenia zużycia pamięci podczas kompresji.

## Uruchomienie od zera

Wszystkie polecenia wykonuj w katalogu głównym repozytorium, obok tego pliku.

### Wymagania

- Node.js `20.19+` albo `22.12+`;
- npm;
- Rust `1.90+` instalowany przez [rustup](https://rustup.rs/);
- target Rust `wasm32-unknown-unknown`;
- `wasm-bindgen-cli` `0.2.128`.

Sprawdź środowisko:

```sh
node --version
npm --version
rustc --version
cargo --version
```

Przygotuj narzędzia WASM. Te dwa polecenia wykonuje się jednorazowo:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.128 --locked
```

Następnie zainstaluj zależności i uruchom pełne środowisko developerskie:

```sh
npm ci
npm run dev
```

Vite wypisze adres aplikacji, zwykle <http://localhost:5173/>. Serwer zatrzymasz
przez `Ctrl+C`.

Repozytorium jest npm workspace'em z jednym plikiem `package-lock.json` w katalogu
głównym. Nie uruchamiaj osobnego `npm install` w `web/`.

## Jak działa development

`npm run dev` jest podstawowym poleceniem do codziennej pracy. Przy starcie:

1. sprawdza, czy artefakty WASM istnieją i są aktualne;
2. w razie potrzeby buduje Rust dla `wasm32-unknown-unknown`;
3. uruchamia Vite;
4. obserwuje źródła Rust i automatycznie przebudowuje WASM po zmianach.

Zmiany w TypeScript, HTML i SCSS obsługuje Vite. Zmiany w `crates/optimizer-core`
lub `crates/optimizer-wasm` uruchamiają przebudowę silnika bez restartowania całego
środowiska.

Do pracy wyłącznie nad interfejsem możesz użyć:

```sh
npm run dev:web
```

To polecenie nie buduje i nie obserwuje Rusta. Na świeżym klonie najpierw uruchom
`npm run build:wasm`; w przeciwnym razie UI się otworzy, ale kompresja zachowująca
format nie będzie dostępna.

## Polecenia

| Polecenie | Zastosowanie |
| --- | --- |
| `npm run dev` | Pełny development: WASM, automatyczna przebudowa Rusta i Vite |
| `npm run dev:web` | Tylko Vite, z już zbudowanym WASM |
| `npm run build:wasm` | Jednorazowy produkcyjny build Rust/WASM |
| `npm run build` | Build WASM i produkcyjny build aplikacji do `web/dist/` |
| `npm run test` | Testy Vitest oraz wszystkie testy Rust |
| `npm run check:web` | Testy frontendowe, TypeScript, build Vite i kontrola licencji npm |
| `npm run check` | Format Rust, testy Rust oraz pełne `check:web` |

## Build produkcyjny

```sh
npm run build
```

Gotowa aplikacja trafia do `web/dist/`. Build zawiera moduł WASM, Workery oraz
enkoder i dekoder WebP. Service Worker wersjonuje cache aplikacji i usuwa jego
starsze wersje.

Przed wydaniem uruchom:

```sh
npm run check
cargo clippy --workspace --all-targets -- -D warnings
```

CI dodatkowo wykonuje `cargo deny check`. Lokalnie wymaga to jednorazowej
instalacji:

```sh
cargo install cargo-deny --locked
cargo deny check
```

## CLI

CLI korzysta bezpośrednio z tego samego rdzenia Rust:

```sh
cargo run -p optimizer-cli -- optimize photo.jpg --profile balanced --json
cargo run -p optimizer-cli -- benchmark ./corpus --csv
```

Katalog `corpus/` opisuje układ lokalnego corpusu regresyjnego. Obrazy testowe nie
są commitowane; manifest powinien dokumentować ich pochodzenie i licencję.

## Struktura repozytorium

```text
crates/optimizer-core/   algorytmy JPEG i PNG, metryki oraz limity zasobów
crates/optimizer-wasm/   API łączące rdzeń Rust z Workerem przeglądarkowym
crates/optimizer-cli/    narzędzie CLI i benchmark corpusu
web/                     interfejs Vite, kolejka, Workery i pobieranie wyników
scripts/                 build WASM, development i kontrola licencji
```

Opis zachowania produktu znajduje się w [PRODUCT.md](PRODUCT.md), a zasady
interfejsu w [DESIGN.md](DESIGN.md).

## Najczęstsze problemy

### Brak `wasm32-unknown-unknown`

Jeśli Cargo zgłasza, że target nie jest zainstalowany:

```sh
rustup target add wasm32-unknown-unknown
npm run build:wasm
```

### Brak lub niezgodna wersja `wasm-bindgen`

```sh
cargo install wasm-bindgen-cli --version 0.2.128 --locked --force
npm run build:wasm
```

Skrypt obsługuje `wasm-pack` jako mechanizm zapasowy, jeśli jest zainstalowany,
ale standardowa ścieżka projektu używa `cargo` i `wasm-bindgen-cli`.

### UI działa, ale silnik zachowania formatu jest niedostępny

Najczęściej oznacza to uruchomienie samego `npm run dev:web` bez wcześniejszego
builda WASM:

```sh
npm run build:wasm
npm run dev:web
```

Po przebudowaniu odśwież stronę.

### Port 5173 jest zajęty

Vite wybierze kolejny dostępny port i wypisze właściwy adres w terminalu. Korzystaj
z adresu podanego po `Local:`.

### Windows blokuje kompilator Rust

Jeśli Windows Application Control blokuje `rustc.exe`, kompilację trzeba wykonać
w zatwierdzonym środowisku albo dopuścić oficjalny toolchain Rust. Sam frontend
można uruchomić przez `npm run dev:web`, jeśli artefakty WASM zostały wcześniej
zbudowane.

## Prywatność i licencje

Pliki obrazów są przekazywane bezpośrednio do lokalnego Workera. Service Worker
pobiera wyłącznie zasoby aplikacji; nie otrzymuje obrazów użytkownika.

Kod projektu jest dostępny na licencji MIT. Kontrola licencji dopuszcza MIT,
Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC i Zlib. Zależności GPL, AGPL,
komercyjne oraz `libimagequant` są blokowane.
