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

