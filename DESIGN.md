---
name: Squeeze
description: Spokojne, lokalne narzędzie do kompresji obrazów
colors:
  action-green: "#147453"
  action-green-dark: "#0d543c"
  action-green-soft: "#e4f1eb"
  ink: "#17211d"
  muted-ink: "#626c67"
  warm-paper: "#f3f1eb"
  surface: "#fffefa"
  hairline: "#d8dbd5"
  danger: "#a33f36"
  danger-soft: "#f8e9e6"
typography:
  display:
    fontFamily: "Avenir Next, Avenir, Segoe UI, sans-serif"
    fontSize: "clamp(38px, 5vw, 54px)"
    fontWeight: 700
    lineHeight: 1.02
    letterSpacing: "-0.035em"
  title:
    fontFamily: "Avenir Next, Avenir, Segoe UI, sans-serif"
    fontSize: "22px"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "-0.025em"
  body:
    fontFamily: "Avenir Next, Avenir, Segoe UI, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.58
  label:
    fontFamily: "Avenir Next, Avenir, Segoe UI, sans-serif"
    fontSize: "12px"
    fontWeight: 680
    lineHeight: 1.3
rounded:
  control: "10px"
  container: "14px"
  shell: "18px"
spacing:
  compact: "8px"
  control: "14px"
  section: "24px"
components:
  button-primary:
    backgroundColor: "{colors.action-green}"
    textColor: "{colors.surface}"
    rounded: "{rounded.control}"
    padding: "0 14px"
    height: "40px"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "0 14px"
    height: "40px"
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "0 34px 0 11px"
    height: "42px"
---

# Design System: Squeeze

## Overview

**Creative North Star: „Spokojny warsztat”**

Squeeze jest kompaktowym narzędziem operacyjnym. Interfejs skraca drogę od obrazu do pobranego wyniku, a jakość automatyki udowadnia konkretnymi rozmiarami przed i po oraz dostępnym porównaniem. Ciepła, jasna powierzchnia przypomina narzędzie używane codziennie; zieleń oznacza działanie, gotowość i oszczędność.

**Direction contract:** THESIS — zadanie ma prowadzić od dodania do pobrania bez zbędnych kroków. OWN-WORLD — ciepły papier, ciemny atrament, zielone działanie. STORY — dropzone kurczy się, kolejka przejmuje uwagę, podsumowanie zamyka paczkę. FIRST VIEWPORT — krótki tytuł, opis, dropzone i trzy ustawienia. FORM — `user-pinned: compact-operate-v1`; kierunek ustalił użytkownik, dlatego nie istnieje seed losowania. Quality bar stanowi produkcyjna klarowność, pełne stany, klawiatura i responsywność.

**Key Characteristics:**

- narzędzie widoczne w pierwszym widoku;
- stabilne wiersze kolejki zamiast wymienianych fragmentów DOM;
- rzeczowy polski język i mierzalne wyniki;
- dekoracja ograniczona do koloru, rytmu i jednego poziomu głębi.

## Colors

Paleta opiera się na jednym rzadko używanym akcencie oraz ciepłych neutralach.

### Primary

- **Zieleń działania:** główne przyciski, stan gotowości i dodatnia oszczędność.
- **Ciemna zieleń:** hover i tekst o lokalnym działaniu.
- **Miękka zieleń:** tło poprawnych komunikatów i znaczników wyniku.

### Neutral

- **Atrament:** tytuły, wynik i ciemne podsumowanie.
- **Ciepły papier:** tło strony.
- **Powierzchnia:** narzędzie, wiersze i kontrolki.
- **Wyciszony atrament:** opisy, metadane i działania drugorzędne.
- **Delikatna linia:** pola, podziały i wiersze bez dodatkowego cienia.

**The One Accent Rule.** Zieleń sygnalizuje działanie albo poprawny stan; nie służy do dekoracyjnego kolorowania całej strony.

## Typography

Display i tekst roboczy korzystają z jednej humanistycznej rodziny bez technicznego kostiumu. Hierarchię budują skala, ciężar i ograniczona szerokość tekstu.

- **Display:** krótki tytuł strony; na telefonie skala spada do 34–42 px.
- **Title:** nagłówki kolejki i sekcji wyniku.
- **Body:** pojedyncze zdania objaśniające o krótkiej mierze.
- **Label:** kontrolki, stany i dane plików.

**The Task Before Voice Rule.** Tytuł może nadać charakter, ale nie może wypchnąć dropzone poza pierwszy widok.

## Layout

Główna kolumna ma maksymalnie 1080 px i 40 px marginesu łącznie, zmniejszanego na telefonie do 24 px. Desktop łączy tytuł z krótkim opisem i układa trzy ustawienia w jednym rzędzie. Poniżej 760 px wszystkie ustawienia przechodzą do jednej kolumny, akcje wiersza zawijają się, a podsumowanie układa się pionowo. Dropzone po dodaniu pliku zmienia się z dużego pola wejściowego w kompaktowy pasek.

## Elevation & Depth

System jest płaski w środku narzędzia. Szeroki, miękki cień występuje tylko pod główną powłoką (`0 22px 60px rgba(34, 47, 41, .1)`), a dialog dostaje mocniejszą separację. Wiersze opierają się na pojedynczej linii, bez łączenia obramowania z dodatkowym cieniem.

**The One Lift Rule.** Podniesiona jest powłoka narzędzia lub aktywny dialog; zawartość wewnętrzna pozostaje spokojna.

## Shapes

Kontrolki używają promienia 10 px, wiersze i sekcje 14 px, a główna powłoka 18 px. Pełne pigułki są zarezerwowane dla przełącznika i małego znacznika procentowego. Ikony mają wspólny obrys SVG o zaokrąglonych zakończeniach.

## Components

### Buttons

Przyciski mają 40 px wysokości. Primary używa zielonego tła i białego tekstu; secondary korzysta z białej powierzchni i delikatnej linii; quiet nie ma stałego tła. Wszystkie warianty otrzymują widoczny focus i krótką reakcję active.

### Inputs

Select ma 42 px wysokości, białą powierzchnię, cienką neutralną linię i promień 10 px. Etykieta stoi nad polem. Przełącznik autostartu łączy nazwę, opis i widoczny stan, zachowując natywny checkbox dla dostępności.

### Queue row

Wiersz łączy miniaturę, nazwę, rozmiar przed/po, mierzony stan i kolejne działania. Ten sam element DOM trwa przez cały cykl zadania, więc fokus i otwarte szczegóły nie znikają. Na telefonie działania przechodzą pod dane obrazu.

### Comparison dialog

Dialog chroni fokus podczas porównania. Duży obszar obrazu ma szachownicę przezroczystości, suwak przed/po dostępny klawiaturą, zoom oraz skalę 1:1. Escape zamyka dialog i oddaje fokus przyciskowi, który go otworzył.

## Do's and Don'ts

### Do

- **Do** pokazywać rozmiar przed i po obok siebie.
- **Do** utrzymywać podstawowe ustawienia widoczne przed dodaniem pliku.
- **Do** stosować jeden poziom podsumowania dla całej paczki.
- **Do** pisać błędy jako problem i możliwy sposób naprawy.

### Don't

- **Don't** przywracać dużego marketingowego hero, które opóźnia dostęp do narzędzia.
- **Don't** używać zieleni do przypadkowej dekoracji.
- **Don't** zastępować ikon znakami Unicode ani emoji.
- **Don't** wymieniać całego wiersza podczas aktualizacji postępu.
