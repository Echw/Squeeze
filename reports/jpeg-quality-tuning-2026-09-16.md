# JPEG: dobór jakości po pomiarach, 2026-09-16

## Decyzja

Squeeze pozostawia jedno kodowanie Jpegli i 4:2:0. Nie ma wyboru w interfejsie
ani powtórnego kodowania. Analiza obrazu wybiera jakość od Q70 do Q85:

- Q85 dla niemal płaskich zdjęć, aby nie wprowadzić pasm;
- Q74 dla drobnych, mało zaszumionych szczegółów;
- Q73 dla gładkich powierzchni z wyraźnymi krawędziami;
- Q72 dla dużych gładkich obszarów;
- Q70 w pozostałych przypadkach.

Reguły są w [`web/src/worker/jpegli.ts`](../web/src/worker/jpegli.ts), a ich
granice chroni test jednostkowy. To nadal jeden przebieg dla każdego pliku.

## Badanie wariantów poza aplikacją

Użyto tych samych ośmiu publicznych JPEG-ów i pobranych wyników TinyPNG z
raportu [`jpegli-feasibility-2026-09-16.md`](jpegli-feasibility-2026-09-16.md).
Pomiary kodują ten sam artefakt `web/public/wasm/jpegli.wasm`; metryki liczy
lokalne CLI po dekodowaniu obrazu.

| Wariant | Bajty | Różnica do TinyPNG | Wniosek |
| --- | ---: | ---: | --- |
| Q70, 4:2:0 | 3 234 995 | −6,34% | Najmniejszy, lecz za słaby dla `forest`, `house` i `road`. |
| Q70, 4:2:2 | 3 627 007 | +5,01% | Większy od TinyPNG. |
| Q68, 4:4:4 | 3 714 782 | +7,55% | Większy i wolniejszy od TinyPNG. |
| Poprzedni dobór Q72/Q85, 4:2:0 | 3 417 768 | −1,05% | Punkt wyjścia. |
| **Nowy automatyczny dobór, 4:2:0** | **3 376 059** | **−2,26%** | Wybrany wariant. |

Rząd „nowy” obejmuje surowe wyniki kodera, przed dołączeniem profilu ICC.
Aplikacja zachowuje ICC, więc plik z takim profilem może być nieco większy.

Sprawdzone 4:2:2 nie poprawiło też metryk na `architecture`, `forest`,
`house`, `lake` ani `road` w stopniu uzasadniającym wzrost rozmiaru. 4:2:0
pozostaje jedynym próbkowaniem w produkcie.

## Wynik wybranej automatyki

| Plik | Jakość | Squeeze | TinyPNG | Squeeze: SSIMULACRA2 / Butteraugli | TinyPNG: SSIMULACRA2 / Butteraugli |
| --- | ---: | ---: | ---: | ---: | ---: |
| `architecture.jpg` | 72 | 699 180 B | 633 015 B | 69,49 / 2,91 | 63,57 / 3,16 |
| `forest.jpg` | 74 | 615 404 B | 621 871 B | 70,54 / 2,86 | 70,57 / 2,96 |
| `hills.jpg` | 70 | 650 239 B | 628 088 B | 64,52 / 3,36 | 62,00 / 3,77 |
| `house.jpg` | 73 | 308 334 B | 333 943 B | 67,70 / 3,23 | 67,69 / 3,49 |
| `lake.jpg` | 70 | 355 582 B | 362 897 B | 68,26 / 3,31 | 66,29 / 3,81 |
| `mountain.jpg` | 70 | 490 639 B | 469 980 B | 67,66 / 3,18 | 63,41 / 4,40 |
| `road.jpg` | 85 | 87 457 B | 77 952 B | 78,02 / 2,15 | 78,84 / 2,17 |
| `valley.jpg` | 70 | 169 224 B | 326 219 B | 70,15 / 2,89 | 38,80 / 51,44 |

Squeeze nie wygrywa rozmiarem w każdym pojedynczym JPEG-u. Wybrana automatyka
jest mniejsza łącznie, a w przypadkach większych utrzymuje wyraźnie lepszą lub
zbliżoną jakość. `road.jpg` dostaje Q85, ponieważ niższa jakość była widocznie
gorsza od TinyPNG.

## Weryfikacja aplikacji

W Chromium uruchomiono aktualny Worker na całej paczce ośmiu JPEG-ów. Wszystkie
zadania zakończyły się poprawnie, bez ostrzeżeń konsoli; aplikacja pokazała
**65,1%** łącznej oszczędności względem wejścia. Każdy wynik pozostał JPEG-em.
Ten test sprawdza ścieżkę `createImageBitmap → analiza → Jpegli WASM →
weryfikacja dekodowania`, a nie tylko kodowanie z Node.

Nadal potrzebny jest taki sam test w Firefoxie oraz szerszy korpus JPEG-ów,
zanim można deklarować wynik dla każdego rodzaju obrazu.
