# JPEG: jeden dobór jakości po aktualnym pomiarze, 2026-09-16

## Reguła

Squeeze koduje każdy JPEG dokładnie raz przez Jpegli WASM, przy 4:2:0 i bez
opcji w interfejsie. Analiza obrazu wybiera jedną stałą jakość: Q83 dla prawie
płaskiego zdjęcia, Q74 dla drobnych mało zaszumionych detali, Q73 dla gładkich
powierzchni z wyraźnymi krawędziami i Q68 w pozostałych przypadkach.

## Aktualny wynik Worker → TinyPNG

Wyniki Squeeze pochodzą z pełnej paczki uruchomionej w produkcyjnym Workerze
Chromium; TinyPNG to pobrany wynik tego samego oryginału i zachowanego formatu.

| Plik | Jakość | Squeeze | TinyPNG | SSIMULACRA2 S / T | Butteraugli S / T |
| --- | ---: | ---: | ---: | ---: | ---: |
| `architecture.jpg` | 68 | 628 337 B | 633 015 B | 68,49 / 63,57 | 2,96 / 3,16 |
| `forest.jpg` | 74 | 602 793 B | 621 871 B | 70,53 / 70,57 | 3,12 / 2,96 |
| `hills.jpg` | 68 | 603 366 B | 628 088 B | 62,43 / 62,00 | 3,40 / 3,77 |
| `house.jpg` | 73 | 307 846 B | 333 943 B | 68,10 / 67,69 | 3,23 / 3,49 |
| `lake.jpg` | 68 | 340 311 B | 362 897 B | 68,09 / 66,29 | 3,52 / 3,81 |
| `mountain.jpg` | 68 | 457 978 B | 469 980 B | 66,72 / 63,41 | 3,42 / 4,40 |
| `road.jpg` | 83 | 77 189 B | 77 952 B | 78,05 / 78,84 | 2,08 / 2,17 |
| `valley.jpg` | 68 | 161 216 B | 326 219 B | 70,55 / 38,80 | 3,28 / 51,44 |
| **Razem** | — | **3 179 036 B** | **3 453 965 B** | — | — |

Dla tego kontrolowanego korpusu Squeeze jest o **7,96% mniejszy** od TinyPNG.
W `forest.jpg` TinyPNG ma minimalnie lepsze metryki, a w `road.jpg` wyższy
SSIM, lecz niższy Butteraugli ma Squeeze. To nie jest deklaracja przewagi dla
każdego JPEG-a; potrzeba jeszcze pomiarów w Firefoxie i szerszej próby zdjęć.
