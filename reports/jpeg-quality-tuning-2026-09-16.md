# JPEG: jeden dobór jakości po pomiarach, 2026-09-16

## Decyzja

Squeeze pozostawia jedno kodowanie Jpegli i próbkowanie 4:2:0. Nie ma wyboru
w interfejsie ani powtórnego kodowania. Analiza obrazu wybiera jedną jakość:

- Q83 dla niemal płaskich zdjęć, gdzie obniżenie do Q68 daje pasma;
- Q74 dla drobnych, mało zaszumionych szczegółów;
- Q73 dla gładkich powierzchni z wyraźnymi krawędziami;
- Q68 w pozostałych przypadkach.

To są stałe progi w
[`jpegli.ts`](../web/src/worker/jpegli.ts); test chroni ich kolejność. Każdy
plik przechodzi nadal przez jeden przebieg Jpegli WASM.

## Wynik ponownego strojenia

Do wcześniejszego korpusu ośmiu publicznych JPEG-ów dodano Q68 dla pięciu
przypadków, w których dawniej wybierane było Q70 lub Q72, oraz Q83 zamiast Q85
dla `road.jpg`. Kodowanie używa tego samego artefaktu `jpegli.wasm` co Worker;
metryki liczy lokalne CLI po dekodowaniu. Wynik z `createImageBitmap` w
Chromium sprawdzono również na całej paczce, a `road.jpg` dodatkowo po zmianie
Q84 → Q83.

| Plik | Jakość | Squeeze | TinyPNG | Squeeze: SSIMULACRA2 / Butteraugli | TinyPNG: SSIMULACRA2 / Butteraugli |
| --- | ---: | ---: | ---: | ---: | ---: |
| `architecture.jpg` | 68 | 625 175 B | 633 015 B | 68,49 / 2,96 | 63,57 / 3,16 |
| `forest.jpg` | 74 | 615 404 B | 621 871 B | 70,54 / 2,86 | 70,57 / 2,96 |
| `hills.jpg` | 68 | 600 204 B | 628 088 B | 62,43 / 3,40 | 62,00 / 3,77 |
| `house.jpg` | 73 | 308 334 B | 333 943 B | 67,70 / 3,23 | 67,69 / 3,49 |
| `lake.jpg` | 68 | 337 149 B | 362 897 B | 68,09 / 3,52 | 66,29 / 3,81 |
| `mountain.jpg` | 68 | 454 816 B | 469 980 B | 66,72 / 3,42 | 63,41 / 4,40 |
| `road.jpg` | 83 | 74 027 B | 77 952 B | 78,05 / 2,08 | 78,84 / 2,17 |
| `valley.jpg` | 68 | 160 258 B | 326 219 B | 70,55 / 3,28 | 38,80 / 51,44 |

Łącznie: **3 178 537 B**, wobec **3 453 965 B** dla TinyPNG, czyli 7,97%
mniej. To nie jest deklaracja wyniku dla dowolnego JPEG-a; dotyczy tego
kontrolowanego korpusu i pobranych wyników TinyPNG.

## Weryfikacja w przeglądarce

Aktualny Worker Chromium przetworzył wszystkie osiem plików bez błędów i bez
ostrzeżeń konsoli. Siedem niezmienionych reguł dało bezpośrednio wyniki poniżej
rozmiaru TinyPNG; po zmianie ostatniej reguły `road.jpg` miał w UI **75,4 KB**
wobec 77 952 B u TinyPNG. Wszystkie wyniki pozostały JPEG i zachowały wymiary.

Nadal potrzebny jest ten sam test w Firefoxie oraz szerszy korpus przed
deklarowaniem przewagi dla każdego rodzaju zdjęcia.
