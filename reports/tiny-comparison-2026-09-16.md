# Squeeze vs TinyPNG — corpus publiczny, 2026-09-16

## Metoda

Ten sam corpus 24 publicznych plików z
[`public-corpus-2026-09-15.json`](../corpus/public-corpus-2026-09-15.json)
przetworzono w Squeeze oraz na stronie TinyPNG w przeglądarce. W TinyPNG
wyłączono automatyczną konwersję; użyto dwóch zwykłych partii (20 oraz 4 pliki),
bez konta i bez API.

Z pierwszej partii pobrano ZIP TinyPNG i zmierzono jego pliki względem
oryginałów przez SSIMULACRA2 i Butteraugli. Dla czterech PNG z drugiej partii
strona TinyPNG pokazała rozmiary, lecz automat przeglądarki nie udostępnił
drugiego ZIP do pomiaru pikselowego; te wyniki są oznaczone `około`.

Squeeze uruchomiono na produkcyjnym rdzeniu Rust/WASM z jednym przebiegiem.
Wyniki bajtowe odpowiadają przebiegowi lokalnego rdzenia; `road.jpg` oraz
`screenshot.png` dodatkowo potwierdzono w interfejsie przeglądarkowym.

## Wynik: pełne metryki dla 20 plików

| Grupa | Pliki | Wejście | Squeeze | TinyPNG | Oszczędność Squeeze | Oszczędność TinyPNG | SSIM Squeeze / TinyPNG | Butteraugli Squeeze / TinyPNG |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| JPEG | 8 | 9 611 717 B | 6 290 541 B | 3 453 965 B | 34,55% | 64,07% | 81,62 / 64,93 | 2,37 / 3,63 |
| PNG: emoji, tekst, alpha | 12 | 474 536 B | 474 536 B | 147 114 B | 0,00% | 69,00% | 100,00 / 88,59 | 0,00 / 4,32 |
| **Razem** | **20** | **10 086 253 B** | **6 765 077 B** | **3 601 079 B** | **32,93%** | **64,30%** | — | — |

Squeeze zwraca oryginał, gdy pojedynczy bezpieczny przebieg PNG nie daje
oszczędności. To celowa decyzja: 12 PNG zachowało identyczne piksele. TinyPNG
uzyskał znacznie mniejsze pliki przez stratną redukcję palety i alpha.

### Reprezentatywne przypadki

| Plik | Wejście | Squeeze | TinyPNG | Wniosek |
| --- | ---: | ---: | ---: | --- |
| `architecture.jpg` | 1 998 122 B | 1 372 670 B, SSIM 82,60 | 633 015 B, SSIM 63,57 | TinyPNG jest o 739 655 B mniejszy, Squeeze zachowuje więcej szczegółów. |
| `road.jpg` | 357 239 B | 136 356 B, SSIM 81,67 | 77 952 B, SSIM 78,84 | Najbliższy przypadek JPEG: TinyPNG jest mniejszy, Squeeze ma wyższą jakość. |
| `valley.jpg` | 649 468 B | 404 078 B, SSIM 83,80 | 326 219 B, SSIM 38,80 | TinyPNG zmniejsza plik mocniej kosztem dużej degradacji metrycznej. |
| `emoji-camera.png` | 16 714 B | 16 714 B, SSIM 100,00 | 6 669 B, SSIM 85,46 | TinyPNG wygrywa rozmiarem, Squeeze zachowuje piksele. |
| `transparency.png` | 224 566 B | 224 566 B, piksele bez zmian | 48 295 B | Oba pliki zachowują 800×600 i alpha, ale TinyPNG zmienił 382 894 z 480 000 pikseli oraz alpha w 121 597 pikselach. |

Kontrola wizualna w skali 1:1 potwierdziła metryki: TinyPNG zachowuje ogólny
kształt `road.jpg`, ale wygładza detale. Dla PNG z przezroczystością zmiana
kolorów i alpha jest widoczna w podglądzie.

## Druga partia TinyPNG: cztery PNG

| Plik | Squeeze | TinyPNG pokazane przez stronę |
| --- | ---: | ---: |
| `gradient-large.png` | 2 860 599 B, bez zmiany | około 484 KB |
| `screenshot.png` | 142 390 B | około 50 KB |
| `logo-commons.png` | 81 527 B, bez zmiany | około 30 KB |
| `logo-bdl.png` | 127 874 B | około 35 KB |

TinyPNG wygrywa rozmiarem również w tej partii. Nie przypisuję tym czterem
plikom metryk jakości, bo nie ma dla nich lokalnie pobranych wyników TinyPNG.

## Gdzie Squeeze jest już lepszy

1. **Prywatność i praca offline.** Squeeze nie wysyła obrazu poza urządzenie;
   TinyPNG wymaga uploadu.
2. **Priorytet jakości.** W ośmiu JPEG mediana SSIMULACRA2 Squeeze wynosi
   81,62 wobec 64,93 dla TinyPNG. W PNG Squeeze zachował piksele tam, gdzie
   agresywna redukcja TinyPNG zmienia kolory albo alpha.
3. **Przewidywalność.** Jeden wybór parametrów, brak konwersji formatu i zasada
   „wynik nie większy od wejścia” eliminują zwiększenie pliku i nieoczekiwaną
   utratę przezroczystości.

## Gdzie TinyPNG pozostaje lepszy

TinyPNG jest mniejszy dla każdego porównanego JPEG i PNG. Squeeze **nie jest
jeszcze lepszy pod względem samego współczynnika kompresji**. Różnica wynika z
polityki jakości: TinyPNG używa mocniejszej kompresji stratnej i kwantyzacji,
a aktualny Squeeze wybiera wyższą jakość JPEG oraz nie degraduje PNG z alpha.

Aby konkurować także rozmiarem przy podobnej jakości, potrzebny jest kolejny
benchmark silników JPEG i PNG z pierwotnego planu, nie pętla wariantów
uruchamiana u użytkownika.
