Ich möchte eine Software bauen um Piano zu lernen

## Generell
- die Software soll Open Source sein, jeder der will kann sie benutzen
- oberstes Gebot ist Performance, ich betreibe sie auf einem uralten Linux Tablet
- Musikeingaben mache ich per meinem Keyboard per Midi Schnittstelle
- Ich möchte MusicXML Files einlesen können um diese dann zu spielen
- Du sollst beim Start die Datei compositions.xml einlesen und alle vorhanden Stücke schön visuell darstellen.
- Jedes Stück liegt in mehreren Schwierigkeitsstufen vor, diese sind ebenfalls im compositions.xml beschrieben, mit <difficulty id="beginner"> wobei hier die id ein entsprechendes Subfolder darstellt
- Beim Klick auf ein Stück im Dashboard, sollst du alle vorhandenen Schwierigkeiten des Stücks finden und als Auswahl aufzeigen, analog zu mockups/difficulty-selection.jpg (ohne die Schlösser, alles ist OpenSource). Bei Klick auf eine Schwierigkeit, wird immer <composition>/<difficulty>/full.musicxml abgespielt und zwar mit assets/sound-fonts/default.sf2
- Gibt es neben full.musicxml noch mehr files die mit 0x-part*.musicxml beginnen, handelt es sich um spezielle Lektionen von Sektionen dieses Stücks. Ich möchte diese in einer sauberen Benutzerführung durchspielen können. Nach jeder "Lektion" machst du eine Bewertung wie gespielt wurde und fragst: "Wiederholen" oder "Weiter". Nach dem spielen aller 0x-part*.musicxml lädst du die full.musicxml
- Ich möchte unten eine Klaviatur visualisiert haben
- Über der Klaviatur, sollen die Noten als Wasserfall herunterkommen
- Wenn im MusicXML Informationen zum Fingersatz enthalten sind, soll der Fingersatz dargesetellt werden

## Geklärte Entscheidungen (2026-11-15)

### Zielplattform & Technik
- Referenz-Hardware: Asus Eee Slate EP121 (Core i5-470UM 1.33 GHz, 2 Kerne, 8 GB RAM, Intel HD Graphics Gen5 / Ironlake), Fedora Silverblue, Touchscreen
- Läuft möglichst überall: Linux x86_64 **und ARM** (aarch64), idealerweise auch Windows/macOS
- Sprache: **native Rust**, ein einzelnes Binary
- Grafik: muss mit **OpenGL 2.1 / GLES 2.0** laufen (kein wgpu/Vulkan) → `macroquad`/`miniquad`, UI vollständig selbst gezeichnet
- Audio: `rustysynth` (SF2) in eigenem Audio-Thread, `cpal` als Ausgabe, begrenzte Polyphonie
- MIDI: `midir`
- Distribution: Flatpak (x86_64 + aarch64)

### Bedienung
- UI ist **Touch-optimiert** (große Trefferflächen, keine Hover-Abhängigkeit), Maus funktioniert ebenfalls
- UI-Sprache: Systemsprache (Fallback Englisch), in den Einstellungen umschaltbar (de/en)

### Üben
- Zwei Spielmodi, umschaltbar im Player:
  - **Warte-Modus (Default):** Wasserfall hält an, bis die richtige(n) Taste(n) gedrückt sind
  - **Echtzeit-Modus:** Noten laufen im Tempo, Treffer/Timing werden gewertet
- Hand-Auswahl: **links / rechts / beide**. Die nicht geübte Hand spielt die App automatisch mit (Staff 1 = rechte Hand, Staff 2 = linke Hand)
- Bewertung nach jeder Lektion: **1–3 Sterne + Prozent** (Trefferquote, Fehlernoten; im Echtzeit-Modus zusätzlich Timing)
- Lektionsname: aus `<work-title>` der MusicXML, Fallback Dateiname
- Fortschritt (erledigte Lektionen, beste Sterne pro Stück/Schwierigkeit) wird lokal gespeichert (`~/.local/share/…` bzw. Plattform-Äquivalent)
- Songtext (lyrics) und `notes.pdf` werden **nicht** angezeigt

### compositions.xml
- Wurzelelement `<library>` mit `<difficulties>` und `<compositions>`
- Optionales Feld `<composer>` pro Komposition (wird in der Schwierigkeitsauswahl angezeigt)
- Thumbnail: immer `thumbnail.png` (Pfad aus `<thumbnail src>`)
