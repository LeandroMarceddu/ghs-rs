# Touch shell layout review

Status: first M0 shell reviewed; **layout not approved**. The visual language
(dark shop-floor, teal accents, card treatment, status colors) is good and stays.

## What is wrong with the first pass

Observed at 2048×1152:

- The bottom area is two rows of cards with **different heights and widths**:
  MDI/DRO/Work are taller than Feed/Jog/Spindle; widths range from ~210 px to
  ~1400 px.
- **MDI is ~850 px wide while the console above it is ~380 px.** The MDI input
  is one line but its card is mostly empty space.
- **Feed override is ~210 px wide** and cramped, while **Jog is ~1400 px** and
  mostly empty.
- The **Work card is ~200 px wide** and its values are clipped/tiny.
- The deck is ~350 px tall. At 1280×800 that consumes ~44% of the window and
  leaves too little viewer.
- Top mode tabs are ~40 px, below the 48 px touch target.
- Signals/Work labels are ~10 px, below the comfortable touch reading size.

## Confirmed layout: asymmetric three-column grid

C1 and C2 have three rows. C3 has two rows: its top cell spans C1/C2 rows 1–2
and holds Coolant → Macros → Signals → Work as one continuous area, with **no
row separator between Signals and Work**. Its bottom cell is Spindle, aligned
with the Feed/Jog row.

```text
┌────────────────┬──────────────────────────────┬─────────────────┐
│ Console/Program│ Viewer                       │ Coolant         │
│                │                              │ Macros          │
│                │                              │ Signals         │
│                │                              │ Work            │
├────────────────┼──────────────────────────────┤                 │
│ MDI            │ DRO                          │                 │
├────────────────┼──────────────────────────────┼─────────────────┤
│ Feed           │ Jog                          │ Spindle         │
└────────────────┴──────────────────────────────┴─────────────────┘
```

Shared guides:

- C1/C2 row 2 ends where C3's tall top cell ends.
- C1/C2 row 3 ends where C3's Spindle ends.
- C3 has no guide between Signals and Work; the tall top cell is continuous.
- MDI aligns under the console; DRO under the viewer; Feed under MDI; Jog under
  DRO.
- Spindle is exactly the height of the Feed/Jog row.

Rules:

- Left column: console/program (flex), MDI (112 px), Feed (112 px).
- Center column: viewer (flex), DRO (112 px), Jog (112 px).
- Right column: one flex cell containing Coolant, Macros, Signals, Work with no
  divider between Signals and Work; then Spindle (112 px).
- The right flex cell must end on the MDI/DRO bottom line. If its content
  overflows, scroll inside it; never push Spindle down.
- Work finally gets the height of rows 1+2, so its four rows can breathe.
- Every card fills its grid cell; no fixed pixel widths.
- The bottom controls are not a separate full-width strip; they are the bottom
  of each column.

## Grid and tokens

| Token | Value |
|---|---|
| Top bar | 56 px |
| Mode tabs | min 48 px height, 56 px min width |
| MDI/DRO and Feed/Jog rows | 112 px (min 104, max 120) |
| Right top cell | spans rows 1–2; bottom aligns with DRO |
| Card gap | 12 px |
| Card padding | 12 px horizontal, 10 px vertical |
| Card radius | 8 px |
| Section label | 11 px uppercase, muted, tracking +0.08em |
| Body | 13 px |
| DRO value | 26–28 px tabular |
| Touch target | 48×48 px minimum |
| Primary action | 48 px height |

Column widths:

- 1920×1080: left 400, right 260, center flexible.
- 1280×800: left 340, right 220, center flexible. MDI/DRO and Feed/Jog rows 104 px.
- Portrait: top bar; viewer ~45% height; console collapsible; bottom controls
  become a 2×2 or 2×3 grid. Test 1080×1920.

## Card requirements

### MDI

- One row: 48 px text input + 80 px Send button, 8 px gap.
- Hint ("Mock send appends to console") is one 11 px line under the input.
- No giant empty card. Height 112 px.
- Input never spans the viewer.

### DRO

- Three equal cells for X/Y/Z. Each: axis label, 26–28 px value, unit, 48 px
  zero button.
- Same height as MDI. No wide empty gaps.
- If the controller reports more axes, add a compact second row or a scrollable
  extra-axis strip; do not shrink the three primary axes.

### Jog

- Axis selector (X/Y/Z) as a 48 px segmented control.
- Step selector (0.01, 0.1, 1, 10, 100) as a 5-cell 48 px row.
- Minus/plus as 64×64 px targets on either side of the step row.
- Continuous toggle in the header.
- No large empty area; use a compact two-row grid.

### Feed

- Label, 24 px percentage, full-width slider, then −10 / 100% / +10 in a
  three-button row. Height 112 px; width = left column (340–400 px).

### Spindle

- On/off switch, RPM field, CW/CCW segmented control, override slider and value.
- Height 112 px; width = right column; aligns with Feed/Jog.

### Work

- Lives in the tall right top cell, directly below Signals, with no divider
  between the two.
- Key/value rows in a 2-column grid: Origin X/Y/Z and Feed.
- 11–12 px labels, 14 px values; no clipping. Use the extra height; do not
  squeeze it into a 112 px card.
- If it does not fit at 1280×800, let the right top cell scroll internally
  before shrinking the text below 11 px.

### Console, viewer, right rail

- Console fills the remaining left height; 12–13 px monospace, 20 px line height.
- Viewer fills the remaining center height; keep the job header, progress, WCS
  and unit chips.
- Right rail: coolant, macros, signals; keep labels at 11 px or larger.
  Signals may scroll if needed; do not squeeze Work/Spindle.

## Visual cleanup

- Keep the dark shop-floor theme, teal accent, amber probe, red E-stop,
  rounded cards.
- Use one border tone and one card background tone; avoid nested borders.
- Align all card headers to the same baseline.
- Remove duplicate unit labels inside DRO fields; one unit per axis is enough.
- The Reset/Stop pair stays large and high-contrast.

## Review gate

Before approval, capture at least:

1. 1920×1080 landscape.
2. 1280×800 landscape (the target mini-PC).
3. 1080×1920 portrait.

For each: no clipped text, no horizontal scroll, no card with a different
height than its row (except the intentional C3 top cell spanning rows 1–2),
all touch targets ≥48 px, console and MDI aligned, C3's top cell ending on the
DRO baseline, viewer still usable. The style is already right; this is a
layout/pacing pass, not a redesign.
