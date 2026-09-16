# Squeeze

Squeeze zmniejsza obrazy JPEG i PNG lokalnie, w przeglądarce. JPEG pozostaje
JPEG, PNG pozostaje PNG. Nazwy i piksele obrazów nie opuszczają urządzenia.

## Działanie

- Jeden automatyczny przebieg na plik, bez profili i trybu eksperckiego.
- JPEG: Jpegli WASM z jednym doborem jakości po lekkiej analizie obrazu;
  zachowuje JPEG, wymiary, widoczny obrót i profil ICC. Gdy przeglądarka nie
  udostępnia wymaganych prymitywów, wraca do bezpiecznej ścieżki MozJPEG.
- PNG: jedna paleta 256 kolorów dla bezpiecznych, nieprzezroczystych grafik;
  pozostałe PNG otrzymują jedną bezstratną optymalizację z zachowaniem alpha,
  16-bitów i wrażliwych informacji o kolorze.
- Wynik większy od wejścia jest odrzucany. Aplikacja zwraca wtedy oryginał z
  komunikatem „Brak oszczędności w tym przebiegu”.
- Jeden Worker wykonuje kolejkę kolejno, ograniczając użycie pamięci.
- Porównanie przed i po oraz ZIP są tworzone dopiero po żądaniu użytkownika.

Obsługiwane są pliki JPEG i PNG do 100 MB i 24 MP. Animowane PNG oraz formaty
inne niż JPEG/PNG nie są przetwarzane.

## Uruchomienie

Wymagane są Node.js `20.19+` (lub `>=22.12`), Rust `1.90+`, target
`wasm32-unknown-unknown` i `wasm-bindgen-cli 0.2.128`.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.128 --locked
npm ci
npm run dev
```

Aplikacja będzie dostępna zwykle pod <http://localhost:5173/>.

## Kontrole

```sh
npm run build:wasm
npm run check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
```

`npm run build` tworzy produkcyjną aplikację w `web/dist/`. Service Worker
przechowuje tylko zasoby aplikacji z tego samego źródła; nie pobiera plików
użytkownika.

## Benchmark

CLI używa tego samego rdzenia co Worker. Tworzy też deterministyczne, legalne
fixture’y poza repozytorium:

```sh
cargo run -p optimizer-cli -- fixtures /tmp/squeeze-fixtures
cargo run -p optimizer-cli -- benchmark /tmp/squeeze-fixtures --warmup 1 --runs 5 --json
cargo run -p optimizer-cli -- optimize photo.jpg --json
```

Benchmark liczy metryki jakości wyłącznie poza codzienną ścieżką kompresji.
Format lokalnego corpusu i wymagania dla źródeł publicznych opisuje
[corpus/README.md](corpus/README.md).

## Struktura

```text
crates/optimizer-core/   kompresja JPEG/PNG, ograniczenia i narzędzia benchmarku
crates/optimizer-wasm/   API Rust/WASM dla Workera
crates/optimizer-cli/    CLI, fixture’y i benchmarki
web/                     interfejs, kolejka, Workery i pobieranie wyników
```

## Licencja

Kod Squeeze jest dostępny na licencji [MIT](LICENSE). Jpegli i jego wrapper
mają licencję BSD-3-Clause, a połączone Highway Apache-2.0 lub BSD-3-Clause;
pochodzenie artefaktu jest w [third_party/jpegli](third_party/jpegli). Projekt
nie używa libimagequant ani zależności GPL.
