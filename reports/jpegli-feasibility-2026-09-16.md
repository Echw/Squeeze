# Jpegli w przeglądarce: sprawdzenie kandydata, 2026-09-16

## Werdykt

Jpegli zastępuje MozJPEG dla JPEG w jednej automatyce. Na tych samych ośmiu
publicznych zdjęciach jego pojedynczy przebieg Q72, 4:2:0 dał **3 380 660
B**, czyli o **2,12% mniej** od TinyPNG (3 453 965 B). W sześciu przypadkach
metryki były lepsze lub bardzo bliskie TinyPNG; `road.jpg` wymaga Q85, aby
dorównać jakości.

Artefakt w aplikacji został zbudowany lokalnie z przypiętego commit-u Jpegli i
oficjalnego obrazu WASI SDK. Pochodzenie, suma SHA-256 i licencje znajdują się
w [`third_party/jpegli`](../third_party/jpegli/README.md). W Chromium pełny
Worker zmniejszył `road.jpg` z 348,9 kB do 88,4 kB (−74,7%) i zachował jego
profil ICC.

## Wyniki lokalne

| Plik | Jpegli Q72 | TinyPNG | Jpegli: SSIMULACRA2 / Butteraugli | TinyPNG: SSIMULACRA2 / Butteraugli |
| --- | ---: | ---: | ---: | ---: |
| `architecture.jpg` | 699 180 B | 633 015 B | 69,49 / 2,91 | 63,57 / 3,16 |
| `forest.jpg` | 585 235 B | 621 871 B | 69,65 / 2,94 | 70,57 / 2,96 |
| `hills.jpg` | 684 459 B | 628 088 B | 65,85 / 3,16 | 62,00 / 3,77 |
| `house.jpg` | 300 317 B | 333 943 B | 67,15 / 3,52 | 67,69 / 3,49 |
| `lake.jpg` | 371 013 B | 362 897 B | 69,06 / 3,42 | 66,29 / 3,81 |
| `mountain.jpg` | 513 002 B | 469 980 B | 68,87 / 3,01 | 63,41 / 4,40 |
| `road.jpg` | 50 349 B | 77 952 B | 73,26 / 2,42 | 78,84 / 2,17 |
| `valley.jpg` | 177 105 B | 326 219 B | 70,71 / 2,98 | 38,80 / 51,44 |
| **Razem** | **3 380 660 B** | **3 453 965 B** | — | — |

Jakość Q72 dla `road.jpg` nie przechodzi porównania wizualnego z TinyPNG.
Pojedyncza, wewnętrzna reguła dla niemal płaskiego obrazu (Q85, bez wyboru w
interfejsie) daje 87 457 B, SSIMULACRA2 78,02 i Butteraugli 2,15. Z ośmiu
wyników daje to 3 417 768 B, około 1,05% mniej od TinyPNG, z jakością bliższą
lub lepszą dla całego zestawu.

## Szybkość i kontrola w przeglądarce

Po rozgrzaniu modułu Q72 kodowanie w V8 trwało od 58 do 167 ms (mediana na
plik, bez dekodowania). W Chromium faktycznie uruchomiono WASM na obrazie
2400×3600: wynik JPEG miał poprawne 2400×3600, 688 065 B, a samo kodowanie
trwało 153,5 ms. To nie jest jeszcze pełny benchmark aplikacji ani wynik dla
Firefox.

Jpegli deklaruje jakość przy rozmiarze mniejszym niż tradycyjne implementacje
JPEG i szybkość porównywalną z MozJPEG w swoim ogłoszeniu
[Google Open Source](https://opensource.googleblog.com/2024/04/introducing-jpegli-new-jpeg-coding-library.html).

## Licencja i granice wyniku

Kod Jpegli i wrapper `gen2brain/jpegli` są BSD-3-Clause; połączone Highway ma
licencję Apache-2.0 lub BSD-3-Clause. Oficjalny projekt nie publikuje gotowego
artefaktu przeglądarkowego; jego zgłoszenie dotyczące WASM pozostaje otwarte:
[google/jpegli#13](https://github.com/google/jpegli/issues/13).

Weryfikacja Chromium obejmuje wynik 2400×3600, zachowanie wymiarów oraz pełny
Worker z ICC. Nadal trzeba wykonać pełny corpus i pomiar Firefox przed
deklarowaniem przewagi dla każdego JPEG.
