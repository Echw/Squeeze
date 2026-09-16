# PNG z alpha — aktualna automatyka, 2026-09-16

## Reguła

Squeeze zachowuje PNG i alpha. Nie tworzy wariantów w interfejsie. Gdy obraz
ma co najmniej 25% całkowicie przezroczystego tła, nie więcej niż 6 MP i paleta
mieści się w ograniczonej analizie, tworzy jedną paletę RGBA dopasowaną do
kompozycji na bieli i czerni. Paleta przechodzi limit średniego błędu 1,25 na
kanał; gdy go nie spełnia albo wynik rośnie, wraca bezstratny wynik.

Dla bardzo płaskiego logo wybierane jest 20 wpisów palety, dla pozostałych
256. To decyzja analizy przed kodowaniem, nie przeszukiwanie kilku plików.
OxiPNG jest potem jednym bezstratnym etapem zapisu.

## Wyniki z produkcyjnego Workera

| Plik | Squeeze | TinyPNG | SSIM W/B: Squeeze / TinyPNG | Butteraugli W/B: Squeeze / TinyPNG |
| --- | ---: | ---: | ---: | ---: |
| `gradient-large.png` | 297 620 B | 484 157 B | S: W 77.47 / B 77.72<br>T: W 82.71 / B 82.51 | S: W 4.92 / B 4.99<br>T: W 4.97 / B 5.63 |
| `logo-bdl.png` | 34 272 B | 35 375 B | S: W 91.68 / B 94.43<br>T: W 91.69 / B 94.30 | S: W 1.55 / B 1.74<br>T: W 1.20 / B 2.38 |
| `logo-commons.png` | 28 612 B | 29 580 B | S: W 92.97 / B 94.67<br>T: W 93.05 / B 94.37 | S: W 1.00 / B 1.43<br>T: W 1.04 / B 1.41 |
| `transparency.png` | 38 791 B | 48 295 B | S: W 65.38 / B 73.22<br>T: W 68.71 / B 74.08 | S: W 9.34 / B 7.58<br>T: W 9.33 / B 7.76 |
| `emoji-globe.png` | 19 063 B | 19 397 B | S: W 93.61 / B 93.68<br>T: W 91.95 / B 92.99 | S: W 1.09 / B 1.10<br>T: W 1.83 / B 1.82 |

Wszystkie powyższe wyniki pobrano z ZIP-a produkcyjnego Workera. Kanał alpha,
format PNG i wymiary zostały zachowane. `gradient-large.png` pokazuje
największy zysk w bajtach, ale ma niższy SSIM niż TinyPNG; nie traktujemy go
jako ogólnej przewagi jakościowej.
