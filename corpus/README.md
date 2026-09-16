# Corpus regresyjny

Pliki corpusu nie są commitowane, żeby repozytorium nie przejmowało praw do cudzych
obrazów. Przed porównaniem wersji uzupełnij lokalnie katalogi:

```text
photos/ screenshots/ graphics/ text/ gradients/ noise/ alpha/
orientation/ icc-srgb/ icc-display-p3/ cmyk/ large/ malformed/ apng/
```

Każdy obraz powinien mieć wpis w `manifest.example.json` z pochodzeniem/licencją,
oczekiwanym formatem, wymiarami, orientacją i informacją o alpha/ICC. Wynik polecenia
`optimizer benchmark corpus --json` można zachować jako artefakt CI i porównać z
zaakceptowanym baseline'em poza repozytorium.

`public-corpus-2026-09-15.json` zapisuje źródła i licencje 24 plików użytych do
lokalnego porównania. Same pobrane binaria pozostają w katalogu tymczasowym,
poza Git.

## Fixture generowany w repozytorium

Do testów i benchmarków bez obciążeń licencyjnych użyj deterministycznego
generatora. Zapisuje on pliki wyłącznie do wskazanego katalogu (najlepiej
tymczasowego), więc żaden obraz binarny nie trafia do Git:

```sh
cargo run -p optimizer-cli -- fixtures <temporary-directory>/squeeze-fixtures
cargo run -p optimizer-cli -- benchmark <temporary-directory>/squeeze-fixtures --warmup 1 --runs 5 --json
```

W PowerShell użyj na przykład `$env:TEMP\squeeze-fixtures`; w powłoce Unix
użyj katalogu zwróconego przez `mktemp -d`.

Raport zawiera medianę, p95 oraz średni czas faz rdzenia. Metryki jakości są
liczone tylko przez CLI podczas benchmarku, nigdy w codziennej ścieżce Workera.
