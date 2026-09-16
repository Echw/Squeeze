# PNG z miękką przezroczystością: przebieg jakościowy

## Zakres

Ten przebieg dotyczy wyłącznie PNG z dużym przezroczystym tłem i istotną liczbą półprzezroczystych pikseli. Nie jest wyborem trybu przez użytkownika: Squeeze wybiera go automatycznie, a pozostałe PNG z alpha pozostają bezstratne albo trafiają do dokładnej palety RGBA.

Algorytm buduje jedną paletę 256 RGBA. Kolory są najpierw rozdzielane po osiach największej wariancji w przestrzeni premultiplied alpha, potem 16 razy dopracowywane względem błędu widocznego po złożeniu na czarnym i białym tle. W końcowym dopasowaniu RGB ma stałą, łagodną krzywą percepcyjną 1,15, która lepiej zachowuje detale widoczne na bieli. Rzeczywisty błąd po kompozycji na obu tłach nadal przechodzi osobną kontrolę. Po zakodowaniu jeden stały przebieg OxiPNG zmniejsza wyłącznie zapis PNG.

Bezpieczniki:

- maksymalnie 1 MP i 100 000 różnych kolorów wejściowych,
- co najmniej 25% pikseli całkowicie przezroczystych i 5% półprzezroczystych,
- średni błąd po kompozycji na obu tłach nie większy niż 1,25 na kanał,
- gdy paleta albo zapis nie daje oszczędności, zwracany jest poprzedni bezstratny wynik.

## Ten sam plik co TinyPNG

Plik: `transparency.png`, 800 × 600, 224 566 B. Wynik TinyPNG pobrano wcześniej dla tego samego pliku i zachowanego formatu PNG.

| Wynik | Bajty | Względem TinyPNG | SSIMULACRA2 białe / czarne | Butteraugli białe / czarne |
| --- | ---: | ---: | ---: | ---: |
| Squeeze: paleta alpha + OxiPNG | 38 739 | **-19,8%** | 65,38 / 73,22 | 9,34 / 7,58 |
| TinyPNG | 48 295 | — | 68,71 / 74,08 | 9,33 / 7,76 |

Średni bezwzględny błąd RGB po kompozycji wynosi dla Squeeze 1,08 na czarnym i 0,97 na białym tle; dla TinyPNG odpowiednio 1,15 i 0,98. Squeeze zachował 323 693 całkowicie przezroczyste piksele z 323 179 w oryginale; TinyPNG ma ich 323 914.

Squeeze wygrywa rozmiarem o 9 556 B. Na czarnym tle ma praktycznie ten sam wynik SSIMULACRA2, a według Butteraugli lepszy. Na białym tle Butteraugli jest praktycznie remisem (różnica 0,006), choć TinyPNG nadal ma wyższy SSIMULACRA2. Dlatego nie jest to ogólne twierdzenie o przewadze jakości nad TinyPNG: to mniejszy wynik, który na tym pliku ma porównywalny błąd na bieli oraz lepszy Butteraugli na czerni.

## Kontrola w przeglądarce

Produkcyjny Worker w Chromium dla tego pliku pokazał `219,3 KB → 37,9 KB`, `−82,7%`, komunikat o palecie dopasowanej do jasnego i ciemnego tła oraz brak błędów i ostrzeżeń w konsoli. Wynik zachowuje PNG, wymiary i kanał alpha; sprawdzenie końcowego przebiegu OxiPNG potwierdziło także `hasAlpha: yes`.
