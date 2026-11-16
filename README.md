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
./target/release/learn-piano                 # sucht compositions.xml im aktuellen Ordner
./target/release/learn-piano --library DIR   # oder in DIR
./target/release/learn-piano --fullscreen
./target/release/learn-piano --x11          # X11/XWayland erzwingen (Standard: Wayland, falls die Sitzung Wayland ist)
./target/release/learn-piano --debug        # FPS und Eingaben anzeigen, Log in ~/.local/share/learn-piano/debug.log
```

Ohne `--library` wird außerdem `$LEARN_PIANO_LIBRARY` und der Ordner des Binaries
(samt übergeordneten Ordnern) durchsucht.

In einer Wayland-Sitzung läuft die App nativ unter Wayland, sonst unter X11.
Unter XWayland funktioniert Touch nicht (es kommt nie ein Loslassen an); mit `--x11`
bzw. `--wayland` oder `LEARN_PIANO_BACKEND=x11|wayland` lässt sich die Wahl erzwingen.

Ein MIDI-Keyboard wird automatisch erkannt (auch wenn es später eingesteckt wird).
Ohne Keyboard kann man die Tasten auf dem Bildschirm antippen.

## Bibliothek

```
compositions.xml              Liste der Stücke und Schwierigkeitsstufen
sound-fonts/default.sf2       Klang
compositions/<stück>/
  thumbnail.png
  <schwierigkeit>/            z.B. prep, beginner, duet – muss in compositions.xml definiert sein
    01-part-A.musicxml        Lektionen (optional), in Namensreihenfolge
    02-part-B.musicxml
    full.musicxml             ganzes Stück (Pflicht)
```

`path` und `thumbnail src` in `compositions.xml` sind relativ zum Ordner `compositions/`.
Notenzeile 1 ist die rechte Hand, Notenzeile 2 die linke. Fingersätze werden aus
`<fingering>` gelesen.

**Duette:** Eine Stufe mit `kind="duet"` in `compositions.xml` ist keine Schwierigkeit,
sondern ein Duett. Sie wird in der Auswahl abgesetzt angezeigt. Die MusicXML enthält zwei
`<part>`s: den ersten für den Schüler, den zweiten für den Lehrer.

## Bedienung

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
