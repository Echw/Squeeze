# Squeeze

Squeeze zmniejsza obrazy JPEG i PNG lokalnie, w przeglądarce. JPEG pozostaje
JPEG, PNG pozostaje PNG. Nazwy i piksele obrazów nie opuszczają urządzenia.

## Działanie

- Jeden automatyczny przebieg na plik, bez profili i trybu eksperckiego.
- JPEG: Jpegli WASM. Jakość jest dobierana na kilku kafelkach obrazu tak, by
  najsłabszy z nich osiągnął ustalony próg SSIMULACRA2 (wyższy dla gładkich
  zdjęć); cały obraz jest kodowany raz, przy wybranej jakości. Zdjęcia
  zachowują 4:2:0, zrzuty ekranu i grafiki 4:4:4, skala szarości pozostaje
  jednokanałowa. Profil Display P3 jest zachowywany, sRGB usuwany jako zbędny,
  a szersze przestrzenie (Adobe RGB, ProPhoto) i CMYK przeglądarka konwertuje
  do sRGB. Równolegle powstaje bezstratnie przepisany JPEG (te same
  współczynniki, lepsze kodowanie); wygrywa, gdy wersja stratna nie jest
  wyraźnie mniejsza. Gdy przeglądarka nie udostępnia wymaganych prymitywów,
  działa zapasowa ścieżka MozJPEG.
- PNG: jedna paleta z portu libimagequant 2.4.1 (BSD) z ditheringiem
  ograniczonym do płaskich obszarów — także dla zdjęć, zrzutów ekranu i obrazów
  z przezroczystością. Obraz z najwyżej 256 kolorami RGBA zachowuje je
  dokładnie, chyba że mniejsza paleta osiąga cel jakości. Profile i metadane wyświetlania (ICC, sRGB, gAMA, cHRM,
  pHYs, eXIf) przechodzą do wyniku. OxiPNG w rdzeniu Rust wybiera głębię,
  filtry i kompresję; małe pliki dostają Zopfli. PNG 16-bit pozostaje
  bezstratny.
- Wynik większy od wejścia jest odrzucany. Aplikacja zwraca wtedy oryginał z
  komunikatem „Brak oszczędności w tym przebiegu”.
- Jeden Worker wykonuje kolejkę kolejno, ograniczając użycie pamięci.
- Porównanie przed i po oraz ZIP są tworzone dopiero po żądaniu użytkownika.

Obsługiwane są pliki JPEG i PNG do 100 MB i 24 MP. Animowane PNG oraz formaty
inne niż JPEG/PNG nie są przetwarzane.

## Uruchomienie

Wymagane są Node.js `20.19+` (lub `>=22.12`), Rust `1.90+`, target
`wasm32-unknown-unknown`, komponent `llvm-tools`, `wasm-bindgen-cli 0.2.128`
oraz Clang z celem WebAssembly (Xcode Command Line Tools na macOS, pakiet
`clang` na Linuksie) do biblioteki libdeflate używanej przez OxiPNG.

```sh
rustup target add wasm32-unknown-unknown
rustup component add llvm-tools
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

Benchmark liczy pełne metryki jakości poza codzienną ścieżką kompresji; w
aplikacji SSIMULACRA2 ocenia tylko kilka kafelków przy wyborze jakości JPEG.
Aktualne porównanie z Tiny i poprzednią wersją opisuje
[raport z 6 października](reports/compression-overhaul-2026-10-06.md).
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
pochodzenie artefaktu jest w [third_party/jpegli](third_party/jpegli). Kwantyzer
PNG jest portem libimagequant 2.4.1 na licencji BSD-2-Clause, ostatniej wersji
przed zmianą na GPL; noty są w [third_party/libimagequant](third_party/libimagequant).
OxiPNG ma licencję MIT, libdeflate MIT, a Zopfli Apache-2.0. Projekt nie używa
zależności GPL.
