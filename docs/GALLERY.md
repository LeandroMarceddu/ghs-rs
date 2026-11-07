# Gallery

Real captures from the current `ghs-rs` build running against the in-process
FlexiHAL simulator. They show the touch-first layout, safety sheets, probing,
utilities, and viewer. No screenshot is a hardware validation: the reference
FlexiHAL controller is still offline, and every build remains simulator-first
evidence until the checklist in [SAFETY.md](SAFETY.md) passes.

[Back to the README](../README.md)

## Themes

The same Job workspace split diagonally between Light and Dark. High Contrast
is a third persisted theme; see the Utility captures below.

![Light and Dark themes](images/theme-light-dark.png)

## Setup and safety

| First-run setup | Over-travel confirmation |
|---|---|
| ![Setup wizard](images/gallery/setup-wizard.png) | ![Over-travel confirmation](images/gallery/over-travel-confirm.png) |

| Controller reset | Localized setup (Dutch) |
|---|---|
| ![Controller reset](images/gallery/controller-reset.png) | ![Dutch setup](images/gallery/localization-nl.png) |

## Jobs and files

| Job history | Recent files |
|---|---|
| ![Job history](images/gallery/job-history.png) | ![Recent files](images/gallery/recent-files.png) |

| Program check | Material surfacing |
|---|---|
| ![Program check](images/gallery/program-check.png) | ![Material surfacing](images/gallery/surfacing.png) |

## Probing

| Tool-length apply | Probe macros |
|---|---|
| ![Tool-length apply](images/gallery/tool-length-apply.png) | ![Probe macros](images/gallery/probe-macros.png) |

## Utilities

| Stepper calibration | Gantry squaring |
|---|---|
| ![Stepper calibration](images/gallery/stepper-calibration.png) | ![Gantry squaring](images/gallery/gantry-squaring.png) |

| Source rotation | Source scaling |
|---|---|
| ![Source rotation](images/gallery/source-rotation.png) | ![Source scaling](images/gallery/source-scale.png) |

## Viewer and themes

| 3D stock mesh | High contrast |
|---|---|
| ![3D stock mesh](images/gallery/3d-stock.png) | ![High contrast](images/gallery/high-contrast.png) |

## Portrait

The same shell in a 1080×1920 portrait viewport: one vertically scrolling page,
two-row top bar, and all six workspaces plus RESET/STOP reachable.

![Portrait Job workspace](images/gallery/portrait-job.png)

## What the captures do not prove

- No FlexiHAL hardware validation, probing sign-off, or spindle/travel check.
- No signed or notarized installer.
- No claim that a generated or transformed program is correct for a specific
  machine, stock, tool, or fixture.

Use the [operator guide](USER-GUIDE.md), [safety rules](SAFETY.md), and
[parity checklist](parity.md) before connecting a machine.
