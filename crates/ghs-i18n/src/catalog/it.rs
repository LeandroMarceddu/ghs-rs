//! it catalog.
//!
//! Keys are the exact English UI literals; lookups are case-sensitive.

pub(crate) static CATALOG: &[(&str, &str)] = &[
    ("Job", "Lavoro"),
    ("Probe", "Sonda"),
    ("Offset", "Origini"),
    ("Utility", "Utilità"),
    ("Tools", "Utensili"),
    ("Settings", "Impostazioni"),
    ("JOB", "LAVORO"),
    ("PROBING", "SONDA"),
    ("WORK OFFSETS", "ORIGINI PEZZO"),
    ("UTILITIES", "UTILITÀ"),
    ("TOOL TABLE", "TABELLA UTENSILI"),
    ("SETTINGS", "IMPOSTAZIONI"),
    ("Console", "Console"),
    ("Program", "Programma"),
    ("Light", "Chiaro"),
    ("Apply", "Applica"),
    ("APPLY", "APPLICA"),
    ("Save", "Salva"),
    ("SAVE", "SALVA"),
    ("Cancel", "Annulla"),
    ("CANCEL", "ANNULLA"),
    ("Close", "Chiudi"),
    ("CLOSE", "CHIUDI"),
    ("Edit", "Modifica"),
    ("EDIT", "MODIFICA"),
    ("Delete", "Elimina"),
    ("DEL", "ELIM."),
    ("Refresh", "Aggiorna"),
    ("REFRESH", "AGGIORNA"),
    ("Export", "Esporta"),
    ("EXPORT", "ESPORTA"),
    ("Import", "Importa"),
    ("IMPORT", "IMPORTA"),
    ("Validate", "Valida"),
    ("VALIDATE", "VALIDA"),
    ("Confirm", "Conferma"),
    ("CONFIRM", "CONFERMA"),
    ("Keep", "Mantieni"),
    ("KEEP", "MANTIENI"),
    ("Revert", "Ripristina"),
    ("REVERT", "RIPRISTINA"),
    ("Zero", "Azzera"),
    ("ZERO", "AZZERA"),
    ("Home", "Origine"),
    ("HOME", "ORIGINE"),
    ("Unlock", "Sblocca"),
    ("UNLOCK", "SBLOCCA"),
    ("Macro", "Macro"),
    ("Startup Lines", "Righe di avvio"),
    ("STARTUP LINES", "RIGHE DI AVVIO"),
    ("Keymap", "Mappa tasti"),
    ("KEYMAP", "MAPPA TASTI"),
    ("Diagnostics", "Diagnostica"),
    ("DIAGNOSTICS", "DIAGNOSTICA"),
    ("RESET", "RESET"),
    ("STOP", "ARRESTO"),
    ("Stop", "Arresta"),
    ("SLEEP", "SOSPENSIONE"),
    ("CONNECT", "CONNETTI"),
    ("DISCONNECT", "DISCONNETTI"),
    ("CONNECTION", "CONNESSIONE"),
    ("TRANSPORT", "TRASPORTO"),
    ("CURRENT LINK", "COLLEGAMENTO ATTUALE"),
    ("SERIAL PORT", "PORTA SERIALE"),
    ("Demo", "Demo"),
    ("Serial port", "Porta seriale"),
    ("No serial ports found", "Nessuna porta seriale trovata"),
    (
        "Disconnect before switching transport.",
        "Disconnetti prima di cambiare trasporto.",
    ),
    ("MOCK", "SIMULAZIONE"),
    ("CONNECTING", "CONNESSIONE IN CORSO"),
    ("READY", "PRONTO"),
    ("DISCONNECTED", "DISCONNESSO"),
    ("ERROR", "ERRORE"),
    ("NOT CONNECTED", "NON CONNESSO"),
    ("STALE", "NON AGGIORNATO"),
    ("Alarm", "Allarme"),
    ("Door", "Sportello"),
    ("LOAD PROGRAM", "CARICA PROGRAMMA"),
    ("START", "AVVIA"),
    ("PAUSE", "PAUSA"),
    ("RESUME", "RIPRENDI"),
    ("RUN OPTIONS", "OPZIONI ESECUZIONE"),
    ("RAPID", "RAPIDO"),
    ("Rapid", "Rapido"),
    ("ETA", "TEMPO RIMANENTE"),
    ("TIME", "TEMPO"),
    ("TOOL CHANGE REQUIRED", "CAMBIO UTENSILE RICHIESTO"),
    ("TOOL CHANGE", "CAMBIO UTENSILE"),
    ("RUN TO", "ESEGUI FINO A"),
    ("SET BREAKPOINT", "IMPOSTA PUNTO DI INTERRUZIONE"),
    ("BP", "PI"),
    ("SEARCH", "CERCA"),
    ("FILTER", "FILTRO"),
    ("HIGHLIGHT", "EVIDENZIA"),
    ("NEXT", "SUCCESSIVO"),
    ("PREV", "PRECEDENTE"),
    ("NO PROGRAM LOADED", "NESSUN PROGRAMMA CARICATO"),
    ("NO PROGRAM", "NESSUN PROGRAMMA"),
    ("PROGRAM", "PROGRAMMA"),
    (
        "HEIGHT MAP / COMPENSATED",
        "MAPPA DELLE ALTEZZE / COMPENSATA",
    ),
    ("Cut", "Taglio"),
    ("Plunge", "Entrata"),
    ("IDLE", "INATTIVO"),
    ("RUNNING", "IN ESECUZIONE"),
    ("PAUSED", "IN PAUSA"),
    ("COMPLETED", "COMPLETATO"),
    ("ABORTED", "INTERROTTO"),
    ("INTERRUPTED", "SOSPESO"),
    ("JOB ACTIVE", "LAVORO ATTIVO"),
    ("JOB RUNNING", "LAVORO IN ESECUZIONE"),
    ("STOP JOB", "ARRESTA LAVORO"),
    ("TAP A LINE TO SELECT", "TOCCA UNA RIGA PER SELEZIONARLA"),
    ("No matching lines", "Nessuna riga corrispondente"),
    ("NO MATCH", "NESSUN RISULTATO"),
    ("SAVE LOG", "SALVA LOG"),
    ("DRO", "VISUALIZZATORE"),
    ("JOG", "JOG"),
    ("MDI", "MDI"),
    ("FEED OVERRIDE", "OVERRIDE AVANZAMENTO"),
    ("SPINDLE OVERRIDE", "OVERRIDE MANDRINO"),
    ("RAPID OVERRIDE", "OVERRIDE RAPIDO"),
    ("COOLANT", "REFRIGERANTE"),
    ("SPINDLE", "MANDRINO"),
    ("SIGNALS", "SEGNALI"),
    ("WORK", "PEZZO"),
    ("SEND", "INVIA"),
    ("UP", "SU"),
    ("DN", "GIÙ"),
    ("CONT", "CONTINUO"),
    ("CW", "ORARIO"),
    ("CCW", "ANTIORARIO"),
    ("Spindle off", "Mandrino spento"),
    ("RESET 100%", "RIPRISTINA 100%"),
    ("Feed", "Avanzamento"),
    ("MACHINE", "MACCHINA"),
    ("TOOL", "UTENSILE"),
    ("POCKET", "TASCA"),
    ("RADIUS", "RAGGIO"),
    ("TOOLPATH", "PERCORSO UTENSILE"),
    ("CYCLES", "CICLI"),
    ("CYCLE PARAMETERS", "PARAMETRI CICLO"),
    ("GRID SUMMARY", "RIEPILOGO GRIGLIA"),
    ("PREFLIGHT", "CONTROLLO PRELIMINARE"),
    ("PROBE RUN", "ESECUZIONE SONDA"),
    ("RAW PRB (machine mm)", "PRB GREZZO (mm macchina)"),
    ("SAVED MAPS", "MAPPE SALVATE"),
    ("SAVED MAP", "MAPPA SALVATA"),
    ("MEASUREMENT FRAME", "SISTEMA DI MISURA"),
    ("HEIGHT MAP RESULT", "RISULTATO MAPPA DELLE ALTEZZE"),
    ("CONFIRM APPLY", "CONFERMA APPLICAZIONE"),
    ("PROBE OFFSET X/Y", "OFFSET SONDA X/Y"),
    ("PROBE OFFSET", "OFFSET SONDA"),
    (
        "SEARCH BOUND (CONTROLLED-POINT XY)",
        "LIMITE RICERCA (PUNTO CONTROLLATO XY)",
    ),
    ("APPLY HEIGHT MAP", "APPLICA MAPPA DELLE ALTEZZE"),
    ("CLOSE MAP", "CHIUDI MAPPA"),
    ("SAVE MAP", "SALVA MAPPA"),
    ("EXPORT CSV", "ESPORTA CSV"),
    ("RETRY PREFLIGHT", "RIPROVA CONTROLLO"),
    ("CONFIGURE", "CONFIGURA"),
    ("Probe connected", "Sonda collegata"),
    ("ABORT", "INTERROMPI"),
    ("Height map", "Mappa delle altezze"),
    ("Z touch plate", "Piastra di contatto Z"),
    ("XY touch plate", "Piastra di contatto XY"),
    ("Edge X", "Bordo X"),
    ("Edge Y", "Bordo Y"),
    ("Corner", "Angolo"),
    ("Center circle", "Cerchio centrato"),
    ("Center rectangle", "Rettangolo centrato"),
    ("Rotation", "Rotazione"),
    ("Tool length", "Lunghezza utensile"),
    ("MEASURED Z", "Z MISURATO"),
    ("SAFE Z", "Z SICURA"),
    ("SURFACE Z", "Z SUPERFICIE"),
    ("MAP Z OFFSET", "OFFSET Z MAPPA"),
    ("MAP TRAVEL", "CORSA MAPPA"),
    ("GRID ORIGIN", "ORIGINE GRIGLIA"),
    ("COLUMNS", "COLONNE"),
    ("ROWS", "RIGHE"),
    ("TOUCHES", "CONTATTI"),
    ("STEP X", "PASSO X"),
    ("STEP Y", "PASSO Y"),
    ("EDGE OFFSET", "OFFSET BORDO"),
    ("SEPARATION", "SEPARAZIONE"),
    ("PLATE THICK", "SPESSORE PIASTRA"),
    ("PLATE CORNER X", "ANGOLO PIASTRA X"),
    ("PLATE CORNER Y", "ANGOLO PIASTRA Y"),
    ("PROBE HEIGHT", "ALTEZZA SONDA"),
    ("PROBE DIA", "DIA SONDA"),
    ("REFERENCE Z", "Z RIFERIMENTO"),
    ("RECT WIDTH", "LARGHEZZA RETT."),
    ("RECT HEIGHT", "ALTEZZA RETT."),
    ("EXPECTED X", "X ATTESO"),
    ("EXPECTED Y", "Y ATTESO"),
    ("EXPECTED Z", "Z ATTESO"),
    ("Measured height", "Altezza misurata"),
    ("Measured width", "Larghezza misurata"),
    ("Measured X edge", "Bordo X misurato"),
    ("Measured Y edge", "Bordo Y misurato"),
    ("SURFACE", "SUPERFICIE"),
    ("COMPENSATION", "COMPENSAZIONE"),
    ("PLAN", "PIANO"),
    ("MAP", "MAPPA"),
    ("SAFETY", "SICUREZZA"),
    ("PROBE VERIFIED", "SONDA VERIFICATA"),
    ("NOT CONFIRMED", "NON CONFERMATO"),
    ("TRIGGERED", "ATTIVATO"),
    ("NOT READY", "NON PRONTO"),
    ("UNITS", "UNITÀ"),
    ("TOOL LENGTH DELTA", "DELTA LUNGHEZZA UTENSILE"),
    ("RMS fit error", "Errore di adattamento RMS"),
    ("Deviation", "Deviazione"),
    ("Nominal", "Nominale"),
    ("WORK XYZ", "PEZZO XYZ"),
    ("MACHINE XYZ", "MACCHINA XYZ"),
    ("SEARCH SETTINGS", "CERCA IMPOSTAZIONI"),
    ("All settings", "Tutte le impostazioni"),
    ("General", "Generale"),
    ("Axes", "Assi"),
    ("Limits", "Limiti"),
    ("Network address", "Indirizzo di rete"),
    ("Work mode", "Modalità di lavoro"),
    ("Step pulse time", "Tempo impulso passo"),
    ("Step idle delay", "Ritardo riposo motori"),
    ("Maximum spindle speed", "Velocità massima del mandrino"),
    ("Homing cycle enable", "Abilita ciclo di referenziazione"),
    ("Soft limits enable", "Abilita limiti software"),
    ("Status report mask", "Maschera rapporti di stato"),
    ("Stepper", "Passo-passo"),
    ("X steps per millimetre", "Passi X per millimetro"),
    ("APPLIED", "APPLICATO"),
    ("PENDING", "IN ATTESA"),
    ("REBOOT REQUIRED", "RIAVVIO NECESSARIO"),
    ("INVALID", "NON VALIDO"),
    ("FORMAT", "FORMATO"),
    ("WRITING", "SCRITTURA"),
    ("Search results", "Risultati della ricerca"),
    ("out of range", "fuori intervallo"),
    ("WCS OFFSETS", "ORIGINI WCS"),
    ("WCS OFFSET", "OFFSET WCS"),
    ("CLEAR G92", "AZZERA G92"),
    ("G28 STORED POSITION", "POSIZIONE MEMORIZZATA G28"),
    ("G30 STORED POSITION", "POSIZIONE MEMORIZZATA G30"),
    ("GOTO G28", "VAI A G28"),
    ("GOTO G30", "VAI A G30"),
    ("REPORTED", "RIPORTATO"),
    ("NOT REPORTED", "NON RIPORTATO"),
    ("NON-PERSISTENT", "NON PERSISTENTE"),
    ("ZERO ALL", "AZZERA TUTTO"),
    ("ZERO X", "AZZERA X"),
    ("ZERO Y", "AZZERA Y"),
    ("ZERO Z", "AZZERA Z"),
    ("CONFIRM CLEAR G92", "CONFERMA AZZERAMENTO G92"),
    ("CONFIRM G28 GOTO", "CONFERMA SPOSTAMENTO G28"),
    ("CONFIRM G30 GOTO", "CONFERMA SPOSTAMENTO G30"),
    ("EXPORT OFFSETS", "ESPORTA ORIGINI"),
    ("LOAD OFFSETS FILE", "CARICA FILE ORIGINI"),
    ("REFRESH $#", "AGGIORNA $#"),
    ("OFFSETS", "ORIGINI"),
    ("PREDEFINED POSITIONS / G92", "POSIZIONI PREDEFINITE / G92"),
    ("OLD G92 OFFSET", "VECCHIO OFFSET G92"),
    ("CONTROLLER TOOLS", "UTENSILI DEL CONTROLLER"),
    ("NEW TOOL", "NUOVO UTENSILE"),
    ("NEW RECORD", "NUOVO RECORD"),
    ("SAVE TOOL", "SALVA UTENSILE"),
    ("TOOL NUMBER", "NUMERO UTENSILE"),
    ("TOOL NAME", "NOME UTENSILE"),
    ("TOOL DIAMETER (MM)", "DIAMETRO UTENSILE (MM)"),
    ("TOOL LENGTH (MM)", "LUNGHEZZA UTENSILE (MM)"),
    ("POCKET NUMBER", "NUMERO TASCA"),
    ("DIAMETER (MM)", "DIAMETRO (MM)"),
    ("LENGTH (MM)", "LUNGHEZZA (MM)"),
    ("APP TOOL", "UTENSILE APP"),
    ("Profiles", "Profili"),
    ("BACKUPS", "BACKUP"),
    ("PROFILE", "PROFILO"),
    ("PROFILE RECORD", "RECORD PROFILO"),
    ("CONTROLLER PROFILE", "PROFILO CONTROLLER"),
    ("SAVE PROFILE", "SALVA PROFILO"),
    ("SAVE APP", "SALVA APP"),
    ("USE LOCAL DATA", "USA DATI LOCALI"),
    ("BACKUP BUNDLE", "PACCHETTO BACKUP"),
    ("VALIDATED FILE", "FILE VALIDATO"),
    ("LOCAL ONLY", "SOLO LOCALE"),
    ("NO CONTROLLER", "NESSUN CONTROLLER"),
    ("UNVERIFIED", "NON VERIFICATO"),
    ("PENDING CONFIRMATION", "IN ATTESA DI CONFERMA"),
    ("CORRUPT", "CORROTTO"),
    ("MISSING", "MANCANTE"),
    ("VALID", "VALIDO"),
    ("MACROS", "MACRO"),
    ("NEW MACRO", "NUOVA MACRO"),
    ("SAVE MACRO", "SALVA MACRO"),
    ("RUN MACRO", "ESEGUI MACRO"),
    ("MACRO NAME", "NOME MACRO"),
    ("MACRO COMMAND", "COMANDO MACRO"),
    ("RESET DEFAULTS", "RIPRISTINA PREDEFINITI"),
    ("USE KEY", "USA TASTO"),
    ("PRESS A KEY...", "PREMI UN TASTO..."),
    ("PRESS A KEY COMBINATION", "PREMI UNA COMBINAZIONE DI TASTI"),
    ("RECORD KEY", "REGISTRA TASTO"),
    ("CONTROLLER DIAGNOSTICS", "DIAGNOSTICA CONTROLLER"),
    ("OPEN PINS / LIMITS", "APRI PIN / LIMITI"),
    ("PINS / LIMITS", "PIN / LIMITI"),
    ("PIN MAP ($PINS)", "MAPPA PIN ($PINS)"),
    ("PIN STATE ($PINSTATE)", "STATO PIN ($PINSTATE)"),
    ("LAST EVENTS ($LEV)", "ULTIMI EVENTI ($LEV)"),
    ("CONTROLLER SLEEP", "SOSPENSIONE CONTROLLER"),
    ("PUT CONTROLLER TO SLEEP", "METTI CONTROLLER IN SOSPENSIONE"),
    ("CONFIRM SLEEP", "CONFERMA SOSPENSIONE"),
    ("DELETE MACRO", "ELIMINA MACRO"),
    ("CONFIRM WRITE", "CONFERMA SCRITTURA"),
    ("CONFIRM OFFSET WRITE", "CONFERMA SCRITTURA OFFSET"),
    ("CONFIRM REQUIRED", "CONFERMA NECESSARIA"),
    ("CONFIRM IMPORT", "CONFERMA IMPORTAZIONE"),
    ("ACKNOWLEDGED", "CONFERMATO"),
    ("1. ACKNOWLEDGE", "1. CONFERMA"),
    ("2. RESUME", "2. RIPRENDI"),
    ("OPEN", "APRI"),
    ("FOLDER", "CARTELLA"),
    ("PROGRAMS", "PROGRAMMI"),
    ("No file selected", "Nessun file selezionato"),
    (
        "Select a program file first",
        "Seleziona prima un file di programma",
    ),
    ("FILE", "FILE"),
    ("PATH", "PERCORSO"),
    ("NAME", "NOME"),
    ("NEW", "NUOVO"),
    ("SET", "IMPOSTA"),
    ("GO", "VAI"),
    ("CLEAR", "CANCELLA"),
    ("ALL", "TUTTI"),
    ("SELECT", "SELEZIONA"),
    ("SELECTED", "SELEZIONATO"),
    ("RELOAD", "RICARICA"),
    ("RUN", "ESEGUI"),
    ("ON", "ACCESO"),
    ("OFF", "SPENTO"),
    ("IMPORTED", "IMPORTATO"),
    ("EXPORTED", "ESPORTATO"),
    ("FAILED", "FALLITO"),
    ("COMPLETE", "COMPLETO"),
    ("PARTIAL", "PARZIALE"),
    ("ACTIVE", "ATTIVO"),
    ("CURRENT", "ATTUALE"),
    ("SETUP", "CONFIGURAZIONE"),
    ("VIEW", "VISTA"),
    ("FIT", "ADATTA"),
    ("CHECK", "CONTROLLO"),
    ("CHECKING", "CONTROLLO IN CORSO"),
    ("CHECK COMPLETE", "CONTROLLO COMPLETATO"),
    ("CHECK STOPPED", "CONTROLLO INTERROTTO"),
    (
        "CHECK PROGRAM (DRY RUN)",
        "CONTROLLA PROGRAMMA (PROVA A SECCO)",
    ),
    ("START CHECK", "AVVIA CONTROLLO"),
    ("DRY RUN - NO MOTION", "PROVA A SECCO - NESSUN MOVIMENTO"),
    (
        "WAITING FOR CONTROLLER CHECK MODE ($C)",
        "IN ATTESA DELLA MODALITÀ CONTROLLO ($C)",
    ),
    ("WHAT THE DRY RUN DOES", "COSA FA LA PROVA A SECCO"),
    ("MOTION IS SUPPRESSED", "MOVIMENTO SOPPRESSO"),
    ("SPINDLE / COOLANT", "MANDRINO / REFRIGERANTE"),
    ("WHEN FINISHED", "AL TERMINE"),
    (
        "The controller parses and streams the loaded program with check mode ($C) enabled. \
        Nothing in this sheet starts a real job or moves an axis.",
        "Il controller analizza ed esegue il programma caricato con la modalità controllo \
        ($C) attiva. Nulla in questo pannello avvia un lavoro reale o muove un asse.",
    ),
    (
        "MOTION IS SUPPRESSED: the controller acknowledges program lines without moving the \
        machine.",
        "MOVIMENTO SOPPRESSO: il controller conferma le righe del programma senza muovere \
        la macchina.",
    ),
    (
        "SPINDLE / COOLANT: M3, M4, M5, M7, M8 and M9 are parsed and acknowledged but never \
        run as real outputs.",
        "MANDRINO / REFRIGERANTE: M3, M4, M5, M7, M8 e M9 vengono analizzati e confermati \
        ma non attivano uscite reali.",
    ),
    (
        "WHEN FINISHED: the engine leaves check mode and waits for the machine to report \
        IDLE. The program stays loaded for a separate real START.",
        "AL TERMINE: il motore esce dalla modalità controllo e attende che la macchina \
        riporti IDLE. Il programma resta caricato per un AVVIO reale separato.",
    ),
    (
        "STOP remains available while the check streams; it never resumes or restarts the \
        run.",
        "ARRESTO resta disponibile durante il controllo; non riprende né riavvia mai \
        l'esecuzione.",
    ),
    ("CANCEL sends nothing.", "ANNULLA non invia nulla."),
    (
        "Parse and stream the loaded program in check mode ($C); no motion.",
        "Analizza ed esegue il programma caricato in modalità controllo ($C); nessun \
        movimento.",
    ),
    ("LOADED", "CARICATO"),
    ("LANGUAGE", "LINGUA"),
    ("FOLDERS", "CARTELLE"),
    ("LIVE", "ATTIVO"),
    ("JUMP", "VAI A"),
    ("DEFAULTS", "PREDEFINITI"),
    ("RUN AGAIN", "ESEGUI DI NUOVO"),
    ("APPLY TEXT", "APPLICA TESTO"),
    ("KEEP DRAFT", "MANTIENI BOZZA"),
    ("DIRECT", "DIRETTO"),
    ("SYNTHETIC", "SINTETICO"),
    ("TOUCH KEYPAD", "TASTIERA TOUCH"),
    ("NO LINE SELECTED", "NESSUNA RIGA SELEZIONATA"),
    ("REPORTS REQUESTED", "RAPPORTI RICHIESTI"),
    ("OFFLINE", "NON IN LINEA"),
    ("UNSAVED", "NON SALVATO"),
    ("USER-DEFINED", "DEFINITA DALL'UTENTE"),
    ("READ-ONLY", "SOLA LETTURA"),
    ("NOT LOADED", "NON CARICATO"),
    (
        "Choose a file for the job; loading never starts it.",
        "Scegli un file per il lavoro; il caricamento non lo avvia mai.",
    ),
    (
        "No folders or program files here",
        "Nessuna cartella o file di programma qui",
    ),
    (
        "Tap UP or type another folder path.",
        "Tocca SU o digita un altro percorso.",
    ),
    (
        "48 px rows | folders open on tap | hidden files and symlinks are not shown | .nc \
        .gcode .tap .ngc | 20 MB limit",
        "Righe da 48 px | le cartelle si aprono al tocco | file nascosti e collegamenti non \
        mostrati | .nc .gcode .tap .ngc | limite 20 MB",
    ),
    (
        "Tap a program file to select it",
        "Tocca un file di programma per selezionarlo",
    ),
    ("READY TO LOAD", "PRONTO DA CARICARE"),
    ("MOCK / BROWSE ONLY", "SIMULAZIONE / SOLO SFOGLIA"),
    ("LINK NOT READY", "COLLEGAMENTO NON PRONTO"),
    ("LIVE / READY", "ATTIVO / PRONTO"),
    ("DIR", "CART."),
    ("OPTIONAL STOP", "ARRESTO OPZIONALE"),
    ("SINGLE BLOCK", "BLOCCO SINGOLO"),
    ("BLOCK DELETE", "ELIMINA BLOCCO"),
    (
        "M1 optional stops are honored",
        "Gli arresti opzionali M1 sono rispettati",
    ),
    ("Hold after each block", "Pausa dopo ogni blocco"),
    (
        "Skip blocks marked with /",
        "Salta i blocchi contrassegnati con /",
    ),
    (
        "Each tap sends one command. A row keeps showing the value the controller reports; \
        it changes on the next poll, not when the button is pressed.",
        "Ogni tocco invia un comando. Una riga continua a mostrare il valore riportato dal \
        controller; cambia al poll successivo, non quando si preme il pulsante.",
    ),
    (
        "Mock mode: no controller",
        "Modalità simulazione: nessun controller",
    ),
    ("Link not ready", "Collegamento non pronto"),
    (
        "Locked while a job runs or pauses",
        "Bloccato durante l'esecuzione o la pausa",
    ),
    (
        "MOCK MODE: run options need a live controller",
        "MODALITÀ SIMULAZIONE: le opzioni richiedono un controller attivo",
    ),
    (
        "LINK NOT READY: run options are disabled until the handshake completes",
        "COLLEGAMENTO NON PRONTO: le opzioni sono disattivate finché l'handshake non \
        termina",
    ),
    (
        "BLOCK DELETE IS LOCKED WHILE A JOB IS RUNNING OR PAUSED",
        "ELIMINA BLOCCO È BLOCCATO MENTRE UN LAVORO È IN ESECUZIONE O IN PAUSA",
    ),
    (
        "Closing this sheet sends nothing.",
        "Chiudere questo pannello non invia nulla.",
    ),
    (
        "CANCEL, the scrim, and closing this sheet send nothing.",
        "ANNULLA, lo sfondo e la chiusura di questo pannello non inviano nulla.",
    ),
    ("CONFIRMING WILL", "CONFERMANDO"),
    ("RUN TO LINE", "ESEGUI FINO ALLA RIGA"),
    (
        "SET BREAKPOINT AT LINE",
        "IMPOSTA PUNTO DI INTERRUZIONE ALLA RIGA",
    ),
    ("START PROGRAM AT LINE 1", "AVVIA PROGRAMMA DALLA RIGA 1"),
    ("PAUSE BEFORE LINE", "PAUSA PRIMA DELLA RIGA"),
    (
        "LEAVE THE ACTIVE JOB AT ITS CURRENT POSITION",
        "LASCIA IL LAVORO ATTIVO NELLA POSIZIONE ATTUALE",
    ),
    ("RUN TO BLOCKED", "ESECUZIONE FINO A BLOCCATA"),
    ("SET BP + START", "IMPOSTA PI + AVVIA"),
    (
        "A job is already running or paused. Confirming only sets a breakpoint at the \
        selected line; it does not start, restart, or resume the job.",
        "Un lavoro è già in esecuzione o in pausa. La conferma imposta solo un punto di \
        interruzione sulla riga selezionata; non avvia, riavvia o riprende il lavoro.",
    ),
    (
        "All earlier lines still run from the beginning. The job starts at line 1 and \
        pauses before the selected line. Start-from-line is not supported.",
        "Tutte le righe precedenti vengono comunque eseguite dall'inizio. Il lavoro parte \
        dalla riga 1 e si ferma prima della riga selezionata. L'avvio da riga non è \
        supportato.",
    ),
    (
        "One step per press; off while typing. RESET DEFAULTS restores.",
        "Un passo per pressione; disattivata durante la digitazione. RIPRISTINA PREDEFINITI \
        ripristina.",
    ),
    (
        "Tap for one step; hold a mapped jog key to jog continuously. Off while typing.",
        "Tocca per un passo; tieni premuto un tasto di jog mappato per jog continuo. \
        Disattivato durante la digitazione.",
    ),
    (
        "Named single-line MDI commands, saved in this controller profile. RUN sends only \
        after a click; CONFIRM macros require a second explicit confirmation.",
        "Comandi MDI a riga singola con nome, salvati in questo profilo controller. ESEGUI \
        invia solo dopo un clic; le macro CONFERMA richiedono una seconda conferma \
        esplicita.",
    ),
    (
        "No user macros for this controller profile. Add a named, single-line MDI command; \
        it is saved only here and never runs on connect.",
        "Nessuna macro utente per questo profilo controller. Aggiungi un comando MDI a riga \
        singola con nome; viene salvato solo qui e non viene mai eseguito alla \
        connessione.",
    ),
    ("MACRO SAVES LOCKED", "SALVATAGGIO MACRO BLOCCATO"),
    ("MACRO RUNS LOCKED", "ESECUZIONE MACRO BLOCCATA"),
    (
        "Startup lines execute automatically on every controller reset and can command \
        motion, spindle, or coolant. Writes are allowed only while the controller is Idle; \
        nothing is written until APPLY and CONFIRM WRITE.",
        "Le righe di avvio vengono eseguite automaticamente a ogni reset del controller e \
        possono comandare movimento, mandrino o refrigerante. Le scritture sono consentite \
        solo con controller inattivo; nulla viene scritto finché non si preme APPLICA e \
        CONFERMA SCRITTURA.",
    ),
    (
        "WRITE NOT APPLIED - THE PREVIOUS VALUE IS STILL ACTIVE",
        "SCRITTURA NON APPLICATA - IL VALORE PRECEDENTE È ANCORA ATTIVO",
    ),
    (
        "This startup line executes automatically on every controller reset. It can command \
        motion, spindle, or coolant.",
        "Questa riga di avvio viene eseguita automaticamente a ogni reset del controller. \
        Può comandare movimento, mandrino o refrigerante.",
    ),
    ("EXACT TEXT TO STORE", "TESTO ESATTO DA SALVARE"),
    (
        "Previous controller value",
        "Valore precedente del controller",
    ),
    (
        "CONFIRM WRITE sends the exact text shown. The app does not update the row itself: \
        it reads the line back and reports the result.",
        "CONFERMA SCRITTURA invia il testo esatto mostrato. L'app non aggiorna la riga da \
        sola: rilegge la riga e riporta il risultato.",
    ),
    ("RESET EXECUTION", "ESECUZIONE AL RESET"),
    (
        "CONFIRM FIRST; CANCEL SENDS NOTHING",
        "CONFERMA PRIMA; ANNULLA NON INVIA NULLA",
    ),
    (
        "Sleep stops controller status polling until RESET or a status wake. The app does \
        not wake the controller automatically; the DRO and job view stay frozen until the \
        controller answers again.",
        "La sospensione ferma il polling di stato del controller fino a RESET o a un \
        risveglio di stato. L'app non risveglia il controller automaticamente; DRO e vista \
        lavoro restano congelati finché il controller non risponde di nuovo.",
    ),
    ("BEFORE YOU CONFIRM", "PRIMA DI CONFERMARE"),
    (
        "The controller must report Idle and no job may be running or paused. One confirmed \
        tap sends one sleep command; CANCEL sends nothing.",
        "Il controller deve riportare inattivo e nessun lavoro deve essere in esecuzione o \
        in pausa. Un tocco confermato invia un comando di sospensione; ANNULLA non invia \
        nulla.",
    ),
    (
        "Pin map, auxiliary pin levels, limit switches and last control events. Opening the \
        panel sends nothing; REFRESH sends only $PINS, $PINSTATE, $LIM and $LEV.",
        "Mappa pin, livelli pin ausiliari, finecorsa e ultimi eventi di controllo. Aprire \
        il pannello non invia nulla; AGGIORNA invia solo $PINS, $PINSTATE, $LIM e $LEV.",
    ),
    (
        "No $PINS report in this session",
        "Nessun rapporto $PINS in questa sessione",
    ),
    (
        "The controller reported no pin entries",
        "Il controller non ha riportato voci pin",
    ),
    ("pin entries reported", "voci pin riportate"),
    (
        "Applies immediately and is saved in app preferences; the controller is not \
        contacted.",
        "Si applica subito e viene salvata nelle preferenze dell'app; il controller non \
        viene contattato.",
    ),
    (
        "Export writes app prefs, identity, settings, offsets, tools, macros, startup \
        lines, and keymap; import validates before writing and never pushes to the \
        controller.",
        "L'esportazione scrive preferenze, identità, impostazioni, origini, utensili, \
        macro, righe di avvio e mappa tasti; l'importazione valida prima di scrivere e non \
        invia mai al controller.",
    ),
    (
        "The app owns this table; it persists in the per-controller profile. Controller \
        [T:] entries stay read-only and are never merged silently.",
        "Questa tabella appartiene all'app; viene salvata nel profilo del controller. Le \
        voci [T:] del controller restano in sola lettura e non vengono mai unite in \
        silenzio.",
    ),
    (
        "No controller tool table has been reported yet.",
        "Nessuna tabella utensili del controller ancora riportata.",
    ),
    (
        "The controller reports an empty tool table (0 entries).",
        "Il controller riporta una tabella utensili vuota (0 voci).",
    ),
    (
        "Reported by the controller through [T:...]; this panel never writes, overwrites, \
        or merges into the app table above.",
        "Riportato dal controller tramite [T:...]; questo pannello non scrive, sovrascrive \
        o unisce mai nella tabella app sopra.",
    ),
    ("STARTUP WRITES LOCKED", "SCRITTURE DI AVVIO BLOCCATE"),
    ("SAVE LOCKED", "SALVATAGGIO BLOCCATO"),
    ("KEYMAP NOT VALID", "MAPPA TASTI NON VALIDA"),
    ("Program has no lines", "Il programma non ha righe"),
    (
        "Load a non-empty source file to search it.",
        "Carica un file sorgente non vuoto per cercarlo.",
    ),
    ("Nothing matches", "Nessuna corrispondenza"),
    (
        "Switch to HIGHLIGHT or clear the field to restore the full list.",
        "Passa a EVIDENZIA o svuota il campo per ripristinare l'elenco completo.",
    ),
    (
        "DRAG ORBIT | SHIFT/MIDDLE PAN | WHEEL ZOOM",
        "TRASCINA ORBITA | SHIFT/TASTO CENTRALE PAN | ROTELLA ZOOM",
    ),
    ("GENERATED PROGRAM", "PROGRAMMA GENERATO"),
    (
        "No axes reported by the controller",
        "Nessun asse riportato dal controller",
    ),
    (
        "ONE WRITE AT A TIME | READ-BACK VERIFIED",
        "UNA SCRITTURA ALLA VOLTA | RILETTURA VERIFICATA",
    ),
    ("WRITES LOCKED", "SCRITTURE BLOCCATE"),
    ("IDLE: WRITES ENABLED", "INATTIVO: SCRITTURE ABILITATE"),
    (
        "MOCK MODE: NO CONTROLLER SETTINGS",
        "MODALITÀ SIMULAZIONE: NESSUNA IMPOSTAZIONE CONTROLLER",
    ),
    (
        "The generated editor is built from the controller's $$ values and $ES/$EG \
        metadata. The design-review mock supplies neither. Start without --mock to connect \
        the in-process FlexiHAL demo controller.",
        "L'editor generato si basa sui valori $$ del controller e sui metadati $ES/$EG. La \
        simulazione di revisione non fornisce né gli uni né gli altri. Avvia senza --mock \
        per collegare il controller demo FlexiHAL in-process.",
    ),
    ("WAITING FOR SETTINGS", "IN ATTESA DELLE IMPOSTAZIONI"),
    (
        "The handshake has not reported $$, $ES, or $EG yet. The form appears as soon as \
        the controller answers.",
        "L'handshake non ha ancora riportato $$, $ES o $EG. Il modulo appare appena il \
        controller risponde.",
    ),
    (
        "THE CONTROLLER REPORTED NO SETTINGS",
        "IL CONTROLLER NON HA RIPORTATO IMPOSTAZIONI",
    ),
    (
        "$$ returned no values and $ES/$EG returned no metadata. The editor stays empty \
        rather than inventing a setting list.",
        "$$ non ha restituito valori e $ES/$EG non hanno restituito metadati. L'editor \
        resta vuoto invece di inventare un elenco.",
    ),
    (
        "EXAMPLE SHAPE (NO VALUES ARE WRITTEN)",
        "ESEMPIO (NESSUN VALORE VIENE SCRITTO)",
    ),
    ("LOW", "BASSO"),
    ("HIGH", "ALTO"),
    ("dashed = missing", "tratteggiato = mancante"),
    ("QUEUED", "IN CODA"),
    ("BLANK ALLOWED", "VUOTO CONSENTITO"),
    ("BIT 0 ENABLES THE OTHERS", "IL BIT 0 ABILITA GLI ALTRI"),
    ("INFERRED FROM VALUE", "DEDOTTO DAL VALORE"),
    (
        "No saved maps for this controller profile yet.",
        "Nessuna mappa salvata per questo profilo controller.",
    ),
    (
        "Read-only view; opening does not apply compensation or start a job",
        "Vista in sola lettura; l'apertura non applica compensazioni né avvia un lavoro",
    ),
    (
        "Nothing is applied automatically",
        "Nulla viene applicato automaticamente",
    ),
    (
        "No automatic retry or resume. Recover the machine, then run again.",
        "Nessun nuovo tentativo o ripresa automatica. Ripristina la macchina, poi esegui di \
        nuovo.",
    ),
    (
        "every gate must pass before RUN",
        "ogni controllo deve passare prima di ESEGUI",
    ),
    ("PROBE CYCLES", "CICLI DI SONDA"),
    (
        "Pick a card, inspect the preflight, then press RUN",
        "Scegli una scheda, controlla il preflight, poi premi ESEGUI",
    ),
    (
        "millimetres; tap a value to edit",
        "millimetri; tocca un valore per modificarlo",
    ),
    ("recorded vs live", "registrato vs attuale"),
    ("per controller profile", "per profilo controller"),
    ("APPLY OFFSET", "APPLICA OFFSET"),
    ("NOT SENT YET", "NON ANCORA INVIATO"),
    ("EXACT MDI LINE", "RIGA MDI ESATTA"),
    (
        "CURRENT DRO (DISPLAY UNITS)",
        "DRO ATTUALE (UNITÀ VISUALIZZATE)",
    ),
    (
        "DRO CHANGED SINCE THIS SHEET OPENED; CANCEL AND RECHECK",
        "DRO CAMBIATA DALL'APERTURA; ANNULLA E RICONTROLLA",
    ),
    ("READ-ONLY $# PARAMETERS", "PARAMETRI $# IN SOLA LETTURA"),
    (
        "X / Y / Z IN DISPLAY UNITS",
        "X / Y / Z IN UNITÀ VISUALIZZATE",
    ),
    ("TAP TO EDIT", "TOCCA PER MODIFICARE"),
    (
        "RESTORE WORK OFFSETS - PREVIEW",
        "RIPRISTINA ORIGINI PEZZO - ANTEPRIMA",
    ),
    (
        "G10 L2 LINES IN THIS FILE (VALUES IN MILLIMETRES)",
        "RIGHE G10 L2 IN QUESTO FILE (VALORI IN MILLIMETRI)",
    ),
    (
        "THE FILE BEGINS WITH G21: EVERY VALUE IS INTERPRETED IN MILLIMETRES.",
        "IL FILE INIZIA CON G21: OGNI VALORE È INTERPRETATO IN MILLIMETRI.",
    ),
    ("LOAD AS PROGRAM", "CARICA COME PROGRAMMA"),
    (
        "LOCKED: A JOB IS ACTIVE; STOP IT BEFORE LOADING A PROGRAM",
        "BLOCCATO: UN LAVORO È ATTIVO; ARRESTALO PRIMA DI CARICARE UN PROGRAMMA",
    ),
    ("SET X", "IMPOSTA X"),
    ("SET Y", "IMPOSTA Y"),
    ("SET Z", "IMPOSTA Z"),
    ("SET AXIS", "IMPOSTA ASSE"),
    ("MDI TEXT", "TESTO MDI"),
    ("SPINDLE RPM", "GIRI MANDRINO"),
    ("SETTING", "IMPOSTAZIONE"),
    ("OFFSET", "OFFSET"),
    ("TOKENS + DIGITS", "TOKEN + CIFRE"),
    ("G10 L20 | DISPLAY UNITS", "G10 L20 | UNITÀ VISUALIZZATE"),
    ("RANGE N/A", "INTERVALLO N/D"),
    ("TOOL FIELD", "CAMPO UTENSILE"),
    ("PROBE FIELD (mm)", "CAMPO SONDA (mm)"),
    ("Demo FlexiHAL", "Demo FlexiHAL"),
    (
        "In-process FlexiHAL simulator over the loopback transport. No serial port is \
        opened.",
        "Simulatore FlexiHAL in-process sul trasporto loopback. Nessuna porta seriale viene \
        aperta.",
    ),
    (
        "Same engine, handshake, streamer and safety gateway as a real controller.",
        "Stesso motore, handshake, streamer e gateway di sicurezza di un controller reale.",
    ),
    (
        "Tap a port to select it; nothing opens until CONNECT.",
        "Tocca una porta per selezionarla; nulla si apre prima di CONNETTI.",
    ),
    (
        "Line settings are fixed at 8 data bits, no parity, one stop bit. DTR reset pulses \
        are not implemented in M2.",
        "Le impostazioni di linea sono fisse: 8 bit dati, nessuna parità, 1 bit di stop. \
        Gli impulsi di reset DTR non sono implementati in M2.",
    ),
    ("serial port", "porta seriale"),
    ("NOT REQUESTED", "NON RICHIESTO"),
    ("READING CONTROLLER", "LETTURA CONTROLLER"),
    (
        "CONTROLLER BUSY - RETRYING",
        "CONTROLLER OCCUPATO - NUOVO TENTATIVO",
    ),
    (
        "No $PINS report has arrived in this session.",
        "Nessun rapporto $PINS è arrivato in questa sessione.",
    ),
    (
        "No $PINSTATE report has arrived in this session.",
        "Nessun rapporto $PINSTATE è arrivato in questa sessione.",
    ),
    (
        "The controller reported no auxiliary pin levels.",
        "Il controller non ha riportato livelli pin ausiliari.",
    ),
    (
        "No $LIM report has arrived in this session.",
        "Nessun rapporto $LIM è arrivato in questa sessione.",
    ),
    (
        "No $LEV report has arrived in this session.",
        "Nessun rapporto $LEV è arrivato in questa sessione.",
    ),
    (
        "The controller reports no control or limit events yet.",
        "Il controller non riporta ancora eventi di controllo o limite.",
    ),
    ("PROBE PLAN", "PIANO SONDA"),
    ("CUSTOM", "PERSONALIZZATA"),
    (
        "No settings in this group",
        "Nessuna impostazione in questo gruppo",
    ),
    ("No setting matches", "Nessuna impostazione corrisponde a"),
    (
        "Pick another group in the rail.",
        "Scegli un altro gruppo nella barra.",
    ),
    (
        "Search matches a number, name, or group. Clear the search to browse groups.",
        "La ricerca corrisponde a numero, nome o gruppo. Svuota la ricerca per sfogliare i \
        gruppi.",
    ),
    ("CONTROLLER STATE", "STATO CONTROLLER"),
    (
        "CONTROLLER DIAGNOSTIC DATA - READ-ONLY",
        "DATI DIAGNOSTICI CONTROLLER - SOLA LETTURA",
    ),
    ("SAVE BLOCKED", "SALVATAGGIO BLOCCATO"),
    (
        "KEPT: offsets are unchanged. Apply stays available.",
        "MANTENUTO: le origini sono invariate. L'applicazione resta disponibile.",
    ),
    ("COMMAND", "COMANDO"),
    ("Ask before running", "Chiedi prima di eseguire"),
    (
        "New macros default to CONFIRM. A direct macro runs on one click.",
        "Le nuove macro usano CONFERMA per impostazione predefinita. Una macro diretta \
        viene eseguita con un clic.",
    ),
    ("EXACT COMMAND TO SEND", "COMANDO ESATTO DA INVIARE"),
    (
        "Jogging is allowed while the job waits; the Jog panel stays active. The job will \
        not send another line until you resume.",
        "Il jog è consentito mentre il lavoro attende; il pannello Jog resta attivo. Il \
        lavoro non invierà altre righe finché non riprendi.",
    ),
    (
        "Jogging is allowed while the job waits; the Jog panel stays active. The job will \
        not send another line until you resume. RESET and STOP above remain available.",
        "Il jog è consentito mentre il lavoro attende; il pannello Jog resta attivo. Il \
        lavoro non invierà altre righe finché non riprendi. RESET e STOP in alto restano \
        disponibili.",
    ),
    ("VERIFYING", "VERIFICA IN CORSO"),
    ("VERIFIED", "VERIFICATO"),
    ("MISMATCH", "NON CORRISPONDENTE"),
    ("MEASUREMENT ONLY", "SOLO MISURA"),
    ("READY TO APPLY", "PRONTO DA APPLICARE"),
    ("REVERTED", "RIPRISTINATO"),
    ("FEED", "AVANZAMENTO"),
    ("RETRACT", "RITIRO"),
    ("CENTER X", "CENTRO X"),
    ("CENTER Y", "CENTRO Y"),
    ("ORIGIN X WORK", "ORIGINE X PEZZO"),
    ("ORIGIN Y WORK", "ORIGINE Y PEZZO"),
    ("SURFACE Z MACH", "Z SUPERFICIE MACCHINA"),
    ("PROBE OFFSET X", "OFFSET SONDA X"),
    ("PROBE OFFSET Y", "OFFSET SONDA Y"),
    (
        "MOCK MODE: browsing only; loading needs a live controller",
        "MODALITÀ SIMULAZIONE: solo sfoglia; il caricamento richiede un controller attivo",
    ),
    (
        "LINK NOT READY: finish the connection handshake before loading a program",
        "COLLEGAMENTO NON PRONTO: completa l'handshake prima di caricare un programma",
    ),
    (
        "JOB ACTIVE: stop the running job before loading another program",
        "LAVORO ATTIVO: arresta il lavoro in esecuzione prima di caricare un altro \
        programma",
    ),
    (
        "MOCK MODE: a check run needs a live controller",
        "MODALITÀ SIMULAZIONE: il controllo richiede un controller attivo",
    ),
    (
        "LINK NOT READY: finish the connection handshake before a check run",
        "COLLEGAMENTO NON PRONTO: completa l'handshake prima del controllo",
    ),
    (
        "MACHINE LOCKED: clear Alarm or Door before a check run",
        "MACCHINA BLOCCATA: elimina Allarme o Sportello prima del controllo",
    ),
    (
        "NO PROGRAM: load a program before a check run",
        "NESSUN PROGRAMMA: carica un programma prima del controllo",
    ),
    (
        "JOB ACTIVE: stop the running job before a check run",
        "LAVORO ATTIVO: arresta il lavoro in esecuzione prima del controllo",
    ),
    (
        "MACHINE NOT IDLE: a check run starts only from IDLE",
        "MACCHINA NON INATTIVA: il controllo parte solo da IDLE",
    ),
    (
        "CHECK RUN ACTIVE: wait for the controller to finish check mode",
        "CONTROLLO ATTIVO: attendi che il controller termini la modalità controllo",
    ),
    (
        "PROBE PROGRAM: a generated probe plan cannot be checked from the job toolbar",
        "PROGRAMMA SONDA: un piano sonda generato non può essere controllato dalla barra \
        lavoro",
    ),
    (
        "1. Confirm the physical E-stop and independent spindle stop. 2. Open \
        CONNECTION and select the serial port. 3. Wait for READY and Idle. 4. Check \
        the DRO with one small jog. 5. Verify units, WCS, and soft limits before \
        loading a program.",
        "1. Conferma l'E-stop fisico e l'arresto indipendente del mandrino. 2. Apri \
        CONNESSIONE e seleziona la porta seriale. 3. Attendi PRONTO e Inattivo. 4. \
        Controlla il DRO con un piccolo jog. 5. Verifica unità, WCS e limiti software \
        prima di caricare un programma.",
    ),
    ("ADD LINE NUMBERS", "AGGIUNGI NUMERI DI RIGA"),
    ("APP PREFERENCE ONLY", "SOLO PREFERENZA APP"),
    (
        "APPLY is a separate, confirmed step and is allowed only while the controller \
        is Idle.",
        "APPLICA è un passaggio separato e confermato, consentito solo mentre il \
        controller è Inattivo.",
    ),
    (
        "APPLY sends exactly one Command::SetSetting through the engine. The engine \
        writes the value, reads it back, and only then marks it APPLIED. CANCEL sends \
        nothing.",
        "APPLICA invia esattamente un Command::SetSetting tramite il motore. Il motore \
        scrive il valore, lo rilegge e solo allora lo contrassegna come APPLICATO. \
        ANNULLA non invia nulla.",
    ),
    ("AXIS", "ASSE"),
    (
        "After setup, use the normal Connection panel. Start with the simulator or with \
        the spindle off, then run only status, DRO, and small jogs.",
        "Dopo la configurazione, usa il normale pannello Connessione. Inizia con il \
        simulatore o con il mandrino spento, poi esegui solo stato, DRO e piccoli jog.",
    ),
    ("BACK", "INDIETRO"),
    (
        "Back-and-forth raster at each depth, with a safe-Z retract between passes. The \
        wizard validates every field and caps the program before LOAD AS PROGRAM; it \
        never starts the job.",
        "Raster avanti e indietro a ogni profondità, con ritiro in Z sicura tra le \
        passate. La procedura guidata convalida ogni campo e limita il programma prima \
        di CARICA COME PROGRAMMA; non avvia mai il lavoro.",
    ),
    ("CALCULATE", "CALCOLA"),
    ("CALIBRATION CONFIRMATION", "CONFERMA CALIBRAZIONE"),
    ("CALIBRATION DISTANCE (mm)", "DISTANZA DI CALIBRAZIONE (mm)"),
    (
        "CALIBRATION WRITES LOCKED",
        "SCRITTURE DI CALIBRAZIONE BLOCCATE",
    ),
    (
        "Calculate to preview the new steps/mm and the change before writing.",
        "Calcola per visualizzare in anteprima i nuovi steps/mm e la variazione prima \
        di scrivere.",
    ),
    (
        "DEFAULT DISPLAY UNITS",
        "UNITÀ DI VISUALIZZAZIONE PREDEFINITE",
    ),
    ("DISPLAY UNITS", "UNITÀ DI VISUALIZZAZIONE"),
    (
        "ESTIMATED MACHINE BOUNDS (WCS + G92)",
        "LIMITI MACCHINA STIMATI (WCS + G92)",
    ),
    (
        "EVERY ITEM IS REQUIRED BEFORE NEXT",
        "OGNI VOCE È OBBLIGATORIA PRIMA DI SUCCESSIVO",
    ),
    ("FINISH SETUP", "TERMINA CONFIGURAZIONE"),
    ("FIRST CONNECTION ORDER", "ORDINE DELLA PRIMA CONNESSIONE"),
    ("GENERATE PREVIEW", "GENERA ANTEPRIMA"),
    (
        "GENERATE PREVIEW to validate the fields and compute passes, path length, and \
        time.",
        "GENERA ANTEPRIMA per convalidare i campi e calcolare passate, lunghezza \
        percorso e tempo.",
    ),
    ("GENERATED / NOT CHECKED", "GENERATO / NON CONTROLLATO"),
    (
        "HEIGHT MAP / COMPENSATED: loading a transform replaces the compensated program \
        and its Revert original. Keep an external copy if needed.",
        "MAPPA DELLE ALTEZZE / COMPENSATA: caricare una trasformazione sostituisce il \
        programma compensato e il suo originale di ripristino. Conserva una copia \
        esterna se necessario.",
    ),
    (
        "I measured the actual travel and want to write this steps/mm value",
        "Ho misurato la corsa effettiva e voglio scrivere questo valore steps/mm",
    ),
    (
        "I understand this loads the previewed source and does not start it",
        "Comprendo che questo carica il sorgente visualizzato in anteprima e non lo \
        avvia",
    ),
    (
        "I understand this program is generated, not checked against the machine",
        "Comprendo che questo programma è generato e non controllato rispetto alla \
        macchina",
    ),
    ("INCHES (IN)", "POLLICI (IN)"),
    (
        "INFORMATIONAL CHECK: WCS changes, G10 writes, G92 offsets, and tool-length \
        offsets are not enforced here. Verify the setup and soft limits before \
        cutting.",
        "CONTROLLO INFORMATIVO: modifiche WCS, scritture G10, offset G92 e offset \
        lunghezza utensile non vengono applicati qui. Verifica il setup e i limiti \
        software prima di tagliare.",
    ),
    ("INFORMATIONAL ONLY", "SOLO INFORMATIVO"),
    (
        "Informational only: START is never blocked. The machine estimate adds the \
        reported active WCS plus G92; TLO is not included. Missing travel or offset \
        data shows NOT CHECKED, never a safety claim. No controller writes.",
        "Solo informativo: AVVIA non viene mai bloccato. La stima della macchina somma \
        il WCS attivo riportato e G92; il TLO non è incluso. Se mancano dati di corsa \
        o offset, viene mostrato NON CONTROLLATO, mai un'affermazione di sicurezza. \
        Nessuna scrittura al controller.",
    ),
    (
        "Jog or run the test move separately, then enter the commanded distance and the \
        measured travel. The wizard only calculates and writes the steps/mm setting; \
        it never moves an axis.",
        "Esegui il movimento di prova separatamente con jog o MDI, poi inserisci la \
        distanza comandata e la corsa misurata. La procedura guidata calcola e scrive \
        solo l'impostazione steps/mm; non muove mai un asse.",
    ),
    ("MACHINE SETUP", "CONFIGURAZIONE MACCHINA"),
    ("MATERIAL SURFACING", "SPIANATURA MATERIALE"),
    (
        "MATERIAL SURFACING WIZARD",
        "PROCEDURA GUIDATA SPIANATURA MATERIALE",
    ),
    ("MILLIMETRES (MM)", "MILLIMETRI (MM)"),
    ("NEW STEPS/MM", "NUOVI STEPS/MM"),
    ("NO MOTION COMMANDED", "NESSUN MOVIMENTO COMANDATO"),
    (
        "NO PROGRAM: load a program before transforming its source",
        "NESSUN PROGRAMMA: carica un programma prima di trasformarne il sorgente",
    ),
    ("NOT COMPLETED", "NON COMPLETATA"),
    (
        "No generated program yet. Open the wizard, validate the fields, and preview.",
        "Nessun programma generato. Apri la procedura guidata, convalida i campi e \
        visualizza l'anteprima.",
    ),
    (
        "No tools in the app table. Add one so the M6 sheet can show a name, diameter, \
        and length instead of only the number.",
        "Nessun utensile nella tabella dell'app. Aggiungine uno così la finestra M6 può \
        mostrare nome, diametro e lunghezza invece del solo numero.",
    ),
    ("OFF omits both M3 and M5", "SPENTO omette sia M3 sia M5"),
    ("OLD STEPS/MM", "VECCHI STEPS/MM"),
    ("OPEN SETUP", "APRI CONFIGURAZIONE"),
    ("OPEN WIZARD", "APRI PROCEDURA GUIDATA"),
    ("PREVIEW", "ANTEPRIMA"),
    ("PREVIEW FIRST", "PRIMA L'ANTEPRIMA"),
    ("PREVIEW SUMMARY", "RIEPILOGO ANTEPRIMA"),
    (
        "PROGRAM BOUNDS (WORK COORDINATES)",
        "LIMITI PROGRAMMA (COORDINATE PEZZO)",
    ),
    (
        "PROGRAM EXCEEDS REPORTED TRAVEL",
        "IL PROGRAMMA SUPERA LA CORSA RIPORTATA",
    ),
    ("PROGRAM LIMITS", "LIMITI PROGRAMMA"),
    ("PROPOSED STEPS/MM", "STEPS/MM PROPOSTI"),
    ("REMIND ME LATER", "RICORDAMELO PIÙ TARDI"),
    (
        "REOPENS AT SAFETY TO REVIEW OR UPDATE THE CHOICES",
        "SI RIAPRE SU SICUREZZA PER RIVEDERE O AGGIORNARE LE SCELTE",
    ),
    ("REPORTED TRAVEL", "CORSA RIPORTATA"),
    ("REVIEW & APPLY", "RIVEDI E APPLICA"),
    ("SAFETY ITEMS ACKNOWLEDGED", "VOCI DI SICUREZZA CONFERMATE"),
    (
        "SETUP IS NOT A MACHINE CERTIFICATION",
        "LA CONFIGURAZIONE NON È UNA CERTIFICAZIONE DELLA MACCHINA",
    ),
    ("SOURCE TRANSFORMS", "TRASFORMAZIONI SORGENTE"),
    ("SPINDLE ON (M3 S... / M5)", "MANDRINO ON (M3 S... / M5)"),
    ("START ANYWAY", "AVVIA COMUNQUE"),
    (
        "START HERE BEFORE THE FIRST COMMANDED MOVE",
        "INIZIA QUI PRIMA DEL PRIMO MOVIMENTO COMANDATO",
    ),
    ("STEPPER CALIBRATION", "CALIBRAZIONE PASSO-PASSO"),
    ("STRIP COMMENTS", "RIMUOVI COMMENTI"),
    (
        "THIS WIZARD DOES NOT TEST THE MACHINE",
        "QUESTA PROCEDURA GUIDATA NON TESTA LA MACCHINA",
    ),
    ("TRANSFORM PREVIEW", "ANTEPRIMA TRASFORMAZIONE"),
    (
        "The controller, its firmware, and the physical safety wiring remain \
        authoritative. Acknowledge only what you have checked on this machine.",
        "Il controller, il suo firmware e il cablaggio di sicurezza fisico restano \
        autorevoli. Conferma solo ciò che hai verificato su questa macchina.",
    ),
    (
        "The loaded program's estimated machine bounds leave the travel reported by the \
        controller. START is held until you confirm.",
        "I limiti macchina stimati del programma caricato escono dalla corsa riportata \
        dal controller. AVVIA resta in attesa finché non confermi.",
    ),
    (
        "The loaded source is unchanged until LOAD AS PROGRAM. This loads one new \
        program and never starts it.",
        "Il sorgente caricato resta invariato fino a CARICA COME PROGRAMMA. Questo \
        carica un nuovo programma e non lo avvia mai.",
    ),
    (
        "The program is generated from these fields. It is not checked against this \
        machine's travel, offsets, tool, or stock. LOAD AS PROGRAM does not start it.",
        "Il programma è generato da questi campi. Non è controllato rispetto a corsa, \
        offset, utensile o materiale di questa macchina. CARICA COME PROGRAMMA non lo \
        avvia.",
    ),
    (
        "The wizard never connects, homes, jogs, probes, or writes controller settings.",
        "La procedura guidata non si connette, non referenzia, non esegue jog, non \
        sonda e non scrive impostazioni del controller.",
    ),
    (
        "The wizard reviews the physical safety checklist, the app display unit \
        preference, and what the first connection should look like. It never connects, \
        homes, jogs, probes, or writes controller settings.",
        "La procedura guidata illustra la lista di controllo di sicurezza fisica, la \
        preferenza delle unità di visualizzazione dell'app e come dovrebbe avvenire la \
        prima connessione. Non si connette, non referenzia, non esegue jog, non sonda \
        e non scrive impostazioni del controller.",
    ),
    (
        "This choice is saved for the next session. The wizard does not send G20 or G21 \
        and does not change the controller's units. The live controller parser state \
        still controls the current session.",
        "Questa scelta viene salvata per la sessione successiva. La procedura guidata \
        non invia G20 o G21 e non cambia le unità del controller. Lo stato del parser \
        del controller attivo controlla ancora la sessione corrente.",
    ),
    (
        "This records that you reviewed the checklist and chose an app preference. It \
        does not test, certify, or configure the machine. Verify every motion, limit, \
        and safety device yourself.",
        "Questo registra che hai esaminato la lista di controllo e scelto una \
        preferenza dell'app. Non testa, certifica o configura la macchina. Verifica \
        personalmente ogni movimento, limite e dispositivo di sicurezza.",
    ),
    (
        "This writes one controller setting. It does not move the machine: the test \
        move is your separate jog or MDI command. The controller stores steps/mm in \
        EEPROM; avoid unnecessary writes.",
        "Questo scrive un'impostazione del controller. Non muove la macchina: il \
        movimento di prova è il tuo comando jog o MDI separato. Il controller \
        memorizza steps/mm in EEPROM; evita scritture inutili.",
    ),
    ("UTILITY FIELD", "CAMPO UTILITÀ"),
    ("WRITE LOCKED", "SCRITTURA BLOCCATA"),
    ("STOCK WIDTH", "LARGHEZZA MATERIALE"),
    ("STOCK DEPTH", "PROFONDITÀ MATERIALE"),
    ("CUTTER DIA", "DIA FRESA"),
    ("STEPOVER", "PASSO TRASVERSALE"),
    ("TOTAL DEPTH", "PROFONDITÀ TOTALE"),
    ("DEPTH/PASS", "PROFONDITÀ/PASSATA"),
    ("PLUNGE FEED", "AVANZAMENTO ENTRATA"),
    ("LINE START", "RIGA INIZIALE"),
    ("LINE STEP", "PASSO RIGA"),
    ("COMMANDED MOVE", "MOVIMENTO COMANDATO"),
    ("MEASURED MOVE", "MOVIMENTO MISURATO"),
    ("CALCULATED", "CALCOLATO"),
    ("CONFIRMED", "CONFERMATO"),
    (
        "ALLOWED: comments `(...)` or `;...`, `G4 P0..60`, `M0`, `M1`, `M5`, `M9`.",
        "CONSENTITI: commenti `(...)` o `;...`, `G4 P0..60`, `M0`, `M1`, `M5`, `M9`.",
    ),
    ("ANGLE", "ANGOLO"),
    ("APPLY TLO", "APPLICA TLO"),
    (
        "BACKUP EXPORTED THIS SESSION",
        "BACKUP ESPORTATO IN QUESTA SESSIONE",
    ),
    (
        "BACKUP EXPORTED THIS SESSION - RESET ACKNOWLEDGED",
        "BACKUP ESPORTATO IN QUESTA SESSIONE - RESET CONFERMATO",
    ),
    ("BACKUP FILE", "FILE DI BACKUP"),
    ("BACKUP REQUIRED", "BACKUP RICHIESTO"),
    ("CALCULATOR ONLY", "SOLO CALCOLATORE"),
    (
        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY",
        "SOLO CALCOLATORE - L'APP NON MUOVE E NON REGOLA IL PORTALE",
    ),
    ("CLEAR ALL HISTORY", "CANCELLA TUTTA LA CRONOLOGIA"),
    ("CLEAR ALL HISTORY?", "CANCELLARE TUTTA LA CRONOLOGIA?"),
    (
        "CLEAR ASKS FOR CONFIRMATION FIRST",
        "LA CANCELLAZIONE RICHIEDE PRIMA UNA CONFERMA",
    ),
    ("CLEAR HISTORY", "CANCELLA CRONOLOGIA"),
    ("CLEAR TLO", "AZZERA TLO"),
    ("CONTROLLER RESET", "RESET CONTROLLER"),
    (
        "DATA LOSS ACKNOWLEDGED - RESET MAY PROCEED",
        "PERDITA DATI CONFERMATA - RESET CONSENTITO",
    ),
    (
        "DATA LOSS ACKNOWLEDGED BY THE OPERATOR",
        "PERDITA DATI CONFERMATA DALL'OPERATORE",
    ),
    ("DELTA", "DELTA"),
    ("DELTA (TOTAL)", "DELTA (TOTALE)"),
    ("DESTRUCTIVE", "DISTRUTTIVO"),
    (
        "DROP PROGRAM TO LOAD",
        "TRASCINA IL PROGRAMMA PER CARICARLO",
    ),
    (
        "Drop exactly one program file",
        "Trascina esattamente un file di programma",
    ),
    (
        "Drop one .nc, .gcode, .tap, or .ngc program file",
        "Trascina un file .nc, .gcode, .tap o .ngc",
    ),
    ("EMPTY", "VUOTO"),
    ("EXPORT BACKUP", "ESPORTA BACKUP"),
    (
        "EXPORT IN FLIGHT - WAIT FOR THE WRITE TO FINISH",
        "ESPORTAZIONE IN CORSO - ATTENDI LA FINE DELLA SCRITTURA",
    ),
    (
        "Enter a folder path first",
        "Inserisci prima un percorso di cartella",
    ),
    (
        "Enter millimetres. The span is the distance between the two measurement points; the \
         readings are the Y values at the left and right points. CALCULATE is local \
         arithmetic.",
        "Inserisci millimetri. La distanza è tra i due punti di misura; le letture sono i \
         valori Y ai punti sinistro e destro. CALCOLA è aritmetica locale.",
    ),
    (
        "Finished jobs are stored in app.json on this computer. The newest 100 records are \
         kept; the card shows the newest 10.",
        "I lavori terminati sono salvati in app.json su questo computer. Vengono conservati \
         gli ultimi 100 record; la scheda mostra i 10 più recenti.",
    ),
    (
        "GANTRY IS SQUARE WITHIN MEASUREMENT RESOLUTION; NO CORRECTION INDICATED",
        "IL PORTALE È IN SQUADRO ENTRO LA RISOLUZIONE DI MISURA; NESSUNA CORREZIONE INDICATA",
    ),
    ("GANTRY MEASUREMENT (mm)", "MISURA PORTALE (mm)"),
    ("GANTRY SQUARING", "SQUADRATURA PORTALE"),
    (
        "GANTRY SQUARING CALCULATOR",
        "CALCOLATORE SQUADRATURA PORTALE",
    ),
    (
        "I HAVE A BACKUP OR ACCEPT DATA LOSS",
        "HO UN BACKUP O ACCETTO LA PERDITA DI DATI",
    ),
    ("JOB HISTORY", "CRONOLOGIA LAVORI"),
    ("LEFT FORWARD", "SINISTRO AVANTI"),
    (
        "LEFT SIDE IS {amount} mm FORWARD; MOVE IT BACK {amount} mm OR SHIM THE RIGHT SIDE",
        "IL LATO SINISTRO È AVANTI DI {amount} mm; ARRETRALO DI {amount} mm O SPESSA IL LATO \
         DESTRO",
    ),
    ("LINES", "RIGHE"),
    (
        "LOCAL ONLY - NOT SENT TO THE CONTROLLER",
        "SOLO LOCALE - NON INVIATO AL CONTROLLER",
    ),
    (
        "LOCAL ONLY - NOT SENT TO THE CONTROLLER. Records are kept in app.json on this \
         computer, newest first.",
        "SOLO LOCALE - NON INVIATO AL CONTROLLER. I record sono conservati in app.json su \
         questo computer, dal più recente.",
    ),
    ("LOCAL PROFILE", "PROFILO LOCALE"),
    ("LOCAL TIME", "ORA LOCALE"),
    ("MAX", "MAX"),
    ("MEASUREMENT RESULT", "RISULTATO MISURA"),
    ("MOST DESTRUCTIVE", "PIÙ DISTRUTTIVO"),
    ("Max", "Max"),
    (
        "Measure the Y distance from a common reference to the left and right ends of the \
         gantry, enter the span between those points, and calculate the delta, angle, and \
         per-side correction. Follow the machine's squaring procedure for any adjustment.",
        "Misura la distanza Y da un riferimento comune alle estremità sinistra e destra del \
         portale, inserisci la distanza tra quei punti e calcola delta, angolo e correzione \
         per lato. Per qualsiasi regolazione segui la procedura di squadratura della \
         macchina.",
    ),
    ("NEWEST FIRST", "PIÙ RECENTI PRIMA"),
    (
        "NO BACKUP EXPORTED THIS SESSION",
        "NESSUN BACKUP ESPORTATO IN QUESTA SESSIONE",
    ),
    ("NO JOBS RECORDED", "NESSUN LAVORO REGISTRATO"),
    (
        "NO MOTION / NO WRITE",
        "NESSUN MOVIMENTO / NESSUNA SCRITTURA",
    ),
    ("NO PER-SIDE CORRECTION", "NESSUNA CORREZIONE PER LATO"),
    ("NOT SAVED", "NON SALVATO"),
    (
        "No measurement yet. Enter the span and both Y readings in the calculator.",
        "Nessuna misura ancora. Inserisci la distanza e le due letture Y nel calcolatore.",
    ),
    (
        "No motion, probing, spindle-on, coolant-on, tool change, coordinate, or system \
         commands.",
        "Nessun movimento, tastatura, mandrino acceso, refrigerante acceso, cambio utensile, \
         coordinata o comando di sistema.",
    ),
    (
        "No recent files yet. A loaded local file appears here.",
        "Nessun file recente. Un file locale caricato compare qui.",
    ),
    ("OF", "DI"),
    ("ONE COMMAND", "UN COMANDO"),
    ("OPEN CALCULATOR", "APRI CALCOLATORE"),
    ("OPEN HISTORY", "APRI CRONOLOGIA"),
    ("OPEN MACROS", "APRI MACRO"),
    (
        "One .nc, .gcode, .tap, or .ngc file. Loading never starts the job.",
        "Un file .nc, .gcode, .tap o .ngc. Il caricamento non avvia mai il lavoro.",
    ),
    ("PER-SIDE", "PER LATO"),
    ("PER-SIDE (PIVOT CENTRE)", "PER LATO (CENTRO DI ROTAZIONE)"),
    (
        "PIVOT CENTRE: MOVE THE LEFT SIDE BACK {amount} mm AND THE RIGHT SIDE FORWARD \
         {amount} mm",
        "CENTRO DI ROTAZIONE: ARRETRA IL LATO SINISTRO DI {amount} mm E AVANZA IL LATO DESTRO \
         DI {amount} mm",
    ),
    (
        "PIVOT CENTRE: MOVE THE RIGHT SIDE BACK {amount} mm AND THE LEFT SIDE FORWARD \
         {amount} mm",
        "CENTRO DI ROTAZIONE: ARRETRA IL LATO DESTRO DI {amount} mm E AVANZA IL LATO SINISTRO \
         DI {amount} mm",
    ),
    ("POST", "POST"),
    (
        "POST MACRO - after the final safe-Z retract",
        "MACRO POST - dopo il rientro finale in Z sicura",
    ),
    ("PRE", "PRE"),
    (
        "PRE MACRO - after the safety preamble",
        "MACRO PRE - dopo il preambolo di sicurezza",
    ),
    ("PREFLIGHT OK", "CONTROLLO PRELIMINARE OK"),
    ("PROBE DEFAULTS", "PREDEFINITI SONDA"),
    ("PROBE MACROS", "MACRO SONDA"),
    (
        "Press CALCULATE to validate the fields and show the correction.",
        "Premi CALCOLA per validare i campi e mostrare la correzione.",
    ),
    ("RECENT", "RECENTE"),
    ("RECENT FILES", "FILE RECENTI"),
    ("REFUSED", "RIFIUTATO"),
    ("REMOVE", "RIMUOVI"),
    (
        "RESET GATES READY - TWO-STEP CONFIRMATION REQUIRED",
        "CONDIZIONI RESET PRONTE - RICHIESTA CONFERMA IN DUE PASSAGGI",
    ),
    ("RESET LOCKED", "RESET BLOCCATO"),
    ("RESULT", "ESITO"),
    ("RIGHT FORWARD", "DESTRO AVANTI"),
    (
        "RIGHT SIDE IS {amount} mm FORWARD; MOVE IT BACK {amount} mm OR SHIM THE LEFT SIDE",
        "IL LATO DESTRO È AVANTI DI {amount} mm; ARRETRALO DI {amount} mm O SPESSA IL LATO \
         SINISTRO",
    ),
    (
        "Recent file was not removed",
        "Il file recente non è stato rimosso",
    ),
    (
        "Recent files were not cleared",
        "I file recenti non sono stati cancellati",
    ),
    ("SAVE DEFAULTS", "SALVA PREDEFINITI"),
    ("SAVE MACROS", "SALVA MACRO"),
    ("SAVED", "SALVATO"),
    (
        "SAVED PER CONTROLLER PROFILE",
        "SALVATI PER PROFILO CONTROLLER",
    ),
    ("SHOWING", "VISUALIZZATI"),
    ("SPAN BETWEEN POINTS", "DISTANZA TRA I PUNTI"),
    ("SQUARE", "IN SQUADRO"),
    ("STEP", "PASSO"),
    (
        "STOCK INFERRED FROM PROGRAM BOUNDS | NOT A MEASURED MODEL",
        "PEZZO DEDOTTO DAI LIMITI DEL PROGRAMMA | NON È UN MODELLO MISURATO",
    ),
    ("STOCK OFF", "PEZZO OFF"),
    ("STOCK ON", "PEZZO ON"),
    ("STORED", "SALVATI"),
    ("TAP FOR", "TOCCA PER"),
    ("THEME", "TEMA"),
    (
        "The controller may reset or re-handshake after this command. The app does not \
         reconnect, home, jog, or resume anything. CANCEL sends nothing.",
        "Il controller può resettarsi o rifare l'handshake dopo questo comando. L'app non si \
         riconnette, non referenzia, non muove e non riprende nulla. ANNULLA non invia nulla.",
    ),
    (
        "There is no APPLY. Move or shim the gantry by hand, then measure again.",
        "Non esiste APPLICA. Muovi o spessa il portale a mano, poi misura di nuovo.",
    ),
    (
        "These lines run inside the generated probe program and are still subject to every \
         preflight gate. SAVE MACROS writes only the local controller profile: no controller \
         command, setting, offset, or start.",
        "Queste righe vengono eseguite nel programma di tastatura generato e restano soggette \
         a ogni controllo preliminare. SALVA MACRO scrive solo il profilo locale del \
         controller: nessun comando, impostazione, origine o avvio al controller.",
    ),
    (
        "This removes every stored job record from app.json on this computer. It does not \
         change the loaded program or send anything to the controller.",
        "Rimuove ogni record di lavoro salvato da app.json su questo computer. Non modifica \
         il programma caricato e non invia nulla al controller.",
    ),
    (
        "bounded pre/post lines; local profile; still subject to preflight",
        "righe pre/post limitate; profilo locale; comunque soggette al controllo preliminare",
    ),
    ("bytes and", "byte e"),
    ("correction is not finite", "non è finita"),
    (
        "drop one program file",
        "trascina un solo file di programma",
    ),
    (
        "file contains binary data, not a G-code program",
        "il file contiene dati binari, non un programma G-code",
    ),
    ("file is", "il file è"),
    ("file is empty", "il file è vuoto"),
    (
        "file is not valid UTF-8 text",
        "il file non è testo UTF-8 valido",
    ),
    ("is empty", "è vuoto"),
    (
        "is not a .nc, .gcode, .tap, or .ngc program",
        "non è un programma .nc, .gcode, .tap o .ngc",
    ),
    ("is not a readable folder", "non è una cartella leggibile"),
    ("is not a regular file", "non è un file regolare"),
    ("is not finite", "non è finito"),
    ("is out of range", "è fuori intervallo"),
    ("items were dropped", "elementi sono stati trascinati"),
    ("lines per macro.", "righe per macro."),
    (
        "local JSON only; nothing is sent to the controller",
        "solo JSON locale; nulla viene inviato al controller",
    ),
    (
        "mm | probe tip - controlled point | plan shifts controlled by -offset",
        "mm | punta sonda - punto controllato | il piano sposta il controllato di -offset",
    ),
    ("must be a whole number", "deve essere un numero intero"),
    ("must be at most", "può essere al massimo"),
    ("must be finite", "deve essere finito"),
    ("must be greater than 0", "deve essere maggiore di 0"),
    ("must be numeric", "deve essere numerico"),
    ("must be within", "deve essere tra"),
    (
        "the limit for the file browser is",
        "il limite per il browser file è",
    ),
    ("REVIEW", "RIEPILOGO"),
    (
        "PHYSICAL E-STOP LOCATED AND TESTED",
        "E-STOP FISICO INDIVIDUATO E COLLAUDATO",
    ),
    (
        "SPINDLE CAN BE STOPPED INDEPENDENTLY",
        "IL MANDRINO PUÒ ESSERE FERMATO INDIPENDENTEMENTE",
    ),
    (
        "MACHINE TRAVEL AND SOFT LIMITS VERIFIED",
        "CORSA MACCHINA E LIMITI SOFTWARE VERIFICATI",
    ),
    (
        "WORKHOLDING AND TOOL CLEARANCE CHECKED",
        "FISSAGGIO PEZZO E SPAZIO UTENSILE CONTROLLATI",
    ),
    (
        "SIMULATOR OR SPINDLE-OFF FIRST CONNECTION",
        "PRIMA CONNESSIONE CON SIMULATORE O A MANDRINO SPENTO",
    ),
    (
        "Know where it is, reach it, and prove it stops the machine before any commanded \
         move.",
        "Sai dove si trova, riesci a raggiungerlo e hai verificato che fermi la macchina \
         prima di qualsiasi movimento comandato.",
    ),
    (
        "Use the VFD or the power control, not this sender, to stop the cutter.",
        "Usa il VFD o il controllo di alimentazione, non questo sender, per fermare la fresa.",
    ),
    (
        "Confirm the physical travel and the controller's soft-limit settings.",
        "Conferma la corsa fisica e le impostazioni dei limiti software del controller.",
    ),
    (
        "Secure the stock, clear the tool, and keep hands clear of every axis.",
        "Fissa il pezzo, libera l'utensile e tieni le mani lontane da tutti gli assi.",
    ),
    (
        "Learn the sender in the simulator, then connect with the spindle off and run only \
         status, DRO, and small jogs.",
        "Impara il sender nel simulatore, poi collega a mandrino spento ed esegui solo stato, \
         DRO e piccoli jog.",
    ),
    ("STOPPED", "FERMATO"),
    ("CONFIRM APPLY TLO", "CONFERMA APPLICA TLO"),
    ("CONFIRM CLEAR TLO", "CONFERMA AZZERA TLO"),
    ("HIGH CONTRAST", "ALTO CONTRASTO"),
    ("LIGHT", "CHIARO"),
    ("DARK", "SCURO"),
    ("LEFT Y READING", "LETTURA Y SINISTRA"),
    ("RIGHT Y READING", "LETTURA Y DESTRA"),
    (
        "OFFSETS AND CONTROLLER TOOL DATA",
        "ORIGINI E DATI UTENSILI DEL CONTROLLER",
    ),
    (
        "CORE SETTING DEFAULTS",
        "PREDEFINITI IMPOSTAZIONI PRINCIPALI",
    ),
    ("DRIVER AND PLUGIN DEFAULTS", "PREDEFINITI DRIVER E PLUGIN"),
    (
        "ALL SETTINGS, PARAMETERS, STARTUP LINES, AND BUILD INFO",
        "TUTTE LE IMPOSTAZIONI, I PARAMETRI, LE RIGHE DI AVVIO E LE INFO BUILD",
    ),
    (
        "Offsets and controller tool data. Local profile, backups, and saved maps stay.",
        "Origini e dati utensili del controller. Profilo locale, backup e mappe salvate \
         restano.",
    ),
    (
        "Core setting defaults. Offsets and tool data stay.",
        "Predefiniti delle impostazioni principali. Origini e dati utensili restano.",
    ),
    (
        "Driver and plugin defaults. Core settings, offsets, and tool data stay.",
        "Predefiniti di driver e plugin. Impostazioni principali, origini e dati utensili \
         restano.",
    ),
    (
        "All settings, parameters, startup lines, and build info. Most destructive.",
        "Tutte le impostazioni, i parametri, le righe di avvio e le info build. Il più \
         distruttivo.",
    ),
    (
        "Offsets ($#) and controller tool data are reset. Work coordinate systems, G28/G30, \
         G92, TLO, and the controller's tool records may be lost. Local profile data, \
         backups, and saved height maps stay.",
        "Le origini ($#) e i dati utensili del controller vengono azzerati. I sistemi di \
         coordinate di lavoro, G28/G30, G92, TLO e i record utensili del controller possono \
         andare persi. Dati del profilo locale, backup e mappe di altezza salvate restano.",
    ),
    (
        "Core controller settings return to their defaults. Offsets and controller tool data \
         stay. Export a backup before changing settings again.",
        "Le impostazioni principali del controller tornano ai valori predefiniti. Origini e \
         dati utensili del controller restano. Esporta un backup prima di modificare di nuovo \
         le impostazioni.",
    ),
    (
        "Driver and plugin settings return to their defaults. Core settings, offsets, and \
         controller tool data stay.",
        "Le impostazioni di driver e plugin tornano ai valori predefiniti. Impostazioni \
         principali, origini e dati utensili del controller restano.",
    ),
    (
        "ALL settings, parameters, startup lines, and build info are reset. Offsets, tool \
         data, and controller parameters may be unrecoverable. This is the most destructive \
         reset and the controller will usually re-handshake.",
        "TUTTE le impostazioni, i parametri, le righe di avvio e le info build vengono \
         azzerati. Origini, dati utensili e parametri del controller possono essere \
         irrecuperabili. Questo è il reset più distruttivo e il controller di solito rifà \
         l'handshake.",
    ),
    (
        "Requested read-only `$#` refresh.",
        "Richiesto aggiornamento in sola lettura `$#`.",
    ),
    (
        "Requested read-only `$$` settings dump.",
        "Richiesto dump impostazioni in sola lettura `$$`.",
    ),
    (
        "No read-only report is defined for driver parameters.",
        "Nessun report in sola lettura definito per i parametri driver.",
    ),
    (
        "Requested read-only `$#` and `$$` refreshes.",
        "Richiesti aggiornamenti in sola lettura `$#` e `$$`.",
    ),
    (
        "MOCK MODE: controller reset needs a live controller",
        "MODALITÀ MOCK: il reset del controller richiede un controller attivo",
    ),
    (
        "NOT LIVE: connect the controller before reset",
        "NON ATTIVO: collega il controller prima del reset",
    ),
    (
        "LINK NOT READY: reset is disabled until the handshake completes",
        "COLLEGAMENTO NON PRONTO: reset disabilitato finché l'handshake non è completato",
    ),
    (
        "ALARM OR DOOR: clear the alarm or door state before reset",
        "ALLARME O PORTA: cancella lo stato di allarme o porta prima del reset",
    ),
    (
        "JOB ACTIVE: stop or finish the job before reset",
        "LAVORO ATTIVO: arresta o termina il lavoro prima del reset",
    ),
    (
        "TOOL CHANGE PENDING: finish or stop the tool change before reset",
        "CAMBIO UTENSILE IN SOSPESO: termina o arresta il cambio prima del reset",
    ),
    (
        "PROBE ACTIVE: finish or abort the probe run before reset",
        "SONDA ATTIVA: termina o interrompi l'esecuzione della sonda prima del reset",
    ),
    (
        "HEIGHT MAP BUSY: finish the Apply or Revert before reset",
        "MAPPA ALTEZZE OCCUPATA: completa Applica o Ripristina prima del reset",
    ),
    (
        "PENDANT ACTIVE (MPG:1): release the pendant before reset",
        "PENDANT ATTIVO (MPG:1): rilascia il pendant prima del reset",
    ),
    (
        "SETTINGS WRITE PENDING: finish or discard it before reset",
        "SCRITTURA IMPOSTAZIONI IN SOSPESO: completala o scartala prima del reset",
    ),
    (
        "OFFSET WRITE PENDING: finish the write and refresh before reset",
        "SCRITTURA ORIGINE IN SOSPESA: completa la scrittura e aggiorna prima del reset",
    ),
    (
        "NOT IDLE: reset is available only while the controller reports Idle",
        "NON INATTIVO: il reset è disponibile solo mentre il controller riporta Idle",
    ),
    (
        "BACKUP EXPORT IN FLIGHT: wait for the export to finish",
        "ESPORTAZIONE BACKUP IN CORSO: attendi la fine dell'esportazione",
    ),
    (
        "BACKUP NOT ACKNOWLEDGED: export a backup or accept data loss first",
        "BACKUP NON CONFERMATO: esporta un backup o accetta prima la perdita di dati",
    ),
    (
        "LOCAL PROFILE IS NOT CHANGED AUTOMATICALLY",
        "IL PROFILO LOCALE NON VIENE MODIFICATO AUTOMATICAMENTE",
    ),
    (
        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY. Follow the machine's \
         squaring procedure, move or shim the gantry by hand, and verify the result with a \
         fresh measurement before the next cut.",
        "SOLO CALCOLATORE - L'APP NON MUOVE E NON REGOLA IL PORTALE. Segui la procedura di \
         squadratura della macchina, muovi o spessa il portale a mano e verifica il risultato \
         con una nuova misura prima del taglio successivo.",
    ),
    (
        "Positive delta and angle: the right measurement point is further along +Y (forward) \
         than the left. Negative: the left point is further along +Y. Per-side is delta / 2: \
         with the centre fixed, each end moves half the delta in opposite directions.",
        "Delta e angolo positivi: il punto di misura destro è più avanti lungo +Y rispetto al \
         sinistro. Negativi: il punto sinistro è più avanti lungo +Y. Per lato è delta / 2: \
         con il centro fisso, ogni estremità si sposta di metà delta in direzioni opposte.",
    ),
    ("SENT", "INVIATO"),
    (
        "NO PROGRAM: load a program before rotating its source",
        "NESSUN PROGRAMMA: carica un programma prima di ruotarne il sorgente",
    ),
    (
        "NO PROGRAM: load a program before scaling its source",
        "NESSUN PROGRAMMA: carica un programma prima di scalarne il sorgente",
    ),
    ("NO WARNINGS", "NESSUN AVVISO"),
    (
        "No program loaded: the source check has nothing to scan.",
        "Nessun programma caricato: il controllo del sorgente non ha nulla da analizzare.",
    ),
    ("PREVIEW ROTATION", "ANTEPRIMA ROTAZIONE"),
    ("PREVIEW SCALE", "ANTEPRIMA SCALA"),
    ("PROGRAM CHECK", "CONTROLLO PROGRAMMA"),
    ("REVIEW PREVIEW", "RIVEDI ANTEPRIMA"),
    ("ROTATE PROGRAM", "RUOTA PROGRAMMA"),
    ("ROTATION PREVIEW", "ANTEPRIMA ROTAZIONE"),
    ("SCALE PREVIEW", "ANTEPRIMA SCALATURA"),
    ("SCALE PROGRAM", "SCALA PROGRAMMA"),
    (
        "UNIFORM / WORK PIVOT",
        "UNIFORME / CENTRO DI ROTAZIONE PEZZO",
    ),
    ("USE PROBE ANGLE", "USA ANGOLO SONDA"),
    ("WARNINGS", "AVVISI"),
    (
        "WORK PIVOT / CCW +",
        "CENTRO DI ROTAZIONE PEZZO / ANTIORARIO +",
    ),
    (
        "I understand this loads the rotated source and does not start it",
        "Comprendo che questo carica il sorgente ruotato e non lo avvia",
    ),
    (
        "I understand this loads the scaled source and does not start it",
        "Comprendo che questo carica il sorgente scalato e non lo avvia",
    ),
    (
        "INFORMATIONAL - START IS NOT BLOCKED. NOT A MACHINE/TRAVEL CHECK.",
        "INFORMATIVO - AVVIO NON BLOCCATO. NON È UN CONTROLLO MACCHINA/CORSA.",
    ),
    (
        "Missing app tool-table entries are warnings, not blockers. The check reads source text \
         and parsed counts only; it cannot verify controller state, offsets, tool length, \
         fixtures, or actual travel. No controller command is sent.",
        "Le voci mancanti nella tabella utensili dell'app sono avvisi, non blocchi. Il controllo \
         legge solo il testo del sorgente e i conteggi analizzati; non può verificare lo stato del \
         controller, le origini, la lunghezza utensile, i fissaggi o la corsa effettiva. Non viene \
         inviato alcun comando al controller.",
    ),
    (
        "No feed, spindle, tool, machine-coordinate, or probe warnings in this program.",
        "Nessun avviso di avanzamento, mandrino, utensile, coordinate macchina o sonda in questo \
         programma.",
    ),
    (
        "Pivot is in work/program coordinates; a positive angle is counter-clockwise. Unsupported \
         programs are refused before any load.",
        "Il centro di rotazione è in coordinate pezzo/programma; un angolo positivo è antiorario. \
         I programmi non supportati vengono rifiutati prima di qualsiasi caricamento.",
    ),
    (
        "Rotation is reported only. USE PROBE ANGLE fills the Utility angle with -deviation; the \
         pivot stays in work coordinates and nothing is loaded or sent.",
        "La rotazione è solo riportata. USA ANGOLO SONDA inserisce -deviazione nell'angolo di \
         Utilità; il centro di rotazione resta in coordinate pezzo e non viene caricato né inviato \
         nulla.",
    ),
    (
        "SOURCE SCAN WAS TRUNCATED: the check did not read every line.",
        "SCANSIONE SORGENTE TRONCATA: il controllo non ha letto ogni riga.",
    ),
    (
        "Uniform positive factor about a pivot in work/program millimetres. Feeds, speeds, and Z \
         depths are unchanged. Unsupported programs are refused before any load.",
        "Fattore positivo uniforme attorno a un centro di rotazione in millimetri pezzo/programma. \
         Avanzamenti, velocità e profondità Z restano invariati. I programmi non supportati \
         vengono rifiutati prima di qualsiasi caricamento.",
    ),
];
