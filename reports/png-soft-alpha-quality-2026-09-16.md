# PNG z miękką przezroczystością: przebieg jakościowy

## Zakres

Ten przebieg dotyczy wyłącznie PNG z dużym przezroczystym tłem i istotną liczbą półprzezroczystych pikseli. Nie jest wyborem trybu przez użytkownika: Squeeze wybiera go automatycznie, a pozostałe PNG z alpha pozostają bezstratne albo trafiają do dokładnej palety RGBA.

Algorytm buduje jedną paletę 256 RGBA. Kolory są najpierw rozdzielane po osiach największej wariancji w przestrzeni premultiplied alpha, potem 16 razy dopracowywane względem błędu widocznego po złożeniu na czarnym i białym tle. Po zakodowaniu jeden stały przebieg OxiPNG zmniejsza wyłącznie zapis PNG.

Bezpieczniki:

- maksymalnie 1 MP i 100 000 różnych kolorów wejściowych,
- co najmniej 25% pikseli całkowicie przezroczystych i 5% półprzezroczystych,
- średni błąd po kompozycji na obu tłach nie większy niż 1,25 na kanał,
- gdy paleta albo zapis nie daje oszczędności, zwracany jest poprzedni bezstratny wynik.

## Ten sam plik co TinyPNG

Plik: `transparency.png`, 800 × 600, 224 566 B. Wynik TinyPNG pobrano wcześniej dla tego samego pliku i zachowanego formatu PNG.

| Wynik | Bajty | Względem TinyPNG | SSIMULACRA2 białe / czarne | Butteraugli białe / czarne |
| --- | ---: | ---: | ---: | ---: |
| Squeeze: paleta alpha + OxiPNG | 39 465 | **-18,3%** | 63,79 / 73,19 | 10,62 / 7,68 |
| TinyPNG | 48 295 | — | 68,71 / 74,08 | 9,33 / 7,76 |

Średni bezwzględny błąd RGB po kompozycji wynosi dla Squeeze 1,05 na czarnym i 0,99 na białym tle; dla TinyPNG odpowiednio 1,15 i 0,98. Squeeze zachował 323 693 całkowicie przezroczyste piksele z 323 179 w oryginale; TinyPNG ma ich 323 914.

Squeeze wygrywa rozmiarem o 8 830 B. Na czarnym tle ma praktycznie ten sam wynik jakościowy, a według Butteraugli minimalnie lepszy. Na białym tle TinyPNG nadal ma wyższy wynik SSIMULACRA2 i niższy Butteraugli. Dlatego nie jest to ogólne twierdzenie o przewadze jakości nad TinyPNG: to bardziej kompaktowy wynik o wizualnie porównywalnej średniej różnicy, z pozostającą różnicą w metryce percepcyjnej na białym tle.

## Kontrola w przeglądarce

Produkcyjny Worker w Chromium dla tego pliku pokazał `219,3 KB → 38,5 KB`, `−82,4%`, komunikat o palecie dopasowanej do jasnego i ciemnego tła oraz brak błędów i ostrzeżeń w konsoli. Wynik zachowuje PNG, wymiary i kanał alpha.
