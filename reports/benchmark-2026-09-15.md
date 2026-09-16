# Squeeze — benchmark 2026-09-15

## Zakres i corpus

W katalogu tymczasowym utworzono corpus 24 publicznych plików: 8 JPEG i 16
PNG. Obejmuje zdjęcia, screenshot z tekstem, grafikę, gradient i pliki z
przezroczystością. Lista źródeł oraz licencji jest w
[`corpus/public-corpus-2026-09-15.json`](../corpus/public-corpus-2026-09-15.json);
binaria nie trafiają do Git.

Pomiary uruchomiono natywnym CLI tego samego rdzenia Rust: jedna rozgrzewka,
pięć przebiegów, mediana i p95. Metryki SSIMULACRA2 oraz Butteraugli są liczone
poza pętlą produkcyjnej kompresji.

Pełny wiersz dla każdego pliku znajduje się w
[`public-corpus-2026-09-15.csv`](public-corpus-2026-09-15.csv). Puste kolumny
TinyPNG są celowe i opisane jako `not-run`, a nie jako wynik zero.

| Format | Liczba | Wejście | Wynik | Łączna oszczędność | Mediana median | Bez oszczędności |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| JPEG | 8 | 9 611 717 B | 6 290 541 B | 34,55% | 917,1 ms | 0 |
| PNG | 16 | 3 710 120 B | 3 686 926 B | 0,63% | 12,0 ms | 14 |

Stały JPEG Q88 utrzymał SSIMULACRA2 od 78,32 do 83,80 w ośmiu publicznych
zdjęciach, przy zachowaniu 34,55% łącznej oszczędności. Wyniki PNG są oczekiwane dla tego corpusu: większość transparentnych PNG Noto
Emoji oraz gradient CC0 były już mniejsze lub równe pojedynczemu bezstratnemu
zapisowi. Aplikacja zwróciła oryginał, zamiast zwiększyć plik albo zniszczyć
alpha. Dwa zyski to screenshot: 152 339 → 142 390 B (6,5%) i logo: 141 119 →
127 874 B (9,4%).

## Szybsza ścieżka PNG

Ten sam generowany fixture 1317×1030 PNG mierzył wcześniej medianę 1 945 ms:
pełne metryki i końcowa rekombinacja zajmowały ok. 1 894 ms. Po uproszczeniu
wynik to 836 849 → 432 181 B, mediana 192 ms, p95 193 ms. To ok. 10,1× krótszy
przebieg rdzenia. Nowy plik jest większy niż wcześniejszy wynik wielowariantowy
(432 181 B wobec 336 771 B), ponieważ aplikacja świadomie nie szuka już
wariantów ani nie wykonuje kosztownej końcowej optymalizacji.

## Kontrola formatu i pikseli

Sprawdzono lokalnie JPEG zdjęcia, PNG screenshotu, PNG z alpha i duży PNG z
gradientem. Każdy wynik zachował format oraz wymiary; oba PNG z alpha zachowały
alpha. Żaden z czterech wyników nie przekroczył rozmiaru wejścia.

Produkcjny build WASM uruchomił się w Chromium (stan interfejsu: „Gotowy”) bez
starych ustawień WebP lub trybu eksperckiego. Pełny upload automatycznym
file chooserem nie doszedł do skutku: warstwa automatyzacji przeglądarki utraciła
uchwyt elementu po otwarciu wyboru pliku. To ograniczenie bieżącego testu, nie
wynik aplikacji; testy kolejki, Workera, formatów i build zostały wykonane
lokalnie. Firefox i tryb offline wymagają osobnego przebiegu na środowisku z
działającym chooserem.

## TinyPNG

Porównanie wykonane później w przeglądarce jest w
[`tiny-comparison-2026-09-16.md`](tiny-comparison-2026-09-16.md). TinyPNG
wygrywa rozmiarem, a Squeeze jakością JPEG, zachowaniem PNG i pracą lokalną.
Raport nie deklaruje przewagi Squeeze w samym współczynniku kompresji.

## Wniosek produktowy

Przebieg jest zgodny z prostym modelem: jeden Worker, jeden wybór parametrów,
brak konwersji formatów i wynik nigdy większy od wejścia. Dane potwierdzają
wyraźne przyspieszenie problematycznej ścieżki PNG. Porównanie z TinyPNG
potwierdza przewagę Squeeze w jakości i lokalności, ale nie w rozmiarze pliku.
