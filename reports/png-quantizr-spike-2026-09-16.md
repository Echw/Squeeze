# PNG z alpha: benchmark kandydatow WASM, 2026-09-16

> Status: historyczny spike badawczy. Quantizr **nie jest** włączony do
> produktu: złożone półprzezroczyste obrazy nie przeszły bramki jakości wobec
> TinyPNG. Aktualny tryb jakości używa wyłącznie dokładnej palety RGBA dla
> prostych grafik oraz bezstratnego OxiPNG po już wybranej palecie.

## Werdykt

**Quantizr + indeksowany PNG + OxiPNG WASM poziom 2** jest obecnie najlepsza
dopuszczalna technicznie sciezka dla prostych, przezroczystych grafik. Jest
czysto lokalna, wynik nadal jest PNG i Quantizr potwierdzono dla
`wasm32-unknown-unknown`.

Nie jest jeszcze uniwersalnym rozwiazaniem dla PNG z alpha. Zlozony obraz z
polprzezroczystym gradientem nadal wymaga bezstratnej sciezki, bo wszystkie
badane palety byly wyraznie gorsze od TinyPNG na bialym lub czarnym tle.

## Ustawienia i material

- Wspolny material: 10 przezroczystych ikon 512x512 z
  [`public-corpus-2026-09-15.json`](../corpus/public-corpus-2026-09-15.json),
  przy tym samym lokalnie pobranym wyniku TinyPNG.
- Quantizr 1.4.3: 256 wpisow RGBA, dithering `0`.
- Zapis: indeksowany PNG z `PLTE` i `tRNS`.
- Domkniecie pliku: [OxiPNG](https://github.com/oxipng/oxipng) WASM, poziom 2,
  bez zmiany pikseli.
- Jakosc: SSIMULACRA2 i Butteraugli po zlozeniu na bialym oraz czarnym tle.
- Transparentne piksele o alpha `0` nie zostaly utracone w zadnej z 10 ikon.

## Wynik dla ikon

| Zbior 10 ikon | Wejscie | Quantizr + OxiPNG | TinyPNG |
| --- | ---: | ---: | ---: |
| Bajty | 243 660 B | 103 052 B | 94 549 B |
| Oszczednosc od wejscia | - | 57,71% | 61,20% |
| Roznica do TinyPNG | - | +8,99% | - |

Squeeze jest juz mniejszy od TinyPNG w dwoch obrazach i zachowuje porownywalna
albo lepsza jakosc w obu kompozycjach:

| Plik | Quantizr + OxiPNG | TinyPNG |
| --- | ---: | ---: |
| `emoji-globe.png` | **16 861 B** | 19 397 B |
| `emoji-puzzle.png` | **12 064 B** | 12 276 B |

W pozostalym zbiorze Quantizr jest zwykle o kilka do kilkudziesieciu procent
wiekszy od TinyPNG, ale nadal zmniejsza oryginal. Nie przedstawiam tego jako
wygranej sumarycznej: TinyPNG jest dla tych 10 ikon o 8,99% mniejszy.

## Kontrola jakosci i szybkosci

- Dla ikon bez zlozonych polprzezroczystych gradientow kandydat byl lepszy w
  obu metrykach albo bardzo blisko TinyPNG. Przypadki graniczne to globus,
  paleta i puzzle; roznice pozostaja niewielkie, wiec przed domyslna integracja
  trzeba rozszerzyc corpus o niezalezne ikony, logotypy i interfejsy.
- Sam Quantizr wraz z zapisem mierzony natywnie zajmowal 6-19 ms na ikonie
  512x512. OxiPNG WASM poziom 2 po inicjalizacji zajmowal 90-147 ms na tych
  plikach. Poziom 4 potrzebowal okolo 0,6-0,9 s i nie daje wartosci dla
  podstawowego, szybkiego trybu.
- `cargo build --release --target wasm32-unknown-unknown` dla spike'a Quantizr
  zakonczyl sie powodzeniem. To potwierdza kompatybilnosc silnika, ale nie jest
  jeszcze pomiarem pelnego Workera przegladarki.

## Dlaczego nie wlaczac go dla kazdego PNG z alpha

`transparency.png` (800x600, duzo polprzezroczystych pikseli) daje 36 952 B
po Quantizr + OxiPNG, mniej niz 48 295 B TinyPNG. Jakosc jest jednak wyraznie
slabsza:

| Tlo | SSIMULACRA2 Quantizr / TinyPNG | Butteraugli Quantizr / TinyPNG |
| --- | ---: | ---: |
| Biale | 58,86 / 68,71 | 12,25 / 9,33 |
| Czarne | 65,64 / 74,08 | 10,17 / 7,76 |

Pixo rowniez uzyskal maly plik, 41 037 B, lecz pogorszyl obraz jeszcze mocniej
i zmienil 7 824 pierwotnie calkowicie przezroczyste piksele. Odpada.

## Kontrprzyklad: duzy gradient z alpha

W drugim, niezaleznym obrazie `gradient-large.png` (2 860 599 B) lokalny
Quantizr + OxiPNG osiagnal **196 536 B**. TinyPNG po ponownym przeslaniu tego
samego publicznego pliku pokazal **484 KB**. To bylby wynik Squeeze o okolo 59%
mniejszy od TinyPNG, ale nie jest wygrana produktu: po kompozycji na bialym tle
kandydat uzyskal SSIMULACRA2 69,76 i Butteraugli 8,11, a na czarnym 69,99 i
7,79. Sa widoczne pasma w gradiencie.

Ten przypadek uzasadnia konserwatywna bramke jakosci. Sam niski rozmiar nie
wystarcza, nawet gdy przewaga nad TinyPNG jest duza. Wynik TinyPNG byl dostepny
z interfejsu uslugi jako zaokraglony rozmiar; pobrany plik nie zostal udostepniony
automatyzacji, wiec nie porownujemy tu metryk TinyPNG.

## Decyzja do wdrozenia

Nastepny etap powinien byc waski i jeden na plik:

1. Przepuszczac przez Quantizr tylko przezroczyste grafiki o niskiej zlozonosci
   i duzym obszarze plaskim, po uprzednim rozszerzeniu corpus i zamrozeniu
   progow.
2. Po jednorazowym zapisie uruchamiac OxiPNG WASM poziom 2 jako czesc tego
   samego pipeline'u, nie jako wybor wariantow.
3. Dla zlozonych obrazow z alpha, 16-bit PNG i plikow z profilem kolorow
   zachowac obecny bezstratny przebieg.
4. Zachowac regule niepowiekszania pliku oraz walidacje wymiarow, PNG i alpha.

Pierwotne zrodla licencji, mozliwosci i odrzuconych silnikow sa w
[`png-approaches-primary-research-2026-09-16.md`](png-approaches-primary-research-2026-09-16.md).
