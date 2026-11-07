//! de catalog.
//!
//! Keys are the exact English UI literals; lookups are case-sensitive.

pub(crate) static CATALOG: &[(&str, &str)] = &[
    ("Job", "Auftrag"),
    ("Probe", "Antasten"),
    ("Offset", "Nullpunkte"),
    ("Utility", "Dienstprogramme"),
    ("Tools", "Werkzeuge"),
    ("Settings", "Einstellungen"),
    ("JOB", "AUFTRAG"),
    ("PROBING", "ANTRASTEN"),
    ("WORK OFFSETS", "WERKSTÜCKNULLPUNKTE"),
    ("UTILITIES", "DIENSTPROGRAMME"),
    ("TOOL TABLE", "WERKZEUGTABELLE"),
    ("SETTINGS", "EINSTELLUNGEN"),
    ("Console", "Konsole"),
    ("Program", "Programm"),
    ("Light", "Hell"),
    ("Apply", "Anwenden"),
    ("APPLY", "ANWENDEN"),
    ("Save", "Speichern"),
    ("SAVE", "SPEICHERN"),
    ("Cancel", "Abbrechen"),
    ("CANCEL", "ABBRECHEN"),
    ("Close", "Schließen"),
    ("CLOSE", "SCHLIESSEN"),
    ("Edit", "Bearbeiten"),
    ("EDIT", "BEARBEITEN"),
    ("Delete", "Löschen"),
    ("DEL", "LÖSCHEN"),
    ("Refresh", "Aktualisieren"),
    ("REFRESH", "AKTUALISIEREN"),
    ("Export", "Exportieren"),
    ("EXPORT", "EXPORTIEREN"),
    ("Import", "Importieren"),
    ("IMPORT", "IMPORTIEREN"),
    ("Validate", "Prüfen"),
    ("VALIDATE", "PRÜFEN"),
    ("Confirm", "Bestätigen"),
    ("CONFIRM", "BESTÄTIGEN"),
    ("Keep", "Behalten"),
    ("KEEP", "BEHALTEN"),
    ("Revert", "Zurücksetzen"),
    ("REVERT", "ZURÜCKSETZEN"),
    ("Zero", "Nullen"),
    ("ZERO", "NULLEN"),
    ("Home", "Referenzfahrt"),
    ("HOME", "REFERENZFAHRT"),
    ("Unlock", "Entsperren"),
    ("UNLOCK", "ENTSPERREN"),
    ("Macro", "Makro"),
    ("Startup Lines", "Startzeilen"),
    ("STARTUP LINES", "STARTZEILEN"),
    ("Keymap", "Tastenbelegung"),
    ("KEYMAP", "TASTENBELEGUNG"),
    ("Diagnostics", "Diagnose"),
    ("DIAGNOSTICS", "DIAGNOSE"),
    ("RESET", "RESET"),
    ("STOP", "STOPP"),
    ("Stop", "Stoppen"),
    ("SLEEP", "RUHEZUSTAND"),
    ("CONNECT", "VERBINDEN"),
    ("DISCONNECT", "TRENNEN"),
    ("CONNECTION", "VERBINDUNG"),
    ("TRANSPORT", "ÜBERTRAGUNG"),
    ("CURRENT LINK", "AKTUELLE VERBINDUNG"),
    ("SERIAL PORT", "SERIELLE SCHNITTSTELLE"),
    ("Demo", "Demo"),
    ("Serial port", "Serielle Schnittstelle"),
    (
        "No serial ports found",
        "Keine seriellen Schnittstellen gefunden",
    ),
    (
        "Disconnect before switching transport.",
        "Vor dem Wechsel der Übertragung trennen.",
    ),
    ("MOCK", "SIMULATION"),
    ("CONNECTING", "VERBINDUNG WIRD HERGESTELLT"),
    ("READY", "BEREIT"),
    ("DISCONNECTED", "GETRENNT"),
    ("ERROR", "FEHLER"),
    ("NOT CONNECTED", "NICHT VERBUNDEN"),
    ("STALE", "VERALTET"),
    ("Alarm", "Alarm"),
    ("Door", "Tür"),
    ("LOAD PROGRAM", "PROGRAMM LADEN"),
    ("START", "START"),
    ("PAUSE", "PAUSE"),
    ("RESUME", "FORTSETZEN"),
    ("RUN OPTIONS", "LAUFOPTIONEN"),
    ("RAPID", "EILGANG"),
    ("Rapid", "Eilgang"),
    ("ETA", "RESTZEIT"),
    ("TIME", "ZEIT"),
    ("TOOL CHANGE REQUIRED", "WERKZEUGWECHSEL ERFORDERLICH"),
    ("TOOL CHANGE", "WERKZEUGWECHSEL"),
    ("RUN TO", "AUSFÜHREN BIS"),
    ("SET BREAKPOINT", "HALTPUNKT SETZEN"),
    ("BP", "HP"),
    ("SEARCH", "SUCHEN"),
    ("FILTER", "FILTER"),
    ("HIGHLIGHT", "HERVORHEBEN"),
    ("NEXT", "WEITER"),
    ("PREV", "ZURÜCK"),
    ("NO PROGRAM LOADED", "KEIN PROGRAMM GELADEN"),
    ("NO PROGRAM", "KEIN PROGRAMM"),
    ("PROGRAM", "PROGRAMM"),
    ("HEIGHT MAP / COMPENSATED", "HÖHENKARTE / KOMPENSIERT"),
    ("Cut", "Schnitt"),
    ("Plunge", "Eintauchen"),
    ("IDLE", "LEERLAUF"),
    ("RUNNING", "LÄUFT"),
    ("PAUSED", "PAUSIERT"),
    ("COMPLETED", "ABGESCHLOSSEN"),
    ("ABORTED", "ABGEBROCHEN"),
    ("INTERRUPTED", "UNTERBROCHEN"),
    ("JOB ACTIVE", "AUFTRAG AKTIV"),
    ("JOB RUNNING", "AUFTRAG LÄUFT"),
    ("STOP JOB", "AUFTRAG STOPPEN"),
    ("TAP A LINE TO SELECT", "ZEILE ZUM AUSWÄHLEN ANTIPPEN"),
    ("No matching lines", "Keine passenden Zeilen"),
    ("NO MATCH", "KEIN TREFFER"),
    ("SAVE LOG", "PROTOKOLL SPEICHERN"),
    ("DRO", "ANZEIGE"),
    ("JOG", "HANDFAHREN"),
    ("MDI", "MDI"),
    ("FEED OVERRIDE", "VORSCHUB-OVERRIDE"),
    ("SPINDLE OVERRIDE", "SPINDEL-OVERRIDE"),
    ("RAPID OVERRIDE", "EILGANG-OVERRIDE"),
    ("COOLANT", "KÜHLMITTEL"),
    ("SPINDLE", "SPINDEL"),
    ("SIGNALS", "SIGNALE"),
    ("WORK", "WERKSTÜCK"),
    ("SEND", "SENDEN"),
    ("UP", "AUF"),
    ("DN", "AB"),
    ("CONT", "DAUERLAUF"),
    ("CW", "RECHTS"),
    ("CCW", "LINKS"),
    ("Spindle off", "Spindel aus"),
    ("RESET 100%", "100 % ZURÜCKSETZEN"),
    ("Feed", "Vorschub"),
    ("MACHINE", "MASCHINE"),
    ("TOOL", "WERKZEUG"),
    ("POCKET", "TASCHE"),
    ("RADIUS", "RADIUS"),
    ("TOOLPATH", "WERKZEUGBAHN"),
    ("CYCLES", "ZYKLEN"),
    ("CYCLE PARAMETERS", "ZYKLUSPARAMETER"),
    ("GRID SUMMARY", "RASTERÜBERSICHT"),
    ("PREFLIGHT", "VORPRÜFUNG"),
    ("PROBE RUN", "ANTATZLAUF"),
    ("RAW PRB (machine mm)", "ROH-PRB (Maschinen-mm)"),
    ("SAVED MAPS", "GESPEICHERTE KARTEN"),
    ("SAVED MAP", "GESPEICHERTE KARTE"),
    ("MEASUREMENT FRAME", "MESSUNGSSYSTEM"),
    ("HEIGHT MAP RESULT", "HÖHENKARTEN-ERGEBNIS"),
    ("CONFIRM APPLY", "ANWENDEN BESTÄTIGEN"),
    ("PROBE OFFSET X/Y", "TASTER-OFFSET X/Y"),
    ("PROBE OFFSET", "TASTER-OFFSET"),
    (
        "SEARCH BOUND (CONTROLLED-POINT XY)",
        "SUCHGRENZE (STEUERUNGSPUNKT XY)",
    ),
    ("APPLY HEIGHT MAP", "HÖHENKARTE ANWENDEN"),
    ("CLOSE MAP", "KARTE SCHLIESSEN"),
    ("SAVE MAP", "KARTE SPEICHERN"),
    ("EXPORT CSV", "CSV EXPORTIEREN"),
    ("RETRY PREFLIGHT", "VORPRÜFUNG WIEDERHOLEN"),
    ("CONFIGURE", "KONFIGURIEREN"),
    ("Probe connected", "Taster verbunden"),
    ("ABORT", "ABBRECHEN"),
    ("Height map", "Höhenkarte"),
    ("Z touch plate", "Z-Tastplatte"),
    ("XY touch plate", "XY-Tastplatte"),
    ("Edge X", "Kante X"),
    ("Edge Y", "Kante Y"),
    ("Corner", "Ecke"),
    ("Center circle", "Kreis mittig"),
    ("Center rectangle", "Rechteck mittig"),
    ("Rotation", "Drehung"),
    ("Tool length", "Werkzeuglänge"),
    ("MEASURED Z", "GEMESSENE Z"),
    ("SAFE Z", "SICHERE Z"),
    ("SURFACE Z", "OBERFLÄCHEN-Z"),
    ("MAP Z OFFSET", "KARTEN-Z-OFFSET"),
    ("MAP TRAVEL", "KARTENWEG"),
    ("GRID ORIGIN", "RASTERURSPRUNG"),
    ("COLUMNS", "SPALTEN"),
    ("ROWS", "ZEILEN"),
    ("TOUCHES", "TASTPUNKTE"),
    ("STEP X", "SCHRITT X"),
    ("STEP Y", "SCHRITT Y"),
    ("EDGE OFFSET", "KANTEN-OFFSET"),
    ("SEPARATION", "ABSTAND"),
    ("PLATE THICK", "PLATTENDICKE"),
    ("PLATE CORNER X", "PLATTENECKE X"),
    ("PLATE CORNER Y", "PLATTENECKE Y"),
    ("PROBE HEIGHT", "TASTERHÖHE"),
    ("PROBE DIA", "TASTER-DM"),
    ("REFERENCE Z", "REFERENZ-Z"),
    ("RECT WIDTH", "RECHTECKBREITE"),
    ("RECT HEIGHT", "RECHTECKHÖHE"),
    ("EXPECTED X", "X ERWARTET"),
    ("EXPECTED Y", "Y ERWARTET"),
    ("EXPECTED Z", "Z ERWARTET"),
    ("Measured height", "Gemessene Höhe"),
    ("Measured width", "Gemessene Breite"),
    ("Measured X edge", "Gemessene X-Kante"),
    ("Measured Y edge", "Gemessene Y-Kante"),
    ("SURFACE", "OBERFLÄCHE"),
    ("COMPENSATION", "KOMPENSATION"),
    ("PLAN", "PLAN"),
    ("MAP", "KARTE"),
    ("SAFETY", "SICHERHEIT"),
    ("PROBE VERIFIED", "TASTER GEPRÜFT"),
    ("NOT CONFIRMED", "NICHT BESTÄTIGT"),
    ("TRIGGERED", "AUSGELÖST"),
    ("NOT READY", "NICHT BEREIT"),
    ("UNITS", "EINHEITEN"),
    ("TOOL LENGTH DELTA", "WERKZEUGLÄNGEN-DELTA"),
    ("RMS fit error", "RMS-Anpassungsfehler"),
    ("Deviation", "Abweichung"),
    ("Nominal", "Nennwert"),
    ("WORK XYZ", "WERKSTÜCK XYZ"),
    ("MACHINE XYZ", "MASCHINE XYZ"),
    ("SEARCH SETTINGS", "EINSTELLUNGEN SUCHEN"),
    ("All settings", "Alle Einstellungen"),
    ("General", "Allgemein"),
    ("Axes", "Achsen"),
    ("Limits", "Grenzen"),
    ("Network address", "Netzwerkadresse"),
    ("Work mode", "Arbeitsmodus"),
    ("Step pulse time", "Schrittimpulszeit"),
    ("Step idle delay", "Schrittmotor-Leerlaufverzögerung"),
    ("Maximum spindle speed", "Maximale Spindeldrehzahl"),
    ("Homing cycle enable", "Referenzfahrt aktivieren"),
    ("Soft limits enable", "Software-Grenzen aktivieren"),
    ("Status report mask", "Statusmeldungs-Maske"),
    ("Stepper", "Schrittmotor"),
    ("X steps per millimetre", "X-Schritte pro Millimeter"),
    ("APPLIED", "ANGEWENDET"),
    ("PENDING", "AUSSTEHEND"),
    ("REBOOT REQUIRED", "NEUSTART ERFORDERLICH"),
    ("INVALID", "UNGÜLTIG"),
    ("FORMAT", "FORMAT"),
    ("WRITING", "SCHREIBEN"),
    ("Search results", "Suchergebnisse"),
    ("out of range", "außerhalb des Bereichs"),
    ("WCS OFFSETS", "WCS-NULLPUNKTE"),
    ("WCS OFFSET", "WCS-OFFSET"),
    ("CLEAR G92", "G92 LÖSCHEN"),
    ("G28 STORED POSITION", "G28 GESPEICHERTE POSITION"),
    ("G30 STORED POSITION", "G30 GESPEICHERTE POSITION"),
    ("GOTO G28", "ZU G28 FAHREN"),
    ("GOTO G30", "ZU G30 FAHREN"),
    ("REPORTED", "GEMELDET"),
    ("NOT REPORTED", "NICHT GEMELDET"),
    ("NON-PERSISTENT", "NICHT PERSISTENT"),
    ("ZERO ALL", "ALLE NULLEN"),
    ("ZERO X", "X NULLEN"),
    ("ZERO Y", "Y NULLEN"),
    ("ZERO Z", "Z NULLEN"),
    ("CONFIRM CLEAR G92", "G92-LÖSCHUNG BESTÄTIGEN"),
    ("CONFIRM G28 GOTO", "G28-FAHRT BESTÄTIGEN"),
    ("CONFIRM G30 GOTO", "G30-FAHRT BESTÄTIGEN"),
    ("EXPORT OFFSETS", "NULLPUNKTE EXPORTIEREN"),
    ("LOAD OFFSETS FILE", "NULLPUNKT-DATEI LADEN"),
    ("REFRESH $#", "$# AKTUALISIEREN"),
    ("OFFSETS", "NULLPUNKTE"),
    (
        "PREDEFINED POSITIONS / G92",
        "VORDEFINIERTE POSITIONEN / G92",
    ),
    ("OLD G92 OFFSET", "ALTER G92-OFFSET"),
    ("CONTROLLER TOOLS", "STEUERUNGSWERKZEUGE"),
    ("NEW TOOL", "NEUES WERKZEUG"),
    ("NEW RECORD", "NEUER EINTRAG"),
    ("SAVE TOOL", "WERKZEUG SPEICHERN"),
    ("TOOL NUMBER", "WERKZEUGNUMMER"),
    ("TOOL NAME", "WERKZEUGNAME"),
    ("TOOL DIAMETER (MM)", "WERKZEUGDURCHMESSER (MM)"),
    ("TOOL LENGTH (MM)", "WERKZEUGLÄNGE (MM)"),
    ("POCKET NUMBER", "TASCHENNUMMER"),
    ("DIAMETER (MM)", "DURCHMESSER (MM)"),
    ("LENGTH (MM)", "LÄNGE (MM)"),
    ("APP TOOL", "APP-WERKZEUG"),
    ("Profiles", "Profile"),
    ("BACKUPS", "SICHERUNGEN"),
    ("PROFILE", "PROFIL"),
    ("PROFILE RECORD", "PROFILEINTRAG"),
    ("CONTROLLER PROFILE", "STEUERUNGSPROFIL"),
    ("SAVE PROFILE", "PROFIL SPEICHERN"),
    ("SAVE APP", "APP SPEICHERN"),
    ("USE LOCAL DATA", "LOKALE DATEN VERWENDEN"),
    ("BACKUP BUNDLE", "SICHERUNGSPAKET"),
    ("VALIDATED FILE", "GEPRÜFTE DATEI"),
    ("LOCAL ONLY", "NUR LOKAL"),
    ("NO CONTROLLER", "KEINE STEUERUNG"),
    ("UNVERIFIED", "UNGEPRÜFT"),
    ("PENDING CONFIRMATION", "BESTÄTIGUNG AUSSTEHEND"),
    ("CORRUPT", "BESCHÄDIGT"),
    ("MISSING", "FEHLT"),
    ("VALID", "GÜLTIG"),
    ("MACROS", "MAKROS"),
    ("NEW MACRO", "NEUES MAKRO"),
    ("SAVE MACRO", "MAKRO SPEICHERN"),
    ("RUN MACRO", "MAKRO AUSFÜHREN"),
    ("MACRO NAME", "MAKRONAME"),
    ("MACRO COMMAND", "MAKROBEFEHL"),
    ("RESET DEFAULTS", "STANDARD WIEDERHERSTELLEN"),
    ("USE KEY", "TASTE VERWENDEN"),
    ("PRESS A KEY...", "TASTE DRÜCKEN..."),
    ("PRESS A KEY COMBINATION", "TASTENKOMBINATION DRÜCKEN"),
    ("RECORD KEY", "TASTE AUFNEHMEN"),
    ("CONTROLLER DIAGNOSTICS", "STEUERUNGSDIAGNOSE"),
    ("OPEN PINS / LIMITS", "PINS / GRENZEN ÖFFNEN"),
    ("PINS / LIMITS", "PINS / GRENZEN"),
    ("PIN MAP ($PINS)", "PIN-BELEGUNG ($PINS)"),
    ("PIN STATE ($PINSTATE)", "PIN-STATUS ($PINSTATE)"),
    ("LAST EVENTS ($LEV)", "LETZTE EREIGNISSE ($LEV)"),
    ("CONTROLLER SLEEP", "STEUERUNG IM RUHEZUSTAND"),
    (
        "PUT CONTROLLER TO SLEEP",
        "STEUERUNG IN DEN RUHEZUSTAND VERSETZEN",
    ),
    ("CONFIRM SLEEP", "RUHEZUSTAND BESTÄTIGEN"),
    ("DELETE MACRO", "MAKRO LÖSCHEN"),
    ("CONFIRM WRITE", "SCHREIBEN BESTÄTIGEN"),
    ("CONFIRM OFFSET WRITE", "OFFSET-SCHREIBEN BESTÄTIGEN"),
    ("CONFIRM REQUIRED", "BESTÄTIGUNG ERFORDERLICH"),
    ("CONFIRM IMPORT", "IMPORT BESTÄTIGEN"),
    ("ACKNOWLEDGED", "BESTÄTIGT"),
    ("1. ACKNOWLEDGE", "1. BESTÄTIGEN"),
    ("2. RESUME", "2. FORTSETZEN"),
    ("OPEN", "ÖFFNEN"),
    ("FOLDER", "ORDNER"),
    ("PROGRAMS", "PROGRAMME"),
    ("No file selected", "Keine Datei ausgewählt"),
    (
        "Select a program file first",
        "Zuerst eine Programmdatei auswählen",
    ),
    ("FILE", "DATEI"),
    ("PATH", "PFAD"),
    ("NAME", "NAME"),
    ("NEW", "NEU"),
    ("SET", "SETZEN"),
    ("GO", "LOS"),
    ("CLEAR", "LEEREN"),
    ("ALL", "ALLE"),
    ("SELECT", "AUSWÄHLEN"),
    ("SELECTED", "AUSGEWÄHLT"),
    ("RELOAD", "NEU LADEN"),
    ("RUN", "AUSFÜHREN"),
    ("ON", "EIN"),
    ("OFF", "AUS"),
    ("IMPORTED", "IMPORTIERT"),
    ("EXPORTED", "EXPORTIERT"),
    ("FAILED", "FEHLGESCHLAGEN"),
    ("COMPLETE", "ABGESCHLOSSEN"),
    ("PARTIAL", "TEILWEISE"),
    ("ACTIVE", "AKTIV"),
    ("CURRENT", "AKTUELL"),
    ("SETUP", "EINRICHTUNG"),
    ("VIEW", "ANSICHT"),
    ("FIT", "EINPASSEN"),
    ("CHECK", "PRÜFEN"),
    ("CHECKING", "PRÜFLAUF LÄUFT"),
    ("CHECK COMPLETE", "PRÜFLAUF ABGESCHLOSSEN"),
    ("CHECK STOPPED", "PRÜFLAUF GESTOPPT"),
    ("CHECK PROGRAM (DRY RUN)", "PROGRAMM PRÜFEN (TROCKENLAUF)"),
    ("START CHECK", "PRÜFLAUF STARTEN"),
    ("DRY RUN - NO MOTION", "TROCKENLAUF - KEINE BEWEGUNG"),
    (
        "WAITING FOR CONTROLLER CHECK MODE ($C)",
        "WARTE AUF PRÜFMODUS ($C)",
    ),
    ("WHAT THE DRY RUN DOES", "WAS DER TROCKENLAUF TUT"),
    ("MOTION IS SUPPRESSED", "BEWEGUNG UNTERDRÜCKT"),
    ("SPINDLE / COOLANT", "SPINDEL / KÜHLMITTEL"),
    ("WHEN FINISHED", "WENN FERTIG"),
    (
        "The controller parses and streams the loaded program with check mode ($C) enabled. \
        Nothing in this sheet starts a real job or moves an axis.",
        "Der Controller liest das geladene Programm im Prüfmodus ($C) und streamt es. \
        Nichts in diesem Fenster startet einen echten Auftrag oder bewegt eine Achse.",
    ),
    (
        "MOTION IS SUPPRESSED: the controller acknowledges program lines without moving the \
        machine.",
        "BEWEGUNG UNTERDRÜCKT: Der Controller bestätigt Programmzeilen, ohne die Maschine \
        zu bewegen.",
    ),
    (
        "SPINDLE / COOLANT: M3, M4, M5, M7, M8 and M9 are parsed and acknowledged but never \
        run as real outputs.",
        "SPINDEL / KÜHLMITTEL: M3, M4, M5, M7, M8 und M9 werden gelesen und bestätigt, aber \
        nie als echte Ausgänge geschaltet.",
    ),
    (
        "WHEN FINISHED: the engine leaves check mode and waits for the machine to report \
        IDLE. The program stays loaded for a separate real START.",
        "WENN FERTIG: Die Engine verlässt den Prüfmodus und wartet, bis die Maschine IDLE \
        meldet. Das Programm bleibt für einen separaten echten START geladen.",
    ),
    (
        "STOP remains available while the check streams; it never resumes or restarts the \
        run.",
        "STOPP bleibt während des Prüflaufs verfügbar; er setzt den Lauf niemals fort oder \
        startet ihn neu.",
    ),
    ("CANCEL sends nothing.", "ABBRECHEN sendet nichts."),
    (
        "Parse and stream the loaded program in check mode ($C); no motion.",
        "Lädt das geladene Programm im Prüfmodus ($C); keine Bewegung.",
    ),
    ("LOADED", "GELADEN"),
    ("LANGUAGE", "SPRACHE"),
    ("FOLDERS", "ORDNER"),
    ("LIVE", "LIVE"),
    ("JUMP", "SPRINGEN"),
    ("DEFAULTS", "STANDARD"),
    ("RUN AGAIN", "ERNEUT STARTEN"),
    ("APPLY TEXT", "TEXT ANWENDEN"),
    ("KEEP DRAFT", "ENTWURF BEHALTEN"),
    ("DIRECT", "DIREKT"),
    ("SYNTHETIC", "SYNTHETISCH"),
    ("TOUCH KEYPAD", "TOUCH-TASTATUR"),
    ("NO LINE SELECTED", "KEINE ZEILE AUSGEWÄHLT"),
    ("REPORTS REQUESTED", "BERICHTE ANGEFORDERT"),
    ("OFFLINE", "OFFLINE"),
    ("UNSAVED", "UNGESPEICHERT"),
    ("USER-DEFINED", "BENUTZERDEFINIERT"),
    ("READ-ONLY", "NUR LESEN"),
    ("NOT LOADED", "NICHT GELADEN"),
    (
        "Choose a file for the job; loading never starts it.",
        "Datei für den Auftrag wählen; Laden startet sie nie.",
    ),
    (
        "No folders or program files here",
        "Keine Ordner oder Programmdateien hier",
    ),
    (
        "Tap UP or type another folder path.",
        "Tippe AUF oder gib einen anderen Ordnerpfad ein.",
    ),
    (
        "48 px rows | folders open on tap | hidden files and symlinks are not shown | .nc \
        .gcode .tap .ngc | 20 MB limit",
        "48-px-Zeilen | Ordner öffnen per Tipp | versteckte Dateien und Symlinks werden \
        nicht angezeigt | .nc .gcode .tap .ngc | 20-MB-Grenze",
    ),
    (
        "Tap a program file to select it",
        "Tippe eine Programmdatei zum Auswählen",
    ),
    ("READY TO LOAD", "BEREIT ZUM LADEN"),
    ("MOCK / BROWSE ONLY", "SIMULATION / NUR DURCHSUCHEN"),
    ("LINK NOT READY", "VERBINDUNG NICHT BEREIT"),
    ("LIVE / READY", "LIVE / BEREIT"),
    ("DIR", "ORDN."),
    ("OPTIONAL STOP", "OPTIONALER STOPP"),
    ("SINGLE BLOCK", "EINZELSATZ"),
    ("BLOCK DELETE", "SATZ LÖSCHEN"),
    (
        "M1 optional stops are honored",
        "Optionale Stopps M1 werden beachtet",
    ),
    ("Hold after each block", "Halt nach jedem Satz"),
    ("Skip blocks marked with /", "Sätze mit / überspringen"),
    (
        "Each tap sends one command. A row keeps showing the value the controller reports; \
        it changes on the next poll, not when the button is pressed.",
        "Jeder Tipp sendet einen Befehl. Eine Zeile zeigt weiter den vom Controller \
        gemeldeten Wert; er ändert sich beim nächsten Poll, nicht beim Tastendruck.",
    ),
    ("Mock mode: no controller", "Mock-Modus: kein Controller"),
    ("Link not ready", "Verbindung nicht bereit"),
    (
        "Locked while a job runs or pauses",
        "Gesperrt während Auftrag läuft oder pausiert",
    ),
    (
        "MOCK MODE: run options need a live controller",
        "MOCK-MODUS: Laufoptionen benötigen einen aktiven Controller",
    ),
    (
        "LINK NOT READY: run options are disabled until the handshake completes",
        "VERBINDUNG NICHT BEREIT: Laufoptionen sind bis zum Abschluss des Handshakes \
        deaktiviert",
    ),
    (
        "BLOCK DELETE IS LOCKED WHILE A JOB IS RUNNING OR PAUSED",
        "SATZ LÖSCHEN IST GESPERRT, WÄHREND EIN AUFTRAG LÄUFT ODER PAUSIERT",
    ),
    (
        "Closing this sheet sends nothing.",
        "Dieses Fenster zu schließen sendet nichts.",
    ),
    (
        "CANCEL, the scrim, and closing this sheet send nothing.",
        "ABBRECHEN, der Hintergrund und das Schließen dieses Fensters senden nichts.",
    ),
    ("CONFIRMING WILL", "BESTÄTIGUNG BEWIRKT"),
    ("RUN TO LINE", "LAUF BIS ZEILE"),
    ("SET BREAKPOINT AT LINE", "HALTESTELLE SETZEN BEI ZEILE"),
    ("START PROGRAM AT LINE 1", "PROGRAMM AB ZEILE 1 STARTEN"),
    ("PAUSE BEFORE LINE", "PAUSE VOR ZEILE"),
    (
        "LEAVE THE ACTIVE JOB AT ITS CURRENT POSITION",
        "AKTIVEN AUFTRAG AN DER AKTUELLEN POSITION LASSEN",
    ),
    ("RUN TO BLOCKED", "LAUF BIS BLOCKIERT"),
    ("SET BP + START", "HP SETZEN + START"),
    (
        "A job is already running or paused. Confirming only sets a breakpoint at the \
        selected line; it does not start, restart, or resume the job.",
        "Ein Auftrag läuft bereits oder ist pausiert. Bestätigen setzt nur eine Haltestelle \
        an der gewählten Zeile; der Auftrag wird nicht gestartet, neu gestartet oder \
        fortgesetzt.",
    ),
    (
        "All earlier lines still run from the beginning. The job starts at line 1 and \
        pauses before the selected line. Start-from-line is not supported.",
        "Alle vorherigen Zeilen laufen weiterhin von Anfang an. Der Auftrag startet bei \
        Zeile 1 und pausiert vor der gewählten Zeile. Start ab Zeile wird nicht \
        unterstützt.",
    ),
    (
        "One step per press; off while typing. RESET DEFAULTS restores.",
        "Ein Schritt pro Tastendruck; beim Tippen aus. STANDARD WIEDERHERSTELLEN stellt \
        wieder her.",
    ),
    (
        "Tap for one step; hold a mapped jog key to jog continuously. Off while typing.",
        "Tippen für einen Schritt; eine zugeordnete Jog-Taste halten für kontinuierliches \
        Joggen. Beim Tippen aus.",
    ),
    (
        "Named single-line MDI commands, saved in this controller profile. RUN sends only \
        after a click; CONFIRM macros require a second explicit confirmation.",
        "Benannte einzeilige MDI-Befehle, in diesem Controller-Profil gespeichert. STARTEN \
        sendet erst nach einem Klick; BESTÄTIGEN-Makros verlangen eine zweite \
        ausdrückliche Bestätigung.",
    ),
    (
        "No user macros for this controller profile. Add a named, single-line MDI command; \
        it is saved only here and never runs on connect.",
        "Keine Benutzer-Makros für dieses Controller-Profil. Füge einen benannten \
        einzeiligen MDI-Befehl hinzu; er wird nur hier gespeichert und nie beim Verbinden \
        ausgeführt.",
    ),
    ("MACRO SAVES LOCKED", "MAKRO-SPEICHERN GESPERRT"),
    ("MACRO RUNS LOCKED", "MAKRO-AUSFÜHRUNG GESPERRT"),
    (
        "Startup lines execute automatically on every controller reset and can command \
        motion, spindle, or coolant. Writes are allowed only while the controller is Idle; \
        nothing is written until APPLY and CONFIRM WRITE.",
        "Startzeilen werden bei jedem Controller-Reset automatisch ausgeführt und können \
        Bewegung, Spindel oder Kühlmittel befehlen. Schreiben ist nur bei Leerlauf des \
        Controllers erlaubt; nichts wird geschrieben, bis ANWENDEN und SCHREIBEN \
        BESTÄTIGEN gedrückt wird.",
    ),
    (
        "WRITE NOT APPLIED - THE PREVIOUS VALUE IS STILL ACTIVE",
        "SCHREIBEN NICHT ANGEWENDET - DER VORHERIGE WERT IST WEITER AKTIV",
    ),
    (
        "This startup line executes automatically on every controller reset. It can command \
        motion, spindle, or coolant.",
        "Diese Startzeile wird bei jedem Controller-Reset automatisch ausgeführt. Sie kann \
        Bewegung, Spindel oder Kühlmittel befehlen.",
    ),
    ("EXACT TEXT TO STORE", "EXAKTER TEXT ZUM SPEICHERN"),
    ("Previous controller value", "Vorheriger Controller-Wert"),
    (
        "CONFIRM WRITE sends the exact text shown. The app does not update the row itself: \
        it reads the line back and reports the result.",
        "SCHREIBEN BESTÄTIGEN sendet den exakt angezeigten Text. Die App aktualisiert die \
        Zeile nicht selbst: Sie liest sie zurück und meldet das Ergebnis.",
    ),
    ("RESET EXECUTION", "AUSFÜHRUNG BEI RESET"),
    (
        "CONFIRM FIRST; CANCEL SENDS NOTHING",
        "ERST BESTÄTIGEN; ABBRECHEN SENDET NICHTS",
    ),
    (
        "Sleep stops controller status polling until RESET or a status wake. The app does \
        not wake the controller automatically; the DRO and job view stay frozen until the \
        controller answers again.",
        "Schlafen stoppt die Statusabfrage des Controllers bis RESET oder ein \
        Status-Weckruf. Die App weckt den Controller nicht automatisch; DRO und \
        Auftragsansicht bleiben eingefroren, bis der Controller wieder antwortet.",
    ),
    ("BEFORE YOU CONFIRM", "BEVOR SIE BESTÄTIGEN"),
    (
        "The controller must report Idle and no job may be running or paused. One confirmed \
        tap sends one sleep command; CANCEL sends nothing.",
        "Der Controller muss Leerlauf melden und kein Auftrag darf laufen oder pausiert \
        sein. Ein bestätigter Tipp sendet einen Schlafbefehl; ABBRECHEN sendet nichts.",
    ),
    (
        "Pin map, auxiliary pin levels, limit switches and last control events. Opening the \
        panel sends nothing; REFRESH sends only $PINS, $PINSTATE, $LIM and $LEV.",
        "Pinbelegung, auxiliäre Pinpegel, Endschalter und letzte Steuerereignisse. Das \
        Öffnen des Fensters sendet nichts; AKTUALISIEREN sendet nur $PINS, $PINSTATE, $LIM \
        und $LEV.",
    ),
    (
        "No $PINS report in this session",
        "Kein $PINS-Bericht in dieser Sitzung",
    ),
    (
        "The controller reported no pin entries",
        "Der Controller meldete keine Pin-Einträge",
    ),
    ("pin entries reported", "Pin-Einträge gemeldet"),
    (
        "Applies immediately and is saved in app preferences; the controller is not \
        contacted.",
        "Wird sofort angewendet und in den App-Einstellungen gespeichert; der Controller \
        wird nicht kontaktiert.",
    ),
    (
        "Export writes app prefs, identity, settings, offsets, tools, macros, startup \
        lines, and keymap; import validates before writing and never pushes to the \
        controller.",
        "Export schreibt App-Einstellungen, Identität, Einstellungen, Nullpunkte, \
        Werkzeuge, Makros, Startzeilen und Tastaturbelegung; Import prüft vor dem \
        Schreiben und sendet nie an den Controller.",
    ),
    (
        "The app owns this table; it persists in the per-controller profile. Controller \
        [T:] entries stay read-only and are never merged silently.",
        "Diese Tabelle gehört der App; sie wird im Controller-Profil gespeichert. \
        [T:]-Einträge des Controllers bleiben schreibgeschützt und werden nie \
        stillschweigend zusammengeführt.",
    ),
    (
        "No controller tool table has been reported yet.",
        "Es wurde noch keine Controller-Werkzeugtabelle gemeldet.",
    ),
    (
        "The controller reports an empty tool table (0 entries).",
        "Der Controller meldet eine leere Werkzeugtabelle (0 Einträge).",
    ),
    (
        "Reported by the controller through [T:...]; this panel never writes, overwrites, \
        or merges into the app table above.",
        "Vom Controller über [T:...] gemeldet; dieses Fenster schreibt, überschreibt oder \
        führt nie in die App-Tabelle oben ein.",
    ),
    ("STARTUP WRITES LOCKED", "STARTZEILEN-SCHREIBEN GESPERRT"),
    ("SAVE LOCKED", "SPEICHERN GESPERRT"),
    ("KEYMAP NOT VALID", "TASTATURBELEGUNG UNGÜLTIG"),
    ("Program has no lines", "Programm hat keine Zeilen"),
    (
        "Load a non-empty source file to search it.",
        "Lade eine nicht leere Quelldatei zum Durchsuchen.",
    ),
    ("Nothing matches", "Keine Treffer"),
    (
        "Switch to HIGHLIGHT or clear the field to restore the full list.",
        "Wechsle zu HERVORHEBEN oder leere das Feld, um die vollständige Liste \
        wiederherzustellen.",
    ),
    (
        "DRAG ORBIT | SHIFT/MIDDLE PAN | WHEEL ZOOM",
        "ZIEHEN ORBIT | UMSCHALT/MITTELTASTE PAN | RAD ZOOM",
    ),
    ("GENERATED PROGRAM", "GENERIERTES PROGRAMM"),
    (
        "No axes reported by the controller",
        "Keine Achsen vom Controller gemeldet",
    ),
    (
        "ONE WRITE AT A TIME | READ-BACK VERIFIED",
        "EIN SCHREIBVORGANG ZUGLEICH | RÜCKLESSEN VERIFIZIERT",
    ),
    ("WRITES LOCKED", "SCHREIBEN GESPERRT"),
    ("IDLE: WRITES ENABLED", "LEERLAUF: SCHREIBEN AKTIV"),
    (
        "MOCK MODE: NO CONTROLLER SETTINGS",
        "MOCK-MODUS: KEINE CONTROLLER-EINSTELLUNGEN",
    ),
    (
        "The generated editor is built from the controller's $$ values and $ES/$EG \
        metadata. The design-review mock supplies neither. Start without --mock to connect \
        the in-process FlexiHAL demo controller.",
        "Der generierte Editor basiert auf den $$-Werten und $ES/$EG-Metadaten des \
        Controllers. Die Design-Review-Simulation liefert beides nicht. Starte ohne \
        --mock, um den In-Process-FlexiHAL-Demo-Controller zu verbinden.",
    ),
    ("WAITING FOR SETTINGS", "WARTE AUF EINSTELLUNGEN"),
    (
        "The handshake has not reported $$, $ES, or $EG yet. The form appears as soon as \
        the controller answers.",
        "Der Handshake hat $$, $ES oder $EG noch nicht gemeldet. Das Formular erscheint, \
        sobald der Controller antwortet.",
    ),
    (
        "THE CONTROLLER REPORTED NO SETTINGS",
        "DER CONTROLLER MELDETE KEINE EINSTELLUNGEN",
    ),
    (
        "$$ returned no values and $ES/$EG returned no metadata. The editor stays empty \
        rather than inventing a setting list.",
        "$$ lieferte keine Werte und $ES/$EG keine Metadaten. Der Editor bleibt leer, statt \
        eine Liste zu erfinden.",
    ),
    (
        "EXAMPLE SHAPE (NO VALUES ARE WRITTEN)",
        "BEISPIEL (KEINE WERTE WERDEN GESCHRIEBEN)",
    ),
    ("LOW", "NIEDRIG"),
    ("HIGH", "HOCH"),
    ("dashed = missing", "gestrichelt = fehlt"),
    ("QUEUED", "IN WARTESCHLANGE"),
    ("BLANK ALLOWED", "LEER ERLAUBT"),
    ("BIT 0 ENABLES THE OTHERS", "BIT 0 AKTIVIERT DIE ANDEREN"),
    ("INFERRED FROM VALUE", "AUS WERT ABGELEITET"),
    (
        "No saved maps for this controller profile yet.",
        "Noch keine gespeicherten Karten für dieses Controller-Profil.",
    ),
    (
        "Read-only view; opening does not apply compensation or start a job",
        "Nur-Lese-Ansicht; Öffnen wendet keine Kompensation an und startet keinen Auftrag",
    ),
    (
        "Nothing is applied automatically",
        "Nichts wird automatisch angewendet",
    ),
    (
        "No automatic retry or resume. Recover the machine, then run again.",
        "Kein automatischer Wiederholungs- oder Fortsetzungsversuch. Maschine \
        wiederherstellen, dann erneut starten.",
    ),
    (
        "every gate must pass before RUN",
        "alle Prüfungen müssen vor START bestehen",
    ),
    ("PROBE CYCLES", "TASTZYKLEN"),
    (
        "Pick a card, inspect the preflight, then press RUN",
        "Karte wählen, Vorabprüfung ansehen, dann STARTEN",
    ),
    (
        "millimetres; tap a value to edit",
        "Millimeter; Wert zum Bearbeiten antippen",
    ),
    ("recorded vs live", "aufgezeichnet vs. live"),
    ("per controller profile", "pro Controller-Profil"),
    ("APPLY OFFSET", "VERSATZ ANWENDEN"),
    ("NOT SENT YET", "NOCH NICHT GESENDET"),
    ("EXACT MDI LINE", "EXAKTE MDI-ZEILE"),
    (
        "CURRENT DRO (DISPLAY UNITS)",
        "AKTUELLE DRO (ANZEIGEEINHEITEN)",
    ),
    (
        "DRO CHANGED SINCE THIS SHEET OPENED; CANCEL AND RECHECK",
        "DRO SEIT DEM ÖFFNEN GEÄNDERT; ABBRECHEN UND NEU PRÜFEN",
    ),
    ("READ-ONLY $# PARAMETERS", "$#-PARAMETER NUR LESEN"),
    (
        "X / Y / Z IN DISPLAY UNITS",
        "X / Y / Z IN ANZEIGEEINHEITEN",
    ),
    ("TAP TO EDIT", "ZUM BEARBEITEN ANTIPPEN"),
    (
        "RESTORE WORK OFFSETS - PREVIEW",
        "WERKSTÜCKNULLPUNKTE WIEDERHERSTELLEN - VORSCHAU",
    ),
    (
        "G10 L2 LINES IN THIS FILE (VALUES IN MILLIMETRES)",
        "G10-L2-ZEILEN IN DIESER DATEI (WERTE IN MILLIMETERN)",
    ),
    (
        "THE FILE BEGINS WITH G21: EVERY VALUE IS INTERPRETED IN MILLIMETRES.",
        "DIE DATEI BEGINNT MIT G21: JEDER WERT WIRD IN MILLIMETERN INTERPRETIERT.",
    ),
    ("LOAD AS PROGRAM", "ALS PROGRAMM LADEN"),
    (
        "LOCKED: A JOB IS ACTIVE; STOP IT BEFORE LOADING A PROGRAM",
        "GESPERRT: EIN AUFTRAG IST AKTIV; VOR DEM LADEN STOPPEN",
    ),
    ("SET X", "X SETZEN"),
    ("SET Y", "Y SETZEN"),
    ("SET Z", "Z SETZEN"),
    ("SET AXIS", "ACHSE SETZEN"),
    ("MDI TEXT", "MDI-TEXT"),
    ("SPINDLE RPM", "SPINDEL-DREHZAHL"),
    ("SETTING", "EINSTELLUNG"),
    ("OFFSET", "VERSATZ"),
    ("TOKENS + DIGITS", "TOKENS + ZIFFERN"),
    ("G10 L20 | DISPLAY UNITS", "G10 L20 | ANZEIGEEINHEITEN"),
    ("RANGE N/A", "BEREICH N/V"),
    ("TOOL FIELD", "WERKZEUGFELD"),
    ("PROBE FIELD (mm)", "TASTFELD (mm)"),
    ("Demo FlexiHAL", "Demo FlexiHAL"),
    (
        "In-process FlexiHAL simulator over the loopback transport. No serial port is \
        opened.",
        "In-Process-FlexiHAL-Simulator über den Loopback-Transport. Es wird kein serieller \
        Port geöffnet.",
    ),
    (
        "Same engine, handshake, streamer and safety gateway as a real controller.",
        "Gleiche Engine, Handshake, Streamer und Sicherheits-Gateway wie ein echter \
        Controller.",
    ),
    (
        "Tap a port to select it; nothing opens until CONNECT.",
        "Port antippen zum Auswählen; nichts öffnet vor VERBINDEN.",
    ),
    (
        "Line settings are fixed at 8 data bits, no parity, one stop bit. DTR reset pulses \
        are not implemented in M2.",
        "Leitungseinstellungen sind fest: 8 Datenbits, keine Parität, 1 Stoppbit. \
        DTR-Resetimpulse sind in M2 nicht implementiert.",
    ),
    ("serial port", "serieller Port"),
    ("NOT REQUESTED", "NICHT ANGEFORDERT"),
    ("READING CONTROLLER", "CONTROLLER WIRD GELESEN"),
    (
        "CONTROLLER BUSY - RETRYING",
        "CONTROLLER BESCHÄFTIGT - WIEDERHOLUNG",
    ),
    (
        "No $PINS report has arrived in this session.",
        "In dieser Sitzung ist kein $PINS-Bericht eingegangen.",
    ),
    (
        "No $PINSTATE report has arrived in this session.",
        "In dieser Sitzung ist kein $PINSTATE-Bericht eingegangen.",
    ),
    (
        "The controller reported no auxiliary pin levels.",
        "Der Controller meldete keine auxiliären Pinpegel.",
    ),
    (
        "No $LIM report has arrived in this session.",
        "In dieser Sitzung ist kein $LIM-Bericht eingegangen.",
    ),
    (
        "No $LEV report has arrived in this session.",
        "In dieser Sitzung ist kein $LEV-Bericht eingegangen.",
    ),
    (
        "The controller reports no control or limit events yet.",
        "Der Controller meldet noch keine Steuer- oder Grenzereignisse.",
    ),
    ("PROBE PLAN", "TASTPLAN"),
    ("CUSTOM", "BENUTZERDEFINIERT"),
    (
        "No settings in this group",
        "Keine Einstellungen in dieser Gruppe",
    ),
    ("No setting matches", "Keine Einstellung entspricht"),
    (
        "Pick another group in the rail.",
        "Wähle eine andere Gruppe in der Leiste.",
    ),
    (
        "Search matches a number, name, or group. Clear the search to browse groups.",
        "Die Suche findet Nummer, Name oder Gruppe. Leere die Suche, um Gruppen zu \
        durchsuchen.",
    ),
    ("CONTROLLER STATE", "CONTROLLER-STATUS"),
    (
        "CONTROLLER DIAGNOSTIC DATA - READ-ONLY",
        "CONTROLLER-DIAGNOSEDATEN - NUR LESEN",
    ),
    ("SAVE BLOCKED", "SPEICHERN GESPERRT"),
    (
        "KEPT: offsets are unchanged. Apply stays available.",
        "BEHALTEN: Die Nullpunkte sind unverändert. Anwenden bleibt verfügbar.",
    ),
    ("COMMAND", "BEFEHL"),
    ("Ask before running", "Vor Ausführung fragen"),
    (
        "New macros default to CONFIRM. A direct macro runs on one click.",
        "Neue Makros verwenden standardmäßig BESTÄTIGEN. Ein direktes Makro läuft mit einem \
        Klick.",
    ),
    ("EXACT COMMAND TO SEND", "EXAKTER ZU SENDENDER BEFEHL"),
    (
        "Jogging is allowed while the job waits; the Jog panel stays active. The job will \
        not send another line until you resume.",
        "Joggen ist erlaubt, während der Auftrag wartet; das Jog-Fenster bleibt aktiv. Der \
        Auftrag sendet keine weitere Zeile, bis du fortsetzt.",
    ),
    (
        "Jogging is allowed while the job waits; the Jog panel stays active. The job will \
        not send another line until you resume. RESET and STOP above remain available.",
        "Joggen ist erlaubt, während der Auftrag wartet; das Jog-Fenster bleibt aktiv. Der \
        Auftrag sendet keine weitere Zeile, bis du fortsetzt. RESET und STOPP oben bleiben \
        verfügbar.",
    ),
    ("VERIFYING", "PRÜFUNG LÄUFT"),
    ("VERIFIED", "VERIFIZIERT"),
    ("MISMATCH", "ABWEICHUNG"),
    ("MEASUREMENT ONLY", "NUR MESSUNG"),
    ("READY TO APPLY", "BEREIT ZUM ANWENDEN"),
    ("REVERTED", "ZURÜCKGESETZT"),
    ("FEED", "VORSCHUB"),
    ("RETRACT", "RÜCKZUG"),
    ("CENTER X", "MITTE X"),
    ("CENTER Y", "MITTE Y"),
    ("ORIGIN X WORK", "NULLPUNKT X WERKSTÜCK"),
    ("ORIGIN Y WORK", "NULLPUNKT Y WERKSTÜCK"),
    ("SURFACE Z MACH", "OBERFLÄCHE Z MASCHINE"),
    ("PROBE OFFSET X", "TASTVERSATZ X"),
    ("PROBE OFFSET Y", "TASTVERSATZ Y"),
    (
        "MOCK MODE: browsing only; loading needs a live controller",
        "MOCK-MODUS: nur Durchsuchen; Laden benötigt einen aktiven Controller",
    ),
    (
        "LINK NOT READY: finish the connection handshake before loading a program",
        "VERBINDUNG NICHT BEREIT: Schließe den Handshake ab, bevor du ein Programm lädst",
    ),
    (
        "JOB ACTIVE: stop the running job before loading another program",
        "AUFTRAG AKTIV: Stoppe den laufenden Auftrag, bevor du ein anderes Programm lädst",
    ),
    (
        "MOCK MODE: a check run needs a live controller",
        "MOCK-MODUS: Ein Prüflauf benötigt einen aktiven Controller",
    ),
    (
        "LINK NOT READY: finish the connection handshake before a check run",
        "VERBINDUNG NICHT BEREIT: Schließe den Handshake vor dem Prüflauf ab",
    ),
    (
        "MACHINE LOCKED: clear Alarm or Door before a check run",
        "MASCHINE GESPERRT: Alarm oder Tür vor dem Prüflauf quittieren",
    ),
    (
        "NO PROGRAM: load a program before a check run",
        "KEIN PROGRAMM: Lade vor dem Prüflauf ein Programm",
    ),
    (
        "JOB ACTIVE: stop the running job before a check run",
        "AUFTRAG AKTIV: Stoppe den laufenden Auftrag vor dem Prüflauf",
    ),
    (
        "MACHINE NOT IDLE: a check run starts only from IDLE",
        "MASCHINE NICHT LEERLAUF: Ein Prüflauf startet nur aus IDLE",
    ),
    (
        "CHECK RUN ACTIVE: wait for the controller to finish check mode",
        "PRÜFLAUF AKTIV: Warte, bis der Controller den Prüfmodus beendet",
    ),
    (
        "PROBE PROGRAM: a generated probe plan cannot be checked from the job toolbar",
        "TASTPROGRAMM: Ein generierter Tastplan kann nicht über die Auftragsleiste geprüft \
        werden",
    ),
    (
        "1. Confirm the physical E-stop and independent spindle stop. 2. Open \
        CONNECTION and select the serial port. 3. Wait for READY and Idle. 4. Check \
        the DRO with one small jog. 5. Verify units, WCS, and soft limits before \
        loading a program.",
        "1. Bestätige den physischen Not-Halt und den unabhängigen Spindelstopp. 2. \
        Öffne VERBINDUNG und wähle den seriellen Port. 3. Warte auf BEREIT und \
        Leerlauf. 4. Prüfe die DRO mit einem kleinen Jog. 5. Prüfe Einheiten, WCS und \
        Soft-Limits, bevor du ein Programm lädst.",
    ),
    ("ADD LINE NUMBERS", "ZEILENNUMMERN HINZUFÜGEN"),
    ("APP PREFERENCE ONLY", "NUR APP-EINSTELLUNG"),
    (
        "APPLY is a separate, confirmed step and is allowed only while the controller \
        is Idle.",
        "ANWENDEN ist ein separater, bestätigter Schritt und nur zulässig, solange der \
        Controller im Leerlauf ist.",
    ),
    (
        "APPLY sends exactly one Command::SetSetting through the engine. The engine \
        writes the value, reads it back, and only then marks it APPLIED. CANCEL sends \
        nothing.",
        "ANWENDEN sendet genau einen Command::SetSetting über die Engine. Die Engine \
        schreibt den Wert, liest ihn zurück und markiert ihn erst dann als ANGEWENDET. \
        ABBRECHEN sendet nichts.",
    ),
    ("AXIS", "ACHSE"),
    (
        "After setup, use the normal Connection panel. Start with the simulator or with \
        the spindle off, then run only status, DRO, and small jogs.",
        "Verwende nach der Einrichtung das normale Verbindungsfenster. Starte mit dem \
        Simulator oder bei ausgeschalteter Spindel und führe nur Status, DRO und \
        kleine Jogs aus.",
    ),
    ("BACK", "ZURÜCK"),
    (
        "Back-and-forth raster at each depth, with a safe-Z retract between passes. The \
        wizard validates every field and caps the program before LOAD AS PROGRAM; it \
        never starts the job.",
        "Hin-und-her-Raster in jeder Tiefe mit Rückzug auf sicheres Z zwischen den \
        Durchgängen. Der Assistent prüft jedes Feld und begrenzt das Programm vor ALS \
        PROGRAMM LADEN; er startet den Auftrag nie.",
    ),
    ("CALCULATE", "BERECHNEN"),
    ("CALIBRATION CONFIRMATION", "KALIBRIERUNG BESTÄTIGEN"),
    ("CALIBRATION DISTANCE (mm)", "KALIBRIERSTRECKE (mm)"),
    (
        "CALIBRATION WRITES LOCKED",
        "KALIBRIERUNGS-SCHREIBEN GESPERRT",
    ),
    (
        "Calculate to preview the new steps/mm and the change before writing.",
        "Berechne, um die neuen steps/mm und die Änderung vor dem Schreiben in der \
        Vorschau zu sehen.",
    ),
    ("DEFAULT DISPLAY UNITS", "STANDARD-ANZEIGEEINHEITEN"),
    ("DISPLAY UNITS", "ANZEIGEEINHEITEN"),
    (
        "ESTIMATED MACHINE BOUNDS (WCS + G92)",
        "GESCHÄTZTE MASCHINENGRENZEN (WCS + G92)",
    ),
    (
        "EVERY ITEM IS REQUIRED BEFORE NEXT",
        "JEDES ELEMENT IST VOR WEITER ERFORDERLICH",
    ),
    ("FINISH SETUP", "EINRICHTUNG ABSCHLIESSEN"),
    (
        "FIRST CONNECTION ORDER",
        "REIHENFOLGE DER ERSTEN VERBINDUNG",
    ),
    ("GENERATE PREVIEW", "VORSCHAU ERZEUGEN"),
    (
        "GENERATE PREVIEW to validate the fields and compute passes, path length, and \
        time.",
        "VORSCHAU ERZEUGEN, um die Felder zu prüfen und Durchgänge, Weglänge und Zeit \
        zu berechnen.",
    ),
    ("GENERATED / NOT CHECKED", "ERZEUGT / NICHT GEPRÜFT"),
    (
        "HEIGHT MAP / COMPENSATED: loading a transform replaces the compensated program \
        and its Revert original. Keep an external copy if needed.",
        "HÖHENKARTE / KOMPENSIERT: Das Laden einer Transformation ersetzt das \
        kompensierte Programm und dessen Original zum Zurücksetzen. Behalte bei Bedarf \
        eine externe Kopie.",
    ),
    (
        "I measured the actual travel and want to write this steps/mm value",
        "Ich habe den tatsächlichen Verfahrweg gemessen und möchte diesen steps/mm-Wert \
        schreiben",
    ),
    (
        "I understand this loads the previewed source and does not start it",
        "Mir ist klar, dass dies die Vorschauquelle lädt und sie nicht startet",
    ),
    (
        "I understand this program is generated, not checked against the machine",
        "Mir ist klar, dass dieses Programm erzeugt und nicht gegen die Maschine \
        geprüft wurde",
    ),
    ("INCHES (IN)", "ZOLL (IN)"),
    (
        "INFORMATIONAL CHECK: WCS changes, G10 writes, G92 offsets, and tool-length \
        offsets are not enforced here. Verify the setup and soft limits before \
        cutting.",
        "INFORMATIONSPRÜFUNG: WCS-Änderungen, G10-Schreibvorgänge, G92-Versätze und \
        Werkzeuglängen-Versätze werden hier nicht erzwungen. Prüfe Einrichtung und \
        Soft-Limits vor dem Schneiden.",
    ),
    ("INFORMATIONAL ONLY", "NUR INFORMATIV"),
    (
        "Informational only: START is never blocked. The machine estimate adds the \
        reported active WCS plus G92; TLO is not included. Missing travel or offset \
        data shows NOT CHECKED, never a safety claim. No controller writes.",
        "Nur informativ: START wird nie blockiert. Die Maschinenschätzung addiert das \
        gemeldete aktive WCS plus G92; TLO ist nicht enthalten. Fehlen Verfahrweg- \
        oder Versatzdaten, erscheint NICHT GEPRÜFT, niemals eine Sicherheitsaussage. \
        Keine Schreibvorgänge am Controller.",
    ),
    (
        "Jog or run the test move separately, then enter the commanded distance and the \
        measured travel. The wizard only calculates and writes the steps/mm setting; \
        it never moves an axis.",
        "Führe die Testbewegung separat per Jog oder MDI aus und gib dann die befohlene \
        Strecke und den gemessenen Verfahrweg ein. Der Assistent berechnet und \
        schreibt nur die steps/mm-Einstellung; er bewegt nie eine Achse.",
    ),
    ("MACHINE SETUP", "MASCHINENEINRICHTUNG"),
    ("MATERIAL SURFACING", "MATERIAL PLANFRÄSEN"),
    ("MATERIAL SURFACING WIZARD", "ASSISTENT MATERIAL PLANFRÄSEN"),
    ("MILLIMETRES (MM)", "MILLIMETER (MM)"),
    ("NEW STEPS/MM", "NEUE STEPS/MM"),
    ("NO MOTION COMMANDED", "KEINE BEWEGUNG BEFOHLEN"),
    (
        "NO PROGRAM: load a program before transforming its source",
        "KEIN PROGRAMM: Lade ein Programm, bevor du dessen Quelle transformierst",
    ),
    ("NOT COMPLETED", "NICHT ABGESCHLOSSEN"),
    (
        "No generated program yet. Open the wizard, validate the fields, and preview.",
        "Noch kein erzeugtes Programm. Öffne den Assistenten, prüfe die Felder und \
        zeige die Vorschau.",
    ),
    (
        "No tools in the app table. Add one so the M6 sheet can show a name, diameter, \
        and length instead of only the number.",
        "Keine Werkzeuge in der App-Tabelle. Füge eines hinzu, damit das M6-Fenster \
        Name, Durchmesser und Länge statt nur der Nummer anzeigen kann.",
    ),
    ("OFF omits both M3 and M5", "AUS lässt M3 und M5 weg"),
    ("OLD STEPS/MM", "ALTE STEPS/MM"),
    ("OPEN SETUP", "EINRICHTUNG ÖFFNEN"),
    ("OPEN WIZARD", "ASSISTENT ÖFFNEN"),
    ("PREVIEW", "VORSCHAU"),
    ("PREVIEW FIRST", "ZUERST VORSCHAU"),
    ("PREVIEW SUMMARY", "VORSCHAU-ÜBERSICHT"),
    (
        "PROGRAM BOUNDS (WORK COORDINATES)",
        "PROGRAMMGRENZEN (WERKSTÜCKKOORDINATEN)",
    ),
    (
        "PROGRAM EXCEEDS REPORTED TRAVEL",
        "PROGRAMM ÜBERSCHREITET DEN GEMELDETEN VERFAHRWEG",
    ),
    ("PROGRAM LIMITS", "PROGRAMMGRENZEN"),
    ("PROPOSED STEPS/MM", "VORGESCHLAGENE STEPS/MM"),
    ("REMIND ME LATER", "SPÄTER ERINNERN"),
    (
        "REOPENS AT SAFETY TO REVIEW OR UPDATE THE CHOICES",
        "ÖFFNET BEI SICHERHEIT ERNEUT ZUM PRÜFEN ODER AKTUALISIEREN DER AUSWAHL",
    ),
    ("REPORTED TRAVEL", "GEMELDETER VERFAHRWEG"),
    ("REVIEW & APPLY", "PRÜFEN UND ANWENDEN"),
    ("SAFETY ITEMS ACKNOWLEDGED", "SICHERHEITSPUNKTE BESTÄTIGT"),
    (
        "SETUP IS NOT A MACHINE CERTIFICATION",
        "DIE EINRICHTUNG IST KEINE MASCHINENZERTIFIZIERUNG",
    ),
    ("SOURCE TRANSFORMS", "QUELLTEXT-TRANSFORMATIONEN"),
    ("SPINDLE ON (M3 S... / M5)", "SPINDEL EIN (M3 S... / M5)"),
    ("START ANYWAY", "TROTZDEM STARTEN"),
    (
        "START HERE BEFORE THE FIRST COMMANDED MOVE",
        "HIER VOR DER ERSTEN BEFOHLENEN BEWEGUNG BEGINNEN",
    ),
    ("STEPPER CALIBRATION", "SCHRITTMOTOR-KALIBRIERUNG"),
    ("STRIP COMMENTS", "KOMMENTARE ENTFERNEN"),
    (
        "THIS WIZARD DOES NOT TEST THE MACHINE",
        "DIESER ASSISTENT TESTET DIE MASCHINE NICHT",
    ),
    ("TRANSFORM PREVIEW", "TRANSFORMATIONSVORSCHAU"),
    (
        "The controller, its firmware, and the physical safety wiring remain \
        authoritative. Acknowledge only what you have checked on this machine.",
        "Der Controller, seine Firmware und die physische Sicherheitsverdrahtung \
        bleiben maßgeblich. Bestätige nur, was du an dieser Maschine geprüft hast.",
    ),
    (
        "The loaded program's estimated machine bounds leave the travel reported by the \
        controller. START is held until you confirm.",
        "Die geschätzten Maschinengrenzen des geladenen Programms überschreiten den vom \
        Controller gemeldeten Verfahrweg. START wird zurückgehalten, bis du \
        bestätigst.",
    ),
    (
        "The loaded source is unchanged until LOAD AS PROGRAM. This loads one new \
        program and never starts it.",
        "Die geladene Quelle bleibt bis ALS PROGRAMM LADEN unverändert. Dies lädt ein \
        neues Programm und startet es nie.",
    ),
    (
        "The program is generated from these fields. It is not checked against this \
        machine's travel, offsets, tool, or stock. LOAD AS PROGRAM does not start it.",
        "Das Programm wird aus diesen Feldern erzeugt. Es wird nicht gegen Verfahrweg, \
        Versätze, Werkzeug oder Rohteil dieser Maschine geprüft. ALS PROGRAMM LADEN \
        startet es nicht.",
    ),
    (
        "The wizard never connects, homes, jogs, probes, or writes controller settings.",
        "Der Assistent verbindet nicht, referenziert nicht, joggt nicht, tastet nicht \
        und schreibt keine Controller-Einstellungen.",
    ),
    (
        "The wizard reviews the physical safety checklist, the app display unit \
        preference, and what the first connection should look like. It never connects, \
        homes, jogs, probes, or writes controller settings.",
        "Der Assistent behandelt die physische Sicherheitscheckliste, die \
        Anzeigeeinheit der App und den erwarteten Ablauf der ersten Verbindung. Er \
        verbindet nicht, referenziert nicht, joggt nicht, tastet nicht und schreibt \
        keine Controller-Einstellungen.",
    ),
    (
        "This choice is saved for the next session. The wizard does not send G20 or G21 \
        and does not change the controller's units. The live controller parser state \
        still controls the current session.",
        "Diese Auswahl wird für die nächste Sitzung gespeichert. Der Assistent sendet \
        weder G20 noch G21 und ändert die Einheiten des Controllers nicht. Der \
        Parser-Zustand des aktiven Controllers steuert weiterhin die aktuelle Sitzung.",
    ),
    (
        "This records that you reviewed the checklist and chose an app preference. It \
        does not test, certify, or configure the machine. Verify every motion, limit, \
        and safety device yourself.",
        "Dies hält fest, dass du die Checkliste geprüft und eine App-Einstellung \
        gewählt hast. Es testet, zertifiziert oder konfiguriert die Maschine nicht. \
        Prüfe jede Bewegung, jedes Limit und jede Sicherheitseinrichtung selbst.",
    ),
    (
        "This writes one controller setting. It does not move the machine: the test \
        move is your separate jog or MDI command. The controller stores steps/mm in \
        EEPROM; avoid unnecessary writes.",
        "Dies schreibt eine Controller-Einstellung. Es bewegt die Maschine nicht: Die \
        Testbewegung ist dein separater Jog- oder MDI-Befehl. Der Controller speichert \
        steps/mm im EEPROM; vermeide unnötige Schreibvorgänge.",
    ),
    ("UTILITY FIELD", "DIENSTPROGRAMM-FELD"),
    ("WRITE LOCKED", "SCHREIBEN GESPERRT"),
    ("STOCK WIDTH", "ROHTEILBREITE"),
    ("STOCK DEPTH", "ROHTEILTIEFE"),
    ("CUTTER DIA", "FRÄSER-DM"),
    ("STEPOVER", "SCHRITTWEITE"),
    ("TOTAL DEPTH", "GESAMTTIEFE"),
    ("DEPTH/PASS", "TIEFE/DURCHGANG"),
    ("PLUNGE FEED", "EINTAUCHVORSCHUB"),
    ("LINE START", "STARTZEILE"),
    ("LINE STEP", "ZEILENSCHRITT"),
    ("COMMANDED MOVE", "BEFOHLENE BEWEGUNG"),
    ("MEASURED MOVE", "GEMESSENE BEWEGUNG"),
    ("CALCULATED", "BERECHNET"),
    ("CONFIRMED", "BESTÄTIGT"),
    (
        "ALLOWED: comments `(...)` or `;...`, `G4 P0..60`, `M0`, `M1`, `M5`, `M9`.",
        "ERLAUBT: Kommentare `(...)` oder `;...`, `G4 P0..60`, `M0`, `M1`, `M5`, `M9`.",
    ),
    ("ANGLE", "WINKEL"),
    ("APPLY TLO", "TLO ANWENDEN"),
    (
        "BACKUP EXPORTED THIS SESSION",
        "BACKUP IN DIESER SITZUNG EXPORTIERT",
    ),
    (
        "BACKUP EXPORTED THIS SESSION - RESET ACKNOWLEDGED",
        "BACKUP IN DIESER SITZUNG EXPORTIERT - RESET BESTÄTIGT",
    ),
    ("BACKUP FILE", "BACKUP-DATEI"),
    ("BACKUP REQUIRED", "BACKUP ERFORDERLICH"),
    ("CALCULATOR ONLY", "NUR RECHNER"),
    (
        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY",
        "NUR RECHNER - DIE APP BEWEGT ODER VERSTELLT DEN PORTAL NICHT",
    ),
    ("CLEAR ALL HISTORY", "GESAMTEN VERLAUF LÖSCHEN"),
    ("CLEAR ALL HISTORY?", "GESAMTEN VERLAUF LÖSCHEN?"),
    (
        "CLEAR ASKS FOR CONFIRMATION FIRST",
        "LÖSCHEN VERLANGT ZUERST EINE BESTÄTIGUNG",
    ),
    ("CLEAR HISTORY", "VERLAUF LÖSCHEN"),
    ("CLEAR TLO", "TLO LÖSCHEN"),
    ("CONTROLLER RESET", "CONTROLLER-RESET"),
    (
        "DATA LOSS ACKNOWLEDGED - RESET MAY PROCEED",
        "DATENVERLUST BESTÄTIGT - RESET ERLAUBT",
    ),
    (
        "DATA LOSS ACKNOWLEDGED BY THE OPERATOR",
        "DATENVERLUST VOM BEDIENER BESTÄTIGT",
    ),
    ("DELTA", "DELTA"),
    ("DELTA (TOTAL)", "DELTA (GESAMT)"),
    ("DESTRUCTIVE", "DESTRUKTIV"),
    ("DROP PROGRAM TO LOAD", "PROGRAMM ZUM LADEN ABLEGEN"),
    (
        "Drop exactly one program file",
        "Lege genau eine Programmdatei ab",
    ),
    (
        "Drop one .nc, .gcode, .tap, or .ngc program file",
        "Lege eine .nc-, .gcode-, .tap- oder .ngc-Programmdatei ab",
    ),
    ("EMPTY", "LEER"),
    ("EXPORT BACKUP", "BACKUP EXPORTIEREN"),
    (
        "EXPORT IN FLIGHT - WAIT FOR THE WRITE TO FINISH",
        "EXPORT LÄUFT - WARTE, BIS DAS SCHREIBEN FERTIG IST",
    ),
    (
        "Enter a folder path first",
        "Zuerst einen Ordnerpfad eingeben",
    ),
    (
        "Enter millimetres. The span is the distance between the two measurement points; the \
         readings are the Y values at the left and right points. CALCULATE is local \
         arithmetic.",
        "Millimeter eingeben. Der Abstand ist die Strecke zwischen den beiden Messpunkten; \
         die Messwerte sind die Y-Werte am linken und rechten Punkt. BERECHNEN ist lokale \
         Arithmetik.",
    ),
    (
        "Finished jobs are stored in app.json on this computer. The newest 100 records are \
         kept; the card shows the newest 10.",
        "Beendete Aufträge werden in app.json auf diesem Computer gespeichert. Die neuesten \
         100 Datensätze bleiben erhalten; die Karte zeigt die neuesten 10.",
    ),
    (
        "GANTRY IS SQUARE WITHIN MEASUREMENT RESOLUTION; NO CORRECTION INDICATED",
        "PORTAL IST INNERHALB DER MESSGENAUIGKEIT RECHTWINKLIG; KEINE KORREKTUR ERFORDERLICH",
    ),
    ("GANTRY MEASUREMENT (mm)", "PORTALMESSUNG (mm)"),
    ("GANTRY SQUARING", "PORTALAUSRICHTUNG"),
    ("GANTRY SQUARING CALCULATOR", "RECHNER PORTALAUSRICHTUNG"),
    (
        "I HAVE A BACKUP OR ACCEPT DATA LOSS",
        "ICH HABE EIN BACKUP ODER AKZEPTIERE DATENVERLUST",
    ),
    ("JOB HISTORY", "AUFTRAGSVERLAUF"),
    ("LEFT FORWARD", "LINKS VORAUS"),
    (
        "LEFT SIDE IS {amount} mm FORWARD; MOVE IT BACK {amount} mm OR SHIM THE RIGHT SIDE",
        "DIE LINKE SEITE IST {amount} mm VORAUS; BEWEGE SIE UM {amount} mm ZURÜCK ODER \
         UNTERLEGE DIE RECHTE SEITE",
    ),
    ("LINES", "ZEILEN"),
    (
        "LOCAL ONLY - NOT SENT TO THE CONTROLLER",
        "NUR LOKAL - NICHT AN DEN CONTROLLER GESENDET",
    ),
    (
        "LOCAL ONLY - NOT SENT TO THE CONTROLLER. Records are kept in app.json on this \
         computer, newest first.",
        "NUR LOKAL - NICHT AN DEN CONTROLLER GESENDET. Datensätze liegen in app.json auf \
         diesem Computer, neueste zuerst.",
    ),
    ("LOCAL PROFILE", "LOKALES PROFIL"),
    ("LOCAL TIME", "LOKALE ZEIT"),
    ("MAX", "MAX"),
    ("MEASUREMENT RESULT", "MESSERGEBNIS"),
    ("MOST DESTRUCTIVE", "AM DESTRUKTIVSTEN"),
    ("Max", "Max"),
    (
        "Measure the Y distance from a common reference to the left and right ends of the \
         gantry, enter the span between those points, and calculate the delta, angle, and \
         per-side correction. Follow the machine's squaring procedure for any adjustment.",
        "Miss den Y-Abstand von einer gemeinsamen Referenz zu den linken und rechten Enden \
         des Portals, gib den Abstand zwischen diesen Punkten ein und berechne Delta, Winkel \
         und Korrektur pro Seite. Befolge für jede Einstellung das Ausrichtverfahren der \
         Maschine.",
    ),
    ("NEWEST FIRST", "NEUESTE ZUERST"),
    (
        "NO BACKUP EXPORTED THIS SESSION",
        "IN DIESER SITZUNG KEIN BACKUP EXPORTIERT",
    ),
    ("NO JOBS RECORDED", "KEINE AUFTRÄGE AUFGEZEICHNET"),
    ("NO MOTION / NO WRITE", "KEINE BEWEGUNG / KEIN SCHREIBEN"),
    ("NO PER-SIDE CORRECTION", "KEINE KORREKTUR PRO SEITE"),
    ("NOT SAVED", "NICHT GESPEICHERT"),
    (
        "No measurement yet. Enter the span and both Y readings in the calculator.",
        "Noch keine Messung. Abstand und beide Y-Messwerte im Rechner eingeben.",
    ),
    (
        "No motion, probing, spindle-on, coolant-on, tool change, coordinate, or system \
         commands.",
        "Keine Bewegung, kein Antasten, keine Spindel an, kein Kühlmittel an, kein \
         Werkzeugwechsel, keine Koordinaten- oder Systembefehle.",
    ),
    (
        "No recent files yet. A loaded local file appears here.",
        "Noch keine letzten Dateien. Eine geladene lokale Datei erscheint hier.",
    ),
    ("OF", "VON"),
    ("ONE COMMAND", "EIN BEFEHL"),
    ("OPEN CALCULATOR", "RECHNER ÖFFNEN"),
    ("OPEN HISTORY", "VERLAUF ÖFFNEN"),
    ("OPEN MACROS", "MAKROS ÖFFNEN"),
    (
        "One .nc, .gcode, .tap, or .ngc file. Loading never starts the job.",
        "Eine .nc-, .gcode-, .tap- oder .ngc-Datei. Das Laden startet den Auftrag nie.",
    ),
    ("PER-SIDE", "PRO SEITE"),
    ("PER-SIDE (PIVOT CENTRE)", "PRO SEITE (DREHMITTELPUNKT)"),
    (
        "PIVOT CENTRE: MOVE THE LEFT SIDE BACK {amount} mm AND THE RIGHT SIDE FORWARD \
         {amount} mm",
        "DREHMITTELPUNKT: LINKE SEITE UM {amount} mm ZURÜCK UND RECHTE SEITE UM {amount} mm \
         VOR",
    ),
    (
        "PIVOT CENTRE: MOVE THE RIGHT SIDE BACK {amount} mm AND THE LEFT SIDE FORWARD \
         {amount} mm",
        "DREHMITTELPUNKT: RECHTE SEITE UM {amount} mm ZURÜCK UND LINKE SEITE UM {amount} mm \
         VOR",
    ),
    ("POST", "POST"),
    (
        "POST MACRO - after the final safe-Z retract",
        "POST-MAKRO - nach dem letzten Rückzug auf sichere Z",
    ),
    ("PRE", "PRE"),
    (
        "PRE MACRO - after the safety preamble",
        "PRE-MAKRO - nach der Sicherheitspräambel",
    ),
    ("PREFLIGHT OK", "VORPRÜFUNG OK"),
    ("PROBE DEFAULTS", "TASTER-STANDARDWERTE"),
    ("PROBE MACROS", "TASTER-MAKROS"),
    (
        "Press CALCULATE to validate the fields and show the correction.",
        "BERECHNEN drücken, um die Felder zu prüfen und die Korrektur anzuzeigen.",
    ),
    ("RECENT", "LETZTE"),
    ("RECENT FILES", "LETZTE DATEIEN"),
    ("REFUSED", "ABGELEHNT"),
    ("REMOVE", "ENTFERNEN"),
    (
        "RESET GATES READY - TWO-STEP CONFIRMATION REQUIRED",
        "RESET-BEDINGUNGEN ERFÜLLT - ZWEISTUFIGE BESTÄTIGUNG ERFORDERLICH",
    ),
    ("RESET LOCKED", "RESET GESPERRT"),
    ("RESULT", "ERGEBNIS"),
    ("RIGHT FORWARD", "RECHTS VORAUS"),
    (
        "RIGHT SIDE IS {amount} mm FORWARD; MOVE IT BACK {amount} mm OR SHIM THE LEFT SIDE",
        "DIE RECHTE SEITE IST {amount} mm VORAUS; BEWEGE SIE UM {amount} mm ZURÜCK ODER \
         UNTERLEGE DIE LINKE SEITE",
    ),
    (
        "Recent file was not removed",
        "Letzte Datei wurde nicht entfernt",
    ),
    (
        "Recent files were not cleared",
        "Letzte Dateien wurden nicht gelöscht",
    ),
    ("SAVE DEFAULTS", "STANDARDWERTE SPEICHERN"),
    ("SAVE MACROS", "MAKROS SPEICHERN"),
    ("SAVED", "GESPEICHERT"),
    (
        "SAVED PER CONTROLLER PROFILE",
        "PRO CONTROLLER-PROFIL GESPEICHERT",
    ),
    ("SHOWING", "ANGEZEIGT"),
    ("SPAN BETWEEN POINTS", "ABSTAND ZWISCHEN DEN PUNKTEN"),
    ("SQUARE", "RECHTWINKLIG"),
    ("STEP", "SCHRITT"),
    (
        "STOCK INFERRED FROM PROGRAM BOUNDS | NOT A MEASURED MODEL",
        "ROHTEIL AUS PROGRAMMGRENZEN ABGELEITET | KEIN GEMESSENES MODELL",
    ),
    ("STOCK OFF", "ROHTEIL AUS"),
    ("STOCK ON", "ROHTEIL EIN"),
    ("STORED", "GESPEICHERT"),
    ("TAP FOR", "TIPPEN FÜR"),
    ("THEME", "THEMA"),
    (
        "The controller may reset or re-handshake after this command. The app does not \
         reconnect, home, jog, or resume anything. CANCEL sends nothing.",
        "Der Controller kann nach diesem Befehl zurücksetzen oder erneut aushandeln. Die App \
         verbindet nicht neu, referenziert nicht, fährt nicht und setzt nichts fort. \
         ABBRECHEN sendet nichts.",
    ),
    (
        "There is no APPLY. Move or shim the gantry by hand, then measure again.",
        "Es gibt kein ANWENDEN. Portal von Hand bewegen oder unterlegen, dann erneut messen.",
    ),
    (
        "These lines run inside the generated probe program and are still subject to every \
         preflight gate. SAVE MACROS writes only the local controller profile: no controller \
         command, setting, offset, or start.",
        "Diese Zeilen laufen im erzeugten Tasterprogramm und unterliegen weiterhin jeder \
         Vorprüfung. MAKROS SPEICHERN schreibt nur das lokale Controller-Profil: kein \
         Controller-Befehl, keine Einstellung, kein Offset, kein Start.",
    ),
    (
        "This removes every stored job record from app.json on this computer. It does not \
         change the loaded program or send anything to the controller.",
        "Entfernt jeden gespeicherten Auftragsdatensatz aus app.json auf diesem Computer. Das \
         geladene Programm wird nicht geändert und nichts an den Controller gesendet.",
    ),
    (
        "bounded pre/post lines; local profile; still subject to preflight",
        "begrenzte PRE-/POST-Zeilen; lokales Profil; weiterhin vorprüfungspflichtig",
    ),
    ("bytes and", "Bytes und"),
    ("correction is not finite", "ist nicht endlich"),
    ("drop one program file", "lege nur eine Programmdatei ab"),
    (
        "file contains binary data, not a G-code program",
        "Datei enthält Binärdaten, kein G-code-Programm",
    ),
    ("file is", "Datei ist"),
    ("file is empty", "Datei ist leer"),
    (
        "file is not valid UTF-8 text",
        "Datei ist kein gültiger UTF-8-Text",
    ),
    ("is empty", "ist leer"),
    (
        "is not a .nc, .gcode, .tap, or .ngc program",
        "ist kein .nc-, .gcode-, .tap- oder .ngc-Programm",
    ),
    ("is not a readable folder", "ist kein lesbarer Ordner"),
    ("is not a regular file", "ist keine reguläre Datei"),
    ("is not finite", "ist nicht endlich"),
    ("is out of range", "liegt außerhalb des Bereichs"),
    ("items were dropped", "Elemente wurden abgelegt"),
    ("lines per macro.", "Zeilen pro Makro."),
    (
        "local JSON only; nothing is sent to the controller",
        "nur lokales JSON; nichts wird an den Controller gesendet",
    ),
    (
        "mm | probe tip - controlled point | plan shifts controlled by -offset",
        "mm | Tastspitze - gesteuerter Punkt | Plan verschiebt den gesteuerten Punkt um \
         -offset",
    ),
    ("must be a whole number", "muss eine ganze Zahl sein"),
    ("must be at most", "darf maximal"),
    ("must be finite", "muss endlich sein"),
    ("must be greater than 0", "muss größer als 0 sein"),
    ("must be numeric", "muss eine Zahl sein"),
    ("must be within", "muss liegen zwischen"),
    (
        "the limit for the file browser is",
        "das Limit für den Dateibrowser ist",
    ),
    ("REVIEW", "ÜBERSICHT"),
    (
        "PHYSICAL E-STOP LOCATED AND TESTED",
        "PHYSISCHER NOT-AUS GEFUNDEN UND GEPRÜFT",
    ),
    (
        "SPINDLE CAN BE STOPPED INDEPENDENTLY",
        "SPINDEL KANN UNABHÄNGIG GESTOPPT WERDEN",
    ),
    (
        "MACHINE TRAVEL AND SOFT LIMITS VERIFIED",
        "MASCHINENVERFAHRWEG UND SOFT-LIMITS GEPRÜFT",
    ),
    (
        "WORKHOLDING AND TOOL CLEARANCE CHECKED",
        "WERKSTÜCKSPANNUNG UND WERKZEUGFREIRAUM GEPRÜFT",
    ),
    (
        "SIMULATOR OR SPINDLE-OFF FIRST CONNECTION",
        "ERSTE VERBINDUNG MIT SIMULATOR ODER SPINDEL AUS",
    ),
    (
        "Know where it is, reach it, and prove it stops the machine before any commanded \
         move.",
        "Weiß, wo er ist, erreichst ihn und hast geprüft, dass er die Maschine vor jeder \
         befohlenen Bewegung stoppt.",
    ),
    (
        "Use the VFD or the power control, not this sender, to stop the cutter.",
        "Nutze den VFD oder die Leistungssteuerung, nicht diesen Sender, um den Fräser zu \
         stoppen.",
    ),
    (
        "Confirm the physical travel and the controller's soft-limit settings.",
        "Bestätige den physischen Verfahrweg und die Soft-Limit-Einstellungen des \
         Controllers.",
    ),
    (
        "Secure the stock, clear the tool, and keep hands clear of every axis.",
        "Werkstück sichern, Werkzeug freihalten und Hände von allen Achsen fernhalten.",
    ),
    (
        "Learn the sender in the simulator, then connect with the spindle off and run only \
         status, DRO, and small jogs.",
        "Lerne den Sender im Simulator, verbinde dann mit ausgeschalteter Spindel und führe \
         nur Status, DRO und kleine Jogs aus.",
    ),
    ("STOPPED", "GESTOPPT"),
    ("CONFIRM APPLY TLO", "TLO ANWENDEN BESTÄTIGEN"),
    ("CONFIRM CLEAR TLO", "TLO LÖSCHEN BESTÄTIGEN"),
    ("HIGH CONTRAST", "HOHER KONTRAST"),
    ("LIGHT", "HELL"),
    ("DARK", "DUNKEL"),
    ("LEFT Y READING", "Y-MESSWERT LINKS"),
    ("RIGHT Y READING", "Y-MESSWERT RECHTS"),
    (
        "OFFSETS AND CONTROLLER TOOL DATA",
        "OFFSETS UND CONTROLLER-WERKZEUGDATEN",
    ),
    (
        "CORE SETTING DEFAULTS",
        "STANDARDWERTE DER KERNEINSTELLUNGEN",
    ),
    (
        "DRIVER AND PLUGIN DEFAULTS",
        "TREIBER- UND PLUGIN-STANDARDWERTE",
    ),
    (
        "ALL SETTINGS, PARAMETERS, STARTUP LINES, AND BUILD INFO",
        "ALLE EINSTELLUNGEN, PARAMETER, STARTZEILEN UND BUILD-INFO",
    ),
    (
        "Offsets and controller tool data. Local profile, backups, and saved maps stay.",
        "Offsets und Controller-Werkzeugdaten. Lokales Profil, Backups und gespeicherte \
         Karten bleiben.",
    ),
    (
        "Core setting defaults. Offsets and tool data stay.",
        "Standardwerte der Kerneinstellungen. Offsets und Werkzeugdaten bleiben.",
    ),
    (
        "Driver and plugin defaults. Core settings, offsets, and tool data stay.",
        "Standardwerte für Treiber und Plugins. Kerneinstellungen, Offsets und Werkzeugdaten \
         bleiben.",
    ),
    (
        "All settings, parameters, startup lines, and build info. Most destructive.",
        "Alle Einstellungen, Parameter, Startzeilen und Build-Info. Am destruktivsten.",
    ),
    (
        "Offsets ($#) and controller tool data are reset. Work coordinate systems, G28/G30, \
         G92, TLO, and the controller's tool records may be lost. Local profile data, \
         backups, and saved height maps stay.",
        "Offsets ($#) und Controller-Werkzeugdaten werden zurückgesetzt. \
         Werkstückkoordinatensysteme, G28/G30, G92, TLO und die Werkzeugdatensätze des \
         Controllers können verloren gehen. Lokale Profildaten, Backups und gespeicherte \
         Höhenkarten bleiben.",
    ),
    (
        "Core controller settings return to their defaults. Offsets and controller tool data \
         stay. Export a backup before changing settings again.",
        "Die Kerneinstellungen des Controllers kehren zu ihren Standardwerten zurück. Offsets \
         und Controller-Werkzeugdaten bleiben. Exportiere ein Backup, bevor du Einstellungen \
         erneut änderst.",
    ),
    (
        "Driver and plugin settings return to their defaults. Core settings, offsets, and \
         controller tool data stay.",
        "Treiber- und Plugin-Einstellungen kehren zu ihren Standardwerten zurück. \
         Kerneinstellungen, Offsets und Controller-Werkzeugdaten bleiben.",
    ),
    (
        "ALL settings, parameters, startup lines, and build info are reset. Offsets, tool \
         data, and controller parameters may be unrecoverable. This is the most destructive \
         reset and the controller will usually re-handshake.",
        "ALLE Einstellungen, Parameter, Startzeilen und Build-Info werden zurückgesetzt. \
         Offsets, Werkzeugdaten und Controller-Parameter sind möglicherweise nicht \
         wiederherstellbar. Dies ist der destruktivste Reset; der Controller handelt sich \
         meist neu aus.",
    ),
    (
        "Requested read-only `$#` refresh.",
        "Schreibgeschützte `$#`-Aktualisierung angefordert.",
    ),
    (
        "Requested read-only `$$` settings dump.",
        "Schreibgeschützter `$$`-Einstellungsdump angefordert.",
    ),
    (
        "No read-only report is defined for driver parameters.",
        "Für Treiberparameter ist kein schreibgeschützter Bericht definiert.",
    ),
    (
        "Requested read-only `$#` and `$$` refreshes.",
        "Schreibgeschützte `$#`- und `$$`-Aktualisierungen angefordert.",
    ),
    (
        "MOCK MODE: controller reset needs a live controller",
        "MOCK-MODUS: Controller-Reset benötigt einen aktiven Controller",
    ),
    (
        "NOT LIVE: connect the controller before reset",
        "NICHT AKTIV: Controller vor dem Reset verbinden",
    ),
    (
        "LINK NOT READY: reset is disabled until the handshake completes",
        "VERBINDUNG NICHT BEREIT: Reset ist bis zum Abschluss des Handshakes deaktiviert",
    ),
    (
        "ALARM OR DOOR: clear the alarm or door state before reset",
        "ALARM ODER TÜR: Alarm- oder Türzustand vor dem Reset quittieren",
    ),
    (
        "JOB ACTIVE: stop or finish the job before reset",
        "AUFTRAG AKTIV: Auftrag vor dem Reset stoppen oder beenden",
    ),
    (
        "TOOL CHANGE PENDING: finish or stop the tool change before reset",
        "WERKZEUGWECHSEL AUSSTEHEND: Werkzeugwechsel vor dem Reset beenden oder stoppen",
    ),
    (
        "PROBE ACTIVE: finish or abort the probe run before reset",
        "TASTVORGANG AKTIV: Tastlauf vor dem Reset beenden oder abbrechen",
    ),
    (
        "HEIGHT MAP BUSY: finish the Apply or Revert before reset",
        "HÖHENKARTE BESCHÄFTIGT: Anwenden oder Zurücksetzen vor dem Reset abschließen",
    ),
    (
        "PENDANT ACTIVE (MPG:1): release the pendant before reset",
        "HANDRAD AKTIV (MPG:1): Handrad vor dem Reset freigeben",
    ),
    (
        "SETTINGS WRITE PENDING: finish or discard it before reset",
        "EINSTELLUNGSSCHREIBUNG AUSSTEHEND: vor dem Reset abschließen oder verwerfen",
    ),
    (
        "OFFSET WRITE PENDING: finish the write and refresh before reset",
        "OFFSET-SCHREIBUNG AUSSTEHEND: Schreiben abschließen und vor dem Reset aktualisieren",
    ),
    (
        "NOT IDLE: reset is available only while the controller reports Idle",
        "NICHT LEERLAUF: Reset ist nur verfügbar, während der Controller Leerlauf meldet",
    ),
    (
        "BACKUP EXPORT IN FLIGHT: wait for the export to finish",
        "BACKUP-EXPORT LÄUFT: warte, bis der Export fertig ist",
    ),
    (
        "BACKUP NOT ACKNOWLEDGED: export a backup or accept data loss first",
        "BACKUP NICHT BESTÄTIGT: zuerst ein Backup exportieren oder Datenverlust akzeptieren",
    ),
    (
        "LOCAL PROFILE IS NOT CHANGED AUTOMATICALLY",
        "DAS LOKALE PROFIL WIRD NICHT AUTOMATISCH GEÄNDERT",
    ),
    (
        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY. Follow the machine's \
         squaring procedure, move or shim the gantry by hand, and verify the result with a \
         fresh measurement before the next cut.",
        "NUR RECHNER - DIE APP BEWEGT ODER VERSTELLT DAS PORTAL NICHT. Befolge das \
         Ausrichtverfahren der Maschine, bewege oder unterlege das Portal von Hand und prüfe \
         das Ergebnis mit einer neuen Messung vor dem nächsten Schnitt.",
    ),
    (
        "Positive delta and angle: the right measurement point is further along +Y (forward) \
         than the left. Negative: the left point is further along +Y. Per-side is delta / 2: \
         with the centre fixed, each end moves half the delta in opposite directions.",
        "Positives Delta und positiver Winkel: Der rechte Messpunkt liegt weiter in +Y \
         (vorne) als der linke. Negativ: Der linke Punkt liegt weiter in +Y. Pro Seite ist \
         Delta / 2: Bei festem Mittelpunkt bewegt sich jedes Ende um die halbe Strecke in \
         entgegengesetzte Richtungen.",
    ),
    ("SENT", "GESENDET"),
    (
        "NO PROGRAM: load a program before rotating its source",
        "KEIN PROGRAMM: Lade ein Programm, bevor du dessen Quelle drehst",
    ),
    (
        "NO PROGRAM: load a program before scaling its source",
        "KEIN PROGRAMM: Lade ein Programm, bevor du dessen Quelle skalierst",
    ),
    ("NO WARNINGS", "KEINE WARNUNGEN"),
    (
        "No program loaded: the source check has nothing to scan.",
        "Kein Programm geladen: Die Quellprüfung hat nichts zu prüfen.",
    ),
    ("PREVIEW ROTATION", "VORSCHAU DREHEN"),
    ("PREVIEW SCALE", "VORSCHAU SKALIEREN"),
    ("PROGRAM CHECK", "PROGRAMMPRÜFUNG"),
    ("REVIEW PREVIEW", "VORSCHAU PRÜFEN"),
    ("ROTATE PROGRAM", "PROGRAMM DREHEN"),
    ("ROTATION PREVIEW", "DREHVORSCHAU"),
    ("SCALE PREVIEW", "SKALIERUNGSVORSCHAU"),
    ("SCALE PROGRAM", "PROGRAMM SKALIEREN"),
    ("UNIFORM / WORK PIVOT", "GLEICHMÄSSIG / DREHPUNKT WERKSTÜCK"),
    ("USE PROBE ANGLE", "TASTWINKEL VERWENDEN"),
    ("WARNINGS", "WARNUNGEN"),
    ("WORK PIVOT / CCW +", "DREHPUNKT WERKSTÜCK / LINKS +"),
    (
        "I understand this loads the rotated source and does not start it",
        "Mir ist klar, dass dies die gedrehte Quelle lädt und sie nicht startet",
    ),
    (
        "I understand this loads the scaled source and does not start it",
        "Mir ist klar, dass dies die skalierte Quelle lädt und sie nicht startet",
    ),
    (
        "INFORMATIONAL - START IS NOT BLOCKED. NOT A MACHINE/TRAVEL CHECK.",
        "INFORMATIV - START IST NICHT BLOCKIERT. KEINE MASCHINEN-/VERFAHRWEGSPRÜFUNG.",
    ),
    (
        "Missing app tool-table entries are warnings, not blockers. The check reads source text \
         and parsed counts only; it cannot verify controller state, offsets, tool length, \
         fixtures, or actual travel. No controller command is sent.",
        "Fehlende Einträge in der App-Werkzeugtabelle sind Warnungen, keine Sperren. Die Prüfung \
         liest nur Quelltext und geparste Zählungen; sie kann Controllerstatus, Offsets, \
         Werkzeuglänge, Vorrichtungen oder den tatsächlichen Verfahrweg nicht verifizieren. Es \
         wird kein Controllerbefehl gesendet.",
    ),
    (
        "No feed, spindle, tool, machine-coordinate, or probe warnings in this program.",
        "Keine Warnungen zu Vorschub, Spindel, Werkzeug, Maschinenkoordinaten oder Taster in \
         diesem Programm.",
    ),
    (
        "Pivot is in work/program coordinates; a positive angle is counter-clockwise. Unsupported \
         programs are refused before any load.",
        "Der Drehpunkt liegt in Werkstück-/Programmkoordinaten; ein positiver Winkel ist gegen den \
         Uhrzeigersinn. Nicht unterstützte Programme werden vor dem Laden abgelehnt.",
    ),
    (
        "Rotation is reported only. USE PROBE ANGLE fills the Utility angle with -deviation; the \
         pivot stays in work coordinates and nothing is loaded or sent.",
        "Die Drehung wird nur gemeldet. TASTWINKEL VERWENDEN trägt -Abweichung in den Winkel unter \
         Dienstprogramme ein; der Drehpunkt bleibt in Werkstückkoordinaten und es wird nichts \
         geladen oder gesendet.",
    ),
    (
        "SOURCE SCAN WAS TRUNCATED: the check did not read every line.",
        "QUELLTEXT-PRÜFUNG ABGESCHNITTEN: Die Prüfung hat nicht jede Zeile gelesen.",
    ),
    (
        "Uniform positive factor about a pivot in work/program millimetres. Feeds, speeds, and Z \
         depths are unchanged. Unsupported programs are refused before any load.",
        "Gleichmäßiger positiver Faktor um einen Drehpunkt in Werkstück-/Programmmillimetern. \
         Vorschübe, Drehzahlen und Z-Tiefen bleiben unverändert. Nicht unterstützte Programme \
         werden vor dem Laden abgelehnt.",
    ),
];
