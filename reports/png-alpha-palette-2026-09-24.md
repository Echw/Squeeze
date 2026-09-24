# Mniejsza paleta RGBA dla części ikon — 2026-09-24

Po pierwszym porównaniu małych PNG wprowadzono w automatyce ostrożny wybór 96
kolorów dla prostych, ale nie całkiem płaskich grafik z przezroczystością.
Decyzja zapada przed jednym przebiegiem kwantyzacji: `graphic`, szacowana liczba
kolorów do 300 i entropia co najmniej 2,3. Pozostałe obrazy zachowują dotychczasowy
limit 256 kolorów albo wcześniejszą bezstratną ścieżkę. Gotową paletę odrzucamy,
jeżeli zmienia alfę pikseli całkowicie przezroczystych, średni błąd złożenia na
czerni i bieli przekracza 1,25 lub więcej niż 1% widocznych pikseli ma średni
błąd RGB przekraczający 2 na którymkolwiek tle. Wtedy zostaje dotychczasowa
bezstratna ścieżka PNG.

## Wyniki produkcyjnego Rust/WASM i OxiPNG WASM

| Publiczny PNG | Wcześniej Squeeze | Teraz Squeeze | TinyPNG | Zmiana względem wcześniejszego Squeeze |
| --- | ---: | ---: | ---: | ---: |
| `emoji-camera.png` | 8 438 B | 7 075 B | 6 669 B | −16,2% |
| `emoji-laptop.png` | 10 762 B | 7 221 B | 6 499 B | −32,9% |
| `emoji-target.png` | 10 391 B | **9 079 B** | 9 326 B | −12,6% |

Na `emoji-target.png` Squeeze jest o 247 B (2,6%) mniejszy od TinyPNG. Na
pozostałych dwóch ikonach nadal przegrywa o 406 B i 722 B. Wszystkie trzy
wyniki są PNG 512×512 i mają przejrzystość. Wyniki Squeeze pochodzą z aktualnego
modułu produkcyjnego WASM i jednego OxiPNG WASM, uruchomionych w Node. Nie są
to czasy ani pliki pobrane przez interfejs przeglądarki.

| PNG | Squeeze SSIMULACRA2 biało/czarno | Tiny SSIMULACRA2 biało/czarno | Squeeze Butteraugli biało/czarno | Tiny Butteraugli biało/czarno |
| --- | --- | --- | --- | --- |
| Aparat | 95,32 / 95,61 | 94,25 / 93,78 | 1,20 / 1,18 | 2,07 / 2,07 |
| Laptop | 93,28 / 93,64 | 91,67 / 91,47 | 1,53 / 1,19 | 1,70 / 1,46 |
| Tarcza | 93,82 / 94,01 | 91,58 / 92,39 | 1,50 / 1,39 | 1,87 / 2,52 |

Niższy Butteraugli i wyższy SSIMULACRA2 oznaczają mniejsze odstępstwo od
oryginału. Oceniono obrazy na jasnym i ciemnym tle, a wynik laptopa i tarczy
obejrzano w skali 1:1. Bajty dekodowane z produkcyjnego WASM są identyczne z
obrazami użytymi do pomiaru metryk. Nie oznacza to przewagi na innych ikonach.

## Sprawdzenie poza próbką doboru

Pobrano 16 innych ikon Noto Emoji z tej samej przypiętej rewizji
`43bac1a1272f31cedf0d74c2089fba6c7f952276` (Apache-2.0). Jedna z nich,
żarówka `emoji_u1f4a1.png`, też ma szacowane mniej niż 300 kolorów, ale jej
entropia wynosi 1,82. Paleta 96 kolorów dawałaby 11 422 B przed OxiPNG i
SSIMULACRA2 92,72 na bieli. Nowa reguła zostawia jej paletę 256 kolorów;
wynik końcowy pozostaje 8 995 B, identyczny z poprzednim. Pozostałe 15 ikon
także zachowało dotychczasową ścieżkę.

Na łącznym zbiorze 26 ikon produkcyjny WASM zachował format, wymiary i całkowitą
przezroczystość każdego pierwotnie przezroczystego piksela; żaden wynik nie był
większy od wejścia. `cargo fmt`, Clippy, testy Rust i web, build Vite oraz
kontrola licencji npm przeszły. TinyPNG nie był mierzony dla 16 dodatkowych
ikon, więc nie wyciągamy z nich wniosku o przewadze rozmiaru lub jakości nad
Tiny. Nadal potrzebny jest pełny przebieg pobierania plików w Chromium i
Firefox.

Wersja optymalizatora wzrosła do 6, a cache aplikacji do `squeeze-static-v13`,
aby przeglądarka po aktualizacji pobrała nowy moduł WASM zamiast starego pliku
pod tym samym adresem.
