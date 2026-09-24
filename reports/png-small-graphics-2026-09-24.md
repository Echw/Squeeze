# Małe PNG: poprawka i próby palety — 2026-09-24

## Wdrożona poprawka

`text.png` z [publicznego corpusu](../corpus/public-corpus-2026-09-15.json)
wcześniej wracał bez oszczędności. Jego paleta ma po kwantyzacji tylko 13
kolorów. Zapis indeksów w 4 bitach zamiast 8 sprawia, że wynik rdzenia jest
mniejszy od wejścia; jeden bezstratny etap OxiPNG WASM kończy plik.

| Wynik | Bajty | SSIMULACRA2 do oryginału | Butteraugli do oryginału |
| --- | ---: | ---: | ---: |
| Oryginał | 6 310 | 100,00 | 0,00 |
| Squeeze przed zmianą | 6 310 | 100,00 | 0,00 |
| Squeeze po zmianie | **4 081** | **96,11** | **0,68** |
| TinyPNG | 4 270 | 95,10 | 2,15 |

Nowy Squeeze jest o 189 B (4,4%) mniejszy od TinyPNG na tym pliku i ma lepsze
obie metryki. Oba wyniki pozostają PNG 600×400. Obraz sprawdzono też wizualnie
w skali 1:1. Produkcyjne moduły Rust/WASM i OxiPNG WASM uruchomione w Node
dały kolejno 6 108 B i 4 081 B; pięć rozgrzanych przebiegów obu etapów miało
medianę 87,4 ms i p95 102,1 ms. To nie jest pomiar przeglądarki. Dziesięć ikon
Noto Emoji zachowało identyczny wynik rdzenia jak przed zmianą.

## Próby na ikonach, bez zmiany automatyki

Sprawdzono pojedynczy zapis mniejszej palety RGBA i jeden OxiPNG na dziesięciu
publicznych ikonach. Na części plików istnieje przestrzeń na dalszy zysk:

| Plik i liczba kolorów | Kandydat | TinyPNG | Jakość wobec TinyPNG na bieli i czerni |
| --- | ---: | ---: | --- |
| `emoji-chart.png`, 24 | 1 705 B | 1 730 B | Lepszy SSIMULACRA2 i Butteraugli |
| `emoji-laptop.png`, 64 | 6 280 B | 6 499 B | Lepszy SSIMULACRA2 i Butteraugli |
| `emoji-target.png`, 96 | 9 079 B | 9 326 B | Lepszy SSIMULACRA2 i Butteraugli |

Tego wyboru nie włączono do aplikacji. Jeden próg liczby kolorów nie działa
dobrze na pozostałych ikonach: np. `emoji-grin.png` przy 64 kolorach jest
mniejszy od TinyPNG, ale ma gorsze obie metryki na obu tłach. Obecna bramka
średniego błędu przepuszcza także takie przypadki, więc przed zmianą domyślnej
palety trzeba opracować mocniejszą ocenę krawędzi i sprawdzić ją na nowych,
niewykorzystanych przy doborze progu obrazach. W każdym badanym kandydacie
całkowicie przezroczyste piksele pozostały całkowicie przezroczyste.

## Metoda i granice

- Wejścia Noto Emoji pobrano z przypiętej rewizji `43bac1a` i porównano ich
  rozmiary z [manifestem](../corpus/public-corpus-2026-09-15.json). Plik tekstowy
  ma dokładnie 6 310 B jak we wcześniejszym porównaniu. Wyniki TinyPNG pochodzą
  z pobranego wówczas ZIP-a tych samych publicznych plików.
- Rozmiary kandydatów obejmują jedną finalizację OxiPNG WASM, poziom 2.
  SSIMULACRA2 i Butteraugli policzono poza aplikacją; ikony z alpha oceniono
  po złożeniu na białym i czarnym tle.
- Strona lokalna otwiera się i pokazuje gotowy silnik. Automatyzacja wyboru
  pliku w przeglądarce nie udostępniła pliku do Workera, więc wynik 4 081 B
  potwierdzono na produkcyjnych modułach WASM w Node, nie przez pobrany plik
  z interfejsu. Pełny przebieg w Chromium i Firefox nadal pozostaje do zrobienia.
