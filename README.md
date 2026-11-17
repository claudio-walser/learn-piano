# Learn Piano

Klavier lernen mit MIDI-Keyboard, MusicXML-Lektionen und Noten-Wasserfall.
Geschrieben in Rust, läuft auf OpenGL 2.1 / GLES 2.0 (auch sehr alte Hardware und ARM).

## Bauen

Benötigt Rust und die ALSA-Header (`alsa-lib-devel` auf Fedora, `libasound2-dev` auf Debian/Ubuntu).
Auf Fedora Silverblue in einer Toolbox bauen:

```sh
toolbox run -c fedora-toolbox-43 cargo build --release
```

Das Binary `target/release/learn-piano` braucht zur Laufzeit nur `libasound.so.2`
und läuft direkt auf dem Host.

## Starten

```sh
./target/release/learn-piano                 # sucht die Bibliothek (siehe unten)
./target/release/learn-piano --library DIR   # Bibliothek in DIR
./target/release/learn-piano --fullscreen
./target/release/learn-piano --x11          # X11/XWayland erzwingen (Standard: Wayland, falls die Sitzung Wayland ist)
./target/release/learn-piano --debug        # FPS und Eingaben anzeigen, Log in ~/.local/share/learn-piano/debug.log
./target/release/learn-piano --check-library [DIR]   # fehlende/überzählige Übersetzungen der Bibliothek
```

Die Bibliothek ist der Ordner mit der `compositions.xml`. Gesucht wird in dieser Reihenfolge:
`--library DIR`, `$LEARN_PIANO_LIBRARY`, der in den Einstellungen gewählte Ordner, dann der
aktuelle Ordner und der Ordner des Binaries (samt übergeordneten Ordnern). Jeweils zählt der
Ordner selbst oder sein Unterordner `compositions/`, deshalb funktioniert auch der Projektordner.

In den Einstellungen lässt sich der Ordner unter „Ordner mit den Stücken“ wählen
(„Automatisch“ setzt die Wahl zurück). Findet die App keine Bibliothek, bietet sie die
Ordnerauswahl direkt an.

In einer Wayland-Sitzung läuft die App nativ unter Wayland, sonst unter X11.
Unter XWayland funktioniert Touch nicht (es kommt nie ein Loslassen an); mit `--x11`
bzw. `--wayland` oder `LEARN_PIANO_BACKEND=x11|wayland` lässt sich die Wahl erzwingen.

Ein MIDI-Keyboard wird automatisch erkannt (auch wenn es später eingesteckt wird).
Ohne Keyboard kann man die Tasten auf dem Bildschirm antippen.

## Bibliothek

```
assets/sound-fonts/default.sf2  Klang
compositions/                   die Bibliothek
  compositions.xml            Liste der Stücke, Schwierigkeitsstufen und Kategorien (nur IDs, keine Texte)
  locales/<sprache>/compositions.ftl   Titel in dieser Sprache (en ist Pflicht)
  <stück>/
    thumbnail.png
    <schwierigkeit>/          z.B. prep, beginner, duet – muss in compositions.xml definiert sein
      01-part-A.musicxml      Lektionen (optional), in Namensreihenfolge
      02-part-B.musicxml
      full.musicxml           ganzes Stück (Pflicht)
  exercises/<übung>/          Übungen (Tonleitern, Akkorde, …), gleicher Aufbau wie ein Stück
```

`path` und `thumbnail src` in `compositions.xml` sind relativ zum Ordner der `compositions.xml`.
Der Klang `assets/sound-fonts/default.sf2` wird neben der Bibliothek, in ihrem übergeordneten Ordner,
im aktuellen Ordner oder beim Binary gesucht.
Notenzeile 1 ist die rechte Hand, Notenzeile 2 die linke. Fingersätze werden aus
`<fingering>` gelesen.

**Duette:** Eine Stufe mit `kind="duet"` in `compositions.xml` ist keine Schwierigkeit,
sondern ein Duett. Sie wird in der Auswahl abgesetzt angezeigt. Die MusicXML enthält zwei
`<part>`s: den ersten für den Schüler, den zweiten für den Lehrer.

**Kategorien:** Die Tags der Stücke (`<tag>christmas</tag>`) erscheinen als Kategorien über
der Liste, in der Reihenfolge des Blocks `<tags>` in `compositions.xml`.

**Übungen:** Das Dashboard hat links ein Menü mit „Songs“ und „Übungen“. Übungen stehen in
`compositions.xml` im Block `<exercises>` als `<exercise id="…" path="…"/>` und sind genau wie
Stücke aufgebaut (Schwierigkeits-Ordner, Lektionen, `full.musicxml`). Ihr Titel ist
`exercise-<id>` in `compositions.ftl`. Sie erscheinen in der Reihenfolge der Datei, nicht in
Suche und Filtern der Songs. Ohne `thumbnail` zeigt die Karte eine kleine Klaviatur mit den
Tasten der Übung (rechte Hand blau, linke orange).

## Übersetzungen

Alle Texte werden mit [Fluent](https://projectfluent.org) übersetzt. Englisch ist die
Ausgangssprache; fehlt eine Übersetzung, erscheint der englische Text. Die Sprachauswahl in den
Einstellungen ergibt sich aus den vorhandenen Dateien.

**App:** `locales/<sprache>/learn_piano.ftl`, beim Bauen ins Binary eingebettet. Für eine neue
Sprache `locales/en` nach `locales/<code>` (z.B. `fr`, `pt-BR`) kopieren, die Werte übersetzen
und `language-name` auf den Namen der Sprache in ihr selbst setzen. Jede im Code verwendete ID
prüft `fl!` beim Kompilieren gegen die englische Datei; `cargo test` meldet IDs, die Englisch
nicht kennt.

**Bibliothek:** `compositions/locales/<sprache>/compositions.ftl` mit den IDs aus
`compositions.xml`:

```ftl
difficulty-prep = Vorstufe
tag-christmas = Weihnachten
composition-silent-night = Stille Nacht
```

Diese Dateien liest die App zur Laufzeit, eine neue Sprache wirkt also ohne Neubau und reist mit
der Bibliothek mit. `learn-piano --check-library [DIR]` listet pro Sprache, was fehlt oder
nicht (mehr) in `compositions.xml` steht.

## Bedienung

- **Übersicht:** Die Stücke sind alphabetisch sortiert. Das Suchfeld neben dem Titel durchsucht
  Titel (alle Sprachen), Komponist und Kategorien; die Kategorien darunter filtern ebenfalls.
  Die Kategorienleiste lässt sich wischen, mit dem Mausrad oder den Pfeilen verschieben.

- **Warten** (Standard): Der Wasserfall hält an, bis die richtigen Tasten gedrückt sind.
- **Echtzeit**: Die Noten laufen im Tempo durch; Treffer und Timing werden bewertet.
- **Anhören**: Die App spielt das Stück vor.
- **Links / Rechts / Beide**: Die nicht geübte Hand spielt die App mit.
- **Schüler / Lehrer / Zu zweit** (nur bei Duetten): Den anderen Part spielt die App.
  Bei „Zu zweit“ spielt und bewertet die App beide Parts auf einem Keyboard.
- **50 / 75 / 100 %**: Tempo.
- Tippen auf den Wasserfall pausiert, `Esc` geht zurück.

Fortschritt und Einstellungen liegen in `~/.local/share/learn-piano/progress.json`.

## Entwicklung

```sh
cargo test
LEARN_PIANO_SHOTS=/tmp/shots cargo run   # klickt alle Screens durch, speichert PNGs, beendet sich
```

## Lizenzen

Die Schrift Inter (`assets/fonts`) steht unter der SIL Open Font License, siehe
`assets/fonts/Inter-LICENSE.txt`.
