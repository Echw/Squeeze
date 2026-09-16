# Squeeze vs TinyPNG — aktualny corpus publiczny, 2026-09-16

## Metoda

Ten sam corpus 24 publicznych plików z
[manifestu](../corpus/public-corpus-2026-09-15.json) został przetworzony
przez aktualny produkcyjny Worker Squeeze (Rust/WASM + Jpegli/OxiPNG) oraz
stronę TinyPNG. W TinyPNG wyłączono automatyczną konwersję i pobrano dwa ZIP-y
(20 oraz 4 pliki), więc wszystkie liczby bajtów są odczytane z rzeczywistych
plików, a nie z etykiet strony.

Squeeze uruchomiono w Chromium lokalnie na `localhost:5173`; z końcowego ZIP-a
odczytano 24 wyniki. Jedna kolejka przetworzyła wszystkie pliki kolejno i
zakończyła je w mniej niż 10,2 s od momentu dodania plików. To czas całej
lokalnej partii, nie precyzyjny benchmark pojedynczego pliku. TinyPNG obejmuje
transfer i pracę usługi, dlatego jego czasu nie zestawiam z czasem WASM.

Wyniki metryk potwierdzają wymiary wszystkich 24 obrazów; osobna kontrola
odczytała wszystkie 16 PNG z ZIP-a i potwierdziła zachowanie ich wymiarów oraz
kanału alpha tam, gdzie występował.

SSIMULACRA2 i Butteraugli są liczone poza aplikacją tylko do benchmarku. Dla
PNG z alpha wynik jest liczony po złożeniu na białym (W) i czarnym (B) tle;
niższy Butteraugli jest lepszy. Metryki ułatwiają ocenę, ale nie zastępują
sprawdzenia wizualnego obrazu używanego w docelowym miejscu.

## Wynik zbiorczy

| Grupa | Pliki | Wejście | Squeeze | TinyPNG | Oszczędność Squeeze | Oszczędność TinyPNG | Squeeze względem TinyPNG |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| JPEG | 8 | 9 611 717 B | 3 179 036 B | 3 453 965 B | 66,9% | 64,1% | **7,96% mniej** |
| PNG z alpha | 14 | 3 551 471 B | 509 724 B | 691 956 B | 85,6% | 80,5% | **26,34% mniej** |
| PNG bez alpha | 2 | 158 649 B | 49 358 B | 53 845 B | 68,9% | 66,1% | **8,33% mniej** |
| **Razem** | **24** | **13 321 837 B** | **3 738 118 B** | **4 199 766 B** | **71,9%** | **68,5%** | **10,99% mniej** |

Squeeze jest mniejszy w 14 z 24 plików: `mountain.jpg`, `forest.jpg`, `lake.jpg`, `road.jpg`, `hills.jpg`, `valley.jpg`, `house.jpg`, `architecture.jpg`, `emoji-globe.png`, `transparency.png`, `screenshot.png`, `logo-commons.png`, `logo-bdl.png`, `gradient-large.png`.
TinyPNG pozostaje mniejszy w 10 plikach: `emoji-camera.png`, `emoji-chart.png`, `emoji-grin.png`, `emoji-laptop.png`, `emoji-package.png`, `emoji-palette.png`, `emoji-puzzle.png`, `emoji-rocket.png`, `emoji-target.png`, `text.png`. W tych
ostatnich Squeeze zachowuje zwykle wyższy wynik jakości, ale to nie oznacza, że
każdy kompromis jest lepszy dla każdego zastosowania.

## Każdy plik

W kolumnach jakości `S` oznacza Squeeze, `T` TinyPNG, a `W/B` tło białe/czarne.

| Plik | Wejście | Squeeze | TinyPNG | SSIMULACRA2 S / T | Butteraugli S / T |
| --- | ---: | ---: | ---: | ---: | ---: |
| `mountain.jpg` | 1 334 399 B | 457 978 B (65,7%) | 469 980 B (64,8%) | 66.72 / 63.41 | 3.42 / 4.40 |
| `forest.jpg` | 1 510 271 B | 602 793 B (60,1%) | 621 871 B (58,8%) | 70.53 / 70.57 | 3.12 / 2.96 |
| `lake.jpg` | 1 025 509 B | 340 311 B (66,8%) | 362 897 B (64,6%) | 68.09 / 66.29 | 3.52 / 3.81 |
| `road.jpg` | 357 239 B | 77 189 B (78,4%) | 77 952 B (78,2%) | 78.05 / 78.84 | 2.08 / 2.17 |
| `hills.jpg` | 1 796 053 B | 603 366 B (66,4%) | 628 088 B (65,0%) | 62.43 / 62.00 | 3.40 / 3.77 |
| `valley.jpg` | 649 468 B | 161 216 B (75,2%) | 326 219 B (49,8%) | 70.55 / 38.80 | 3.28 / 51.44 |
| `house.jpg` | 940 656 B | 307 846 B (67,3%) | 333 943 B (64,5%) | 68.10 / 67.69 | 3.23 / 3.49 |
| `architecture.jpg` | 1 998 122 B | 628 337 B (68,6%) | 633 015 B (68,3%) | 68.49 / 63.57 | 2.96 / 3.16 |
| `emoji-camera.png` | 16 714 B | 8438 B (49,5%) | 6669 B (60,1%) | S: W 97.74 / B 98.02<br>T: W 94.25 / B 93.78 | S: W 0.17 / B 0.17<br>T: W 2.07 / B 2.07 |
| `emoji-chart.png` | 3158 B | 1906 B (39,6%) | 1730 B (45,2%) | S: W 100.00 / B 100.00<br>T: W 95.36 / B 95.37 | S: W 0.00 / B 0.00<br>T: W 1.73 / B 1.73 |
| `emoji-globe.png` | 51 812 B | 19 063 B (63,2%) | 19 397 B (62,6%) | S: W 93.61 / B 93.68<br>T: W 91.95 / B 92.99 | S: W 1.09 / B 1.10<br>T: W 1.83 / B 1.82 |
| `emoji-grin.png` | 35 506 B | 17 815 B (49,8%) | 13 628 B (61,6%) | S: W 94.80 / B 95.14<br>T: W 92.58 / B 92.60 | S: W 0.37 / B 0.36<br>T: W 1.12 / B 1.39 |
| `emoji-laptop.png` | 19 840 B | 10 762 B (45,8%) | 6499 B (67,2%) | S: W 96.69 / B 97.42<br>T: W 91.67 / B 91.47 | S: W 0.17 / B 0.11<br>T: W 1.70 / B 1.46 |
| `emoji-package.png` | 10 907 B | 6006 B (44,9%) | 4871 B (55,3%) | S: W 100.00 / B 100.00<br>T: W 93.63 / B 94.02 | S: W 0.00 / B 0.00<br>T: W 1.98 / B 2.25 |
| `emoji-palette.png` | 23 408 B | 10 540 B (55,0%) | 9602 B (59,0%) | S: W 95.32 / B 96.10<br>T: W 94.41 / B 94.83 | S: W 0.95 / B 0.69<br>T: W 0.89 / B 0.89 |
| `emoji-puzzle.png` | 33 797 B | 13 893 B (58,9%) | 12 276 B (63,7%) | S: W 94.56 / B 95.28<br>T: W 93.42 / B 93.62 | S: W 0.79 / B 0.67<br>T: W 1.73 / B 1.40 |
| `emoji-rocket.png` | 26 729 B | 11 615 B (56,5%) | 10 551 B (60,5%) | S: W 94.70 / B 95.38<br>T: W 93.66 / B 94.20 | S: W 0.97 / B 1.10<br>T: W 1.39 / B 1.80 |
| `emoji-target.png` | 21 789 B | 10 391 B (52,3%) | 9326 B (57,2%) | S: W 96.83 / B 96.88<br>T: W 91.58 / B 92.39 | S: W 0.39 / B 0.39<br>T: W 1.87 / B 2.52 |
| `transparency.png` | 224 566 B | 38 791 B (82,7%) | 48 295 B (78,5%) | S: W 65.38 / B 73.22<br>T: W 68.71 / B 74.08 | S: W 9.34 / B 7.58<br>T: W 9.33 / B 7.76 |
| `text.png` | 6310 B | 6310 B (0,0%) | 4270 B (32,3%) | 100.00 / 95.10 | 0.00 / 2.15 |
| `screenshot.png` | 152 339 B | 43 048 B (71,7%) | 49 575 B (67,5%) | 80.49 / 89.61 | 3.25 / 2.63 |
| `logo-commons.png` | 81 527 B | 28 612 B (64,9%) | 29 580 B (63,7%) | S: W 92.97 / B 94.67<br>T: W 93.05 / B 94.37 | S: W 1.00 / B 1.43<br>T: W 1.04 / B 1.41 |
| `logo-bdl.png` | 141 119 B | 34 272 B (75,7%) | 35 375 B (74,9%) | S: W 91.68 / B 94.43<br>T: W 91.69 / B 94.30 | S: W 1.55 / B 1.74<br>T: W 1.20 / B 2.38 |
| `gradient-large.png` | 2 860 599 B | 297 620 B (89,6%) | 484 157 B (83,1%) | S: W 77.47 / B 77.72<br>T: W 82.71 / B 82.51 | S: W 4.92 / B 4.99<br>T: W 4.97 / B 5.63 |

## Co zmieniło wynik PNG

Automatyka nadal wykonuje tylko jeden przebieg. Dla PNG z co najmniej 25%
przezroczystego płótna rozszerzono bezpieczną paletę RGBA do 6 MP. Przed
kodowaniem wybierana jest jedna liczba kolorów: 20 dla bardzo płaskich logo,
256 w pozostałych przypadkach. Każda paleta przechodzi bramkę średniego błędu
po złożeniu na bieli i czerni; następnie pojedynczy OxiPNG poprawia wyłącznie
zapis PNG.

Najważniejsze przypadki:

- `gradient-large.png`: **297 620 B** wobec **484 157 B** TinyPNG. Squeeze
  jest o 38,5% mniejszy; SSIM jest niższy, ale Butteraugli jest nieco lepszy na
  bieli i wyraźnie lepszy na czerni. To mocna oszczędność, lecz nie podstawa do
  twierdzenia o ogólnej przewadze jakości.
- `logo-bdl.png`: **34 272 B** wobec **35 375 B**. SSIM na bieli jest
  praktycznie identyczny (91,68 / 91,69), a na czerni Squeeze jest wyższy
  (94,43 / 94,30).
- `logo-commons.png`: **28 612 B** wobec **29 580 B**; jakość jest praktycznie
  remisem na bieli i czerni.
- `emoji-globe.png`: **19 063 B** wobec **19 397 B**, przy lepszym SSIM i
  Butteraugli w Squeeze na obu tłach.

## Pozostałe braki względem TinyPNG

1. **Najmniejsze emoji i tekst.** TinyPNG nadal lepiej pakuje 10 małych plików,
   szczególnie `emoji-laptop.png`, `emoji-grin.png` i `text.png`. Squeeze
   kupuje tam wyższą wierność, ale nie wygrywa rozmiarem.
2. **Jakość w wybranych przypadkach.** Dla `gradient-large.png` TinyPNG ma
   wyższy SSIM, dla `screenshot.png` także ma wyższą jakość przy większym
   pliku. Tych różnic nie należy maskować sumą bajtów.
3. **Walidacja poza Chromium.** Aktualny przebieg został sprawdzony w
   produkcyjnym Workerze Chromium. Pozostają Firefox, pomiar cold/warm p50/p95,
   test offline Service Workera oraz ręczna kontrola nowych wyników w skali 1:1
   w rzeczywistych zastosowaniach.
