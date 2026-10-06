# Przebudowa silnika kompresji — Squeeze vs Tiny, 2026-10-06

## Wynik

Porównanie z Tiny obejmuje teraz 65 plików: 24 z pierwszego korpusu publicznego
i 41 z drugiego (31 JPEG i 10 PNG, wysłane do Tiny przez stronę 2026-10-06
w drugiej części prac). Rozmiar jest sumą bajtów względem Tiny, jakość średnią
różnicą SSIMULACRA2 (wyższa = lepiej Squeeze).

| Grupa | Rozmiar vs Tiny | Jakość vs Tiny | Pliki mniejsze i nie gorsze |
| --- | ---: | ---: | ---: |
| JPEG, korpus 1 (8), przed przebudową | −7,9% | +0,06 | — |
| JPEG, korpus 1 (8) | +11,6% | +4,40 | 2/8 |
| JPEG, korpus 2 (29, bez CMYK i pliku kodowanego bezstratnie) | −0,9% | +2,36 | 12/29 |
| JPEG, razem (38, bez CMYK) | +2,1% | +3,38 | 14/38 |
| PNG, korpus 1 (16), przed przebudową | −26,2% | +0,48 | 5/16 |
| PNG, korpus 1 (16) | −0,5% | +0,11 | 12/16 |
| PNG, korpus 2 (10) | −4,1% | +0,16 | 7/10 |

„Nie gorsze” oznacza wynik SSIMULACRA2 nie niższy niż u Tiny o więcej niż 0,5.
JPEG z CMYK pominięto w średnich, bo Tiny psuje w nim kolory (19,4 wobec 74,8).

Poprzednia przewaga rozmiarowa PNG brała się głównie z jednego pliku
(`gradient-large.png`, 297 KB z widocznymi pasmami) i z braku ditheringu.
Teraz Squeeze trzyma jakość Tiny i jest od niego minimalnie mniejszy.

**JPEG: Tiny dobiera jakość bardzo nierówno.** Jego wyniki mają SSIMULACRA2 od
61 do 85 (tablice IJG od Q63 do Q87), a ta wartość nie daje się przewidzieć ani
z aktywności obrazu, ani z wyniku kafelków (korelacja ≤ 0,25). Dla szczegółowych
zdjęć wybiera niską jakość (np. `hills.jpg` 62, `web-architecture-11.jpg` 61),
dla części pejzaży wysoką (`web-landscapes-21.jpg` 85). Stały cel jakości nie
może więc wygrać z nim na każdym pliku, a porównanie liczone przy tym samym
rozmiarze jest uczciwsze:

- przy rozmiarze Tiny krzywa Squeeze ma wyższy SSIMULACRA2 w 27 z 28 zdjęć
  (średnio +2,85, mediana oszczędności przy tej samej jakości 12%);
- przy jakości Tiny ten sam wynik daje pliki o około 12% mniejsze.

Cel kafelka wybrano z symulacji na 36 zdjęciach: przy 68 pliki są o 7% mniejsze
od Tiny, ale tylko równe jakością (średnio +0,4); przy 71 rozmiar jest równy, a
jakość +2,2; przy 72 pliki są o 3% większe, a jakość +2,8. Wybrano 71.
Pozostaje 8 zdjęć (głównie pejzaże z korpusu 2), na których Tiny daje wyższą
jakość kosztem pliku o 10–30% większego niż Squeeze; wyjątek to `road.jpg`.

Drugi, szerszy korpus (44 pliki) obejmuje przypadki, których pierwszy nie
zawierał:

| Przypadek | Przed | Teraz |
| --- | --- | --- |
| Zdjęcia zapisane jako PNG (4 × Kodak) | 0% oszczędności | −68…−74%, SSIMULACRA2 67–78 |
| Zrzut ekranu „z macOS” (Display P3, 144 dpi) | −3% (ścieżka bezstratna) | −68%, profil i dpi zachowane |
| Produkt wycięty z tła (alpha < 25%) | 0% | −73% |
| PNG 24 MP | 0% | −62% |
| Szczegółowe zdjęcia JPEG z sieci | SSIMULACRA2 57–63 | 66–80 (cel kafelka 71) |
| JPEG CMYK z profilem | błędne kolory (SSIMULACRA2 1,4) | poprawne kolory (52,3, kompresja stratna) |
| Dobrze skompresowany JPEG (Q86) | −13%, ponowne kodowanie stratne | −16%, bezstratnie |

Dane każdego pliku są w [CSV](compression-overhaul-2026-10-06.csv), a źródła
i licencje nowych plików w
[manifeście](../corpus/public-corpus-2026-10-06.json).

## Dlaczego Tiny wygrywał

**PNG.** Tiny to w praktyce pngquant. Liczby kolorów jego wyników pokrywają
się z kwantyzerem libimagequant przy celu jakości 97: `text.png` 19/19,
`emoji-grin.png` 101/99, `emoji-globe.png` 189/184. Squeeze miał dwa własne
kwantyzery bez ditheringu i wąskie warunki wejścia:

- bez ditheringu gradienty dostawały pasma (`gradient-large.png`);
- antyaliasing tekstu tracił odcienie (`screenshot.png`: 80,4 wobec 89,4 w Tiny);
- PNG z profilem ICC (każdy zrzut z macOS), zdjęcia, szum i obrazy z alpha
  zajmującą mniej niż 25% płótna szły ścieżką bezstratną, prawie bez
  oszczędności.

**JPEG.** Stała heurystyka dawała Q68 prawie każdemu zdjęciu:

- szczegółowe zdjęcia z sieci kończyły z SSIMULACRA2 57–63, z rozmytą teksturą;
- gładkie zdjęcia, na których artefakty widać najbardziej, nie dostawały
  wyższej jakości;
- Tiny dobiera jakość każdemu obrazowi osobno: Q61–76 według tablic DQT, a dla
  gładkich `road` i `valley` także 4:4:4;
- Squeeze interpretował profile ProPhoto i Adobe RGB jak sRGB;
- profil CMYK doklejał do wyniku RGB, co psuło kolory;
- każdy plik dostawał ponownie profil sRGB (+3 KB).

## Co zmieniono

### PNG

- **Kwantyzer.** Port libimagequant 2.4.1 do Rusta. To ostatnia wersja
  pngquant na licencji BSD; późniejsze są GPL. Port daje te same wyniki co
  referencja w C (np. jakość 75 i 57 na testach syntetycznych).
- **Poprawka algorytmu.** 2.4.1 przy wyszukiwaniu najbliższego koloru pomijał
  część palety i dla półprzezroczystych pikseli zwracał kolor do 2× dalszy niż
  najbliższy. Wyszukiwanie jest teraz dokładne: −0,6% bajtów i +0,26
  SSIMULACRA2 na korpusie.
- **Dithering i paleta.** Dithering działa tylko w płaskich obszarach, według
  mapy krawędzi i szumu. Cel jakości to 97 w skali pngquant, minimum 40.
- **Dokładna paleta.** Obraz z najwyżej 256 kolorami zostaje zapisany
  dokładnie, chyba że mniejsza paleta osiąga cel jakości. Na `text.png` daje
  to 4,2 KB zamiast 6,3 KB.
- **OxiPNG w rdzeniu Rust** zamiast `@jsquash/oxipng` w Workerze. CLI i
  Worker dają te same pliki. Brute-force filtrów obejmuje obrazy do 1 MP oraz
  zrzuty UI z małą liczbą kolorów do 8 MP. Zopfli działa do 0,3 MP i daje
  2–5% (do 20% na mikroikonach).
- **Metadane wyświetlania** przechodzą do wyniku: cICP, iCCP, sRGB, gAMA,
  cHRM, pHYs i eXIf. Profil sRGB w iCCP zamienia się na 1-bajtowy chunk sRGB.
  Tekst i znaczniki czasu są usuwane.

### JPEG

- **Pętla jakości na kafelkach.** Squeeze wybiera 6 kafelków 256×256 na
  siatce MCU, rozłożonych od najgładszego do najbardziej szczegółowego.
  Kafelki są kodowane w Jpegli i oceniane SSIMULACRA2 (`fast-ssim2`,
  referencja liczona raz, SIMD128). Wyszukiwanie znajduje najniższe Q, przy
  którym najsłabszy kafelek ma co najmniej 71, a na gładkim obrazie 77.
  Gładki obraz to taki, którego 75. percentyl aktywności kafelków jest
  poniżej 6. Cały obraz jest kodowany raz.
- **Zgodność z pełną metryką.** Minimum z kafelków koreluje z pełnym
  SSIMULACRA2 na poziomie 0,977. Średni błąd wybranego Q względem idealnego
  wynosi 1,4.
- **Zakres jakości Q68–92.** Q68 było dotychczasową jakością domyślną, więc
  żadne zdjęcie nie jest kompresowane mocniej niż wcześniej.
- **4:4:4 tylko dla zrzutów ekranu i grafik.** Na zdjęciach 4:4:4 nie
  poprawiało wyniku przy celu jakości, a kosztowało średnio 6–9% więcej.
- **Profile ICC.**
  - sRGB (sprawdzane po prymariach i krzywej) jest usuwany.
  - Display P3 zostaje.
  - Adobe RGB, ProPhoto i CMYK przeglądarka konwertuje do sRGB.
  - Dla `valley.jpg` (ProPhoto) konwersja sama obniża rozmiar przy jakości
    Tiny o ok. 26%.
- **Skala szarości** jest kodowana jako jeden kanał.
- **Bezstratna transkodacja** (nowa funkcja `transcode` we wrapperze Jpegli):
  współczynniki zostają, Huffman jest optymalizowany, skany progresywne.
  Wygrywa, gdy wynik stratny nie jest o co najmniej 10% mniejszy. Na JPEG-ach
  z sieci i z aparatów sama transkodacja daje 4–17%; we wszystkich 44
  sprawdzonych plikach piksele po dekodowaniu były identyczne.
- **Odporność.** Po błędzie Jpegli Worker ładuje nową instancję modułu.
  Ścieżka zapasowa Rust zwraca CMYK bez zmian.

### Budowanie

- **Jpegli WASM** budowany przez `third_party/jpegli/build.sh` w przypiętym
  kontenerze WASI SDK. Na tych samych wejściach JPEG-i są bajtowo identyczne
  z poprzednim artefaktem.
- **libdeflate dla OxiPNG** wymaga Clanga z celem WebAssembly i `llvm-ar` z
  komponentu rustupa `llvm-tools`. `scripts/build-wasm.mjs` wskazuje go
  automatycznie, a CI instaluje ten komponent.
- **SIMD128** dla `wasm32-unknown-unknown` w `.cargo/config.toml`. Jpegli i
  tak już go wymagał.

## Metoda

- **Przebiegi.** Oba silniki uruchomiono w Chromium 151 przez rzeczywisty
  Worker: stary z `main` na osobnym serwerze Vite, nowy z tej gałęzi.
  Pobrano wyniki i porównanie wyświetlanego obrazu wejścia i wyjścia.
- **Tiny.** Wyniki Tiny to pobrane wcześniej pliki dla tych samych 24
  publicznych wejść.
- **Metryki.** SSIMULACRA2 i Butteraugli (p3) z libjxl 0.12 liczono poza
  aplikacją, ze świadomością profili ICC. Dla PNG z alpha bierzemy gorszy wynik
  z tła białego i czarnego; wyższy SSIMULACRA2 i niższy Butteraugli oznaczają
  lepszą jakość.
- **Kalibracja progów JPEG.** Użyto produkcyjnego wyboru kafelków i scorera
  WASM na 36 zdjęciach z dwóch źródeł:
  - 8 zdjęć Unsplash Q95;
  - 28 zdjęć Wikimedia Commons: miniatury Q87, oryginały z aparatów
    Q97–98 i warianty Q75.

## Czas

| | Przed | Teraz |
| --- | ---: | ---: |
| JPEG, mediana / maksimum | 123 / 615 ms | 377 / 1563 ms |
| PNG, mediana / maksimum (bez 24 MP) | 213 / 3687 ms | 954 / 1937 ms |
| PNG 24 MP | 7,7 s (bez oszczędności) | 15,4 s (−62%) |

Pomiar na tym komputerze, w Chromium, z czasem dekodowania i weryfikacji.
Szczytowa pamięć rdzenia dla PNG 24 MP wynosi 389 MB (natywnie), w budżecie
768 MB.

## Ograniczenia

- **Zdjęcia w PNG z mocno nasyconymi barwami.** 2.4.1 jest słabszy od
  nowoczesnego (GPL) libimagequant: `kodim23` ma 66,6 wobec 74,6 w pngquant
  4. Tiny osiąga tu 67,9 (146 KB wobec 144 KB u nas), więc jego wynik jest
  zbliżony do naszego, a luka dotyczy dopiero nowszego pngquanta. Dodatkowe
  iteracje k-means, inny dithering ani wolniejsze ustawienia tej luki nie
  zamykają. Rozdział palety wymaga osobnej pracy.
- **Pejzaże, na których Tiny wybiera wysoką jakość.** Osiem JPEG-ów
  (`web-landscapes-20…23`, `web-architecture-12`, `-15`, `road`, `web-people-00`)
  ma niższy SSIMULACRA2 niż u Tiny o 1,2–13,9, ale pliki są o 10–31% mniejsze.
  Wyniki kafelków ani aktywność obrazu nie przewidują, kiedy Tiny wybierze Q80+
  (`web-landscapes-21.jpg` ma wszystkie kafelki na ok. 70 przy Q70, a Tiny daje
  tam 85). Wyższy cel dla wszystkich zdjęć przenosi koszt na pozostałe pliki:
  przy 72 rozmiar rośnie o 3%, a lista przegranych skraca się o 1–2 pliki.
- **`road.jpg`.** Tiny ma 78,8 wobec 77,5 przy pliku o 16% większym. Cel 79 dla
  gładkich obrazów dawał 91 KB (+44%) za +2,5 punktu, więc został przy 77.
- **Wyłączenie adaptacyjnej kwantyzacji Jpegli** daje o 4% mniejsze pliki przy
  tym samym SSIMULACRA2, ale o 7% większe przy tym samym Butteraugli. Metryki
  się nie zgadzają, więc zmiany nie wdrożono.
- **Porównywalność z Tiny.** 65 plików, w tym 41 z drugiego korpusu. Progi JPEG
  były kalibrowane na tych samych zdjęciach, na których je oceniono; nowy
  korpus nie jest więc niezależnym zbiorem testowym.
- **Przeglądarki.** Test w Chromium objął CMYK, szarość, P3 i sRGB. Firefox
  i Safari nie zostały jeszcze sprawdzone.

## Licencje

| Komponent | Licencja |
| --- | --- |
| Port libimagequant 2.4.1 | BSD-2-Clause i nota Poskanzera ([third_party/libimagequant](../third_party/libimagequant)) |
| OxiPNG | MIT |
| libdeflate | MIT |
| Zopfli | Apache-2.0 |
| fast-ssim2 | BSD-2-Clause |
| archmage, magetypes | MIT lub Apache-2.0 |
| yuvxyb | MIT |
| Jpegli i wrapper | BSD-3-Clause |

`cargo deny check` przechodzi.
