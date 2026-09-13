# Corpus regresyjny

Pliki corpusu nie są commitowane, żeby repozytorium nie przejmowało praw do cudzych
obrazów. Przed zatwierdzeniem `profileSetVersion = 1` uzupełnij lokalnie katalogi:

```text
photos/ screenshots/ graphics/ text/ gradients/ noise/ alpha/
orientation/ icc-srgb/ icc-display-p3/ cmyk/ large/ malformed/ apng/
```

Każdy obraz powinien mieć wpis w `manifest.example.json` z pochodzeniem/licencją,
oczekiwanym formatem, wymiarami, orientacją i informacją o alpha/ICC. Wynik polecenia
`optimizer benchmark corpus --json` można zachować jako artefakt CI i porównać z
zaakceptowanym baseline'em poza repozytorium.

## Fixture generowany w repozytorium

Do testów i benchmarków bez obciążeń licencyjnych użyj deterministycznego
generatora. Zapisuje on pliki wyłącznie do wskazanego katalogu (najlepiej
tymczasowego), więc żaden obraz binarny nie trafia do Git:

```sh
cargo run -p optimizer-cli -- fixtures <temporary-directory>/squeeze-fixtures
cargo run -p optimizer-cli -- benchmark <temporary-directory>/squeeze-fixtures --profile maximum-compression --method auto --search-effort auto --warmup 1 --runs 5 --json
```

W PowerShell użyj na przykład `$env:TEMP\squeeze-fixtures`; w powłoce Unix
użyj katalogu zwróconego przez `mktemp -d`.

Raport zawiera medianę, p95 oraz średni czas faz rdzenia. Diagnostyczny Worker
WASM przyjmuje identyczne opcje i może zwrócić czasy faz na żądanie. Testy
przeglądarkowe w Chromium i Firefox są świadomie odłożone jako TODO, bez
dodawania obecnie Playwrighta.
