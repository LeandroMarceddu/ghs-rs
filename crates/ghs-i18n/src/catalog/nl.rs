//! nl catalog.
//!
//! Keys are the exact English UI literals; lookups are case-sensitive.

pub(crate) static CATALOG: &[(&str, &str)] = &[
    ("Job", "Taak"),
    ("Probe", "Taster"),
    ("Offset", "Nulpunten"),
    ("Utility", "Hulpprogramma's"),
    ("Tools", "Gereedschappen"),
    ("Settings", "Instellingen"),
    ("JOB", "TAAK"),
    ("PROBING", "TASTEN"),
    ("WORK OFFSETS", "WERKSTUKNULPUNTEN"),
    ("UTILITIES", "HULPPROGRAMMA'S"),
    ("TOOL TABLE", "GEREEDSCHAPSTABEL"),
    ("SETTINGS", "INSTELLINGEN"),
    ("Console", "Console"),
    ("Program", "Programma"),
    ("Light", "Licht"),
    ("Apply", "Toepassen"),
    ("APPLY", "TOEPASSEN"),
    ("Save", "Opslaan"),
    ("SAVE", "OPSLAAN"),
    ("Cancel", "Annuleren"),
    ("CANCEL", "ANNULEREN"),
    ("Close", "Sluiten"),
    ("CLOSE", "SLUITEN"),
    ("Edit", "Bewerken"),
    ("EDIT", "BEWERKEN"),
    ("Delete", "Verwijderen"),
    ("DEL", "VERW."),
    ("Refresh", "Vernieuwen"),
    ("REFRESH", "VERNIEUWEN"),
    ("Export", "Exporteren"),
    ("EXPORT", "EXPORTEREN"),
    ("Import", "Importeren"),
    ("IMPORT", "IMPORTEREN"),
    ("Validate", "Valideren"),
    ("VALIDATE", "VALIDEREN"),
    ("Confirm", "Bevestigen"),
    ("CONFIRM", "BEVESTIGEN"),
    ("Keep", "Behouden"),
    ("KEEP", "BEHOUDEN"),
    ("Revert", "Herstellen"),
    ("REVERT", "HERSTELLEN"),
    ("Zero", "Nul zetten"),
    ("ZERO", "NUL ZETTEN"),
    ("Home", "Referentie"),
    ("HOME", "REFERENTIE"),
    ("Unlock", "Ontgrendelen"),
    ("UNLOCK", "ONTGRENDELEN"),
    ("Macro", "Macro"),
    ("Startup Lines", "Opstartregels"),
    ("STARTUP LINES", "OPSTARTREGELS"),
    ("Keymap", "Toetsenindeling"),
    ("KEYMAP", "TOETSENINDELING"),
    ("Diagnostics", "Diagnostiek"),
    ("DIAGNOSTICS", "DIAGNOSTIEK"),
    ("RESET", "RESET"),
    ("STOP", "STOP"),
    ("Stop", "Stoppen"),
    ("SLEEP", "SLAAPSTAND"),
    ("CONNECT", "VERBINDEN"),
    ("DISCONNECT", "VERBREKEN"),
    ("CONNECTION", "VERBINDING"),
    ("TRANSPORT", "TRANSPORT"),
    ("CURRENT LINK", "HUIDIGE VERBINDING"),
    ("SERIAL PORT", "SERIËLE POORT"),
    ("Demo", "Demo"),
    ("Serial port", "Seriële poort"),
    ("No serial ports found", "Geen seriële poorten gevonden"),
    (
        "Disconnect before switching transport.",
        "Verbinding verbreken voordat u van transport wisselt.",
    ),
    ("MOCK", "SIMULATIE"),
    ("CONNECTING", "VERBINDEN"),
    ("READY", "GEREED"),
    ("DISCONNECTED", "VERBROKEN"),
    ("ERROR", "FOUT"),
    ("NOT CONNECTED", "NIET VERBONDEN"),
    ("STALE", "VEROUDERD"),
    ("Alarm", "Alarm"),
    ("Door", "Deur"),
    ("LOAD PROGRAM", "PROGRAMMA LADEN"),
    ("START", "START"),
    ("PAUSE", "PAUZE"),
    ("RESUME", "HERVATTEN"),
    ("RUN OPTIONS", "UITVOERINGSOPTIES"),
    ("RAPID", "SNEL"),
    ("Rapid", "Snel"),
    ("ETA", "RESTTIJD"),
    ("TIME", "TIJD"),
    ("TOOL CHANGE REQUIRED", "GEREEDSCHAPSWISSEL VEREIST"),
    ("TOOL CHANGE", "GEREEDSCHAPSWISSEL"),
    ("RUN TO", "UITVOEREN TOT"),
    ("SET BREAKPOINT", "BREEKPUNT INSTELLEN"),
    ("BP", "BP"),
    ("SEARCH", "ZOEKEN"),
    ("FILTER", "FILTER"),
    ("HIGHLIGHT", "MARKEREN"),
    ("NEXT", "VOLGENDE"),
    ("PREV", "VORIGE"),
    ("NO PROGRAM LOADED", "GEEN PROGRAMMA GELADEN"),
    ("NO PROGRAM", "GEEN PROGRAMMA"),
    ("PROGRAM", "PROGRAMMA"),
    ("HEIGHT MAP / COMPENSATED", "HOOGTEKAART / GECOMPENSEERD"),
    ("Cut", "Snijden"),
    ("Plunge", "Inzetten"),
    ("IDLE", "INACTIEF"),
    ("RUNNING", "LOPEND"),
    ("PAUSED", "GEPAUZEERD"),
    ("COMPLETED", "VOLTOOID"),
    ("ABORTED", "AFGEBROKEN"),
    ("INTERRUPTED", "ONDERBROKEN"),
    ("JOB ACTIVE", "TAAK ACTIEF"),
    ("JOB RUNNING", "TAAK LOPEND"),
    ("STOP JOB", "TAAK STOPPEN"),
    ("TAP A LINE TO SELECT", "TIK OP EEN REGEL OM TE SELECTEREN"),
    ("No matching lines", "Geen overeenkomende regels"),
    ("NO MATCH", "GEEN RESULTAAT"),
    ("SAVE LOG", "LOG OPSLAAN"),
    ("DRO", "COÖRDINATEN"),
    ("JOG", "JOG"),
    ("MDI", "MDI"),
    ("FEED OVERRIDE", "VOEDINGSSNELHEID-OVERRIDE"),
    ("SPINDLE OVERRIDE", "SPINDEL-OVERRIDE"),
    ("RAPID OVERRIDE", "SNELVERPLAATSING-OVERRIDE"),
    ("COOLANT", "KOELMIDDEL"),
    ("SPINDLE", "SPINDEL"),
    ("SIGNALS", "SIGNALEN"),
    ("WORK", "WERKSTUK"),
    ("SEND", "VERZENDEN"),
    ("UP", "OMHOOG"),
    ("DN", "OMLAAG"),
    ("CONT", "DOORLOPEND"),
    ("CW", "RECHTSOM"),
    ("CCW", "LINKSOM"),
    ("Spindle off", "Spindel uit"),
    ("RESET 100%", "100% HERSTELLEN"),
    ("Feed", "Voedingssnelheid"),
    ("MACHINE", "MACHINE"),
    ("TOOL", "GEREEDSCHAP"),
    ("POCKET", "TAS"),
    ("RADIUS", "STRAAL"),
    ("TOOLPATH", "GEREEDSCHAPSBAN"),
    ("CYCLES", "CYCLI"),
    ("CYCLE PARAMETERS", "CYCLUSPARAMETERS"),
    ("GRID SUMMARY", "RASTEROVERZICHT"),
    ("PREFLIGHT", "VOORCONTROLE"),
    ("PROBE RUN", "TASTMETING"),
    ("RAW PRB (machine mm)", "RUWE PRB (machine-mm)"),
    ("SAVED MAPS", "OPGESLAGEN KAARTEN"),
    ("SAVED MAP", "OPGESLAGEN KAART"),
    ("MEASUREMENT FRAME", "MEETKADER"),
    ("HEIGHT MAP RESULT", "RESULTAAT HOOGTEKAART"),
    ("CONFIRM APPLY", "TOEPASSEN BEVESTIGEN"),
    ("PROBE OFFSET X/Y", "TASTER-OFFSET X/Y"),
    ("PROBE OFFSET", "TASTER-OFFSET"),
    (
        "SEARCH BOUND (CONTROLLED-POINT XY)",
        "ZOEKGRENS (BESTURINGSPUNT XY)",
    ),
    ("APPLY HEIGHT MAP", "HOOGTEKAART TOEPASSEN"),
    ("CLOSE MAP", "KAART SLUITEN"),
    ("SAVE MAP", "KAART OPSLAAN"),
    ("EXPORT CSV", "CSV EXPORTEREN"),
    ("RETRY PREFLIGHT", "VOORCONTROLE OPNIEUW"),
    ("CONFIGURE", "CONFIGUREREN"),
    ("Probe connected", "Taster verbonden"),
    ("ABORT", "AFBREKEN"),
    ("Height map", "Hoogtekaart"),
    ("Z touch plate", "Z-tastplaat"),
    ("XY touch plate", "XY-tastplaat"),
    ("Edge X", "Rand X"),
    ("Edge Y", "Rand Y"),
    ("Corner", "Hoek"),
    ("Center circle", "Cirkelcentrum"),
    ("Center rectangle", "Rechthoekcentrum"),
    ("Rotation", "Rotatie"),
    ("Tool length", "Gereedschaplengte"),
    ("MEASURED Z", "GEMETEN Z"),
    ("SAFE Z", "VEILIGE Z"),
    ("SURFACE Z", "OPPERVLAKTE-Z"),
    ("MAP Z OFFSET", "KAART-Z-OFFSET"),
    ("MAP TRAVEL", "KAARTWEG"),
    ("GRID ORIGIN", "RASTERORSPRONG"),
    ("COLUMNS", "KOLOMMEN"),
    ("ROWS", "RIJEN"),
    ("TOUCHES", "TASTPUNTEN"),
    ("STEP X", "STAP X"),
    ("STEP Y", "STAP Y"),
    ("EDGE OFFSET", "RANDOFFSET"),
    ("SEPARATION", "AFSTAND"),
    ("PLATE THICK", "PLAATDIKTE"),
    ("PLATE CORNER X", "PLAATHOEK X"),
    ("PLATE CORNER Y", "PLAATHOEK Y"),
    ("PROBE HEIGHT", "TASTERHOOGTE"),
    ("PROBE DIA", "TASTER-DIAM."),
    ("REFERENCE Z", "REFERENTIE-Z"),
    ("RECT WIDTH", "RECHTHOEKBREEDTE"),
    ("RECT HEIGHT", "RECHTHOEKHOOGTE"),
    ("EXPECTED X", "VERWACHT X"),
    ("EXPECTED Y", "VERWACHT Y"),
    ("EXPECTED Z", "VERWACHT Z"),
    ("Measured height", "Gemeten hoogte"),
    ("Measured width", "Gemeten breedte"),
    ("Measured X edge", "Gemeten X-rand"),
    ("Measured Y edge", "Gemeten Y-rand"),
    ("SURFACE", "OPPERVLAK"),
    ("COMPENSATION", "COMPENSATIE"),
    ("PLAN", "PLAN"),
    ("MAP", "KAART"),
    ("SAFETY", "VEILIGHEID"),
    ("PROBE VERIFIED", "TASTER GEVERIFIEERD"),
    ("NOT CONFIRMED", "NIET BEVESTIGD"),
    ("TRIGGERED", "GEACTIVEERD"),
    ("NOT READY", "NIET GEREED"),
    ("UNITS", "EENHEDEN"),
    ("TOOL LENGTH DELTA", "GEREEDSCHAPSLENGTE-DELTA"),
    ("RMS fit error", "RMS-fitfout"),
    ("Deviation", "Afwijking"),
    ("Nominal", "Nominaal"),
    ("WORK XYZ", "WERKSTUK XYZ"),
    ("MACHINE XYZ", "MACHINE XYZ"),
    ("SEARCH SETTINGS", "INSTELLINGEN ZOEKEN"),
    ("All settings", "Alle instellingen"),
    ("General", "Algemeen"),
    ("Axes", "Assen"),
    ("Limits", "Limieten"),
    ("Network address", "Netwerkadres"),
    ("Work mode", "Werkmodus"),
    ("Step pulse time", "Stapimpulstijd"),
    ("Step idle delay", "Wachttijd stapmotors"),
    ("Maximum spindle speed", "Maximale spindelsnelheid"),
    ("Homing cycle enable", "Referentiecyclus inschakelen"),
    ("Soft limits enable", "Softlimieten inschakelen"),
    ("Status report mask", "Statusrapportmasker"),
    ("Stepper", "Stapmotor"),
    ("X steps per millimetre", "X-stappen per millimeter"),
    ("APPLIED", "TOEGEPAST"),
    ("PENDING", "IN AFWACHTING"),
    ("REBOOT REQUIRED", "HERSTART VEREIST"),
    ("INVALID", "ONGELDIG"),
    ("FORMAT", "FORMAAT"),
    ("WRITING", "SCHRIJVEN"),
    ("Search results", "Zoekresultaten"),
    ("out of range", "buiten bereik"),
    ("WCS OFFSETS", "WCS-NULPUNTEN"),
    ("WCS OFFSET", "WCS-OFFSET"),
    ("CLEAR G92", "G92 WISSEN"),
    ("G28 STORED POSITION", "G28 OPGESLAGEN POSITIE"),
    ("G30 STORED POSITION", "G30 OPGESLAGEN POSITIE"),
    ("GOTO G28", "GA NAAR G28"),
    ("GOTO G30", "GA NAAR G30"),
    ("REPORTED", "GERAPPORTEERD"),
    ("NOT REPORTED", "NIET GERAPPORTEERD"),
    ("NON-PERSISTENT", "NIET-PERSISTENT"),
    ("ZERO ALL", "ALLES NUL ZETTEN"),
    ("ZERO X", "X NUL ZETTEN"),
    ("ZERO Y", "Y NUL ZETTEN"),
    ("ZERO Z", "Z NUL ZETTEN"),
    ("CONFIRM CLEAR G92", "G92 WISSEN BEVESTIGEN"),
    ("CONFIRM G28 GOTO", "G28-BEWEGING BEVESTIGEN"),
    ("CONFIRM G30 GOTO", "G30-BEWEGING BEVESTIGEN"),
    ("EXPORT OFFSETS", "NULPUNTEN EXPORTEREN"),
    ("LOAD OFFSETS FILE", "NULPUNTENBESTAND LADEN"),
    ("REFRESH $#", "$# VERNIEUWEN"),
    ("OFFSETS", "NULPUNTEN"),
    (
        "PREDEFINED POSITIONS / G92",
        "VOORGEDEFINIEERDE POSITIES / G92",
    ),
    ("OLD G92 OFFSET", "OUD G92-OFFSET"),
    ("CONTROLLER TOOLS", "BESTURINGSGEREEDSCHAPPEN"),
    ("NEW TOOL", "NIEUW GEREEDSCHAP"),
    ("NEW RECORD", "NIEUW RECORD"),
    ("SAVE TOOL", "GEREEDSCHAP OPSLAAN"),
    ("TOOL NUMBER", "GEREEDSCHAPSNUMMER"),
    ("TOOL NAME", "GEREEDSCHAPSNAAM"),
    ("TOOL DIAMETER (MM)", "GEREEDSCHAPSDIAMETER (MM)"),
    ("TOOL LENGTH (MM)", "GEREEDSCHAPSLENGTE (MM)"),
    ("POCKET NUMBER", "TASNUMMER"),
    ("DIAMETER (MM)", "DIAMETER (MM)"),
    ("LENGTH (MM)", "LENGTE (MM)"),
    ("APP TOOL", "APP-GEREEDSCHAP"),
    ("Profiles", "Profielen"),
    ("BACKUPS", "BACK-UPS"),
    ("PROFILE", "PROFIEL"),
    ("PROFILE RECORD", "PROFIELRECORD"),
    ("CONTROLLER PROFILE", "BESTURINGSPROFIEL"),
    ("SAVE PROFILE", "PROFIEL OPSLAAN"),
    ("SAVE APP", "APP OPSLAAN"),
    ("USE LOCAL DATA", "LOKALE GEGEVENS GEBRUIKEN"),
    ("BACKUP BUNDLE", "BACK-UPBUNDEL"),
    ("VALIDATED FILE", "GEVALIDEERD BESTAND"),
    ("LOCAL ONLY", "ALLEEN LOKAAL"),
    ("NO CONTROLLER", "GEEN BESTURING"),
    ("UNVERIFIED", "NIET GEVERIFIEERD"),
    ("PENDING CONFIRMATION", "IN AFWACHTING VAN BEVESTIGING"),
    ("CORRUPT", "BESCHADIGD"),
    ("MISSING", "ONTBREEKT"),
    ("VALID", "GELDIG"),
    ("MACROS", "MACRO'S"),
    ("NEW MACRO", "NIEUWE MACRO"),
    ("SAVE MACRO", "MACRO OPSLAAN"),
    ("RUN MACRO", "MACRO UITVOEREN"),
    ("MACRO NAME", "MACRONAAM"),
    ("MACRO COMMAND", "MACROCOMMANDO"),
    ("RESET DEFAULTS", "STANDAARDWAARDEN HERSTELLEN"),
    ("USE KEY", "TOETS GEBRUIKEN"),
    ("PRESS A KEY...", "DRUK OP EEN TOETS..."),
    ("PRESS A KEY COMBINATION", "DRUK OP EEN TOETSCOMBINATIE"),
    ("RECORD KEY", "TOETS VASTLEGGEN"),
    ("CONTROLLER DIAGNOSTICS", "BESTURINGSDIAGNOSTIEK"),
    ("OPEN PINS / LIMITS", "PINS / LIMIETEN OPENEN"),
    ("PINS / LIMITS", "PINS / LIMIETEN"),
    ("PIN MAP ($PINS)", "PINBELEGING ($PINS)"),
    ("PIN STATE ($PINSTATE)", "PINSTATUS ($PINSTATE)"),
    ("LAST EVENTS ($LEV)", "LAATSTE GEBEURTENISSEN ($LEV)"),
    ("CONTROLLER SLEEP", "BESTURING SLAAPSTAND"),
    ("PUT CONTROLLER TO SLEEP", "BESTURING IN SLAAPSTAND ZETTEN"),
    ("CONFIRM SLEEP", "SLAAPSTAND BEVESTIGEN"),
    ("DELETE MACRO", "MACRO VERWIJDEREN"),
    ("CONFIRM WRITE", "SCHRIJVEN BEVESTIGEN"),
    ("CONFIRM OFFSET WRITE", "OFFSET SCHRIJVEN BEVESTIGEN"),
    ("CONFIRM REQUIRED", "BEVESTIGING VEREIST"),
    ("CONFIRM IMPORT", "IMPORT BEVESTIGEN"),
    ("ACKNOWLEDGED", "BEVESTIGD"),
    ("1. ACKNOWLEDGE", "1. BEVESTIGEN"),
    ("2. RESUME", "2. HERVATTEN"),
    ("OPEN", "OPENEN"),
    ("FOLDER", "MAP"),
    ("PROGRAMS", "PROGRAMMA'S"),
    ("No file selected", "Geen bestand geselecteerd"),
    (
        "Select a program file first",
        "Selecteer eerst een programmabestand",
    ),
    ("FILE", "BESTAND"),
    ("PATH", "PAD"),
    ("NAME", "NAAM"),
    ("NEW", "NIEUW"),
    ("SET", "INSTELLEN"),
    ("GO", "GA"),
    ("CLEAR", "WISSEN"),
    ("ALL", "ALLE"),
    ("SELECT", "SELECTEREN"),
    ("SELECTED", "GESELECTEERD"),
    ("RELOAD", "HERLADEN"),
    ("RUN", "UITVOEREN"),
    ("ON", "AAN"),
    ("OFF", "UIT"),
    ("IMPORTED", "GEÏMPORTEERD"),
    ("EXPORTED", "GEEXPORTEERD"),
    ("FAILED", "MISLUKT"),
    ("COMPLETE", "VOLTOOID"),
    ("PARTIAL", "GEDEELTELIJK"),
    ("ACTIVE", "ACTIEF"),
    ("CURRENT", "HUIDIG"),
    ("SETUP", "CONFIGURATIE"),
    ("VIEW", "WEERGAVE"),
    ("FIT", "PASSEND"),
    ("CHECK", "CONTROLE"),
    ("CHECKING", "CONTROLE BEZIG"),
    ("CHECK COMPLETE", "CONTROLE VOLTOOID"),
    ("CHECK STOPPED", "CONTROLE GESTOPT"),
    (
        "CHECK PROGRAM (DRY RUN)",
        "PROGRAMMA CONTROLEREN (DROOGLOOP)",
    ),
    ("START CHECK", "CONTROLE STARTEN"),
    ("DRY RUN - NO MOTION", "DROOGLOOP - GEEN BEWEGING"),
    (
        "WAITING FOR CONTROLLER CHECK MODE ($C)",
        "WACHT OP CONTROLEMODUS ($C)",
    ),
    ("WHAT THE DRY RUN DOES", "WAT DE DROOGLOOP DOET"),
    ("MOTION IS SUPPRESSED", "BEWEGING ONDERDRUKT"),
    ("SPINDLE / COOLANT", "SPINDEL / KOELMIDDEL"),
    ("WHEN FINISHED", "BIJ VOLTOOIING"),
    (
        "The controller parses and streams the loaded program with check mode ($C) enabled. \
        Nothing in this sheet starts a real job or moves an axis.",
        "De controller leest en streamt het geladen programma met controlemode ($C) \
        ingeschakeld. Niets in dit venster start een echte taak of beweegt een as.",
    ),
    (
        "MOTION IS SUPPRESSED: the controller acknowledges program lines without moving the \
        machine.",
        "BEWEGING ONDERDRUKT: de controller bevestigt programmaregels zonder de machine te \
        bewegen.",
    ),
    (
        "SPINDLE / COOLANT: M3, M4, M5, M7, M8 and M9 are parsed and acknowledged but never \
        run as real outputs.",
        "SPINDEL / KOELMIDDEL: M3, M4, M5, M7, M8 en M9 worden gelezen en bevestigd, maar \
        nooit als echte uitgangen geschakeld.",
    ),
    (
        "WHEN FINISHED: the engine leaves check mode and waits for the machine to report \
        IDLE. The program stays loaded for a separate real START.",
        "BIJ VOLTOOIING: de engine verlaat de controlemode en wacht tot de machine IDLE \
        meldt. Het programma blijft geladen voor een aparte echte START.",
    ),
    (
        "STOP remains available while the check streams; it never resumes or restarts the \
        run.",
        "STOP blijft beschikbaar tijdens de controle; het hervat of herstart de run nooit.",
    ),
    ("CANCEL sends nothing.", "ANNULEREN verstuurt niets."),
    (
        "Parse and stream the loaded program in check mode ($C); no motion.",
        "Leest en streamt het geladen programma in controlemode ($C); geen beweging.",
    ),
    ("LOADED", "GELADEN"),
    ("LANGUAGE", "TAAL"),
    ("FOLDERS", "MAPPEN"),
    ("LIVE", "LIVE"),
    ("JUMP", "SPRING"),
    ("DEFAULTS", "STANDAARD"),
    ("RUN AGAIN", "OPNIEUW STARTEN"),
    ("APPLY TEXT", "TEKST TOEPASSEN"),
    ("KEEP DRAFT", "CONCEPT BEHOUDEN"),
    ("DIRECT", "DIRECT"),
    ("SYNTHETIC", "SYNTHETISCH"),
    ("TOUCH KEYPAD", "TOUCHTOETSENBORD"),
    ("NO LINE SELECTED", "GEEN REGEL GESELECTEERD"),
    ("REPORTS REQUESTED", "RAPPORTEN AANGEVRAAGD"),
    ("OFFLINE", "OFFLINE"),
    ("UNSAVED", "NIET OPGESLAGEN"),
    ("USER-DEFINED", "DOOR GEBRUIKER"),
    ("READ-ONLY", "ALLEEN LEZEN"),
    ("NOT LOADED", "NIET GELADEN"),
    (
        "Choose a file for the job; loading never starts it.",
        "Kies een bestand voor de taak; laden start het nooit.",
    ),
    (
        "No folders or program files here",
        "Geen mappen of programmabestanden hier",
    ),
    (
        "Tap UP or type another folder path.",
        "Tik OP of typ een ander mappad.",
    ),
    (
        "48 px rows | folders open on tap | hidden files and symlinks are not shown | .nc \
        .gcode .tap .ngc | 20 MB limit",
        "Rijen van 48 px | mappen openen bij tik | verborgen bestanden en symlinks niet \
        getoond | .nc .gcode .tap .ngc | limiet 20 MB",
    ),
    (
        "Tap a program file to select it",
        "Tik op een programmabestand om het te selecteren",
    ),
    ("READY TO LOAD", "KLAAR OM TE LADEN"),
    ("MOCK / BROWSE ONLY", "SIMULATIE / ALLEEN BLADEREN"),
    ("LINK NOT READY", "VERBINDING NIET GEREED"),
    ("LIVE / READY", "LIVE / GEREED"),
    ("DIR", "MAP"),
    ("OPTIONAL STOP", "OPTIONELE STOP"),
    ("SINGLE BLOCK", "ENKEL BLOK"),
    ("BLOCK DELETE", "BLOK VERWIJDEREN"),
    (
        "M1 optional stops are honored",
        "Optionele stops M1 worden gerespecteerd",
    ),
    ("Hold after each block", "Pauze na elk blok"),
    ("Skip blocks marked with /", "Blokken met / overslaan"),
    (
        "Each tap sends one command. A row keeps showing the value the controller reports; \
        it changes on the next poll, not when the button is pressed.",
        "Elke tik verstuurt een commando. Een rij blijft de door de controller gemelde \
        waarde tonen; die verandert bij de volgende poll, niet bij de knopdruk.",
    ),
    ("Mock mode: no controller", "Mockmodus: geen controller"),
    ("Link not ready", "Verbinding niet gereed"),
    (
        "Locked while a job runs or pauses",
        "Vergrendeld tijdens taak of pauze",
    ),
    (
        "MOCK MODE: run options need a live controller",
        "MOCKMODUS: uitvoeropties vereisen een actieve controller",
    ),
    (
        "LINK NOT READY: run options are disabled until the handshake completes",
        "VERBINDING NIET GEREED: uitvoeropties zijn uitgeschakeld tot de handshake is \
        voltooid",
    ),
    (
        "BLOCK DELETE IS LOCKED WHILE A JOB IS RUNNING OR PAUSED",
        "BLOK VERWIJDEREN IS VERGRENDELD TIJDENS EEN TAAK OF PAUZE",
    ),
    (
        "Closing this sheet sends nothing.",
        "Dit venster sluiten verstuurt niets.",
    ),
    (
        "CANCEL, the scrim, and closing this sheet send nothing.",
        "ANNULEREN, de achtergrond en het sluiten van dit venster versturen niets.",
    ),
    ("CONFIRMING WILL", "BEVESTIGING DOET"),
    ("RUN TO LINE", "LOPEN TOT REGEL"),
    ("SET BREAKPOINT AT LINE", "BREEKPUNT INSTELLEN BIJ REGEL"),
    ("START PROGRAM AT LINE 1", "PROGRAMMA STARTEN BIJ REGEL 1"),
    ("PAUSE BEFORE LINE", "PAUZE VOOR REGEL"),
    (
        "LEAVE THE ACTIVE JOB AT ITS CURRENT POSITION",
        "ACTIEVE TAAK OP HUIDIGE POSITIE LATEN",
    ),
    ("RUN TO BLOCKED", "LOPEN TOT GEBLOKKEERD"),
    ("SET BP + START", "BP INSTELLEN + START"),
    (
        "A job is already running or paused. Confirming only sets a breakpoint at the \
        selected line; it does not start, restart, or resume the job.",
        "Een taak loopt al of is gepauzeerd. Bevestigen stelt alleen een breekpunt in op de \
        geselecteerde regel; de taak wordt niet gestart, herstart of hervat.",
    ),
    (
        "All earlier lines still run from the beginning. The job starts at line 1 and \
        pauses before the selected line. Start-from-line is not supported.",
        "Alle eerdere regels worden nog steeds vanaf het begin uitgevoerd. De taak start \
        bij regel 1 en pauzeert voor de geselecteerde regel. Starten vanaf een regel wordt \
        niet ondersteund.",
    ),
    (
        "One step per press; off while typing. RESET DEFAULTS restores.",
        "Eén stap per druk; uit tijdens typen. STANDAARD HERSTELLEN zet terug.",
    ),
    (
        "Named single-line MDI commands, saved in this controller profile. RUN sends only \
        after a click; CONFIRM macros require a second explicit confirmation.",
        "Benoemde MDI-opdrachten van één regel, opgeslagen in dit controllerprofiel. \
        STARTEN verstuurt pas na een klik; BEVESTIGEN-macro's vragen een tweede expliciete \
        bevestiging.",
    ),
    (
        "No user macros for this controller profile. Add a named, single-line MDI command; \
        it is saved only here and never runs on connect.",
        "Geen gebruikersmacro's voor dit controllerprofiel. Voeg een benoemde MDI-opdracht \
        van één regel toe; die wordt alleen hier opgeslagen en nooit bij verbinden \
        uitgevoerd.",
    ),
    ("MACRO SAVES LOCKED", "MACRO OPSLAAN VERGRENDELD"),
    ("MACRO RUNS LOCKED", "MACRO UITVOEREN VERGRENDELD"),
    (
        "Startup lines execute automatically on every controller reset and can command \
        motion, spindle, or coolant. Writes are allowed only while the controller is Idle; \
        nothing is written until APPLY and CONFIRM WRITE.",
        "Opstartregels worden bij elke controllerreset automatisch uitgevoerd en kunnen \
        beweging, spindel of koelmiddel aansturen. Schrijven is alleen toegestaan als de \
        controller idle is; niets wordt geschreven tot TOEPASSEN en SCHRIJVEN BEVESTIGEN.",
    ),
    (
        "WRITE NOT APPLIED - THE PREVIOUS VALUE IS STILL ACTIVE",
        "SCHRIJVEN NIET TOEGEPAST - DE VORIGE WAARDE IS NOG ACTIEF",
    ),
    (
        "This startup line executes automatically on every controller reset. It can command \
        motion, spindle, or coolant.",
        "Deze opstartregel wordt bij elke controllerreset automatisch uitgevoerd. Hij kan \
        beweging, spindel of koelmiddel aansturen.",
    ),
    ("EXACT TEXT TO STORE", "EXACTE TEKST OM OP TE SLAAN"),
    ("Previous controller value", "Vorige controllerwaarde"),
    (
        "CONFIRM WRITE sends the exact text shown. The app does not update the row itself: \
        it reads the line back and reports the result.",
        "SCHRIJVEN BEVESTIGEN verstuurt de exact getoonde tekst. De app werkt de rij niet \
        zelf bij: hij leest de regel terug en meldt het resultaat.",
    ),
    ("RESET EXECUTION", "UITVOERING BIJ RESET"),
    (
        "CONFIRM FIRST; CANCEL SENDS NOTHING",
        "EERST BEVESTIGEN; ANNULEREN VERSTUURT NIETS",
    ),
    (
        "Sleep stops controller status polling until RESET or a status wake. The app does \
        not wake the controller automatically; the DRO and job view stay frozen until the \
        controller answers again.",
        "Slaap stopt het statuspeilen van de controller tot RESET of een statuswekker. De \
        app wekt de controller niet automatisch; DRO en taakweergave blijven bevroren tot \
        de controller weer antwoordt.",
    ),
    ("BEFORE YOU CONFIRM", "VOORDAT U BEVESTIGT"),
    (
        "The controller must report Idle and no job may be running or paused. One confirmed \
        tap sends one sleep command; CANCEL sends nothing.",
        "De controller moet idle melden en er mag geen taak lopen of gepauzeerd zijn. Eén \
        bevestigde tik verstuurt één slaapcommando; ANNULEREN verstuurt niets.",
    ),
    (
        "Pin map, auxiliary pin levels, limit switches and last control events. Opening the \
        panel sends nothing; REFRESH sends only $PINS, $PINSTATE, $LIM and $LEV.",
        "Pinmap, auxiliaire pinniveaus, eindschakelaars en laatste \
        besturingsgebeurtenissen. Het venster openen verstuurt niets; VERNIEUWEN verstuurt \
        alleen $PINS, $PINSTATE, $LIM en $LEV.",
    ),
    (
        "No $PINS report in this session",
        "Geen $PINS-rapport in deze sessie",
    ),
    (
        "The controller reported no pin entries",
        "De controller meldde geen pin-items",
    ),
    ("pin entries reported", "pin-items gemeld"),
    (
        "Applies immediately and is saved in app preferences; the controller is not \
        contacted.",
        "Wordt direct toegepast en opgeslagen in de app-voorkeuren; de controller wordt \
        niet benaderd.",
    ),
    (
        "Export writes app prefs, identity, settings, offsets, tools, macros, startup \
        lines, and keymap; import validates before writing and never pushes to the \
        controller.",
        "Export schrijft app-voorkeuren, identiteit, instellingen, nulpunten, \
        gereedschappen, macro's, opstartregels en toetsenkaart; import valideert vóór het \
        schrijven en stuurt nooit naar de controller.",
    ),
    (
        "The app owns this table; it persists in the per-controller profile. Controller \
        [T:] entries stay read-only and are never merged silently.",
        "Deze tabel is van de app; hij wordt opgeslagen in het controllerprofiel. \
        [T:]-items van de controller blijven alleen-lezen en worden nooit stil \
        samengevoegd.",
    ),
    (
        "No controller tool table has been reported yet.",
        "Er is nog geen gereedschapstabel van de controller gemeld.",
    ),
    (
        "The controller reports an empty tool table (0 entries).",
        "De controller meldt een lege gereedschapstabel (0 items).",
    ),
    (
        "Reported by the controller through [T:...]; this panel never writes, overwrites, \
        or merges into the app table above.",
        "Gemeld door de controller via [T:...]; dit venster schrijft, overschrijft of voegt \
        nooit samen in de app-tabel hierboven.",
    ),
    (
        "STARTUP WRITES LOCKED",
        "OPSTARTREGELS SCHRIJVEN VERGRENDELD",
    ),
    ("SAVE LOCKED", "OPSLAAN VERGRENDELD"),
    ("KEYMAP NOT VALID", "TOETSENKAART ONGELDIG"),
    ("Program has no lines", "Programma heeft geen regels"),
    (
        "Load a non-empty source file to search it.",
        "Laad een niet-leeg bronbestand om erin te zoeken.",
    ),
    ("Nothing matches", "Geen overeenkomsten"),
    (
        "Switch to HIGHLIGHT or clear the field to restore the full list.",
        "Schakel naar MARKEREN of wis het veld om de volledige lijst te herstellen.",
    ),
    (
        "DRAG ORBIT | SHIFT/MIDDLE PAN | WHEEL ZOOM",
        "SLEPEN ORBIT | SHIFT/MIDDELMUIS PAN | WIEL ZOOM",
    ),
    ("GENERATED PROGRAM", "GEGENEREERD PROGRAMMA"),
    (
        "No axes reported by the controller",
        "Geen assen gemeldet door de controller",
    ),
    (
        "ONE WRITE AT A TIME | READ-BACK VERIFIED",
        "ÉÉN SCHRIJFACTIE TEGELIJK | TERUGLEZEN GEVERIFIEERD",
    ),
    ("WRITES LOCKED", "SCHRIJVEN VERGRENDELD"),
    ("IDLE: WRITES ENABLED", "IDLE: SCHRIJVEN INGESCHAKELD"),
    (
        "MOCK MODE: NO CONTROLLER SETTINGS",
        "MOCKMODUS: GEEN CONTROLLERINSTELLINGEN",
    ),
    (
        "The generated editor is built from the controller's $$ values and $ES/$EG \
        metadata. The design-review mock supplies neither. Start without --mock to connect \
        the in-process FlexiHAL demo controller.",
        "De gegenereerde editor is gebaseerd op de $$-waarden en $ES/$EG-metadata van de \
        controller. De ontwerpreview-simulatie levert geen van beide. Start zonder --mock \
        om de in-process FlexiHAL-democonroller te verbinden.",
    ),
    ("WAITING FOR SETTINGS", "WACHT OP INSTELLINGEN"),
    (
        "The handshake has not reported $$, $ES, or $EG yet. The form appears as soon as \
        the controller answers.",
        "De handshake heeft $$, $ES of $EG nog niet gemeld. Het formulier verschijnt zodra \
        de controller antwoordt.",
    ),
    (
        "THE CONTROLLER REPORTED NO SETTINGS",
        "DE CONTROLLER MELDDE GEEN INSTELLINGEN",
    ),
    (
        "$$ returned no values and $ES/$EG returned no metadata. The editor stays empty \
        rather than inventing a setting list.",
        "$$ gaf geen waarden en $ES/$EG gaf geen metadata. De editor blijft leeg in plaats \
        van een lijst te verzinnen.",
    ),
    (
        "EXAMPLE SHAPE (NO VALUES ARE WRITTEN)",
        "VOORBEELD (ER WORDEN GEEN WAARDEN GESCHREVEN)",
    ),
    ("LOW", "LAAG"),
    ("HIGH", "HOOG"),
    ("dashed = missing", "gestreept = ontbreekt"),
    ("QUEUED", "IN WACHTRIJ"),
    ("BLANK ALLOWED", "LEEG TOEGESTAAN"),
    ("BIT 0 ENABLES THE OTHERS", "BIT 0 SCHAKELT DE ANDERE IN"),
    ("INFERRED FROM VALUE", "AFGELEID UIT WAARDE"),
    (
        "No saved maps for this controller profile yet.",
        "Nog geen opgeslagen kaarten voor dit controllerprofiel.",
    ),
    (
        "Read-only view; opening does not apply compensation or start a job",
        "Alleen-lezen weergave; openen past geen compensatie toe en start geen taak",
    ),
    (
        "Nothing is applied automatically",
        "Niets wordt automatisch toegepast",
    ),
    (
        "No automatic retry or resume. Recover the machine, then run again.",
        "Geen automatische herhaling of hervatting. Herstel de machine en start opnieuw.",
    ),
    (
        "every gate must pass before RUN",
        "elke controle moet slagen vóór STARTEN",
    ),
    ("PROBE CYCLES", "TASTCYCLI"),
    (
        "Pick a card, inspect the preflight, then press RUN",
        "Kies een kaart, bekijk de voorcontrole en druk op STARTEN",
    ),
    (
        "millimetres; tap a value to edit",
        "millimeters; tik op een waarde om te bewerken",
    ),
    ("recorded vs live", "opgenomen vs. live"),
    ("per controller profile", "per controllerprofiel"),
    ("APPLY OFFSET", "OFFSET TOEPASSEN"),
    ("NOT SENT YET", "NOG NIET VERSTUURD"),
    ("EXACT MDI LINE", "EXACTE MDI-REGEL"),
    (
        "CURRENT DRO (DISPLAY UNITS)",
        "HUIDIGE DRO (WEERGAVEEENHEDEN)",
    ),
    (
        "DRO CHANGED SINCE THIS SHEET OPENED; CANCEL AND RECHECK",
        "DRO GEWIJZIGD SINDS OPENEN; ANNULEREN EN OPNIEUW CONTROLEREN",
    ),
    ("READ-ONLY $# PARAMETERS", "$#-PARAMETERS ALLEEN-LEZEN"),
    (
        "X / Y / Z IN DISPLAY UNITS",
        "X / Y / Z IN WEERGAVEEENHEDEN",
    ),
    ("TAP TO EDIT", "TIK OM TE BEWERKEN"),
    (
        "RESTORE WORK OFFSETS - PREVIEW",
        "WERKSTUKNULPUNTEN HERSTELLEN - VOORBEELD",
    ),
    (
        "G10 L2 LINES IN THIS FILE (VALUES IN MILLIMETRES)",
        "G10 L2-REGELS IN DIT BESTAND (WAARDEN IN MILLIMETERS)",
    ),
    (
        "THE FILE BEGINS WITH G21: EVERY VALUE IS INTERPRETED IN MILLIMETRES.",
        "HET BESTAND BEGINT MET G21: ELKE WAARDE WORDT IN MILLIMETERS GEÏNTERPRETEERD.",
    ),
    ("LOAD AS PROGRAM", "ALS PROGRAMMA LADEN"),
    (
        "LOCKED: A JOB IS ACTIVE; STOP IT BEFORE LOADING A PROGRAM",
        "VERGRENDELD: EEN TAAK IS ACTIEF; STOP DEZE VOOR HET LADEN",
    ),
    ("SET X", "X INSTELLEN"),
    ("SET Y", "Y INSTELLEN"),
    ("SET Z", "Z INSTELLEN"),
    ("SET AXIS", "AS INSTELLEN"),
    ("MDI TEXT", "MDI-TEKST"),
    ("SPINDLE RPM", "SPINDEL TOERENTAL"),
    ("SETTING", "INSTELLING"),
    ("OFFSET", "OFFSET"),
    ("TOKENS + DIGITS", "TOKENS + CIJFERS"),
    ("G10 L20 | DISPLAY UNITS", "G10 L20 | WEERGAVEEENHEDEN"),
    ("RANGE N/A", "BEREIK N.V.T."),
    ("TOOL FIELD", "GEREEDSCHAPVELD"),
    ("PROBE FIELD (mm)", "TASTVELD (mm)"),
    ("Demo FlexiHAL", "Demo FlexiHAL"),
    (
        "In-process FlexiHAL simulator over the loopback transport. No serial port is \
        opened.",
        "In-process FlexiHAL-simulator via loopbacktransport. Er wordt geen seriële poort \
        geopend.",
    ),
    (
        "Same engine, handshake, streamer and safety gateway as a real controller.",
        "Dezelfde engine, handshake, streamer en veiligheidspoort als een echte controller.",
    ),
    (
        "Tap a port to select it; nothing opens until CONNECT.",
        "Tik op een poort om te selecteren; niets opent vóór VERBINDEN.",
    ),
    (
        "Line settings are fixed at 8 data bits, no parity, one stop bit. DTR reset pulses \
        are not implemented in M2.",
        "Lijninstellingen zijn vast: 8 databits, geen pariteit, 1 stopbit. DTR-resetpulsen \
        zijn niet geïmplementeerd in M2.",
    ),
    ("serial port", "seriële poort"),
    ("NOT REQUESTED", "NIET AANGEVRAAGD"),
    ("READING CONTROLLER", "CONTROLLER LEZEN"),
    (
        "CONTROLLER BUSY - RETRYING",
        "CONTROLLER BEZET - OPNIEUW PROBEREN",
    ),
    (
        "No $PINS report has arrived in this session.",
        "Er is in deze sessie geen $PINS-rapport ontvangen.",
    ),
    (
        "No $PINSTATE report has arrived in this session.",
        "Er is in deze sessie geen $PINSTATE-rapport ontvangen.",
    ),
    (
        "The controller reported no auxiliary pin levels.",
        "De controller meldde geen auxiliaire pinniveaus.",
    ),
    (
        "No $LIM report has arrived in this session.",
        "Er is in deze sessie geen $LIM-rapport ontvangen.",
    ),
    (
        "No $LEV report has arrived in this session.",
        "Er is in deze sessie geen $LEV-rapport ontvangen.",
    ),
    (
        "The controller reports no control or limit events yet.",
        "De controller meldt nog geen besturings- of grensgebeurtenissen.",
    ),
    ("PROBE PLAN", "TASTPLAN"),
    ("CUSTOM", "AANGEPAST"),
    (
        "No settings in this group",
        "Geen instellingen in deze groep",
    ),
    ("No setting matches", "Geen instelling komt overeen met"),
    (
        "Pick another group in the rail.",
        "Kies een andere groep in de balk.",
    ),
    (
        "Search matches a number, name, or group. Clear the search to browse groups.",
        "Zoeken matcht nummer, naam of groep. Wis de zoekopdracht om groepen te bekijken.",
    ),
    ("CONTROLLER STATE", "CONTROLLERSTATUS"),
    (
        "CONTROLLER DIAGNOSTIC DATA - READ-ONLY",
        "CONTROLLERDIAGNOSTIEK - ALLEEN LEZEN",
    ),
    ("SAVE BLOCKED", "OPSLAAN VERGRENDELD"),
    (
        "KEPT: offsets are unchanged. Apply stays available.",
        "BEHOUDEN: de nulpunten zijn ongewijzigd. Toepassen blijft beschikbaar.",
    ),
    ("COMMAND", "OPDRACHT"),
    ("Ask before running", "Vragen voor uitvoeren"),
    (
        "New macros default to CONFIRM. A direct macro runs on one click.",
        "Nieuwe macro's gebruiken standaard BEVESTIGEN. Een directe macro loopt met één \
        klik.",
    ),
    ("EXACT COMMAND TO SEND", "EXACT COMMANDO OM TE VERSTUREN"),
    (
        "Jogging is allowed while the job waits; the Jog panel stays active. The job will \
        not send another line until you resume.",
        "Joggen is toegestaan terwijl de taak wacht; het Jog-venster blijft actief. De taak \
        verstuurt geen regel meer tot je hervat.",
    ),
    (
        "Jogging is allowed while the job waits; the Jog panel stays active. The job will \
        not send another line until you resume. RESET and STOP above remain available.",
        "Joggen is toegestaan terwijl de taak wacht; het Jog-venster blijft actief. De taak \
        verstuurt geen regel meer tot je hervat. RESET en STOP hierboven blijven \
        beschikbaar.",
    ),
    ("VERIFYING", "VERIFIËREN"),
    ("VERIFIED", "GEVERIFIEERD"),
    ("MISMATCH", "KOMT NIET OVEREEN"),
    ("MEASUREMENT ONLY", "ALLEEN METING"),
    ("READY TO APPLY", "KLAAR OM TOE TE PASSEN"),
    ("REVERTED", "TERUGGEZET"),
    ("FEED", "VOEDING"),
    ("RETRACT", "TERUGTREKKEN"),
    ("CENTER X", "MIDDEN X"),
    ("CENTER Y", "MIDDEN Y"),
    ("ORIGIN X WORK", "NULPUNT X WERKSTUK"),
    ("ORIGIN Y WORK", "NULPUNT Y WERKSTUK"),
    ("SURFACE Z MACH", "OPPERVLAK Z MACHINE"),
    ("PROBE OFFSET X", "TASTOFFSET X"),
    ("PROBE OFFSET Y", "TASTOFFSET Y"),
    (
        "MOCK MODE: browsing only; loading needs a live controller",
        "MOCKMODUS: alleen bladeren; laden vereist een actieve controller",
    ),
    (
        "LINK NOT READY: finish the connection handshake before loading a program",
        "VERBINDING NIET GEREED: voltooi de handshake voordat je een programma laadt",
    ),
    (
        "JOB ACTIVE: stop the running job before loading another program",
        "TAAK ACTIEF: stop de lopende taak voordat je een ander programma laadt",
    ),
    (
        "MOCK MODE: a check run needs a live controller",
        "MOCKMODUS: een controle vereist een actieve controller",
    ),
    (
        "LINK NOT READY: finish the connection handshake before a check run",
        "VERBINDING NIET GEREED: voltooi de handshake voor een controle",
    ),
    (
        "MACHINE LOCKED: clear Alarm or Door before a check run",
        "MACHINE VERGRENDELD: wis Alarm of Deur voor een controle",
    ),
    (
        "NO PROGRAM: load a program before a check run",
        "GEEN PROGRAMMA: laad een programma voor een controle",
    ),
    (
        "JOB ACTIVE: stop the running job before a check run",
        "TAAK ACTIEF: stop de lopende taak voor een controle",
    ),
    (
        "MACHINE NOT IDLE: a check run starts only from IDLE",
        "MACHINE NIET IDLE: een controle start alleen vanuit IDLE",
    ),
    (
        "CHECK RUN ACTIVE: wait for the controller to finish check mode",
        "CONTROLE ACTIEF: wacht tot de controller de controlemode beëindigt",
    ),
    (
        "PROBE PROGRAM: a generated probe plan cannot be checked from the job toolbar",
        "TASTPROGRAMMA: een gegenereerd tastplan kan niet via de taakbalk worden \
        gecontroleerd",
    ),
    (
        "1. Confirm the physical E-stop and independent spindle stop. 2. Open \
        CONNECTION and select the serial port. 3. Wait for READY and Idle. 4. Check \
        the DRO with one small jog. 5. Verify units, WCS, and soft limits before \
        loading a program.",
        "1. Bevestig de fysieke noodstop en de onafhankelijke spindelstop. 2. Open \
        VERBINDING en selecteer de seriële poort. 3. Wacht op GEREED en inactief. 4. \
        Controleer de DRO met een kleine jog. 5. Controleer eenheden, WCS en \
        soft-limieten voordat je een programma laadt.",
    ),
    ("ADD LINE NUMBERS", "REGELNUMMERS TOEVOEGEN"),
    ("APP PREFERENCE ONLY", "ALLEEN APP-VOORKEUR"),
    (
        "APPLY is a separate, confirmed step and is allowed only while the controller \
        is Idle.",
        "TOEPASSEN is een aparte, bevestigde stap en mag alleen terwijl de controller \
        inactief is.",
    ),
    (
        "APPLY sends exactly one Command::SetSetting through the engine. The engine \
        writes the value, reads it back, and only then marks it APPLIED. CANCEL sends \
        nothing.",
        "TOEPASSEN stuurt precies één Command::SetSetting via de engine. De engine \
        schrijft de waarde, leest die terug en markeert die pas dan als TOEGEPAST. \
        ANNULEREN stuurt niets.",
    ),
    ("AXIS", "AS"),
    (
        "After setup, use the normal Connection panel. Start with the simulator or with \
        the spindle off, then run only status, DRO, and small jogs.",
        "Gebruik na de configuratie het normale Verbindingsvenster. Start met de \
        simulator of met de spindel uit en voer alleen status, DRO en kleine jogs uit.",
    ),
    ("BACK", "TERUG"),
    (
        "Back-and-forth raster at each depth, with a safe-Z retract between passes. The \
        wizard validates every field and caps the program before LOAD AS PROGRAM; it \
        never starts the job.",
        "Heen-en-weer-raster op elke diepte, met terugtrekken naar veilige Z tussen de \
        passes. De wizard valideert elk veld en begrenst het programma vóór ALS \
        PROGRAMMA LADEN; hij start de taak nooit.",
    ),
    ("CALCULATE", "BEREKENEN"),
    ("CALIBRATION CONFIRMATION", "KALIBRATIE BEVESTIGEN"),
    ("CALIBRATION DISTANCE (mm)", "KALIBRATIEAFSTAND (mm)"),
    (
        "CALIBRATION WRITES LOCKED",
        "KALIBRATIESCHRIJVEN VERGRENDELD",
    ),
    (
        "Calculate to preview the new steps/mm and the change before writing.",
        "Bereken om de nieuwe steps/mm en de wijziging vóór het schrijven te bekijken.",
    ),
    ("DEFAULT DISPLAY UNITS", "STANDAARD WEERGAVEEENHEDEN"),
    ("DISPLAY UNITS", "WEERGAVEEENHEDEN"),
    (
        "ESTIMATED MACHINE BOUNDS (WCS + G92)",
        "GESCHATTE MACHINEGRENZEN (WCS + G92)",
    ),
    (
        "EVERY ITEM IS REQUIRED BEFORE NEXT",
        "ELK ITEM IS VEREIST VOOR VOLGENDE",
    ),
    ("FINISH SETUP", "CONFIGURATIE VOLTOOIEN"),
    ("FIRST CONNECTION ORDER", "VOLGORDE VAN EERSTE VERBINDING"),
    ("GENERATE PREVIEW", "VOORBEELD GENEREREN"),
    (
        "GENERATE PREVIEW to validate the fields and compute passes, path length, and \
        time.",
        "VOORBEELD GENEREREN om de velden te valideren en passes, padlengte en tijd te \
        berekenen.",
    ),
    (
        "GENERATED / NOT CHECKED",
        "GEGENEREERD / NIET GECONTROLEERD",
    ),
    (
        "HEIGHT MAP / COMPENSATED: loading a transform replaces the compensated program \
        and its Revert original. Keep an external copy if needed.",
        "HOOGTEKAART / GECOMPENSEERD: een transformatie laden vervangt het \
        gecompenseerde programma en de oorspronkelijke versie voor terugzetten. Bewaar \
        indien nodig een externe kopie.",
    ),
    (
        "I measured the actual travel and want to write this steps/mm value",
        "Ik heb de werkelijke verplaatsing gemeten en wil deze steps/mm-waarde \
        schrijven",
    ),
    (
        "I understand this loads the previewed source and does not start it",
        "Ik begrijp dat dit de voorbeeldbron laadt en niet start",
    ),
    (
        "I understand this program is generated, not checked against the machine",
        "Ik begrijp dat dit programma is gegenereerd en niet tegen de machine is \
        gecontroleerd",
    ),
    ("INCHES (IN)", "INCH (IN)"),
    (
        "INFORMATIONAL CHECK: WCS changes, G10 writes, G92 offsets, and tool-length \
        offsets are not enforced here. Verify the setup and soft limits before \
        cutting.",
        "INFORMATIEVE CONTROLE: WCS-wijzigingen, G10-schrijfacties, G92-offsets en \
        gereedschaplengte-offsets worden hier niet afgedwongen. Controleer de \
        opstelling en soft-limieten vóór het snijden.",
    ),
    ("INFORMATIONAL ONLY", "ALLEEN INFORMATIEF"),
    (
        "Informational only: START is never blocked. The machine estimate adds the \
        reported active WCS plus G92; TLO is not included. Missing travel or offset \
        data shows NOT CHECKED, never a safety claim. No controller writes.",
        "Alleen informatief: START wordt nooit geblokkeerd. De machineschatting telt \
        het gerapporteerde actieve WCS plus G92 op; TLO is niet inbegrepen. Ontbreken \
        verplaatsings- of offsetgegevens, dan verschijnt NIET GECONTROLEERD, nooit een \
        veiligheidsclaim. Geen schrijfacties naar de controller.",
    ),
    (
        "Jog or run the test move separately, then enter the commanded distance and the \
        measured travel. The wizard only calculates and writes the steps/mm setting; \
        it never moves an axis.",
        "Voer de testbeweging apart uit met jog of MDI en voer daarna de opgedragen \
        afstand en de gemeten verplaatsing in. De wizard berekent en schrijft alleen \
        de steps/mm-instelling; hij verplaatst nooit een as.",
    ),
    ("MACHINE SETUP", "MACHINECONFIGURATIE"),
    ("MATERIAL SURFACING", "MATERIAAL VLAKMAKEN"),
    ("MATERIAL SURFACING WIZARD", "WIZARD MATERIAAL VLAKMAKEN"),
    ("MILLIMETRES (MM)", "MILLIMETERS (MM)"),
    ("NEW STEPS/MM", "NIEUWE STEPS/MM"),
    ("NO MOTION COMMANDED", "GEEN BEWEGING GESTUURD"),
    (
        "NO PROGRAM: load a program before transforming its source",
        "GEEN PROGRAMMA: laad een programma voordat je de bron transformeert",
    ),
    ("NOT COMPLETED", "NIET VOLTOOID"),
    (
        "No generated program yet. Open the wizard, validate the fields, and preview.",
        "Nog geen gegenereerd programma. Open de wizard, valideer de velden en bekijk \
        het voorbeeld.",
    ),
    (
        "No tools in the app table. Add one so the M6 sheet can show a name, diameter, \
        and length instead of only the number.",
        "Geen gereedschappen in de app-tabel. Voeg er een toe zodat het M6-venster een \
        naam, diameter en lengte kan tonen in plaats van alleen het nummer.",
    ),
    ("OFF omits both M3 and M5", "UIT laat zowel M3 als M5 weg"),
    ("OLD STEPS/MM", "OUDE STEPS/MM"),
    ("OPEN SETUP", "CONFIGURATIE OPENEN"),
    ("OPEN WIZARD", "WIZARD OPENEN"),
    ("PREVIEW", "VOORBEELD"),
    ("PREVIEW FIRST", "EERST VOORBEELD"),
    ("PREVIEW SUMMARY", "VOORBEELDOVERZICHT"),
    (
        "PROGRAM BOUNDS (WORK COORDINATES)",
        "PROGRAMMAGRENZEN (WERKSTUKCOÖRDINATEN)",
    ),
    (
        "PROGRAM EXCEEDS REPORTED TRAVEL",
        "PROGRAMMA OVERSCHRIJDT DE GERAPPORTEERDE VERPLAATSING",
    ),
    ("PROGRAM LIMITS", "PROGRAMMAGRENZEN"),
    ("PROPOSED STEPS/MM", "VOORGESTELDE STEPS/MM"),
    ("REMIND ME LATER", "HERINNER MIJ LATER"),
    (
        "REOPENS AT SAFETY TO REVIEW OR UPDATE THE CHOICES",
        "OPENT BIJ VEILIGHEID OM DE KEUZES TE CONTROLEREN OF BIJWERKEN",
    ),
    ("REPORTED TRAVEL", "GERAPPORTEERDE VERPLAATSING"),
    ("REVIEW & APPLY", "CONTROLEREN EN TOEPASSEN"),
    ("SAFETY ITEMS ACKNOWLEDGED", "VEILIGHEIDSITEMS BEVESTIGD"),
    (
        "SETUP IS NOT A MACHINE CERTIFICATION",
        "DE CONFIGURATIE IS GEEN MACHINECERTIFICERING",
    ),
    ("SOURCE TRANSFORMS", "BRONTRANSFORMATIES"),
    ("SPINDLE ON (M3 S... / M5)", "SPINDEL AAN (M3 S... / M5)"),
    ("START ANYWAY", "TOCH STARTEN"),
    (
        "START HERE BEFORE THE FIRST COMMANDED MOVE",
        "BEGIN HIER VOOR DE EERSTE OPDRACHTBEWEGING",
    ),
    ("STEPPER CALIBRATION", "STAPMOTOR-KALIBRATIE"),
    ("STRIP COMMENTS", "COMMENTAAR VERWIJDEREN"),
    (
        "THIS WIZARD DOES NOT TEST THE MACHINE",
        "DEZE WIZARD TEST DE MACHINE NIET",
    ),
    ("TRANSFORM PREVIEW", "TRANSFORMATIEVOORBEELD"),
    (
        "The controller, its firmware, and the physical safety wiring remain \
        authoritative. Acknowledge only what you have checked on this machine.",
        "De controller, de firmware en de fysieke veiligheidsbedrading blijven \
        bepalend. Bevestig alleen wat je op deze machine hebt gecontroleerd.",
    ),
    (
        "The loaded program's estimated machine bounds leave the travel reported by the \
        controller. START is held until you confirm.",
        "De geschatte machinegrenzen van het geladen programma vallen buiten de door de \
        controller gerapporteerde verplaatsing. START wordt tegengehouden tot je \
        bevestigt.",
    ),
    (
        "The loaded source is unchanged until LOAD AS PROGRAM. This loads one new \
        program and never starts it.",
        "De geladen bron blijft ongewijzigd tot ALS PROGRAMMA LADEN. Dit laadt één \
        nieuw programma en start het nooit.",
    ),
    (
        "The program is generated from these fields. It is not checked against this \
        machine's travel, offsets, tool, or stock. LOAD AS PROGRAM does not start it.",
        "Het programma wordt uit deze velden gegenereerd. Het is niet gecontroleerd \
        tegen de verplaatsing, offsets, gereedschap of het materiaal van deze machine. \
        ALS PROGRAMMA LADEN start het niet.",
    ),
    (
        "The wizard never connects, homes, jogs, probes, or writes controller settings.",
        "De wizard maakt nooit verbinding, referentieert niet, jogt niet, tast niet en \
        schrijft geen controllerinstellingen.",
    ),
    (
        "The wizard reviews the physical safety checklist, the app display unit \
        preference, and what the first connection should look like. It never connects, \
        homes, jogs, probes, or writes controller settings.",
        "De wizard behandelt de fysieke veiligheidschecklist, de weergave-eenheid van \
        de app en hoe de eerste verbinding eruit moet zien. Hij maakt nooit \
        verbinding, referentieert niet, jogt niet, tast niet en schrijft geen \
        controllerinstellingen.",
    ),
    (
        "This choice is saved for the next session. The wizard does not send G20 or G21 \
        and does not change the controller's units. The live controller parser state \
        still controls the current session.",
        "Deze keuze wordt voor de volgende sessie opgeslagen. De wizard stuurt geen G20 \
        of G21 en wijzigt de eenheden van de controller niet. De parserstatus van de \
        actieve controller bepaalt nog steeds de huidige sessie.",
    ),
    (
        "This records that you reviewed the checklist and chose an app preference. It \
        does not test, certify, or configure the machine. Verify every motion, limit, \
        and safety device yourself.",
        "Dit registreert dat je de checklist hebt doorgenomen en een app-voorkeur hebt \
        gekozen. Het test, certificeert of configureert de machine niet. Controleer \
        elke beweging, limiet en veiligheidsvoorziening zelf.",
    ),
    (
        "This writes one controller setting. It does not move the machine: the test \
        move is your separate jog or MDI command. The controller stores steps/mm in \
        EEPROM; avoid unnecessary writes.",
        "Dit schrijft één controllerinstelling. Het verplaatst de machine niet: de \
        testbeweging is je aparte jog- of MDI-opdracht. De controller bewaart steps/mm \
        in EEPROM; vermijd onnodige schrijfacties.",
    ),
    ("UTILITY FIELD", "HULPPROGRAMMAVELD"),
    ("WRITE LOCKED", "SCHRIJVEN VERGRENDELD"),
    ("STOCK WIDTH", "MATERIAALBREEDTE"),
    ("STOCK DEPTH", "MATERIAALDIEPTE"),
    ("CUTTER DIA", "FREESDIAM."),
    ("STEPOVER", "STAPOVER"),
    ("TOTAL DEPTH", "TOTALE DIEPTE"),
    ("DEPTH/PASS", "DIEPTE/PAS"),
    ("PLUNGE FEED", "INZETVOEDING"),
    ("LINE START", "STARTREGEL"),
    ("LINE STEP", "REGELSTAP"),
    ("COMMANDED MOVE", "OPGEDRAGEN BEWEGING"),
    ("MEASURED MOVE", "GEMETEN BEWEGING"),
    ("CALCULATED", "BEREKEND"),
    ("CONFIRMED", "BEVESTIGD"),
    (
        "ALLOWED: comments `(...)` or `;...`, `G4 P0..60`, `M0`, `M1`, `M5`, `M9`.",
        "TOEGESTAAN: commentaar `(...)` of `;...`, `G4 P0..60`, `M0`, `M1`, `M5`, `M9`.",
    ),
    ("ANGLE", "HOEK"),
    ("APPLY TLO", "TLO TOEPASSEN"),
    (
        "BACKUP EXPORTED THIS SESSION",
        "BACKUP DEZE SESSIE GEËXPORTEERD",
    ),
    (
        "BACKUP EXPORTED THIS SESSION - RESET ACKNOWLEDGED",
        "BACKUP DEZE SESSIE GEËXPORTEERD - RESET BEVESTIGD",
    ),
    ("BACKUP FILE", "BACKUPBESTAND"),
    ("BACKUP REQUIRED", "BACKUP VEREIST"),
    ("CALCULATOR ONLY", "ALLEEN REKENHULP"),
    (
        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY",
        "ALLEEN REKENHULP - DE APP BEWEEGT OF STELT HET PORTAAL NIET AF",
    ),
    ("CLEAR ALL HISTORY", "ALLE GESCHIEDENIS WISSEN"),
    ("CLEAR ALL HISTORY?", "ALLE GESCHIEDENIS WISSEN?"),
    (
        "CLEAR ASKS FOR CONFIRMATION FIRST",
        "WISSEN VRAAGT EERST OM BEVESTIGING",
    ),
    ("CLEAR HISTORY", "GESCHIEDENIS WISSEN"),
    ("CLEAR TLO", "TLO WISSEN"),
    ("CONTROLLER RESET", "CONTROLLER RESETTEN"),
    (
        "DATA LOSS ACKNOWLEDGED - RESET MAY PROCEED",
        "GEGEVENSVERLIES BEVESTIGD - RESET TOEGESTAAN",
    ),
    (
        "DATA LOSS ACKNOWLEDGED BY THE OPERATOR",
        "GEGEVENSVERLIES BEVESTIGD DOOR DE OPERATOR",
    ),
    ("DELTA", "DELTA"),
    ("DELTA (TOTAL)", "DELTA (TOTAAL)"),
    ("DESTRUCTIVE", "DESTRUCTIEF"),
    ("DROP PROGRAM TO LOAD", "SLEEP PROGRAMMA OM TE LADEN"),
    (
        "Drop exactly one program file",
        "Sleep precies één programmabestand",
    ),
    (
        "Drop one .nc, .gcode, .tap, or .ngc program file",
        "Sleep één .nc-, .gcode-, .tap- of .ngc-programmabestand",
    ),
    ("EMPTY", "LEEG"),
    ("EXPORT BACKUP", "BACKUP EXPORTEREN"),
    (
        "EXPORT IN FLIGHT - WAIT FOR THE WRITE TO FINISH",
        "EXPORT LOOPT - WACHT TOT HET SCHRIJVEN KLAAR IS",
    ),
    ("Enter a folder path first", "Voer eerst een mappad in"),
    (
        "Enter millimetres. The span is the distance between the two measurement points; the \
         readings are the Y values at the left and right points. CALCULATE is local \
         arithmetic.",
        "Voer millimeters in. De afstand is de afstand tussen de twee meetpunten; de metingen \
         zijn de Y-waarden links en rechts. BEREKENEN is lokale rekenkunde.",
    ),
    (
        "Finished jobs are stored in app.json on this computer. The newest 100 records are \
         kept; the card shows the newest 10.",
        "Voltooide taken worden opgeslagen in app.json op deze computer. De nieuwste 100 \
         records blijven bewaard; de kaart toont de nieuwste 10.",
    ),
    (
        "GANTRY IS SQUARE WITHIN MEASUREMENT RESOLUTION; NO CORRECTION INDICATED",
        "PORTAAL IS HAAKS BINNEN DE MEETNAUWKEURIGHEID; GEEN CORRECTIE NODIG",
    ),
    ("GANTRY MEASUREMENT (mm)", "PORTAALMETING (mm)"),
    ("GANTRY SQUARING", "PORTAAL UITLIJNEN"),
    ("GANTRY SQUARING CALCULATOR", "REKENHULP PORTAAL UITLIJNEN"),
    (
        "I HAVE A BACKUP OR ACCEPT DATA LOSS",
        "IK HEB EEN BACKUP OF ACCEPTEER GEGEVENSVERLIES",
    ),
    ("JOB HISTORY", "TAAKGESCHIEDENIS"),
    ("LEFT FORWARD", "LINKS VOOR"),
    (
        "LEFT SIDE IS {amount} mm FORWARD; MOVE IT BACK {amount} mm OR SHIM THE RIGHT SIDE",
        "DE LINKERZIJDE STAAT {amount} mm NAAR VOREN; SCHUIF DEZE {amount} mm TERUG OF VUL DE \
         RECHTERZIJDE OP",
    ),
    ("LINES", "REGELS"),
    (
        "LOCAL ONLY - NOT SENT TO THE CONTROLLER",
        "ALLEEN LOKAAL - NIET NAAR DE CONTROLLER VERSTUURD",
    ),
    (
        "LOCAL ONLY - NOT SENT TO THE CONTROLLER. Records are kept in app.json on this \
         computer, newest first.",
        "ALLEEN LOKAAL - NIET NAAR DE CONTROLLER VERSTUURD. Records staan in app.json op deze \
         computer, nieuwste eerst.",
    ),
    ("LOCAL PROFILE", "LOKAAL PROFIEL"),
    ("LOCAL TIME", "LOKALE TIJD"),
    ("MAX", "MAX"),
    ("MEASUREMENT RESULT", "MEETRESULTAAT"),
    ("MOST DESTRUCTIVE", "MEEST DESTRUCTIEF"),
    ("Max", "Max"),
    (
        "Measure the Y distance from a common reference to the left and right ends of the \
         gantry, enter the span between those points, and calculate the delta, angle, and \
         per-side correction. Follow the machine's squaring procedure for any adjustment.",
        "Meet de Y-afstand vanaf een gemeenschappelijk referentiepunt naar de linker- en \
         rechterkant van het portaal, voer de afstand tussen die punten in en bereken delta, \
         hoek en correctie per zijde. Volg voor elke afstelling de uitlijnprocedure van de \
         machine.",
    ),
    ("NEWEST FIRST", "NIEUWSTE EERST"),
    (
        "NO BACKUP EXPORTED THIS SESSION",
        "DEZE SESSIE GEEN BACKUP GEËXPORTEERD",
    ),
    ("NO JOBS RECORDED", "GEEN TAKEN VASTGELEGD"),
    ("NO MOTION / NO WRITE", "GEEN BEWEGING / GEEN SCHRIJFACTIE"),
    ("NO PER-SIDE CORRECTION", "GEEN CORRECTIE PER ZIJDE"),
    ("NOT SAVED", "NIET OPGESLAGEN"),
    (
        "No measurement yet. Enter the span and both Y readings in the calculator.",
        "Nog geen meting. Voer de afstand en beide Y-metingen in de rekenhulp in.",
    ),
    (
        "No motion, probing, spindle-on, coolant-on, tool change, coordinate, or system \
         commands.",
        "Geen beweging, tasten, spindel aan, koelmiddel aan, gereedschapswissel, coördinaat- \
         of systeemopdrachten.",
    ),
    (
        "No recent files yet. A loaded local file appears here.",
        "Nog geen recente bestanden. Een geladen lokaal bestand verschijnt hier.",
    ),
    ("OF", "VAN"),
    ("ONE COMMAND", "ÉÉN COMMANDO"),
    ("OPEN CALCULATOR", "REKENHULP OPENEN"),
    ("OPEN HISTORY", "GESCHIEDENIS OPENEN"),
    ("OPEN MACROS", "MACRO'S OPENEN"),
    (
        "One .nc, .gcode, .tap, or .ngc file. Loading never starts the job.",
        "Eén .nc-, .gcode-, .tap- of .ngc-bestand. Laden start de taak nooit.",
    ),
    ("PER-SIDE", "PER ZIJDE"),
    ("PER-SIDE (PIVOT CENTRE)", "PER ZIJDE (DRAAIPUNT)"),
    (
        "PIVOT CENTRE: MOVE THE LEFT SIDE BACK {amount} mm AND THE RIGHT SIDE FORWARD \
         {amount} mm",
        "DRAAIPUNT: SCHUIF DE LINKERZIJDE {amount} mm TERUG EN DE RECHTERZIJDE {amount} mm \
         NAAR VOREN",
    ),
    (
        "PIVOT CENTRE: MOVE THE RIGHT SIDE BACK {amount} mm AND THE LEFT SIDE FORWARD \
         {amount} mm",
        "DRAAIPUNT: SCHUIF DE RECHTERZIJDE {amount} mm TERUG EN DE LINKERZIJDE {amount} mm \
         NAAR VOREN",
    ),
    ("POST", "POST"),
    (
        "POST MACRO - after the final safe-Z retract",
        "POST-MACRO - na de laatste terugtrek naar veilige Z",
    ),
    ("PRE", "PRE"),
    (
        "PRE MACRO - after the safety preamble",
        "PRE-MACRO - na de veiligheidspreamble",
    ),
    ("PREFLIGHT OK", "VOORCONTROLE OK"),
    ("PROBE DEFAULTS", "TASTERSTANDAARDEN"),
    ("PROBE MACROS", "TASTERMACRO'S"),
    (
        "Press CALCULATE to validate the fields and show the correction.",
        "Druk op BEREKENEN om de velden te controleren en de correctie te tonen.",
    ),
    ("RECENT", "RECENT"),
    ("RECENT FILES", "RECENTE BESTANDEN"),
    ("REFUSED", "GEWEIGERD"),
    ("REMOVE", "VERWIJDEREN"),
    (
        "RESET GATES READY - TWO-STEP CONFIRMATION REQUIRED",
        "RESETVOORWAARDEN OK - TWESTAPPIGE BEVESTIGING VEREIST",
    ),
    ("RESET LOCKED", "RESET VERGRENDELD"),
    ("RESULT", "RESULTAAT"),
    ("RIGHT FORWARD", "RECHTS VOOR"),
    (
        "RIGHT SIDE IS {amount} mm FORWARD; MOVE IT BACK {amount} mm OR SHIM THE LEFT SIDE",
        "DE RECHTERZIJDE STAAT {amount} mm NAAR VOREN; SCHUIF DEZE {amount} mm TERUG OF VUL \
         DE LINKERZIJDE OP",
    ),
    (
        "Recent file was not removed",
        "Recent bestand is niet verwijderd",
    ),
    (
        "Recent files were not cleared",
        "Recente bestanden zijn niet gewist",
    ),
    ("SAVE DEFAULTS", "STANDAARDEN OPSLAAN"),
    ("SAVE MACROS", "MACRO'S OPSLAAN"),
    ("SAVED", "OPGESLAGEN"),
    (
        "SAVED PER CONTROLLER PROFILE",
        "OPGESLAGEN PER CONTROLLERPROFIEL",
    ),
    ("SHOWING", "GETOOND"),
    ("SPAN BETWEEN POINTS", "AFSTAND TUSSEN PUNTEN"),
    ("SQUARE", "HAAKS"),
    ("STEP", "STAP"),
    (
        "STOCK INFERRED FROM PROGRAM BOUNDS | NOT A MEASURED MODEL",
        "MATERIAAL AFGELEID UIT PROGRAMMAGRENZEN | GEEN GEMETEN MODEL",
    ),
    ("STOCK OFF", "MATERIAAL UIT"),
    ("STOCK ON", "MATERIAAL AAN"),
    ("STORED", "OPGESLAGEN"),
    ("TAP FOR", "TIK VOOR"),
    ("THEME", "THEMA"),
    (
        "The controller may reset or re-handshake after this command. The app does not \
         reconnect, home, jog, or resume anything. CANCEL sends nothing.",
        "De controller kan na deze opdracht resetten of opnieuw onderhandelen. De app \
         verbindt niet opnieuw, referentieert niet, beweegt niet en hervat niets. ANNULEREN \
         stuurt niets.",
    ),
    (
        "There is no APPLY. Move or shim the gantry by hand, then measure again.",
        "Er is geen TOEPASSEN. Beweeg of vul het portaal handmatig op en meet opnieuw.",
    ),
    (
        "These lines run inside the generated probe program and are still subject to every \
         preflight gate. SAVE MACROS writes only the local controller profile: no controller \
         command, setting, offset, or start.",
        "Deze regels lopen in het gegenereerde tastprogramma en blijven onderworpen aan elke \
         voorcontrole. MACRO'S OPSLAAN schrijft alleen het lokale controllerprofiel: geen \
         controlleropdracht, instelling, offset of start.",
    ),
    (
        "This removes every stored job record from app.json on this computer. It does not \
         change the loaded program or send anything to the controller.",
        "Verwijdert elk opgeslagen taakrecord uit app.json op deze computer. Het geladen \
         programma verandert niet en er wordt niets naar de controller gestuurd.",
    ),
    (
        "bounded pre/post lines; local profile; still subject to preflight",
        "begrensde PRE/POST-regels; lokaal profiel; nog steeds voorcontroleplichtig",
    ),
    ("bytes and", "bytes en"),
    ("correction is not finite", "is niet eindig"),
    ("drop one program file", "sleep één programmabestand"),
    (
        "file contains binary data, not a G-code program",
        "bestand bevat binaire gegevens, geen G-code-programma",
    ),
    ("file is", "bestand is"),
    ("file is empty", "bestand is leeg"),
    (
        "file is not valid UTF-8 text",
        "bestand is geen geldige UTF-8-tekst",
    ),
    ("is empty", "is leeg"),
    (
        "is not a .nc, .gcode, .tap, or .ngc program",
        "is geen .nc-, .gcode-, .tap- of .ngc-programma",
    ),
    ("is not a readable folder", "is geen leesbare map"),
    ("is not a regular file", "is geen gewoon bestand"),
    ("is not finite", "is niet eindig"),
    ("is out of range", "valt buiten het bereik"),
    ("items were dropped", "items zijn neergezet"),
    ("lines per macro.", "regels per macro."),
    (
        "local JSON only; nothing is sent to the controller",
        "alleen lokale JSON; niets wordt naar de controller gestuurd",
    ),
    (
        "mm | probe tip - controlled point | plan shifts controlled by -offset",
        "mm | tasttip - gecontroleerd punt | plan verschuift het gecontroleerde punt met \
         -offset",
    ),
    ("must be a whole number", "moet een geheel getal zijn"),
    ("must be at most", "mag maximaal"),
    ("must be finite", "moet eindig zijn"),
    ("must be greater than 0", "moet groter dan 0 zijn"),
    ("must be numeric", "moet een getal zijn"),
    ("must be within", "moet liggen tussen"),
    (
        "the limit for the file browser is",
        "de limiet voor de bestandsbrowser is",
    ),
    ("REVIEW", "OVERZICHT"),
    (
        "PHYSICAL E-STOP LOCATED AND TESTED",
        "FYSIEKE NOODSTOP GEVONDEN EN GETEST",
    ),
    (
        "SPINDLE CAN BE STOPPED INDEPENDENTLY",
        "SPINDEL KAN ONAFHANKELIJK WORDEN GESTOPT",
    ),
    (
        "MACHINE TRAVEL AND SOFT LIMITS VERIFIED",
        "MACHINEVERPLAATSING EN SOFTLIMIETEN GEVERIFIEERD",
    ),
    (
        "WORKHOLDING AND TOOL CLEARANCE CHECKED",
        "WERKSTUKOPSPANNING EN GEREEDSCHAPSVRIJHEID GECONTROLEERD",
    ),
    (
        "SIMULATOR OR SPINDLE-OFF FIRST CONNECTION",
        "EERSTE VERBINDING MET SIMULATOR OF SPINDEL UIT",
    ),
    (
        "Know where it is, reach it, and prove it stops the machine before any commanded \
         move.",
        "Weet waar hij is, kunt hem bereiken en hebt bewezen dat hij de machine stopt vóór \
         elke opgedragen beweging.",
    ),
    (
        "Use the VFD or the power control, not this sender, to stop the cutter.",
        "Gebruik de VFD of de voedingsregeling, niet deze sender, om de frees te stoppen.",
    ),
    (
        "Confirm the physical travel and the controller's soft-limit settings.",
        "Bevestig de fysieke verplaatsing en de softlimietinstellingen van de controller.",
    ),
    (
        "Secure the stock, clear the tool, and keep hands clear of every axis.",
        "Zet het werkstuk vast, houd het gereedschap vrij en houd handen weg van alle assen.",
    ),
    (
        "Learn the sender in the simulator, then connect with the spindle off and run only \
         status, DRO, and small jogs.",
        "Leer de sender in de simulator, verbind daarna met de spindel uit en voer alleen \
         status, DRO en kleine jogs uit.",
    ),
    ("STOPPED", "GESTOPT"),
    ("CONFIRM APPLY TLO", "TLO TOEPASSEN BEVESTIGEN"),
    ("CONFIRM CLEAR TLO", "TLO WISSEN BEVESTIGEN"),
    ("HIGH CONTRAST", "HOOG CONTRAST"),
    ("LIGHT", "LICHT"),
    ("DARK", "DONKER"),
    ("LEFT Y READING", "Y-METING LINKS"),
    ("RIGHT Y READING", "Y-METING RECHTS"),
    (
        "OFFSETS AND CONTROLLER TOOL DATA",
        "OFFSETS EN CONTROLLER-GEREEDSCHAPSGEGEVENS",
    ),
    ("CORE SETTING DEFAULTS", "STANDAARD KERNINSTELLINGEN"),
    ("DRIVER AND PLUGIN DEFAULTS", "STANDAARDEN DRIVER EN PLUGIN"),
    (
        "ALL SETTINGS, PARAMETERS, STARTUP LINES, AND BUILD INFO",
        "ALLE INSTELLINGEN, PARAMETERS, STARTREGELS EN BUILD-INFO",
    ),
    (
        "Offsets and controller tool data. Local profile, backups, and saved maps stay.",
        "Offsets en controller-gereedschapsgegevens. Lokaal profiel, back-ups en opgeslagen \
         kaarten blijven.",
    ),
    (
        "Core setting defaults. Offsets and tool data stay.",
        "Standaardwaarden voor kerninstellingen. Offsets en gereedschapsgegevens blijven.",
    ),
    (
        "Driver and plugin defaults. Core settings, offsets, and tool data stay.",
        "Standaardwaarden voor driver en plugins. Kerninstellingen, offsets en \
         gereedschapsgegevens blijven.",
    ),
    (
        "All settings, parameters, startup lines, and build info. Most destructive.",
        "Alle instellingen, parameters, startregels en build-info. Meest destructief.",
    ),
    (
        "Offsets ($#) and controller tool data are reset. Work coordinate systems, G28/G30, \
         G92, TLO, and the controller's tool records may be lost. Local profile data, \
         backups, and saved height maps stay.",
        "Offsets ($#) en controller-gereedschapsgegevens worden gereset. \
         Werkcoördinatenstelsels, G28/G30, G92, TLO en de gereedschapsrecords van de \
         controller kunnen verloren gaan. Lokale profielgegevens, back-ups en opgeslagen \
         hoogtekaarten blijven.",
    ),
    (
        "Core controller settings return to their defaults. Offsets and controller tool data \
         stay. Export a backup before changing settings again.",
        "De kerninstellingen van de controller keren terug naar hun standaardwaarden. Offsets \
         en controller-gereedschapsgegevens blijven. Exporteer een back-up voordat je \
         instellingen opnieuw wijzigt.",
    ),
    (
        "Driver and plugin settings return to their defaults. Core settings, offsets, and \
         controller tool data stay.",
        "Driver- en plugininstellingen keren terug naar hun standaardwaarden. \
         Kerninstellingen, offsets en controller-gereedschapsgegevens blijven.",
    ),
    (
        "ALL settings, parameters, startup lines, and build info are reset. Offsets, tool \
         data, and controller parameters may be unrecoverable. This is the most destructive \
         reset and the controller will usually re-handshake.",
        "ALLE instellingen, parameters, startregels en build-info worden gereset. Offsets, \
         gereedschapsgegevens en controllerparameters zijn mogelijk onherstelbaar. Dit is de \
         meest destructieve reset en de controller onderhandelt meestal opnieuw.",
    ),
    (
        "Requested read-only `$#` refresh.",
        "Alleen-lezen `$#`-verversing aangevraagd.",
    ),
    (
        "Requested read-only `$$` settings dump.",
        "Alleen-lezen `$$`-instellingenoverzicht aangevraagd.",
    ),
    (
        "No read-only report is defined for driver parameters.",
        "Er is geen alleen-lezen rapport gedefinieerd voor driverparameters.",
    ),
    (
        "Requested read-only `$#` and `$$` refreshes.",
        "Alleen-lezen `$#`- en `$$`-verversingen aangevraagd.",
    ),
    (
        "MOCK MODE: controller reset needs a live controller",
        "MOCKMODUS: controller reset vereist een actieve controller",
    ),
    (
        "NOT LIVE: connect the controller before reset",
        "NIET ACTIEF: verbind de controller vóór de reset",
    ),
    (
        "LINK NOT READY: reset is disabled until the handshake completes",
        "VERBINDING NIET GEREED: reset is uitgeschakeld tot de handshake is voltooid",
    ),
    (
        "ALARM OR DOOR: clear the alarm or door state before reset",
        "ALARM OF DEUR: wis de alarm- of deurstatus vóór de reset",
    ),
    (
        "JOB ACTIVE: stop or finish the job before reset",
        "TAAK ACTIEF: stop of voltooi de taak vóór de reset",
    ),
    (
        "TOOL CHANGE PENDING: finish or stop the tool change before reset",
        "GEREEDSCHAPSWISSEL OPENSTAAND: rond de wissel af of stop deze vóór de reset",
    ),
    (
        "PROBE ACTIVE: finish or abort the probe run before reset",
        "TASTACTIE ACTIEF: rond de tastmeting af of breek deze af vóór de reset",
    ),
    (
        "HEIGHT MAP BUSY: finish the Apply or Revert before reset",
        "HOOGTEKAART BEZIG: rond Toepassen of Terugdraaien af vóór de reset",
    ),
    (
        "PENDANT ACTIVE (MPG:1): release the pendant before reset",
        "HANDWIEL ACTIEF (MPG:1): geef het handwiel vrij vóór de reset",
    ),
    (
        "SETTINGS WRITE PENDING: finish or discard it before reset",
        "INSTELLINGSSCHRIJFACTIE OPENSTAAND: rond deze af of verwerp deze vóór de reset",
    ),
    (
        "OFFSET WRITE PENDING: finish the write and refresh before reset",
        "OFFSETSCHRIJFACTIE OPENSTAAND: rond het schrijven af en ververs vóór de reset",
    ),
    (
        "NOT IDLE: reset is available only while the controller reports Idle",
        "NIET INACTIEF: reset is alleen beschikbaar terwijl de controller Inactief meldt",
    ),
    (
        "BACKUP EXPORT IN FLIGHT: wait for the export to finish",
        "BACKUPEXPORT LOOPT: wacht tot de export klaar is",
    ),
    (
        "BACKUP NOT ACKNOWLEDGED: export a backup or accept data loss first",
        "BACKUP NIET BEVESTIGD: exporteer eerst een back-up of accepteer gegevensverlies",
    ),
    (
        "LOCAL PROFILE IS NOT CHANGED AUTOMATICALLY",
        "HET LOKALE PROFIEL WORDT NIET AUTOMATISCH GEWIJZIGD",
    ),
    (
        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY. Follow the machine's \
         squaring procedure, move or shim the gantry by hand, and verify the result with a \
         fresh measurement before the next cut.",
        "ALLEEN REKENHULP - DE APP BEWEEGT OF STELT HET PORTAAL NIET AF. Volg de \
         uitlijnprocedure van de machine, beweeg of vul het portaal handmatig op en verifieer \
         het resultaat met een nieuwe meting vóór de volgende snede.",
    ),
    (
        "Positive delta and angle: the right measurement point is further along +Y (forward) \
         than the left. Negative: the left point is further along +Y. Per-side is delta / 2: \
         with the centre fixed, each end moves half the delta in opposite directions.",
        "Positieve delta en hoek: het rechter meetpunt ligt verder langs +Y (naar voren) dan \
         het linker. Negatief: het linker punt ligt verder langs +Y. Per zijde is delta / 2: \
         met het midden vast beweegt elk uiteinde de helft van de delta in tegengestelde \
         richtingen.",
    ),
    ("SENT", "VERZONDEN"),
];
