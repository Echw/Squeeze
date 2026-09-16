# PNG: lokalna stratna kompresja w przeglądarce — źródła pierwotne

Stan: 2026-09-16. Zakres: wynik pozostaje **PNG**, obrazy mogą mieć częściową
przezroczystość, bez GPL/AGPL ani płatnej licencji. To jest research z repozytoriów,
dokumentacji pakietów i specyfikacji, a nie wynik benchmarku Squeeze.

## Wniosek

Najlepszy pierwszy eksperyment to **Quantizr + obecny encoder indeksowanego PNG**.
Quantizr ma licencję MIT, przyjmuje RGBA, zwraca indeksy i paletę RGBA oraz ma regulowany
dithering. Jego `Cargo.toml` nie deklaruje zależności runtime ani natywnego toolchaina,
więc ma dużą szansę kompilować się do `wasm32-unknown-unknown`; upstream nie publikuje
jednak testu/obietnicy dla tego targetu, więc trzeba to potwierdzić krótkim spike'em.

**Pixo** jest drugim, pełniejszym eksperymentem: MIT, kompletna ścieżka RGBA →
indeksowany PNG z `PLTE`/`tRNS` i jawnie opisana kompilacja `wasm32-unknown-unknown`.
Nie brałbym go od razu jako zależności produktu: jego własne wyniki pokazują, że dla
jednego zdjęcia jest o 14% większy od pngquant, a kod kwantyzacji to median-cut plus
tylko dwie iteracje k-means. Ma sens jako mierzalny punkt porównania oraz źródło
sprawdzonej implementacji zapisu palety z alpha.

Nie ma darmowego, permissive odpowiednika libimagequant/pngquant o udowodnionej dziś
przewadze w jakości i rozmiarze. Techniczny lider odpada licencyjnie: libimagequant i
pngquant są GPLv3 albo wymagają licencji komercyjnej. Skompilowanie ich do WASM nie
zmienia tego faktu.

## Co oznacza „zachować transparentność”

PNG indeksowany może zawierać 1–256 kolorów `PLTE`; jego `tRNS` przechowuje osobne,
8-bitowe wartości alpha dla wpisów palety. To nadal jest zwykły, interoperacyjny PNG.
[PNG Third Edition](https://www.w3.org/TR/png-3/) definiuje ten model wprost.

W stratnej palecie alpha **nie musi pozostać pikselowo identyczna**: maksymalnie 256
kombinacji RGBA oznacza aproksymację również dla półprzezroczystych pikseli. Wymaganie
powinno zatem brzmieć: kanał alpha nie znika, wynik dekoduje się poprawnie, a artefakty
na krawędziach są pod kontrolą. Testy muszą obejmować gradient alpha i półprzezroczyste
obwódki na jasnym oraz ciemnym tle.

## Porównanie

| Opcja | Wynik i alpha | Przeglądarka / WASM | Jakość, rozmiar, szybkość | Licencja i decyzja |
| --- | --- | --- | --- | --- |
| **[Quantizr](https://github.com/DarthSim/quantizr)** | Bezpośrednio RGBA → paleta do 256 wpisów RGBA + indeksy; dither rozprowadza także błąd alpha. Encoder PNG trzeba zapewnić osobno. | Pure Rust, manifest bez zależności runtime. Kompilacja do `wasm32-unknown-unknown` jest bardzo prawdopodobna, ale nieudokumentowana upstream — sprawdzić. | Ma regulację 2–256 kolorów i poziomu ditheringu. Brak pierwotnego, porównywalnego benchmarku z pngquant; nie zakładać wyniku. | MIT w [LICENSE](https://github.com/DarthSim/quantizr/blob/master/LICENSE). **Najlepszy eksperyment integracyjny.** |
| **[Pixo](https://github.com/leerob/pixo)** | Kod obsługuje RGBA, tworzy paletę RGBA, zapisuje `PLTE` i `tRNS`; dither działa na RGB, przy zachowaniu alpha w palecie. Wynik to PNG. | Upstream podaje komendę dla `wasm32-unknown-unknown` i `wasm-bindgen`; API WASM przyjmuje RGBA, a tryb lossy redukuje do 256 kolorów. | Quantizer: median-cut + 2 iteracje k-means, opcjonalny Floyd–Steinberg. Autorski benchmark: na jednym zdjęciu +14% bajtów względem pngquant, na grafice płaskiej -29%; to nie jest niezależny werdykt. | MIT ([manifest](https://github.com/leerob/pixo/blob/main/Cargo.toml)). **Drugi benchmark, nie domyślna zależność bez własnych danych.** |
| **[UPNG.js](https://github.com/photopea/UPNG.js)** | `UPNG.encode()` przyjmuje 8-bitowe RGBA; `cnum=256` daje stratny PNG, a dokumentacja opisuje obsługę transparentności przez premultiplication przed kwantyzacją. | Lokalnie w przeglądarce, ale JavaScript, nie WASM. | K-d tree zamiast starszego k-means. Nie ma w repo powtarzalnego porównania z pngquant; mierzyć na tym samym korpusie. | MIT. **Dobry, mały kontrolny eksperyment JS; nie kandydat do wspólnej Rust/WASM ścieżki.** |
| **[OxiPNG](https://github.com/oxipng/oxipng)** | Nadal PNG, opcja `--alpha`; zachowuje piksele, więc alpha jest bezstratna. | Repo wymienia użycie przez Squoosh i jSquash; changelog odnotowuje wsparcie WASM. Trzeba osobno potwierdzić bieżący feature-set i rozmiar artefaktu. | To kompresja bezstratna: dobra jako fallback/rekompresja IDAT, nie zastąpi silnej redukcji kolorów. Wyższe poziomy są wolniejsze. | MIT. **Zostawić jako ewentualny fallback, nie rozwiązanie stratne.** |
| **[quantette](https://github.com/IanManske/quantette)** | Pipeline publiczny pracuje na `Srgb<u8>` (3 kanały), nie na RGBA. Sam nie rozwiązuje przezroczystości. | `no_std` po wyłączeniu `threads` i `image`; technicznie obiecujący dla WASM. | Oklab, szybki Wu oraz dokładniejszy k-means; repo publikuje przykłady DSSIM. | MIT OR Apache-2.0. **Tylko eksperyment dla całkiem nieprzezroczystych PNG; nie główna ścieżka alpha.** |
| **[libimagequant](https://github.com/ImageOptim/libimagequant) / [pngquant](https://github.com/kornelski/pngquant)** | Dokładnie odpowiednia technika: RGBA → indeksowany PNG z alpha. | libimagequant v4 jest w Ruście i zaleca wyłączyć `threads` dla WASM, więc technicznie jest wykonalny. | Punkt odniesienia dla jakości i małych plików; pngquant ma jakość, prędkość i dither jako parametry. | GPLv3+ albo licencja komercyjna. **Wykluczone.** |
| **Squoosh ImageQuant / [Pngyu](https://github.com/nukesaq88/Pngyu)** | Squoosh ma kodek ImageQuant; Pngyu jest GUI nad pngquant. | Squoosh działa lokalnie w przeglądarce, Pngyu nie jest ścieżką browser/WASM dla produktu. | To nadal ten sam silnik libimagequant/pngquant. | README kodeka Squoosh wskazuje libimagequant v2.12.1 i GPL3; BSD Pngyu nie zmienia licencji silnika. **Wykluczone.** |
| **[zenpng/zenquant](https://github.com/imazen/zenpng)** | Technicznie szerokie wsparcie PNG i alpha. | Pure Rust, ale to nie wystarcza. | Ambitna, świeża ścieżka, z deklarowanymi profilami jakości. | AGPL-3.0 albo komercyjna. **Wykluczone.** |
| JPEG XL | Kompresja do JXL dałaby wynik JXL, nie PNG. | Nie zmienia ograniczenia formatu wyjściowego. | Nie jest porównywalnym rozwiązaniem dla tego wymagania. | **Poza zakresem.** |

### Ważne źródła techniczne

- [Quantizr README](https://github.com/DarthSim/quantizr/blob/master/README.md): raw RGBA, paleta RGBA,
  2–256 kolorów i dithering; [manifest](https://github.com/DarthSim/quantizr/blob/master/Cargo.toml)
  nie ma zależności runtime.
- [Pixo WASM API](https://github.com/leerob/pixo/blob/main/src/wasm.rs) opisuje build
  `wasm32-unknown-unknown` i wejście RGBA; [encoder PNG](https://github.com/leerob/pixo/blob/main/src/png/mod.rs)
  pokazuje ścieżkę `PLTE`/`tRNS`, RGBA i algorytm quantization. Jego
  [benchmark](https://github.com/leerob/pixo/blob/main/benches/BENCHMARKS.md) jest użyteczny
  tylko jako deklaracja autora.
- [UPNG.js README](https://github.com/photopea/UPNG.js/blob/master/README.md) dokumentuje
  `UPNG.encode`, `cnum`, wejście RGBA, binarny wynik PNG oraz zalecenie premultiplication dla alpha.
- [OxiPNG README](https://github.com/oxipng/oxipng/blob/master/README.md) dokumentuje MIT,
  użycie jako biblioteka i obsługę alpha; [changelog](https://github.com/oxipng/oxipng/blob/master/CHANGELOG.md)
  odnotowuje WASM.
- [libimagequant README](https://github.com/ImageOptim/libimagequant/blob/main/README.md) i
  [pngquant README](https://github.com/kornelski/pngquant/blob/main/README.md) potwierdzają
  RGBA/palety oraz GPLv3 lub licencję komercyjną. [README kodeka Squoosh](https://github.com/GoogleChromeLabs/squoosh/blob/dev/codecs/imagequant/README.md)
  nazywa źródło i GPL3 wprost.

## Zalecane eksperymenty, w tej kolejności

1. **Quantizr spike** — wpiąć go tylko za obecną granicą „RGBA → paleta + indeksy”,
   z outputem indeksowanego PNG `PLTE`/`tRNS`. Zbudować `wasm32-unknown-unknown`, bez threads.
   Porównać 256 kolorów z ditheringiem `0`, `0.5` i `1.0`.
2. **Pixo jako niezależny punkt odniesienia** — ten sam korpus, `Force`, 256 kolorów,
   dither off/on. Nie przenosić od razu całej biblioteki; celem jest zmierzenie, czy
   jej median-cut + k-means daje realną przewagę nad Quantizr i bieżącą ścieżką.
3. **UPNG.js jako kontrola browser-only** — `cnum=256`, z właściwym premultiplication
   alpha; wynik pokaże, czy prosty JS jest dostatecznie dobry oraz ile kosztuje brak WASM.
4. Zachować **OxiPNG wyłącznie jako bezstratny fallback** dla plików, dla których żadna
   strata nie przechodzi jakości lub bezpieczeństwa alpha.

Dla każdego wyniku zapisać: rozmiar, czas kodowania w Workerze, rozmiar WASM/JS,
SSIMULACRA2 i Butteraugli względem wejścia, dekodowalność PNG oraz testy transparentnej
krawędzi na czarnym i białym tle. Obowiązkowe są też warunki: nie rosnąć względem wejścia,
zachować wymiary i odrzucić wynik z nieprawidłowym PNG. Decyzję o domyślnej ścieżce
podejmować na sumie danych z korpusu, nie na pojedynczym obrazie ani deklaracji autora.

