# PNG z alpha: benchmark „mniej niż TinyPNG”, 2026-09-16

## Werdykt

**Nie zmieniam trybu jakości.** Na tych samych 12 publicznych PNG, dla których
lokalnie pobrano wynik TinyPNG, nie ma badanego pipeline'u, który jednocześnie:

1. jest mniejszy od TinyPNG dla `transparency.png`,
2. zachowuje kanał alpha piksel po pikselu oraz
3. nie ustępuje TinyPNG na białym i czarnym tle.

Da się uzyskać mniejszy plik. Cena to zmiana częściowo przezroczystych pikseli
i gorszy wynik wizualny na jasnym tle. To nie spełnia reguły dla trybu jakości.

## Metoda

- Wejścia i licencje: [`public-corpus-2026-09-15.json`](../corpus/public-corpus-2026-09-15.json).
- Referencja TinyPNG: ZIP z pierwszej partii opisany w
  [`tiny-comparison-2026-09-16.md`](tiny-comparison-2026-09-16.md).
- Ocena obrazu: SSIMULACRA2 i Butteraugli po złożeniu PNG z tłem białym oraz
  czarnym. Kanał alpha oceniono niezależnie.
- Czasy OxiPNG pochodzą z jego rzeczywistego binarium WASM uruchomionego w
  Node, po inicjalizacji. Są wskaźnikiem kosztu silnika, a nie pomiarem
  produkcyjnego interfejsu przeglądarki.

## Badane ścieżki

| Ścieżka | Status licencji i WASM | Wynik |
| --- | --- | --- |
| [UPNG](https://github.com/photopea/UPNG.js) 4D RGBA → [OxiPNG WASM](https://github.com/jamsinclair/jSquash) | UPNG (MIT) użyty wyłącznie jako referencyjna implementacja; OxiPNG uruchomiony jako WASM | Najbliższy obraz TinyPNG, ale dla półprzezroczystości nadal słabszy na białym tle. |
| [Quantizr](https://github.com/DarthSim/quantizr) RGBA → OxiPNG | Quantizr 1.4.3 (MIT) kompiluje się do `wasm32-unknown-unknown`; OxiPNG WASM | Najmniejszy wynik całej partii, ale widoczne banding i degradacja alpha na półprzezroczystym obrazie. |
| Kolor redukowany, alpha dokładnie zachowany → OxiPNG | Lokalnie, WASM OxiPNG | Alpha jest identyczny, lecz `transparency.png` ma 154 851 B przy 128 kolorach, wobec 48 295 B TinyPNG. |
| [zenquant](https://github.com/imazen/zenquant) | AGPL-3.0 albo licencja komercyjna | Odrzucony licencyjnie. |
| libimagequant / pngquant | GPL-3.0 | Odrzucone licencyjnie. |

## Wynik dla półprzezroczystego PNG

`transparency.png` ma 800×600 px i 323 179 całkowicie przezroczystych pikseli.
Każda ścieżka indeksowana zachowała je jako całkowicie przezroczyste; nie
zachowała jednak wszystkich wartości częściowej alpha.

| Wynik | Rozmiar | Względem TinyPNG | Zmienione piksele alpha | Średnia zmiana alpha | Ocena |
| --- | ---: | ---: | ---: | ---: | --- |
| Wejście | 224 566 B | +365,0% | 0 | 0,0000 | Oryginał. |
| TinyPNG | 48 295 B | — | 121 597 | 1,0568 | Punkt odniesienia. |
| UPNG, parametr palety 256 + OxiPNG WASM poziom 2 | 45 942 B | **-4,9%** | 123 928 | 1,0402 | Wizualnie blisko TinyPNG, ale gorsze metryki po białym tle. |
| Quantizr, 256 kolorów + OxiPNG WASM poziom 2 | 36 952 B | **-23,5%** | 131 251 | 1,1730 | Mniejszy, lecz widoczne pasma; odrzucony w jakości. |
| Quantizr z metryką złożenia na białym i czarnym tle + OxiPNG WASM poziom 2 | 38 789 B | **-19,7%** | — | — | Lepszy niż zwykły Quantizr, nadal słabszy wizualnie od TinyPNG na białym tle. |
| Ten sam wariant, 4 iteracje k-means | 38 056 B | **-21,2%** | — | — | Koszt 1 156 ms w Node, bez poprawy wystarczającej do bramki jakości. |
| Quantizr zoptymalizowany wyłącznie pod białe tło, 4 iteracje k-means | 37 360 B | **-22,6%** | — | — | Białe tło: SSIMULACRA2 69,02 / Butteraugli 7,16; czarne: 63,86 / 11,40. TinyPNG: 68,71 / 9,33 oraz 74,08 / 7,76. Odrzucony. |
| UPNG z alpha zachowanym dokładnie, 128 kolorów + OxiPNG WASM poziom 2 | 154 851 B | +220,6% | **0** | **0,0000** | Za duży. |

W szczególności ścieżka Quantizr na białym tle uzyskała SSIMULACRA2 58,86
wobec 68,71 dla TinyPNG oraz Butteraugli 12,25 wobec 9,33 (mniej znaczy
lepiej). To potwierdza kontrolę wizualną: rozmiar nie jest tu akceptowalnym
zastępnikiem jakości.

## Cała partia 12 PNG

| Pipeline | Łączny rozmiar | Różnica do TinyPNG | Kwalifikuje się do trybu jakości |
| --- | ---: | ---: | --- |
| Wejścia | 474 536 B | +222,6% | Nie dotyczy. |
| TinyPNG | 147 114 B | — | Referencja. |
| UPNG, parametr palety 256 + OxiPNG WASM poziom 2 | 162 305 B | +10,3% | Nie. |
| Quantizr, 256 kolorów + OxiPNG WASM poziom 2 | **146 492 B** | **-0,42%** | Nie: przewaga bajtów bierze się z agresywnej degradacji `transparency.png`. |

Quantizr uzyskał mniejszy wynik zbiorczy od TinyPNG o 622 B, ale nie jest to
uczciwa wygrana jakościowa. Nie wolno jej komunikować jako przewagi Squeeze.

## Szybkość OxiPNG WASM

Przy poziomie 2, po starcie WASM, optymalizacja obrazu 512×512 zajmowała
około 90–175 ms. Poziom 4 dawał zwykle niewielką dodatkową oszczędność,
ale kosztował około 590–1 130 ms. Dlatego ewentualny przyszły pipeline może
rozważać wyłącznie pojedynczy poziom 2; poziom 4 nie uzasadnia kosztu.

## Decyzja implementacyjna

Zostaje obecny commit jakościowy `fa2b47f`: nie zmienia on alpha i nie
podmienia obrazu wynikiem większym od wejścia. Nie dokładam drugiego wyboru
ani trybu eksperckiego.

Żeby zastąpić tę ścieżkę w jednej automatyce, kandydat musi przejść tę samą
bramkę: indywidualny wynik mniejszy od TinyPNG, zachowana całkowita
przezroczystość oraz jakość co najmniej TinyPNG na obu tłach. Obecne
implementacje tej bramki nie przechodzą. Najbliższy niedozwolony technologicznie
punkt odniesienia to libimagequant, ale jego licencja GPL-3.0 lub komercyjna
wyklucza go z tego projektu.
