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

## Uruchomienie

Wymagania: Rust 1.90+, Node 20+, `wasm-pack`.

```powershell
cargo install wasm-pack
npm --prefix web install
npm run build:wasm
npm --prefix web run dev
```

CLI:

```powershell
cargo run -p optimizer-cli -- optimize photo.jpg --profile balanced --json
cargo run -p optimizer-cli -- benchmark .\corpus --csv
```

Pełna walidacja:

```powershell
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
