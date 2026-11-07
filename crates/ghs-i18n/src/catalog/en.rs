//! en catalog.
//!
//! Keys are the exact English UI literals; lookups are case-sensitive.

pub(crate) static CATALOG: &[(&str, &str)] = &[
    ("Job", "Job"),
    ("Probe", "Probe"),
    ("Offset", "Offset"),
    ("Utility", "Utility"),
    ("Tools", "Tools"),
    ("Settings", "Settings"),
    ("JOB", "JOB"),
    ("PROBING", "PROBING"),
    ("WORK OFFSETS", "WORK OFFSETS"),
    ("UTILITIES", "UTILITIES"),
    ("TOOL TABLE", "TOOL TABLE"),
    ("SETTINGS", "SETTINGS"),
    ("Console", "Console"),
    ("Program", "Program"),
    ("Light", "Light"),
    ("Apply", "Apply"),
    ("APPLY", "APPLY"),
    ("Save", "Save"),
    ("SAVE", "SAVE"),
    ("Cancel", "Cancel"),
    ("CANCEL", "CANCEL"),
    ("Close", "Close"),
    ("CLOSE", "CLOSE"),
    ("Edit", "Edit"),
    ("EDIT", "EDIT"),
    ("Delete", "Delete"),
    ("DEL", "DEL"),
    ("Refresh", "Refresh"),
    ("REFRESH", "REFRESH"),
    ("Export", "Export"),
    ("EXPORT", "EXPORT"),
    ("Import", "Import"),
    ("IMPORT", "IMPORT"),
    ("Validate", "Validate"),
    ("VALIDATE", "VALIDATE"),
    ("Confirm", "Confirm"),
    ("CONFIRM", "CONFIRM"),
    ("Keep", "Keep"),
    ("KEEP", "KEEP"),
    ("Revert", "Revert"),
    ("REVERT", "REVERT"),
    ("Zero", "Zero"),
    ("ZERO", "ZERO"),
    ("Home", "Home"),
    ("HOME", "HOME"),
    ("Unlock", "Unlock"),
    ("UNLOCK", "UNLOCK"),
    ("Macro", "Macro"),
    ("Startup Lines", "Startup Lines"),
    ("STARTUP LINES", "STARTUP LINES"),
    ("Keymap", "Keymap"),
    ("KEYMAP", "KEYMAP"),
    ("Diagnostics", "Diagnostics"),
    ("DIAGNOSTICS", "DIAGNOSTICS"),
    ("RESET", "RESET"),
    ("STOP", "STOP"),
    ("Stop", "Stop"),
    ("SLEEP", "SLEEP"),
    ("CONNECT", "CONNECT"),
    ("DISCONNECT", "DISCONNECT"),
    ("CONNECTION", "CONNECTION"),
    ("TRANSPORT", "TRANSPORT"),
    ("CURRENT LINK", "CURRENT LINK"),
    ("SERIAL PORT", "SERIAL PORT"),
    ("Demo", "Demo"),
    ("Serial port", "Serial port"),
    ("No serial ports found", "No serial ports found"),
    (
        "Disconnect before switching transport.",
        "Disconnect before switching transport.",
    ),
    ("MOCK", "MOCK"),
    ("CONNECTING", "CONNECTING"),
    ("READY", "READY"),
    ("DISCONNECTED", "DISCONNECTED"),
    ("ERROR", "ERROR"),
    ("NOT CONNECTED", "NOT CONNECTED"),
    ("STALE", "STALE"),
    ("Alarm", "Alarm"),
    ("Door", "Door"),
    ("LOAD PROGRAM", "LOAD PROGRAM"),
    ("START", "START"),
    ("PAUSE", "PAUSE"),
    ("RESUME", "RESUME"),
    ("RUN OPTIONS", "RUN OPTIONS"),
    ("RAPID", "RAPID"),
    ("Rapid", "Rapid"),
    ("ETA", "ETA"),
    ("TIME", "TIME"),
    ("TOOL CHANGE REQUIRED", "TOOL CHANGE REQUIRED"),
    ("TOOL CHANGE", "TOOL CHANGE"),
    ("RUN TO", "RUN TO"),
    ("SET BREAKPOINT", "SET BREAKPOINT"),
    ("BP", "BP"),
    ("SEARCH", "SEARCH"),
    ("FILTER", "FILTER"),
    ("HIGHLIGHT", "HIGHLIGHT"),
    ("NEXT", "NEXT"),
    ("PREV", "PREV"),
    ("NO PROGRAM LOADED", "NO PROGRAM LOADED"),
    ("NO PROGRAM", "NO PROGRAM"),
    ("PROGRAM", "PROGRAM"),
    ("HEIGHT MAP / COMPENSATED", "HEIGHT MAP / COMPENSATED"),
    ("Cut", "Cut"),
    ("Plunge", "Plunge"),
    ("IDLE", "IDLE"),
    ("RUNNING", "RUNNING"),
    ("PAUSED", "PAUSED"),
    ("COMPLETED", "COMPLETED"),
    ("ABORTED", "ABORTED"),
    ("INTERRUPTED", "INTERRUPTED"),
    ("JOB ACTIVE", "JOB ACTIVE"),
    ("JOB RUNNING", "JOB RUNNING"),
    ("STOP JOB", "STOP JOB"),
    ("TAP A LINE TO SELECT", "TAP A LINE TO SELECT"),
    ("No matching lines", "No matching lines"),
    ("NO MATCH", "NO MATCH"),
    ("SAVE LOG", "SAVE LOG"),
    ("DRO", "DRO"),
    ("JOG", "JOG"),
    ("MDI", "MDI"),
    ("FEED OVERRIDE", "FEED OVERRIDE"),
    ("SPINDLE OVERRIDE", "SPINDLE OVERRIDE"),
    ("RAPID OVERRIDE", "RAPID OVERRIDE"),
    ("COOLANT", "COOLANT"),
    ("SPINDLE", "SPINDLE"),
    ("SIGNALS", "SIGNALS"),
    ("WORK", "WORK"),
    ("SEND", "SEND"),
    ("UP", "UP"),
    ("DN", "DN"),
    ("CONT", "CONT"),
    ("CW", "CW"),
    ("CCW", "CCW"),
    ("Spindle off", "Spindle off"),
    ("RESET 100%", "RESET 100%"),
    ("Feed", "Feed"),
    ("MACHINE", "MACHINE"),
    ("TOOL", "TOOL"),
    ("POCKET", "POCKET"),
    ("RADIUS", "RADIUS"),
    ("TOOLPATH", "TOOLPATH"),
    ("CYCLES", "CYCLES"),
    ("CYCLE PARAMETERS", "CYCLE PARAMETERS"),
    ("GRID SUMMARY", "GRID SUMMARY"),
    ("PREFLIGHT", "PREFLIGHT"),
    ("PROBE RUN", "PROBE RUN"),
    ("RAW PRB (machine mm)", "RAW PRB (machine mm)"),
    ("SAVED MAPS", "SAVED MAPS"),
    ("SAVED MAP", "SAVED MAP"),
    ("MEASUREMENT FRAME", "MEASUREMENT FRAME"),
    ("HEIGHT MAP RESULT", "HEIGHT MAP RESULT"),
    ("CONFIRM APPLY", "CONFIRM APPLY"),
    ("PROBE OFFSET X/Y", "PROBE OFFSET X/Y"),
    ("PROBE OFFSET", "PROBE OFFSET"),
    (
        "SEARCH BOUND (CONTROLLED-POINT XY)",
        "SEARCH BOUND (CONTROLLED-POINT XY)",
    ),
    ("APPLY HEIGHT MAP", "APPLY HEIGHT MAP"),
    ("CLOSE MAP", "CLOSE MAP"),
    ("SAVE MAP", "SAVE MAP"),
    ("EXPORT CSV", "EXPORT CSV"),
    ("RETRY PREFLIGHT", "RETRY PREFLIGHT"),
    ("CONFIGURE", "CONFIGURE"),
    ("Probe connected", "Probe connected"),
    ("ABORT", "ABORT"),
    ("Height map", "Height map"),
    ("Z touch plate", "Z touch plate"),
    ("XY touch plate", "XY touch plate"),
    ("Edge X", "Edge X"),
    ("Edge Y", "Edge Y"),
    ("Corner", "Corner"),
    ("Center circle", "Center circle"),
    ("Center rectangle", "Center rectangle"),
    ("Rotation", "Rotation"),
    ("Tool length", "Tool length"),
    ("MEASURED Z", "MEASURED Z"),
    ("SAFE Z", "SAFE Z"),
    ("SURFACE Z", "SURFACE Z"),
    ("MAP Z OFFSET", "MAP Z OFFSET"),
    ("MAP TRAVEL", "MAP TRAVEL"),
    ("GRID ORIGIN", "GRID ORIGIN"),
    ("COLUMNS", "COLUMNS"),
    ("ROWS", "ROWS"),
    ("TOUCHES", "TOUCHES"),
    ("STEP X", "STEP X"),
    ("STEP Y", "STEP Y"),
    ("EDGE OFFSET", "EDGE OFFSET"),
    ("SEPARATION", "SEPARATION"),
    ("PLATE THICK", "PLATE THICK"),
    ("PLATE CORNER X", "PLATE CORNER X"),
    ("PLATE CORNER Y", "PLATE CORNER Y"),
    ("PROBE HEIGHT", "PROBE HEIGHT"),
    ("PROBE DIA", "PROBE DIA"),
    ("REFERENCE Z", "REFERENCE Z"),
    ("RECT WIDTH", "RECT WIDTH"),
    ("RECT HEIGHT", "RECT HEIGHT"),
    ("EXPECTED X", "EXPECTED X"),
    ("EXPECTED Y", "EXPECTED Y"),
    ("EXPECTED Z", "EXPECTED Z"),
    ("Measured height", "Measured height"),
    ("Measured width", "Measured width"),
    ("Measured X edge", "Measured X edge"),
    ("Measured Y edge", "Measured Y edge"),
    ("SURFACE", "SURFACE"),
    ("COMPENSATION", "COMPENSATION"),
    ("PLAN", "PLAN"),
    ("MAP", "MAP"),
    ("SAFETY", "SAFETY"),
    ("PROBE VERIFIED", "PROBE VERIFIED"),
    ("NOT CONFIRMED", "NOT CONFIRMED"),
    ("TRIGGERED", "TRIGGERED"),
    ("NOT READY", "NOT READY"),
    ("UNITS", "UNITS"),
    ("TOOL LENGTH DELTA", "TOOL LENGTH DELTA"),
    ("RMS fit error", "RMS fit error"),
    ("Deviation", "Deviation"),
    ("Nominal", "Nominal"),
    ("WORK XYZ", "WORK XYZ"),
    ("MACHINE XYZ", "MACHINE XYZ"),
    ("SEARCH SETTINGS", "SEARCH SETTINGS"),
    ("All settings", "All settings"),
    ("General", "General"),
    ("Axes", "Axes"),
    ("Limits", "Limits"),
    ("Network address", "Network address"),
    ("Work mode", "Work mode"),
    ("Step pulse time", "Step pulse time"),
    ("Step idle delay", "Step idle delay"),
    ("Maximum spindle speed", "Maximum spindle speed"),
    ("Homing cycle enable", "Homing cycle enable"),
    ("Soft limits enable", "Soft limits enable"),
    ("Status report mask", "Status report mask"),
    ("Stepper", "Stepper"),
    ("X steps per millimetre", "X steps per millimetre"),
    ("APPLIED", "APPLIED"),
    ("PENDING", "PENDING"),
    ("REBOOT REQUIRED", "REBOOT REQUIRED"),
    ("INVALID", "INVALID"),
    ("FORMAT", "FORMAT"),
    ("WRITING", "WRITING"),
    ("Search results", "Search results"),
    ("out of range", "out of range"),
    ("WCS OFFSETS", "WCS OFFSETS"),
    ("WCS OFFSET", "WCS OFFSET"),
    ("CLEAR G92", "CLEAR G92"),
    ("G28 STORED POSITION", "G28 STORED POSITION"),
    ("G30 STORED POSITION", "G30 STORED POSITION"),
    ("GOTO G28", "GOTO G28"),
    ("GOTO G30", "GOTO G30"),
    ("REPORTED", "REPORTED"),
    ("NOT REPORTED", "NOT REPORTED"),
    ("NON-PERSISTENT", "NON-PERSISTENT"),
    ("ZERO ALL", "ZERO ALL"),
    ("ZERO X", "ZERO X"),
    ("ZERO Y", "ZERO Y"),
    ("ZERO Z", "ZERO Z"),
    ("CONFIRM CLEAR G92", "CONFIRM CLEAR G92"),
    ("CONFIRM G28 GOTO", "CONFIRM G28 GOTO"),
    ("CONFIRM G30 GOTO", "CONFIRM G30 GOTO"),
    ("EXPORT OFFSETS", "EXPORT OFFSETS"),
    ("LOAD OFFSETS FILE", "LOAD OFFSETS FILE"),
    ("REFRESH $#", "REFRESH $#"),
    ("OFFSETS", "OFFSETS"),
    ("PREDEFINED POSITIONS / G92", "PREDEFINED POSITIONS / G92"),
    ("OLD G92 OFFSET", "OLD G92 OFFSET"),
    ("CONTROLLER TOOLS", "CONTROLLER TOOLS"),
    ("NEW TOOL", "NEW TOOL"),
    ("NEW RECORD", "NEW RECORD"),
    ("SAVE TOOL", "SAVE TOOL"),
    ("TOOL NUMBER", "TOOL NUMBER"),
    ("TOOL NAME", "TOOL NAME"),
    ("TOOL DIAMETER (MM)", "TOOL DIAMETER (MM)"),
    ("TOOL LENGTH (MM)", "TOOL LENGTH (MM)"),
    ("POCKET NUMBER", "POCKET NUMBER"),
    ("DIAMETER (MM)", "DIAMETER (MM)"),
    ("LENGTH (MM)", "LENGTH (MM)"),
    ("APP TOOL", "APP TOOL"),
    ("Profiles", "Profiles"),
    ("BACKUPS", "BACKUPS"),
    ("PROFILE", "PROFILE"),
    ("PROFILE RECORD", "PROFILE RECORD"),
    ("CONTROLLER PROFILE", "CONTROLLER PROFILE"),
    ("SAVE PROFILE", "SAVE PROFILE"),
    ("SAVE APP", "SAVE APP"),
    ("USE LOCAL DATA", "USE LOCAL DATA"),
    ("BACKUP BUNDLE", "BACKUP BUNDLE"),
    ("VALIDATED FILE", "VALIDATED FILE"),
    ("LOCAL ONLY", "LOCAL ONLY"),
    ("NO CONTROLLER", "NO CONTROLLER"),
    ("UNVERIFIED", "UNVERIFIED"),
    ("PENDING CONFIRMATION", "PENDING CONFIRMATION"),
    ("CORRUPT", "CORRUPT"),
    ("MISSING", "MISSING"),
    ("VALID", "VALID"),
    ("MACROS", "MACROS"),
    ("NEW MACRO", "NEW MACRO"),
    ("SAVE MACRO", "SAVE MACRO"),
    ("RUN MACRO", "RUN MACRO"),
    ("MACRO NAME", "MACRO NAME"),
    ("MACRO COMMAND", "MACRO COMMAND"),
    ("RESET DEFAULTS", "RESET DEFAULTS"),
    ("USE KEY", "USE KEY"),
    ("PRESS A KEY...", "PRESS A KEY..."),
    ("PRESS A KEY COMBINATION", "PRESS A KEY COMBINATION"),
    ("RECORD KEY", "RECORD KEY"),
    ("CONTROLLER DIAGNOSTICS", "CONTROLLER DIAGNOSTICS"),
    ("OPEN PINS / LIMITS", "OPEN PINS / LIMITS"),
    ("PINS / LIMITS", "PINS / LIMITS"),
    ("PIN MAP ($PINS)", "PIN MAP ($PINS)"),
    ("PIN STATE ($PINSTATE)", "PIN STATE ($PINSTATE)"),
    ("LAST EVENTS ($LEV)", "LAST EVENTS ($LEV)"),
    ("CONTROLLER SLEEP", "CONTROLLER SLEEP"),
    ("PUT CONTROLLER TO SLEEP", "PUT CONTROLLER TO SLEEP"),
    ("CONFIRM SLEEP", "CONFIRM SLEEP"),
    ("DELETE MACRO", "DELETE MACRO"),
    ("CONFIRM WRITE", "CONFIRM WRITE"),
    ("CONFIRM OFFSET WRITE", "CONFIRM OFFSET WRITE"),
    ("CONFIRM REQUIRED", "CONFIRM REQUIRED"),
    ("CONFIRM IMPORT", "CONFIRM IMPORT"),
    ("ACKNOWLEDGED", "ACKNOWLEDGED"),
    ("1. ACKNOWLEDGE", "1. ACKNOWLEDGE"),
    ("2. RESUME", "2. RESUME"),
    ("OPEN", "OPEN"),
    ("FOLDER", "FOLDER"),
    ("PROGRAMS", "PROGRAMS"),
    ("No file selected", "No file selected"),
    ("Select a program file first", "Select a program file first"),
    ("FILE", "FILE"),
    ("PATH", "PATH"),
    ("NAME", "NAME"),
    ("NEW", "NEW"),
    ("SET", "SET"),
    ("GO", "GO"),
    ("CLEAR", "CLEAR"),
    ("ALL", "ALL"),
    ("SELECT", "SELECT"),
    ("SELECTED", "SELECTED"),
    ("RELOAD", "RELOAD"),
    ("RUN", "RUN"),
    ("ON", "ON"),
    ("OFF", "OFF"),
    ("IMPORTED", "IMPORTED"),
    ("EXPORTED", "EXPORTED"),
    ("FAILED", "FAILED"),
    ("COMPLETE", "COMPLETE"),
    ("PARTIAL", "PARTIAL"),
    ("ACTIVE", "ACTIVE"),
    ("CURRENT", "CURRENT"),
    ("SETUP", "SETUP"),
    ("VIEW", "VIEW"),
    ("FIT", "FIT"),
    ("CHECK", "CHECK"),
    ("CHECKING", "CHECKING"),
    ("CHECK COMPLETE", "CHECK COMPLETE"),
    ("CHECK STOPPED", "CHECK STOPPED"),
    ("CHECK PROGRAM (DRY RUN)", "CHECK PROGRAM (DRY RUN)"),
    ("START CHECK", "START CHECK"),
    ("DRY RUN - NO MOTION", "DRY RUN - NO MOTION"),
    (
        "WAITING FOR CONTROLLER CHECK MODE ($C)",
        "WAITING FOR CONTROLLER CHECK MODE ($C)",
    ),
    ("WHAT THE DRY RUN DOES", "WHAT THE DRY RUN DOES"),
    ("MOTION IS SUPPRESSED", "MOTION IS SUPPRESSED"),
    ("SPINDLE / COOLANT", "SPINDLE / COOLANT"),
    ("WHEN FINISHED", "WHEN FINISHED"),
    (
        "The controller parses and streams the loaded program with check mode ($C) enabled. \
        Nothing in this sheet starts a real job or moves an axis.",
        "The controller parses and streams the loaded program with check mode ($C) enabled. \
        Nothing in this sheet starts a real job or moves an axis.",
    ),
    (
        "MOTION IS SUPPRESSED: the controller acknowledges program lines without moving the \
        machine.",
        "MOTION IS SUPPRESSED: the controller acknowledges program lines without moving the \
        machine.",
    ),
    (
        "SPINDLE / COOLANT: M3, M4, M5, M7, M8 and M9 are parsed and acknowledged but never \
        run as real outputs.",
        "SPINDLE / COOLANT: M3, M4, M5, M7, M8 and M9 are parsed and acknowledged but never \
        run as real outputs.",
    ),
    (
        "WHEN FINISHED: the engine leaves check mode and waits for the machine to report \
        IDLE. The program stays loaded for a separate real START.",
        "WHEN FINISHED: the engine leaves check mode and waits for the machine to report \
        IDLE. The program stays loaded for a separate real START.",
    ),
    (
        "STOP remains available while the check streams; it never resumes or restarts the \
        run.",
        "STOP remains available while the check streams; it never resumes or restarts the \
        run.",
    ),
    ("CANCEL sends nothing.", "CANCEL sends nothing."),
    (
        "Parse and stream the loaded program in check mode ($C); no motion.",
        "Parse and stream the loaded program in check mode ($C); no motion.",
    ),
    ("LOADED", "LOADED"),
    ("LANGUAGE", "LANGUAGE"),
    ("FOLDERS", "FOLDERS"),
    ("LIVE", "LIVE"),
    ("JUMP", "JUMP"),
    ("DEFAULTS", "DEFAULTS"),
    ("RUN AGAIN", "RUN AGAIN"),
    ("APPLY TEXT", "APPLY TEXT"),
    ("KEEP DRAFT", "KEEP DRAFT"),
    ("DIRECT", "DIRECT"),
    ("SYNTHETIC", "SYNTHETIC"),
    ("TOUCH KEYPAD", "TOUCH KEYPAD"),
    ("NO LINE SELECTED", "NO LINE SELECTED"),
    ("REPORTS REQUESTED", "REPORTS REQUESTED"),
    ("OFFLINE", "OFFLINE"),
    ("UNSAVED", "UNSAVED"),
    ("USER-DEFINED", "USER-DEFINED"),
    ("READ-ONLY", "READ-ONLY"),
    ("NOT LOADED", "NOT LOADED"),
    (
        "Choose a file for the job; loading never starts it.",
        "Choose a file for the job; loading never starts it.",
    ),
    (
        "No folders or program files here",
        "No folders or program files here",
    ),
    (
        "Tap UP or type another folder path.",
        "Tap UP or type another folder path.",
    ),
    (
        "48 px rows | folders open on tap | hidden files and symlinks are not shown | .nc \
        .gcode .tap .ngc | 20 MB limit",
        "48 px rows | folders open on tap | hidden files and symlinks are not shown | .nc \
        .gcode .tap .ngc | 20 MB limit",
    ),
    (
        "Tap a program file to select it",
        "Tap a program file to select it",
    ),
    ("READY TO LOAD", "READY TO LOAD"),
    ("MOCK / BROWSE ONLY", "MOCK / BROWSE ONLY"),
    ("LINK NOT READY", "LINK NOT READY"),
    ("LIVE / READY", "LIVE / READY"),
    ("DIR", "DIR"),
    ("OPTIONAL STOP", "OPTIONAL STOP"),
    ("SINGLE BLOCK", "SINGLE BLOCK"),
    ("BLOCK DELETE", "BLOCK DELETE"),
    (
        "M1 optional stops are honored",
        "M1 optional stops are honored",
    ),
    ("Hold after each block", "Hold after each block"),
    ("Skip blocks marked with /", "Skip blocks marked with /"),
    (
        "Each tap sends one command. A row keeps showing the value the controller reports; \
        it changes on the next poll, not when the button is pressed.",
        "Each tap sends one command. A row keeps showing the value the controller reports; \
        it changes on the next poll, not when the button is pressed.",
    ),
    ("Mock mode: no controller", "Mock mode: no controller"),
    ("Link not ready", "Link not ready"),
    (
        "Locked while a job runs or pauses",
        "Locked while a job runs or pauses",
    ),
    (
        "MOCK MODE: run options need a live controller",
        "MOCK MODE: run options need a live controller",
    ),
    (
        "LINK NOT READY: run options are disabled until the handshake completes",
        "LINK NOT READY: run options are disabled until the handshake completes",
    ),
    (
        "BLOCK DELETE IS LOCKED WHILE A JOB IS RUNNING OR PAUSED",
        "BLOCK DELETE IS LOCKED WHILE A JOB IS RUNNING OR PAUSED",
    ),
    (
        "Closing this sheet sends nothing.",
        "Closing this sheet sends nothing.",
    ),
    (
        "CANCEL, the scrim, and closing this sheet send nothing.",
        "CANCEL, the scrim, and closing this sheet send nothing.",
    ),
    ("CONFIRMING WILL", "CONFIRMING WILL"),
    ("RUN TO LINE", "RUN TO LINE"),
    ("SET BREAKPOINT AT LINE", "SET BREAKPOINT AT LINE"),
    ("START PROGRAM AT LINE 1", "START PROGRAM AT LINE 1"),
    ("PAUSE BEFORE LINE", "PAUSE BEFORE LINE"),
    (
        "LEAVE THE ACTIVE JOB AT ITS CURRENT POSITION",
        "LEAVE THE ACTIVE JOB AT ITS CURRENT POSITION",
    ),
    ("RUN TO BLOCKED", "RUN TO BLOCKED"),
    ("SET BP + START", "SET BP + START"),
    (
        "A job is already running or paused. Confirming only sets a breakpoint at the \
        selected line; it does not start, restart, or resume the job.",
        "A job is already running or paused. Confirming only sets a breakpoint at the \
        selected line; it does not start, restart, or resume the job.",
    ),
    (
        "All earlier lines still run from the beginning. The job starts at line 1 and \
        pauses before the selected line. Start-from-line is not supported.",
        "All earlier lines still run from the beginning. The job starts at line 1 and \
        pauses before the selected line. Start-from-line is not supported.",
    ),
    (
        "One step per press; off while typing. RESET DEFAULTS restores.",
        "One step per press; off while typing. RESET DEFAULTS restores.",
    ),
    (
        "Tap for one step; hold a mapped jog key to jog continuously. Off while typing.",
        "Tap for one step; hold a mapped jog key to jog continuously. Off while typing.",
    ),
    (
        "Named single-line MDI commands, saved in this controller profile. RUN sends only \
        after a click; CONFIRM macros require a second explicit confirmation.",
        "Named single-line MDI commands, saved in this controller profile. RUN sends only \
        after a click; CONFIRM macros require a second explicit confirmation.",
    ),
    (
        "No user macros for this controller profile. Add a named, single-line MDI command; \
        it is saved only here and never runs on connect.",
        "No user macros for this controller profile. Add a named, single-line MDI command; \
        it is saved only here and never runs on connect.",
    ),
    ("MACRO SAVES LOCKED", "MACRO SAVES LOCKED"),
    ("MACRO RUNS LOCKED", "MACRO RUNS LOCKED"),
    (
        "Startup lines execute automatically on every controller reset and can command \
        motion, spindle, or coolant. Writes are allowed only while the controller is Idle; \
        nothing is written until APPLY and CONFIRM WRITE.",
        "Startup lines execute automatically on every controller reset and can command \
        motion, spindle, or coolant. Writes are allowed only while the controller is Idle; \
        nothing is written until APPLY and CONFIRM WRITE.",
    ),
    (
        "WRITE NOT APPLIED - THE PREVIOUS VALUE IS STILL ACTIVE",
        "WRITE NOT APPLIED - THE PREVIOUS VALUE IS STILL ACTIVE",
    ),
    (
        "This startup line executes automatically on every controller reset. It can command \
        motion, spindle, or coolant.",
        "This startup line executes automatically on every controller reset. It can command \
        motion, spindle, or coolant.",
    ),
    ("EXACT TEXT TO STORE", "EXACT TEXT TO STORE"),
    ("Previous controller value", "Previous controller value"),
    (
        "CONFIRM WRITE sends the exact text shown. The app does not update the row itself: \
        it reads the line back and reports the result.",
        "CONFIRM WRITE sends the exact text shown. The app does not update the row itself: \
        it reads the line back and reports the result.",
    ),
    ("RESET EXECUTION", "RESET EXECUTION"),
    (
        "CONFIRM FIRST; CANCEL SENDS NOTHING",
        "CONFIRM FIRST; CANCEL SENDS NOTHING",
    ),
    (
        "Sleep stops controller status polling until RESET or a status wake. The app does \
        not wake the controller automatically; the DRO and job view stay frozen until the \
        controller answers again.",
        "Sleep stops controller status polling until RESET or a status wake. The app does \
        not wake the controller automatically; the DRO and job view stay frozen until the \
        controller answers again.",
    ),
    ("BEFORE YOU CONFIRM", "BEFORE YOU CONFIRM"),
    (
        "The controller must report Idle and no job may be running or paused. One confirmed \
        tap sends one sleep command; CANCEL sends nothing.",
        "The controller must report Idle and no job may be running or paused. One confirmed \
        tap sends one sleep command; CANCEL sends nothing.",
    ),
    (
        "Pin map, auxiliary pin levels, limit switches and last control events. Opening the \
        panel sends nothing; REFRESH sends only $PINS, $PINSTATE, $LIM and $LEV.",
        "Pin map, auxiliary pin levels, limit switches and last control events. Opening the \
        panel sends nothing; REFRESH sends only $PINS, $PINSTATE, $LIM and $LEV.",
    ),
    (
        "No $PINS report in this session",
        "No $PINS report in this session",
    ),
    (
        "The controller reported no pin entries",
        "The controller reported no pin entries",
    ),
    ("pin entries reported", "pin entries reported"),
    (
        "Applies immediately and is saved in app preferences; the controller is not \
        contacted.",
        "Applies immediately and is saved in app preferences; the controller is not \
        contacted.",
    ),
    (
        "Export writes app prefs, identity, settings, offsets, tools, macros, startup \
        lines, and keymap; import validates before writing and never pushes to the \
        controller.",
        "Export writes app prefs, identity, settings, offsets, tools, macros, startup \
        lines, and keymap; import validates before writing and never pushes to the \
        controller.",
    ),
    (
        "The app owns this table; it persists in the per-controller profile. Controller \
        [T:] entries stay read-only and are never merged silently.",
        "The app owns this table; it persists in the per-controller profile. Controller \
        [T:] entries stay read-only and are never merged silently.",
    ),
    (
        "No controller tool table has been reported yet.",
        "No controller tool table has been reported yet.",
    ),
    (
        "The controller reports an empty tool table (0 entries).",
        "The controller reports an empty tool table (0 entries).",
    ),
    (
        "Reported by the controller through [T:...]; this panel never writes, overwrites, \
        or merges into the app table above.",
        "Reported by the controller through [T:...]; this panel never writes, overwrites, \
        or merges into the app table above.",
    ),
    ("STARTUP WRITES LOCKED", "STARTUP WRITES LOCKED"),
    ("SAVE LOCKED", "SAVE LOCKED"),
    ("KEYMAP NOT VALID", "KEYMAP NOT VALID"),
    ("Program has no lines", "Program has no lines"),
    (
        "Load a non-empty source file to search it.",
        "Load a non-empty source file to search it.",
    ),
    ("Nothing matches", "Nothing matches"),
    (
        "Switch to HIGHLIGHT or clear the field to restore the full list.",
        "Switch to HIGHLIGHT or clear the field to restore the full list.",
    ),
    (
        "DRAG ORBIT | SHIFT/MIDDLE PAN | WHEEL ZOOM",
        "DRAG ORBIT | SHIFT/MIDDLE PAN | WHEEL ZOOM",
    ),
    ("GENERATED PROGRAM", "GENERATED PROGRAM"),
    (
        "No axes reported by the controller",
        "No axes reported by the controller",
    ),
    (
        "ONE WRITE AT A TIME | READ-BACK VERIFIED",
        "ONE WRITE AT A TIME | READ-BACK VERIFIED",
    ),
    ("WRITES LOCKED", "WRITES LOCKED"),
    ("IDLE: WRITES ENABLED", "IDLE: WRITES ENABLED"),
    (
        "MOCK MODE: NO CONTROLLER SETTINGS",
        "MOCK MODE: NO CONTROLLER SETTINGS",
    ),
    (
        "The generated editor is built from the controller's $$ values and $ES/$EG \
        metadata. The design-review mock supplies neither. Start without --mock to connect \
        the in-process FlexiHAL demo controller.",
        "The generated editor is built from the controller's $$ values and $ES/$EG \
        metadata. The design-review mock supplies neither. Start without --mock to connect \
        the in-process FlexiHAL demo controller.",
    ),
    ("WAITING FOR SETTINGS", "WAITING FOR SETTINGS"),
    (
        "The handshake has not reported $$, $ES, or $EG yet. The form appears as soon as \
        the controller answers.",
        "The handshake has not reported $$, $ES, or $EG yet. The form appears as soon as \
        the controller answers.",
    ),
    (
        "THE CONTROLLER REPORTED NO SETTINGS",
        "THE CONTROLLER REPORTED NO SETTINGS",
    ),
    (
        "$$ returned no values and $ES/$EG returned no metadata. The editor stays empty \
        rather than inventing a setting list.",
        "$$ returned no values and $ES/$EG returned no metadata. The editor stays empty \
        rather than inventing a setting list.",
    ),
    (
        "EXAMPLE SHAPE (NO VALUES ARE WRITTEN)",
        "EXAMPLE SHAPE (NO VALUES ARE WRITTEN)",
    ),
    ("LOW", "LOW"),
    ("HIGH", "HIGH"),
    ("dashed = missing", "dashed = missing"),
    ("QUEUED", "QUEUED"),
    ("BLANK ALLOWED", "BLANK ALLOWED"),
    ("BIT 0 ENABLES THE OTHERS", "BIT 0 ENABLES THE OTHERS"),
    ("INFERRED FROM VALUE", "INFERRED FROM VALUE"),
    (
        "No saved maps for this controller profile yet.",
        "No saved maps for this controller profile yet.",
    ),
    (
        "Read-only view; opening does not apply compensation or start a job",
        "Read-only view; opening does not apply compensation or start a job",
    ),
    (
        "Nothing is applied automatically",
        "Nothing is applied automatically",
    ),
    (
        "No automatic retry or resume. Recover the machine, then run again.",
        "No automatic retry or resume. Recover the machine, then run again.",
    ),
    (
        "every gate must pass before RUN",
        "every gate must pass before RUN",
    ),
    ("PROBE CYCLES", "PROBE CYCLES"),
    (
        "Pick a card, inspect the preflight, then press RUN",
        "Pick a card, inspect the preflight, then press RUN",
    ),
    (
        "millimetres; tap a value to edit",
        "millimetres; tap a value to edit",
    ),
    ("recorded vs live", "recorded vs live"),
    ("per controller profile", "per controller profile"),
    ("APPLY OFFSET", "APPLY OFFSET"),
    ("NOT SENT YET", "NOT SENT YET"),
    ("EXACT MDI LINE", "EXACT MDI LINE"),
    ("CURRENT DRO (DISPLAY UNITS)", "CURRENT DRO (DISPLAY UNITS)"),
    (
        "DRO CHANGED SINCE THIS SHEET OPENED; CANCEL AND RECHECK",
        "DRO CHANGED SINCE THIS SHEET OPENED; CANCEL AND RECHECK",
    ),
    ("READ-ONLY $# PARAMETERS", "READ-ONLY $# PARAMETERS"),
    ("X / Y / Z IN DISPLAY UNITS", "X / Y / Z IN DISPLAY UNITS"),
    ("TAP TO EDIT", "TAP TO EDIT"),
    (
        "RESTORE WORK OFFSETS - PREVIEW",
        "RESTORE WORK OFFSETS - PREVIEW",
    ),
    (
        "G10 L2 LINES IN THIS FILE (VALUES IN MILLIMETRES)",
        "G10 L2 LINES IN THIS FILE (VALUES IN MILLIMETRES)",
    ),
    (
        "THE FILE BEGINS WITH G21: EVERY VALUE IS INTERPRETED IN MILLIMETRES.",
        "THE FILE BEGINS WITH G21: EVERY VALUE IS INTERPRETED IN MILLIMETRES.",
    ),
    ("LOAD AS PROGRAM", "LOAD AS PROGRAM"),
    (
        "LOCKED: A JOB IS ACTIVE; STOP IT BEFORE LOADING A PROGRAM",
        "LOCKED: A JOB IS ACTIVE; STOP IT BEFORE LOADING A PROGRAM",
    ),
    ("SET X", "SET X"),
    ("SET Y", "SET Y"),
    ("SET Z", "SET Z"),
    ("SET AXIS", "SET AXIS"),
    ("MDI TEXT", "MDI TEXT"),
    ("SPINDLE RPM", "SPINDLE RPM"),
    ("SETTING", "SETTING"),
    ("OFFSET", "OFFSET"),
    ("TOKENS + DIGITS", "TOKENS + DIGITS"),
    ("G10 L20 | DISPLAY UNITS", "G10 L20 | DISPLAY UNITS"),
    ("RANGE N/A", "RANGE N/A"),
    ("TOOL FIELD", "TOOL FIELD"),
    ("PROBE FIELD (mm)", "PROBE FIELD (mm)"),
    ("Demo FlexiHAL", "Demo FlexiHAL"),
    (
        "In-process FlexiHAL simulator over the loopback transport. No serial port is \
        opened.",
        "In-process FlexiHAL simulator over the loopback transport. No serial port is \
        opened.",
    ),
    (
        "Same engine, handshake, streamer and safety gateway as a real controller.",
        "Same engine, handshake, streamer and safety gateway as a real controller.",
    ),
    (
        "Tap a port to select it; nothing opens until CONNECT.",
        "Tap a port to select it; nothing opens until CONNECT.",
    ),
    (
        "Line settings are fixed at 8 data bits, no parity, one stop bit. DTR reset pulses \
        are not implemented in M2.",
        "Line settings are fixed at 8 data bits, no parity, one stop bit. DTR reset pulses \
        are not implemented in M2.",
    ),
    ("serial port", "serial port"),
    ("NOT REQUESTED", "NOT REQUESTED"),
    ("READING CONTROLLER", "READING CONTROLLER"),
    ("CONTROLLER BUSY - RETRYING", "CONTROLLER BUSY - RETRYING"),
    (
        "No $PINS report has arrived in this session.",
        "No $PINS report has arrived in this session.",
    ),
    (
        "No $PINSTATE report has arrived in this session.",
        "No $PINSTATE report has arrived in this session.",
    ),
    (
        "The controller reported no auxiliary pin levels.",
        "The controller reported no auxiliary pin levels.",
    ),
    (
        "No $LIM report has arrived in this session.",
        "No $LIM report has arrived in this session.",
    ),
    (
        "No $LEV report has arrived in this session.",
        "No $LEV report has arrived in this session.",
    ),
    (
        "The controller reports no control or limit events yet.",
        "The controller reports no control or limit events yet.",
    ),
    ("PROBE PLAN", "PROBE PLAN"),
    ("CUSTOM", "CUSTOM"),
    ("No settings in this group", "No settings in this group"),
    ("No setting matches", "No setting matches"),
    (
        "Pick another group in the rail.",
        "Pick another group in the rail.",
    ),
    (
        "Search matches a number, name, or group. Clear the search to browse groups.",
        "Search matches a number, name, or group. Clear the search to browse groups.",
    ),
    ("CONTROLLER STATE", "CONTROLLER STATE"),
    (
        "CONTROLLER DIAGNOSTIC DATA - READ-ONLY",
        "CONTROLLER DIAGNOSTIC DATA - READ-ONLY",
    ),
    ("SAVE BLOCKED", "SAVE BLOCKED"),
    (
        "KEPT: offsets are unchanged. Apply stays available.",
        "KEPT: offsets are unchanged. Apply stays available.",
    ),
    ("COMMAND", "COMMAND"),
    ("Ask before running", "Ask before running"),
    (
        "New macros default to CONFIRM. A direct macro runs on one click.",
        "New macros default to CONFIRM. A direct macro runs on one click.",
    ),
    ("EXACT COMMAND TO SEND", "EXACT COMMAND TO SEND"),
    (
        "Jogging is allowed while the job waits; the Jog panel stays active. The job will \
        not send another line until you resume.",
        "Jogging is allowed while the job waits; the Jog panel stays active. The job will \
        not send another line until you resume.",
    ),
    (
        "Jogging is allowed while the job waits; the Jog panel stays active. The job will \
        not send another line until you resume. RESET and STOP above remain available.",
        "Jogging is allowed while the job waits; the Jog panel stays active. The job will \
        not send another line until you resume. RESET and STOP above remain available.",
    ),
    ("VERIFYING", "VERIFYING"),
    ("VERIFIED", "VERIFIED"),
    ("MISMATCH", "MISMATCH"),
    ("MEASUREMENT ONLY", "MEASUREMENT ONLY"),
    ("READY TO APPLY", "READY TO APPLY"),
    ("REVERTED", "REVERTED"),
    ("FEED", "FEED"),
    ("RETRACT", "RETRACT"),
    ("CENTER X", "CENTER X"),
    ("CENTER Y", "CENTER Y"),
    ("ORIGIN X WORK", "ORIGIN X WORK"),
    ("ORIGIN Y WORK", "ORIGIN Y WORK"),
    ("SURFACE Z MACH", "SURFACE Z MACH"),
    ("PROBE OFFSET X", "PROBE OFFSET X"),
    ("PROBE OFFSET Y", "PROBE OFFSET Y"),
    (
        "MOCK MODE: browsing only; loading needs a live controller",
        "MOCK MODE: browsing only; loading needs a live controller",
    ),
    (
        "LINK NOT READY: finish the connection handshake before loading a program",
        "LINK NOT READY: finish the connection handshake before loading a program",
    ),
    (
        "JOB ACTIVE: stop the running job before loading another program",
        "JOB ACTIVE: stop the running job before loading another program",
    ),
    (
        "MOCK MODE: a check run needs a live controller",
        "MOCK MODE: a check run needs a live controller",
    ),
    (
        "LINK NOT READY: finish the connection handshake before a check run",
        "LINK NOT READY: finish the connection handshake before a check run",
    ),
    (
        "MACHINE LOCKED: clear Alarm or Door before a check run",
        "MACHINE LOCKED: clear Alarm or Door before a check run",
    ),
    (
        "NO PROGRAM: load a program before a check run",
        "NO PROGRAM: load a program before a check run",
    ),
    (
        "JOB ACTIVE: stop the running job before a check run",
        "JOB ACTIVE: stop the running job before a check run",
    ),
    (
        "MACHINE NOT IDLE: a check run starts only from IDLE",
        "MACHINE NOT IDLE: a check run starts only from IDLE",
    ),
    (
        "CHECK RUN ACTIVE: wait for the controller to finish check mode",
        "CHECK RUN ACTIVE: wait for the controller to finish check mode",
    ),
    (
        "PROBE PROGRAM: a generated probe plan cannot be checked from the job toolbar",
        "PROBE PROGRAM: a generated probe plan cannot be checked from the job toolbar",
    ),
    (
        "1. Confirm the physical E-stop and independent spindle stop. 2. Open \
        CONNECTION and select the serial port. 3. Wait for READY and Idle. 4. Check \
        the DRO with one small jog. 5. Verify units, WCS, and soft limits before \
        loading a program.",
        "1. Confirm the physical E-stop and independent spindle stop. 2. Open \
        CONNECTION and select the serial port. 3. Wait for READY and Idle. 4. Check \
        the DRO with one small jog. 5. Verify units, WCS, and soft limits before \
        loading a program.",
    ),
    ("ADD LINE NUMBERS", "ADD LINE NUMBERS"),
    ("APP PREFERENCE ONLY", "APP PREFERENCE ONLY"),
    (
        "APPLY is a separate, confirmed step and is allowed only while the controller \
        is Idle.",
        "APPLY is a separate, confirmed step and is allowed only while the controller \
        is Idle.",
    ),
    (
        "APPLY sends exactly one Command::SetSetting through the engine. The engine \
        writes the value, reads it back, and only then marks it APPLIED. CANCEL sends \
        nothing.",
        "APPLY sends exactly one Command::SetSetting through the engine. The engine \
        writes the value, reads it back, and only then marks it APPLIED. CANCEL sends \
        nothing.",
    ),
    ("AXIS", "AXIS"),
    (
        "After setup, use the normal Connection panel. Start with the simulator or with \
        the spindle off, then run only status, DRO, and small jogs.",
        "After setup, use the normal Connection panel. Start with the simulator or with \
        the spindle off, then run only status, DRO, and small jogs.",
    ),
    ("BACK", "BACK"),
    (
        "Back-and-forth raster at each depth, with a safe-Z retract between passes. The \
        wizard validates every field and caps the program before LOAD AS PROGRAM; it \
        never starts the job.",
        "Back-and-forth raster at each depth, with a safe-Z retract between passes. The \
        wizard validates every field and caps the program before LOAD AS PROGRAM; it \
        never starts the job.",
    ),
    ("CALCULATE", "CALCULATE"),
    ("CALIBRATION CONFIRMATION", "CALIBRATION CONFIRMATION"),
    ("CALIBRATION DISTANCE (mm)", "CALIBRATION DISTANCE (mm)"),
    ("CALIBRATION WRITES LOCKED", "CALIBRATION WRITES LOCKED"),
    (
        "Calculate to preview the new steps/mm and the change before writing.",
        "Calculate to preview the new steps/mm and the change before writing.",
    ),
    ("DEFAULT DISPLAY UNITS", "DEFAULT DISPLAY UNITS"),
    ("DISPLAY UNITS", "DISPLAY UNITS"),
    (
        "ESTIMATED MACHINE BOUNDS (WCS + G92)",
        "ESTIMATED MACHINE BOUNDS (WCS + G92)",
    ),
    (
        "EVERY ITEM IS REQUIRED BEFORE NEXT",
        "EVERY ITEM IS REQUIRED BEFORE NEXT",
    ),
    ("FINISH SETUP", "FINISH SETUP"),
    ("FIRST CONNECTION ORDER", "FIRST CONNECTION ORDER"),
    ("GENERATE PREVIEW", "GENERATE PREVIEW"),
    (
        "GENERATE PREVIEW to validate the fields and compute passes, path length, and \
        time.",
        "GENERATE PREVIEW to validate the fields and compute passes, path length, and \
        time.",
    ),
    ("GENERATED / NOT CHECKED", "GENERATED / NOT CHECKED"),
    (
        "HEIGHT MAP / COMPENSATED: loading a transform replaces the compensated program \
        and its Revert original. Keep an external copy if needed.",
        "HEIGHT MAP / COMPENSATED: loading a transform replaces the compensated program \
        and its Revert original. Keep an external copy if needed.",
    ),
    (
        "I measured the actual travel and want to write this steps/mm value",
        "I measured the actual travel and want to write this steps/mm value",
    ),
    (
        "I understand this loads the previewed source and does not start it",
        "I understand this loads the previewed source and does not start it",
    ),
    (
        "I understand this program is generated, not checked against the machine",
        "I understand this program is generated, not checked against the machine",
    ),
    ("INCHES (IN)", "INCHES (IN)"),
    (
        "INFORMATIONAL CHECK: WCS changes, G10 writes, G92 offsets, and tool-length \
        offsets are not enforced here. Verify the setup and soft limits before \
        cutting.",
        "INFORMATIONAL CHECK: WCS changes, G10 writes, G92 offsets, and tool-length \
        offsets are not enforced here. Verify the setup and soft limits before \
        cutting.",
    ),
    ("INFORMATIONAL ONLY", "INFORMATIONAL ONLY"),
    (
        "Informational only: START is never blocked. The machine estimate adds the \
        reported active WCS plus G92; TLO is not included. Missing travel or offset \
        data shows NOT CHECKED, never a safety claim. No controller writes.",
        "Informational only: START is never blocked. The machine estimate adds the \
        reported active WCS plus G92; TLO is not included. Missing travel or offset \
        data shows NOT CHECKED, never a safety claim. No controller writes.",
    ),
    (
        "Jog or run the test move separately, then enter the commanded distance and the \
        measured travel. The wizard only calculates and writes the steps/mm setting; \
        it never moves an axis.",
        "Jog or run the test move separately, then enter the commanded distance and the \
        measured travel. The wizard only calculates and writes the steps/mm setting; \
        it never moves an axis.",
    ),
    ("MACHINE SETUP", "MACHINE SETUP"),
    ("MATERIAL SURFACING", "MATERIAL SURFACING"),
    ("MATERIAL SURFACING WIZARD", "MATERIAL SURFACING WIZARD"),
    ("MILLIMETRES (MM)", "MILLIMETRES (MM)"),
    ("NEW STEPS/MM", "NEW STEPS/MM"),
    ("NO MOTION COMMANDED", "NO MOTION COMMANDED"),
    (
        "NO PROGRAM: load a program before transforming its source",
        "NO PROGRAM: load a program before transforming its source",
    ),
    ("NOT COMPLETED", "NOT COMPLETED"),
    (
        "No generated program yet. Open the wizard, validate the fields, and preview.",
        "No generated program yet. Open the wizard, validate the fields, and preview.",
    ),
    (
        "No tools in the app table. Add one so the M6 sheet can show a name, diameter, \
        and length instead of only the number.",
        "No tools in the app table. Add one so the M6 sheet can show a name, diameter, \
        and length instead of only the number.",
    ),
    ("OFF omits both M3 and M5", "OFF omits both M3 and M5"),
    ("OLD STEPS/MM", "OLD STEPS/MM"),
    ("OPEN SETUP", "OPEN SETUP"),
    ("OPEN WIZARD", "OPEN WIZARD"),
    ("PREVIEW", "PREVIEW"),
    ("PREVIEW FIRST", "PREVIEW FIRST"),
    ("PREVIEW SUMMARY", "PREVIEW SUMMARY"),
    (
        "PROGRAM BOUNDS (WORK COORDINATES)",
        "PROGRAM BOUNDS (WORK COORDINATES)",
    ),
    (
        "PROGRAM EXCEEDS REPORTED TRAVEL",
        "PROGRAM EXCEEDS REPORTED TRAVEL",
    ),
    ("PROGRAM LIMITS", "PROGRAM LIMITS"),
    ("PROPOSED STEPS/MM", "PROPOSED STEPS/MM"),
    ("REMIND ME LATER", "REMIND ME LATER"),
    (
        "REOPENS AT SAFETY TO REVIEW OR UPDATE THE CHOICES",
        "REOPENS AT SAFETY TO REVIEW OR UPDATE THE CHOICES",
    ),
    ("REPORTED TRAVEL", "REPORTED TRAVEL"),
    ("REVIEW & APPLY", "REVIEW & APPLY"),
    ("SAFETY ITEMS ACKNOWLEDGED", "SAFETY ITEMS ACKNOWLEDGED"),
    (
        "SETUP IS NOT A MACHINE CERTIFICATION",
        "SETUP IS NOT A MACHINE CERTIFICATION",
    ),
    ("SOURCE TRANSFORMS", "SOURCE TRANSFORMS"),
    ("SPINDLE ON (M3 S... / M5)", "SPINDLE ON (M3 S... / M5)"),
    ("START ANYWAY", "START ANYWAY"),
    (
        "START HERE BEFORE THE FIRST COMMANDED MOVE",
        "START HERE BEFORE THE FIRST COMMANDED MOVE",
    ),
    ("STEPPER CALIBRATION", "STEPPER CALIBRATION"),
    ("STRIP COMMENTS", "STRIP COMMENTS"),
    (
        "THIS WIZARD DOES NOT TEST THE MACHINE",
        "THIS WIZARD DOES NOT TEST THE MACHINE",
    ),
    ("TRANSFORM PREVIEW", "TRANSFORM PREVIEW"),
    (
        "The controller, its firmware, and the physical safety wiring remain \
        authoritative. Acknowledge only what you have checked on this machine.",
        "The controller, its firmware, and the physical safety wiring remain \
        authoritative. Acknowledge only what you have checked on this machine.",
    ),
    (
        "The loaded program's estimated machine bounds leave the travel reported by the \
        controller. START is held until you confirm.",
        "The loaded program's estimated machine bounds leave the travel reported by the \
        controller. START is held until you confirm.",
    ),
    (
        "The loaded source is unchanged until LOAD AS PROGRAM. This loads one new \
        program and never starts it.",
        "The loaded source is unchanged until LOAD AS PROGRAM. This loads one new \
        program and never starts it.",
    ),
    (
        "The program is generated from these fields. It is not checked against this \
        machine's travel, offsets, tool, or stock. LOAD AS PROGRAM does not start it.",
        "The program is generated from these fields. It is not checked against this \
        machine's travel, offsets, tool, or stock. LOAD AS PROGRAM does not start it.",
    ),
    (
        "The wizard never connects, homes, jogs, probes, or writes controller settings.",
        "The wizard never connects, homes, jogs, probes, or writes controller settings.",
    ),
    (
        "The wizard reviews the physical safety checklist, the app display unit \
        preference, and what the first connection should look like. It never connects, \
        homes, jogs, probes, or writes controller settings.",
        "The wizard reviews the physical safety checklist, the app display unit \
        preference, and what the first connection should look like. It never connects, \
        homes, jogs, probes, or writes controller settings.",
    ),
    (
        "This choice is saved for the next session. The wizard does not send G20 or G21 \
        and does not change the controller's units. The live controller parser state \
        still controls the current session.",
        "This choice is saved for the next session. The wizard does not send G20 or G21 \
        and does not change the controller's units. The live controller parser state \
        still controls the current session.",
    ),
    (
        "This records that you reviewed the checklist and chose an app preference. It \
        does not test, certify, or configure the machine. Verify every motion, limit, \
        and safety device yourself.",
        "This records that you reviewed the checklist and chose an app preference. It \
        does not test, certify, or configure the machine. Verify every motion, limit, \
        and safety device yourself.",
    ),
    (
        "This writes one controller setting. It does not move the machine: the test \
        move is your separate jog or MDI command. The controller stores steps/mm in \
        EEPROM; avoid unnecessary writes.",
        "This writes one controller setting. It does not move the machine: the test \
        move is your separate jog or MDI command. The controller stores steps/mm in \
        EEPROM; avoid unnecessary writes.",
    ),
    ("UTILITY FIELD", "UTILITY FIELD"),
    ("WRITE LOCKED", "WRITE LOCKED"),
    ("STOCK WIDTH", "STOCK WIDTH"),
    ("STOCK DEPTH", "STOCK DEPTH"),
    ("CUTTER DIA", "CUTTER DIA"),
    ("STEPOVER", "STEPOVER"),
    ("TOTAL DEPTH", "TOTAL DEPTH"),
    ("DEPTH/PASS", "DEPTH/PASS"),
    ("PLUNGE FEED", "PLUNGE FEED"),
    ("LINE START", "LINE START"),
    ("LINE STEP", "LINE STEP"),
    ("COMMANDED MOVE", "COMMANDED MOVE"),
    ("MEASURED MOVE", "MEASURED MOVE"),
    ("CALCULATED", "CALCULATED"),
    ("CONFIRMED", "CONFIRMED"),
    (
        "ALLOWED: comments `(...)` or `;...`, `G4 P0..60`, `M0`, `M1`, `M5`, `M9`.",
        "ALLOWED: comments `(...)` or `;...`, `G4 P0..60`, `M0`, `M1`, `M5`, `M9`.",
    ),
    ("ANGLE", "ANGLE"),
    ("APPLY TLO", "APPLY TLO"),
    (
        "BACKUP EXPORTED THIS SESSION",
        "BACKUP EXPORTED THIS SESSION",
    ),
    (
        "BACKUP EXPORTED THIS SESSION - RESET ACKNOWLEDGED",
        "BACKUP EXPORTED THIS SESSION - RESET ACKNOWLEDGED",
    ),
    ("BACKUP FILE", "BACKUP FILE"),
    ("BACKUP REQUIRED", "BACKUP REQUIRED"),
    ("CALCULATOR ONLY", "CALCULATOR ONLY"),
    (
        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY",
        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY",
    ),
    ("CLEAR ALL HISTORY", "CLEAR ALL HISTORY"),
    ("CLEAR ALL HISTORY?", "CLEAR ALL HISTORY?"),
    (
        "CLEAR ASKS FOR CONFIRMATION FIRST",
        "CLEAR ASKS FOR CONFIRMATION FIRST",
    ),
    ("CLEAR HISTORY", "CLEAR HISTORY"),
    ("CLEAR TLO", "CLEAR TLO"),
    ("CONTROLLER RESET", "CONTROLLER RESET"),
    (
        "DATA LOSS ACKNOWLEDGED - RESET MAY PROCEED",
        "DATA LOSS ACKNOWLEDGED - RESET MAY PROCEED",
    ),
    (
        "DATA LOSS ACKNOWLEDGED BY THE OPERATOR",
        "DATA LOSS ACKNOWLEDGED BY THE OPERATOR",
    ),
    ("DELTA", "DELTA"),
    ("DELTA (TOTAL)", "DELTA (TOTAL)"),
    ("DESTRUCTIVE", "DESTRUCTIVE"),
    ("DROP PROGRAM TO LOAD", "DROP PROGRAM TO LOAD"),
    (
        "Drop exactly one program file",
        "Drop exactly one program file",
    ),
    (
        "Drop one .nc, .gcode, .tap, or .ngc program file",
        "Drop one .nc, .gcode, .tap, or .ngc program file",
    ),
    ("EMPTY", "EMPTY"),
    ("EXPORT BACKUP", "EXPORT BACKUP"),
    (
        "EXPORT IN FLIGHT - WAIT FOR THE WRITE TO FINISH",
        "EXPORT IN FLIGHT - WAIT FOR THE WRITE TO FINISH",
    ),
    ("Enter a folder path first", "Enter a folder path first"),
    (
        "Enter millimetres. The span is the distance between the two measurement points; the \
         readings are the Y values at the left and right points. CALCULATE is local \
         arithmetic.",
        "Enter millimetres. The span is the distance between the two measurement points; the \
         readings are the Y values at the left and right points. CALCULATE is local \
         arithmetic.",
    ),
    (
        "Finished jobs are stored in app.json on this computer. The newest 100 records are \
         kept; the card shows the newest 10.",
        "Finished jobs are stored in app.json on this computer. The newest 100 records are \
         kept; the card shows the newest 10.",
    ),
    (
        "GANTRY IS SQUARE WITHIN MEASUREMENT RESOLUTION; NO CORRECTION INDICATED",
        "GANTRY IS SQUARE WITHIN MEASUREMENT RESOLUTION; NO CORRECTION INDICATED",
    ),
    ("GANTRY MEASUREMENT (mm)", "GANTRY MEASUREMENT (mm)"),
    ("GANTRY SQUARING", "GANTRY SQUARING"),
    ("GANTRY SQUARING CALCULATOR", "GANTRY SQUARING CALCULATOR"),
    (
        "I HAVE A BACKUP OR ACCEPT DATA LOSS",
        "I HAVE A BACKUP OR ACCEPT DATA LOSS",
    ),
    ("JOB HISTORY", "JOB HISTORY"),
    ("LEFT FORWARD", "LEFT FORWARD"),
    (
        "LEFT SIDE IS {amount} mm FORWARD; MOVE IT BACK {amount} mm OR SHIM THE RIGHT SIDE",
        "LEFT SIDE IS {amount} mm FORWARD; MOVE IT BACK {amount} mm OR SHIM THE RIGHT SIDE",
    ),
    ("LINES", "LINES"),
    (
        "LOCAL ONLY - NOT SENT TO THE CONTROLLER",
        "LOCAL ONLY - NOT SENT TO THE CONTROLLER",
    ),
    (
        "LOCAL ONLY - NOT SENT TO THE CONTROLLER. Records are kept in app.json on this \
         computer, newest first.",
        "LOCAL ONLY - NOT SENT TO THE CONTROLLER. Records are kept in app.json on this \
         computer, newest first.",
    ),
    ("LOCAL PROFILE", "LOCAL PROFILE"),
    ("LOCAL TIME", "LOCAL TIME"),
    ("MAX", "MAX"),
    ("MEASUREMENT RESULT", "MEASUREMENT RESULT"),
    ("MOST DESTRUCTIVE", "MOST DESTRUCTIVE"),
    ("Max", "Max"),
    (
        "Measure the Y distance from a common reference to the left and right ends of the \
         gantry, enter the span between those points, and calculate the delta, angle, and \
         per-side correction. Follow the machine's squaring procedure for any adjustment.",
        "Measure the Y distance from a common reference to the left and right ends of the \
         gantry, enter the span between those points, and calculate the delta, angle, and \
         per-side correction. Follow the machine's squaring procedure for any adjustment.",
    ),
    ("NEWEST FIRST", "NEWEST FIRST"),
    (
        "NO BACKUP EXPORTED THIS SESSION",
        "NO BACKUP EXPORTED THIS SESSION",
    ),
    ("NO JOBS RECORDED", "NO JOBS RECORDED"),
    ("NO MOTION / NO WRITE", "NO MOTION / NO WRITE"),
    ("NO PER-SIDE CORRECTION", "NO PER-SIDE CORRECTION"),
    ("NOT SAVED", "NOT SAVED"),
    (
        "No measurement yet. Enter the span and both Y readings in the calculator.",
        "No measurement yet. Enter the span and both Y readings in the calculator.",
    ),
    (
        "No motion, probing, spindle-on, coolant-on, tool change, coordinate, or system \
         commands.",
        "No motion, probing, spindle-on, coolant-on, tool change, coordinate, or system \
         commands.",
    ),
    (
        "No recent files yet. A loaded local file appears here.",
        "No recent files yet. A loaded local file appears here.",
    ),
    ("OF", "OF"),
    ("ONE COMMAND", "ONE COMMAND"),
    ("OPEN CALCULATOR", "OPEN CALCULATOR"),
    ("OPEN HISTORY", "OPEN HISTORY"),
    ("OPEN MACROS", "OPEN MACROS"),
    (
        "One .nc, .gcode, .tap, or .ngc file. Loading never starts the job.",
        "One .nc, .gcode, .tap, or .ngc file. Loading never starts the job.",
    ),
    ("PER-SIDE", "PER-SIDE"),
    ("PER-SIDE (PIVOT CENTRE)", "PER-SIDE (PIVOT CENTRE)"),
    (
        "PIVOT CENTRE: MOVE THE LEFT SIDE BACK {amount} mm AND THE RIGHT SIDE FORWARD \
         {amount} mm",
        "PIVOT CENTRE: MOVE THE LEFT SIDE BACK {amount} mm AND THE RIGHT SIDE FORWARD \
         {amount} mm",
    ),
    (
        "PIVOT CENTRE: MOVE THE RIGHT SIDE BACK {amount} mm AND THE LEFT SIDE FORWARD \
         {amount} mm",
        "PIVOT CENTRE: MOVE THE RIGHT SIDE BACK {amount} mm AND THE LEFT SIDE FORWARD \
         {amount} mm",
    ),
    ("POST", "POST"),
    (
        "POST MACRO - after the final safe-Z retract",
        "POST MACRO - after the final safe-Z retract",
    ),
    ("PRE", "PRE"),
    (
        "PRE MACRO - after the safety preamble",
        "PRE MACRO - after the safety preamble",
    ),
    ("PREFLIGHT OK", "PREFLIGHT OK"),
    ("PROBE DEFAULTS", "PROBE DEFAULTS"),
    ("PROBE MACROS", "PROBE MACROS"),
    (
        "Press CALCULATE to validate the fields and show the correction.",
        "Press CALCULATE to validate the fields and show the correction.",
    ),
    ("RECENT", "RECENT"),
    ("RECENT FILES", "RECENT FILES"),
    ("REFUSED", "REFUSED"),
    ("REMOVE", "REMOVE"),
    (
        "RESET GATES READY - TWO-STEP CONFIRMATION REQUIRED",
        "RESET GATES READY - TWO-STEP CONFIRMATION REQUIRED",
    ),
    ("RESET LOCKED", "RESET LOCKED"),
    ("RESULT", "RESULT"),
    ("RIGHT FORWARD", "RIGHT FORWARD"),
    (
        "RIGHT SIDE IS {amount} mm FORWARD; MOVE IT BACK {amount} mm OR SHIM THE LEFT SIDE",
        "RIGHT SIDE IS {amount} mm FORWARD; MOVE IT BACK {amount} mm OR SHIM THE LEFT SIDE",
    ),
    ("Recent file was not removed", "Recent file was not removed"),
    (
        "Recent files were not cleared",
        "Recent files were not cleared",
    ),
    ("SAVE DEFAULTS", "SAVE DEFAULTS"),
    ("SAVE MACROS", "SAVE MACROS"),
    ("SAVED", "SAVED"),
    (
        "SAVED PER CONTROLLER PROFILE",
        "SAVED PER CONTROLLER PROFILE",
    ),
    ("SHOWING", "SHOWING"),
    ("SPAN BETWEEN POINTS", "SPAN BETWEEN POINTS"),
    ("SQUARE", "SQUARE"),
    ("STEP", "STEP"),
    (
        "STOCK INFERRED FROM PROGRAM BOUNDS | NOT A MEASURED MODEL",
        "STOCK INFERRED FROM PROGRAM BOUNDS | NOT A MEASURED MODEL",
    ),
    ("STOCK OFF", "STOCK OFF"),
    ("STOCK ON", "STOCK ON"),
    ("STORED", "STORED"),
    ("TAP FOR", "TAP FOR"),
    ("THEME", "THEME"),
    (
        "The controller may reset or re-handshake after this command. The app does not \
         reconnect, home, jog, or resume anything. CANCEL sends nothing.",
        "The controller may reset or re-handshake after this command. The app does not \
         reconnect, home, jog, or resume anything. CANCEL sends nothing.",
    ),
    (
        "There is no APPLY. Move or shim the gantry by hand, then measure again.",
        "There is no APPLY. Move or shim the gantry by hand, then measure again.",
    ),
    (
        "These lines run inside the generated probe program and are still subject to every \
         preflight gate. SAVE MACROS writes only the local controller profile: no controller \
         command, setting, offset, or start.",
        "These lines run inside the generated probe program and are still subject to every \
         preflight gate. SAVE MACROS writes only the local controller profile: no controller \
         command, setting, offset, or start.",
    ),
    (
        "This removes every stored job record from app.json on this computer. It does not \
         change the loaded program or send anything to the controller.",
        "This removes every stored job record from app.json on this computer. It does not \
         change the loaded program or send anything to the controller.",
    ),
    (
        "bounded pre/post lines; local profile; still subject to preflight",
        "bounded pre/post lines; local profile; still subject to preflight",
    ),
    ("bytes and", "bytes and"),
    ("correction is not finite", "correction is not finite"),
    ("drop one program file", "drop one program file"),
    (
        "file contains binary data, not a G-code program",
        "file contains binary data, not a G-code program",
    ),
    ("file is", "file is"),
    ("file is empty", "file is empty"),
    (
        "file is not valid UTF-8 text",
        "file is not valid UTF-8 text",
    ),
    ("is empty", "is empty"),
    (
        "is not a .nc, .gcode, .tap, or .ngc program",
        "is not a .nc, .gcode, .tap, or .ngc program",
    ),
    ("is not a readable folder", "is not a readable folder"),
    ("is not a regular file", "is not a regular file"),
    ("is not finite", "is not finite"),
    ("is out of range", "is out of range"),
    ("items were dropped", "items were dropped"),
    ("lines per macro.", "lines per macro."),
    (
        "local JSON only; nothing is sent to the controller",
        "local JSON only; nothing is sent to the controller",
    ),
    (
        "mm | probe tip - controlled point | plan shifts controlled by -offset",
        "mm | probe tip - controlled point | plan shifts controlled by -offset",
    ),
    ("must be a whole number", "must be a whole number"),
    ("must be at most", "must be at most"),
    ("must be finite", "must be finite"),
    ("must be greater than 0", "must be greater than 0"),
    ("must be numeric", "must be numeric"),
    ("must be within", "must be within"),
    (
        "the limit for the file browser is",
        "the limit for the file browser is",
    ),
    ("REVIEW", "REVIEW"),
    (
        "PHYSICAL E-STOP LOCATED AND TESTED",
        "PHYSICAL E-STOP LOCATED AND TESTED",
    ),
    (
        "SPINDLE CAN BE STOPPED INDEPENDENTLY",
        "SPINDLE CAN BE STOPPED INDEPENDENTLY",
    ),
    (
        "MACHINE TRAVEL AND SOFT LIMITS VERIFIED",
        "MACHINE TRAVEL AND SOFT LIMITS VERIFIED",
    ),
    (
        "WORKHOLDING AND TOOL CLEARANCE CHECKED",
        "WORKHOLDING AND TOOL CLEARANCE CHECKED",
    ),
    (
        "SIMULATOR OR SPINDLE-OFF FIRST CONNECTION",
        "SIMULATOR OR SPINDLE-OFF FIRST CONNECTION",
    ),
    (
        "Know where it is, reach it, and prove it stops the machine before any commanded \
         move.",
        "Know where it is, reach it, and prove it stops the machine before any commanded \
         move.",
    ),
    (
        "Use the VFD or the power control, not this sender, to stop the cutter.",
        "Use the VFD or the power control, not this sender, to stop the cutter.",
    ),
    (
        "Confirm the physical travel and the controller's soft-limit settings.",
        "Confirm the physical travel and the controller's soft-limit settings.",
    ),
    (
        "Secure the stock, clear the tool, and keep hands clear of every axis.",
        "Secure the stock, clear the tool, and keep hands clear of every axis.",
    ),
    (
        "Learn the sender in the simulator, then connect with the spindle off and run only \
         status, DRO, and small jogs.",
        "Learn the sender in the simulator, then connect with the spindle off and run only \
         status, DRO, and small jogs.",
    ),
    ("STOPPED", "STOPPED"),
    ("CONFIRM APPLY TLO", "CONFIRM APPLY TLO"),
    ("CONFIRM CLEAR TLO", "CONFIRM CLEAR TLO"),
    ("HIGH CONTRAST", "HIGH CONTRAST"),
    ("LIGHT", "LIGHT"),
    ("DARK", "DARK"),
    ("LEFT Y READING", "LEFT Y READING"),
    ("RIGHT Y READING", "RIGHT Y READING"),
    (
        "OFFSETS AND CONTROLLER TOOL DATA",
        "OFFSETS AND CONTROLLER TOOL DATA",
    ),
    ("CORE SETTING DEFAULTS", "CORE SETTING DEFAULTS"),
    ("DRIVER AND PLUGIN DEFAULTS", "DRIVER AND PLUGIN DEFAULTS"),
    (
        "ALL SETTINGS, PARAMETERS, STARTUP LINES, AND BUILD INFO",
        "ALL SETTINGS, PARAMETERS, STARTUP LINES, AND BUILD INFO",
    ),
    (
        "Offsets and controller tool data. Local profile, backups, and saved maps stay.",
        "Offsets and controller tool data. Local profile, backups, and saved maps stay.",
    ),
    (
        "Core setting defaults. Offsets and tool data stay.",
        "Core setting defaults. Offsets and tool data stay.",
    ),
    (
        "Driver and plugin defaults. Core settings, offsets, and tool data stay.",
        "Driver and plugin defaults. Core settings, offsets, and tool data stay.",
    ),
    (
        "All settings, parameters, startup lines, and build info. Most destructive.",
        "All settings, parameters, startup lines, and build info. Most destructive.",
    ),
    (
        "Offsets ($#) and controller tool data are reset. Work coordinate systems, G28/G30, \
         G92, TLO, and the controller's tool records may be lost. Local profile data, \
         backups, and saved height maps stay.",
        "Offsets ($#) and controller tool data are reset. Work coordinate systems, G28/G30, \
         G92, TLO, and the controller's tool records may be lost. Local profile data, \
         backups, and saved height maps stay.",
    ),
    (
        "Core controller settings return to their defaults. Offsets and controller tool data \
         stay. Export a backup before changing settings again.",
        "Core controller settings return to their defaults. Offsets and controller tool data \
         stay. Export a backup before changing settings again.",
    ),
    (
        "Driver and plugin settings return to their defaults. Core settings, offsets, and \
         controller tool data stay.",
        "Driver and plugin settings return to their defaults. Core settings, offsets, and \
         controller tool data stay.",
    ),
    (
        "ALL settings, parameters, startup lines, and build info are reset. Offsets, tool \
         data, and controller parameters may be unrecoverable. This is the most destructive \
         reset and the controller will usually re-handshake.",
        "ALL settings, parameters, startup lines, and build info are reset. Offsets, tool \
         data, and controller parameters may be unrecoverable. This is the most destructive \
         reset and the controller will usually re-handshake.",
    ),
    (
        "Requested read-only `$#` refresh.",
        "Requested read-only `$#` refresh.",
    ),
    (
        "Requested read-only `$$` settings dump.",
        "Requested read-only `$$` settings dump.",
    ),
    (
        "No read-only report is defined for driver parameters.",
        "No read-only report is defined for driver parameters.",
    ),
    (
        "Requested read-only `$#` and `$$` refreshes.",
        "Requested read-only `$#` and `$$` refreshes.",
    ),
    (
        "MOCK MODE: controller reset needs a live controller",
        "MOCK MODE: controller reset needs a live controller",
    ),
    (
        "NOT LIVE: connect the controller before reset",
        "NOT LIVE: connect the controller before reset",
    ),
    (
        "LINK NOT READY: reset is disabled until the handshake completes",
        "LINK NOT READY: reset is disabled until the handshake completes",
    ),
    (
        "ALARM OR DOOR: clear the alarm or door state before reset",
        "ALARM OR DOOR: clear the alarm or door state before reset",
    ),
    (
        "JOB ACTIVE: stop or finish the job before reset",
        "JOB ACTIVE: stop or finish the job before reset",
    ),
    (
        "TOOL CHANGE PENDING: finish or stop the tool change before reset",
        "TOOL CHANGE PENDING: finish or stop the tool change before reset",
    ),
    (
        "PROBE ACTIVE: finish or abort the probe run before reset",
        "PROBE ACTIVE: finish or abort the probe run before reset",
    ),
    (
        "HEIGHT MAP BUSY: finish the Apply or Revert before reset",
        "HEIGHT MAP BUSY: finish the Apply or Revert before reset",
    ),
    (
        "PENDANT ACTIVE (MPG:1): release the pendant before reset",
        "PENDANT ACTIVE (MPG:1): release the pendant before reset",
    ),
    (
        "SETTINGS WRITE PENDING: finish or discard it before reset",
        "SETTINGS WRITE PENDING: finish or discard it before reset",
    ),
    (
        "OFFSET WRITE PENDING: finish the write and refresh before reset",
        "OFFSET WRITE PENDING: finish the write and refresh before reset",
    ),
    (
        "NOT IDLE: reset is available only while the controller reports Idle",
        "NOT IDLE: reset is available only while the controller reports Idle",
    ),
    (
        "BACKUP EXPORT IN FLIGHT: wait for the export to finish",
        "BACKUP EXPORT IN FLIGHT: wait for the export to finish",
    ),
    (
        "BACKUP NOT ACKNOWLEDGED: export a backup or accept data loss first",
        "BACKUP NOT ACKNOWLEDGED: export a backup or accept data loss first",
    ),
    (
        "LOCAL PROFILE IS NOT CHANGED AUTOMATICALLY",
        "LOCAL PROFILE IS NOT CHANGED AUTOMATICALLY",
    ),
    (
        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY. Follow the machine's \
         squaring procedure, move or shim the gantry by hand, and verify the result with a \
         fresh measurement before the next cut.",
        "CALCULATOR ONLY - THE APP DOES NOT MOVE OR ADJUST THE GANTRY. Follow the machine's \
         squaring procedure, move or shim the gantry by hand, and verify the result with a \
         fresh measurement before the next cut.",
    ),
    (
        "Positive delta and angle: the right measurement point is further along +Y (forward) \
         than the left. Negative: the left point is further along +Y. Per-side is delta / 2: \
         with the centre fixed, each end moves half the delta in opposite directions.",
        "Positive delta and angle: the right measurement point is further along +Y (forward) \
         than the left. Negative: the left point is further along +Y. Per-side is delta / 2: \
         with the centre fixed, each end moves half the delta in opposite directions.",
    ),
    ("SENT", "SENT"),
    (
        "NO PROGRAM: load a program before rotating its source",
        "NO PROGRAM: load a program before rotating its source",
    ),
    (
        "NO PROGRAM: load a program before scaling its source",
        "NO PROGRAM: load a program before scaling its source",
    ),
    ("NO WARNINGS", "NO WARNINGS"),
    (
        "No program loaded: the source check has nothing to scan.",
        "No program loaded: the source check has nothing to scan.",
    ),
    ("PREVIEW ROTATION", "PREVIEW ROTATION"),
    ("PREVIEW SCALE", "PREVIEW SCALE"),
    ("PROGRAM CHECK", "PROGRAM CHECK"),
    ("REVIEW PREVIEW", "REVIEW PREVIEW"),
    ("ROTATE PROGRAM", "ROTATE PROGRAM"),
    ("ROTATION PREVIEW", "ROTATION PREVIEW"),
    ("SCALE PREVIEW", "SCALE PREVIEW"),
    ("SCALE PROGRAM", "SCALE PROGRAM"),
    ("UNIFORM / WORK PIVOT", "UNIFORM / WORK PIVOT"),
    ("USE PROBE ANGLE", "USE PROBE ANGLE"),
    ("WARNINGS", "WARNINGS"),
    ("WORK PIVOT / CCW +", "WORK PIVOT / CCW +"),
    (
        "I understand this loads the rotated source and does not start it",
        "I understand this loads the rotated source and does not start it",
    ),
    (
        "I understand this loads the scaled source and does not start it",
        "I understand this loads the scaled source and does not start it",
    ),
    (
        "INFORMATIONAL - START IS NOT BLOCKED. NOT A MACHINE/TRAVEL CHECK.",
        "INFORMATIONAL - START IS NOT BLOCKED. NOT A MACHINE/TRAVEL CHECK.",
    ),
    (
        "Missing app tool-table entries are warnings, not blockers. The check reads source text \
         and parsed counts only; it cannot verify controller state, offsets, tool length, \
         fixtures, or actual travel. No controller command is sent.",
        "Missing app tool-table entries are warnings, not blockers. The check reads source text \
         and parsed counts only; it cannot verify controller state, offsets, tool length, \
         fixtures, or actual travel. No controller command is sent.",
    ),
    (
        "No feed, spindle, tool, machine-coordinate, or probe warnings in this program.",
        "No feed, spindle, tool, machine-coordinate, or probe warnings in this program.",
    ),
    (
        "Pivot is in work/program coordinates; a positive angle is counter-clockwise. Unsupported \
         programs are refused before any load.",
        "Pivot is in work/program coordinates; a positive angle is counter-clockwise. Unsupported \
         programs are refused before any load.",
    ),
    (
        "Rotation is reported only. USE PROBE ANGLE fills the Utility angle with -deviation; the \
         pivot stays in work coordinates and nothing is loaded or sent.",
        "Rotation is reported only. USE PROBE ANGLE fills the Utility angle with -deviation; the \
         pivot stays in work coordinates and nothing is loaded or sent.",
    ),
    (
        "SOURCE SCAN WAS TRUNCATED: the check did not read every line.",
        "SOURCE SCAN WAS TRUNCATED: the check did not read every line.",
    ),
    (
        "Uniform positive factor about a pivot in work/program millimetres. Feeds, speeds, and Z \
         depths are unchanged. Unsupported programs are refused before any load.",
        "Uniform positive factor about a pivot in work/program millimetres. Feeds, speeds, and Z \
         depths are unchanged. Unsupported programs are refused before any load.",
    ),
];
