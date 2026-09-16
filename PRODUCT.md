# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Osoby, które chcą szybko zmniejszyć pojedyncze obrazy lub paczkę obrazów przed publikacją, bez wysyłania prywatnych plików do zewnętrznej usługi.

## Product Purpose

Squeeze lokalnie analizuje JPEG i PNG, dobiera bezpieczną strategię kompresji, pozwala porównać wynik i pobrać jeden plik albo całą paczkę. Sukces oznacza odczuwalnie mniejszy plik przy kontrolowanej jakości i krótkiej ścieżce od dodania do pobrania.

## Positioning

Silnik działa w przeglądarce i wykonuje jeden ustalony przebieg, dobrany po
lekkiej analizie obrazu. Metryki percepcyjne służą tylko benchmarkom i testom.
Obrazy, ich nazwy i piksele nie opuszczają urządzenia.

## Capabilities and Constraints

- Wejście: JPEG i PNG, maksymalnie 24 MP i 100 MB na plik.
- JPEG pozostaje JPEG, PNG pozostaje PNG; nie ma konwersji WebP ani opcji
  eksperckich. Migracja usuwa wcześniejsze ustawienia, które mogłyby je przywrócić.
- Rust/WASM pozostaje wspólnym silnikiem dla JPEG i PNG w jednym Workerze.
- Automatyka redukuje kwalifikujący się nieprzezroczysty PNG 8-bit do jednej
  palety 256 kolorów bez ditheringu; wynik większy od wejścia jest odrzucany.
- PNG z alpha, 16-bit, osadzonym zarządzaniem kolorem lub EXIF korzysta z
  bezpiecznej ścieżki lossless, gdy zachowuje format.
- AVIF i przetwarzanie serwerowe nie należą do obecnego zakresu.

## Brand Commitments

Nazwa Squeeze. Naturalny, rzeczowy język polski. Jasny interfejs z zielonym akcentem i bez zbędnej dekoracji.

## Evidence on Hand

Repozytorium zawiera działający silnik JPEG/PNG, lokalny Worker, porównanie
przed/po, kolejkę i ZIP. Corpus obrazów użytkownika nie jest częścią repozytorium.

## Product Principles

- Dodaj, skompresuj, porównaj i pobierz bez zbędnych kroków.
- Domyślna automatyka ma być dobra i nie wymaga ustawień.
- Wynik i ograniczenia muszą być opisane uczciwie.
- Prywatność jest właściwością architektury, nie obietnicą marketingową.
- Testy przeglądarkowe Chromium/Firefox są TODO; nie deklarujemy ich jako
  obecnego pokrycia dopóki nie zostaną wdrożone.
- CI działa obecnie na Ubuntu. TODO: dodać runner Windows do weryfikacji
  natywnego CLI oraz skryptów developerskich przed deklarowaniem ciągłego
  pokrycia tej platformy.
