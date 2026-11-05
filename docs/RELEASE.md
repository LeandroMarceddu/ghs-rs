# Release checklist

This is the release runbook for ghs-rs. It covers the three desktop platforms
built by [`.github/workflows/release.yml`](../.github/workflows/release.yml):
Linux x86_64 (`.tar.gz`, `.deb`, and `.AppImage`), Windows x86_64, and macOS
(the archive name records the runner architecture).

The workflow builds and packages the app, then opens a **draft** GitHub
Release. It does not sign, notarize, test on hardware, or publish anything by
itself. A maintainer runs the gates below, inspects the artifacts, and decides
whether to publish.

> **Hardware gate.** A release is not hardware-validated until the operator
> checklist in [SAFETY.md](SAFETY.md) passes on the reference
> FlexiHAL / STM32F446 controller running grblHAL `1.1f.20240604`. Until that
> checklist passes, every build is a simulator-only pre-alpha preview: keep it
> as a draft or a GitHub pre-release and mark it **not hardware-validated** in
> the notes. Never present such a build as production-ready.
>
> The FlexiHAL board is currently unavailable, so this gate is open and no
> release can claim hardware validation.

## 1. Version and tag

- [ ] Decide the version and update `[workspace.package] version` in
      `Cargo.toml`. It is the single version for the workspace.
- [ ] Refresh the lock file with `cargo check --workspace`, then commit
      `Cargo.toml` and `Cargo.lock` in the release commit.
- [ ] Confirm the tag equals the workspace version with a leading `v`
      (`0.7.0` -> `v0.7.0`). The workflow accepts
      `vMAJOR.MINOR[.PATCH][-pre]` and rejects anything else.
- [ ] Create an annotated tag on the exact commit that passed CI:

      git tag -a v0.7.0 -m "ghs-rs v0.7.0"

- [ ] Never move, force-push, or reuse a published tag. If the tag is wrong and
      nothing has been published or downloaded, delete it and start over.

## 2. Changelog and release notes

- [ ] Update `CHANGELOG.md`: move the relevant `Unreleased` entries under the
      new version and date, then review the notes for accuracy. The workflow
      also asks GitHub to generate release notes from commits and merged pull
      requests; edit that draft.
- [ ] Group the notes under Added, Changed, Fixed, and Safety.
- [ ] Call out protocol surface changes, simulator profile changes, config or
      backup format changes, and known limitations.
- [ ] Do not publish the auto-generated notes unedited.
- [ ] State the release status clearly: pre-alpha and not hardware-validated,
      unless the FlexiHAL gate below has passed for this exact build.

## 3. Local gates

Run from the repository root with the pinned toolchain from
`rust-toolchain.toml`:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p ghs-app
```

- [ ] All four commands pass locally, from a clean checkout of the release
      commit.
- [ ] Optional: `cargo deny check` if `cargo-deny` is installed; `deny.toml`
      covers licenses, advisories, and duplicate dependencies.
- [ ] CI currently runs formatting and clippy on Linux, the workspace test
      suite on Linux, and a debug build on all three OSes. A green CI run still
      does not replace running the release gates locally.
- [ ] Do not tag while any gate is failing, including a flaky test. Re-run it
      and record the result.

## 4. CI green on all three OSes

- [ ] In the Actions tab, open the `CI` run for the release commit.
- [ ] `Format`, `Clippy`, `Test`, and `Build app` must all pass.
- [ ] `Build app` must be green on `ubuntu-latest`, `windows-latest`, and
      `macos-latest`.
- [ ] Confirm the run is for the same commit SHA that will be tagged; a green
      run on a different SHA does not count.
- [ ] Wait for in-flight runs to finish before tagging.

## 5. Simulator scenarios

Run the app against the in-process simulator (`cargo run -p ghs-app` starts
against the FlexiHAL profile). The repository requires coverage of alarm, tool
change, probing, pause/resume, and disconnect:

- [ ] Alarm: trigger an alarm, verify motion is refused, the unlock path asks
      for confirmation, and the job never auto-resumes.
- [ ] Tool change: run the two-tool flow (`--demo-toolchange` preloads it
      stopped at `M6 T2`) and complete the tool-change sheet.
- [ ] Probing: complete a Z touch-plate cycle with safe Z and probe
      confirmation. The demo flags (`--probe-card=z`, `--demo-probe-check`,
      `--demo-probe-run`, `--demo-probe-apply`) exercise the same path without a
      physical probe.
- [ ] Pause/resume: run a program with an `M0` pause (`--demo-m0`) and verify
      resume, single-block, and stop behavior.
- [ ] Disconnect: disconnect while streaming and verify the job pauses and does
      not resume on reconnect.
- [ ] Record the results in the release record below. Any safety failure blocks
      the release.

Also verify the always-visible stop/reset control, the status watchdog, and
that settings writes remain idle-only. These are non-negotiable rules in
[SAFETY.md](SAFETY.md).

## 6. Licenses and notices

- [ ] `LICENSE-APACHE`, `LICENSE-MIT`, and `NOTICE` are present at the
      repository root and unchanged where they should be.
- [ ] `NOTICE` still credits ioSender (behavioral reference only), GPUI, and
      GPUI Kit.
- [ ] Review new dependencies for license compatibility; `cargo deny check
      licenses` is the automated check when available.
- [ ] `packaging/README.md`, `packaging/linux/ghs-rs.desktop`, and
      `packaging/linux/ghs-rs.svg` are current.
- [ ] Validate the desktop entry:

      desktop-file-validate packaging/linux/ghs-rs.desktop

- [ ] The Linux archive includes `ghs-app`, `README.md`, both licenses,
      `NOTICE`, `PACKAGING.md`, `ghs-rs.desktop`, and `ghs-rs.svg`.
- [ ] The Debian package installs `usr/bin/ghs-app`, the desktop entry and
      icon, and the README, licenses, NOTICE, copyright, and changelog under
      `usr/share/doc/ghs-rs/`. Check `ar t` reports `debian-binary`,
      `control.tar.gz`, and `data.tar.gz` when `dpkg-deb` is unavailable.
- [ ] The AppImage contains `AppRun`, a root `ghs-rs.desktop` and `ghs-rs.svg`,
      a relative `.DirIcon` symlink, and the same documentation under
      `usr/share/doc/ghs-rs/`.

## 7. Run the release workflow

- [ ] Push the tag: `git push origin v0.7.0`.
- [ ] Alternatively, open Actions -> `Release` -> `Run workflow` from `main`
      and enter the existing tag in the `tag` input. The workflow does not
      create tags.
- [ ] Watch the jobs: `Resolve release tag`, `Package (Linux x86_64)`,
      `Package (Windows x86_64)`, `Package (macOS)`, and
      `Draft GitHub Release`.
- [ ] The Linux job runs `cargo build --release -p ghs-app`, then
      `packaging/linux/build-deb.sh` and
      `packaging/linux/build-appimage.sh`, and logs the tar, `ar t` members,
      and AppImage layout. It uploads the tar.gz, `.deb`, and `.AppImage` as
      one artifact.
- [ ] The Windows job builds the zip; the macOS job builds `ghs-rs.app`, tars
      the bundle, and uploads one archive.
- [ ] The AppImage build downloads appimagetool 1.9.1 (checksum-pinned) and its
      type 2 runtime, so the Linux job needs network access to GitHub. It uses
      `--appimage-extract-and-run`, so FUSE is not required on the runner.
- [ ] The final job verifies exactly five assets (Linux tar.gz, `.deb`, and
      `.AppImage`, the Windows zip, and one macOS tar.gz), writes
      `SHA256SUMS.txt`, and creates a **draft** release.
- [ ] If a job fails, fix the cause on a branch and re-run. Do not publish a
      partial release.

## 8. Artifact inspection

Expected assets for tag `v0.7.0`. The tar.gz archives contain `README.md`,
`LICENSE-APACHE`, `LICENSE-MIT`, `NOTICE`, and `PACKAGING.md`; the table lists
only the differences.

| Platform | Asset | Contents |
|---|---|---|
| Linux x86_64 | `ghs-rs-v0.7.0-linux-x86_64.tar.gz` | `ghs-app`, `ghs-rs.desktop`, `ghs-rs.svg` |
| Linux x86_64 | `ghs-rs_0.7.0_amd64.deb` | `usr/bin/ghs-app`, desktop entry and icon, docs |
| Linux x86_64 | `ghs-rs-v0.7.0-x86_64.AppImage` | AppDir payload plus `AppRun` |
| Windows x86_64 | `ghs-rs-v0.7.0-windows-x86_64.zip` | `ghs-app.exe` |
| macOS | `ghs-rs-v0.7.0-macos-<arch>.tar.gz` | `ghs-rs.app` bundle |

- [ ] Download every asset from the draft release, not from a local build.
- [ ] List each archive and confirm the layout above. The tar.gz archives must
      unpack to one top-level directory named after the archive; the `.deb` and
      `.AppImage` are single files.
- [ ] Confirm there are no stray files: no `target/` tree, no `.pdb`, no test
      binaries, no local config.
- [ ] Linux: `file ghs-app` reports an x86-64 ELF executable; it starts on a
      desktop with X11/Wayland and Vulkan, and the desktop entry starts the
      same binary via `Exec=ghs-app`.
- [ ] Linux `.deb`: `ar t` reports exactly `debian-binary`, `control.tar.gz`,
      and `data.tar.gz`. On a disposable VM or container,
      `sudo apt install ./ghs-rs_0.7.0_amd64.deb` installs `/usr/bin/ghs-app`,
      the desktop entry, and the icon; `sudo apt remove ghs-rs` removes them.
      `sudo dpkg -i ghs-rs_0.7.0_amd64.deb` plus `sudo apt-get install -f` is
      the fallback when dependencies are missing. If `dpkg` is unavailable,
      record that the package was only structurally validated.
- [ ] Linux `.AppImage`: after `chmod +x`, running
      `./ghs-rs-v0.7.0-x86_64.AppImage --appimage-extract` works without
      FUSE. The extracted `.DirIcon` is a relative symlink to `ghs-rs.svg`;
      the root desktop entry and icon exist; `usr/bin/ghs-app` is mode 755;
      `desktop-file-validate` passes. Run it on a host without `libfuse2`
      using `--appimage-extract-and-run`.
- [ ] Windows: `Get-AuthenticodeSignature` reports `NotSigned` (expected), and
      the executable starts on a clean VM.
- [ ] macOS: the tar contains `ghs-rs.app` with `Contents/MacOS/ghs-app`,
      `Contents/Info.plist` (CFBundleName `grblHAL Sender`, identifier
      `org.ghs-rs.sender`), and licenses/NOTICE in `Contents/Resources/`.
      `codesign -dv ghs-rs.app` reports that the bundle is not signed, and
      `spctl -a -vv ghs-rs.app` rejects it (expected); it starts after the
      per-app quarantine removal described in `packaging/README.md`.
- [ ] Each binary starts against the simulator, shows the connection panel,
      and keeps the stop/reset control visible.
- [ ] Re-check the asset names against the tag. A mismatch means the wrong ref
      was built; delete the draft and fix it.

## 9. Checksums

- [ ] The release job writes `SHA256SUMS.txt` next to the artifacts. It covers
      the Linux tar.gz, `.deb`, and `.AppImage`, the Windows zip, and the macOS
      tar.gz.
- [ ] Download the assets and the checksum file into one directory, then run:

      sha256sum -c SHA256SUMS.txt

- [ ] On Windows, compare `Get-FileHash -Algorithm SHA256` for each asset with
      the matching line.
- [ ] If a checksum does not match, do not publish. Delete the draft and re-run
      the workflow.
- [ ] After publishing, never replace an asset under the same tag. Cut a new
      patch release instead.

## 10. Signing caveat

The workflow uses only `GITHUB_TOKEN` and does no signing. Be explicit about
this in the release notes:

- macOS builds are not code-signed or notarized. Gatekeeper blocks the first
  launch of `ghs-rs.app` until the user allows that app.
- Windows builds have no Authenticode signature. SmartScreen warns on first
  launch.
- Linux assets have no signature. The `.deb` is not from a signed repository,
  and the `.AppImage` has no embedded signature or update information. Users
  must verify the SHA-256 checksum themselves.

Never tell users to disable Gatekeeper or SmartScreen system-wide; only
per-file or per-app overrides are acceptable. Code signing, notarization, and an
update channel are follow-ups that need maintainer certificates and managed
secrets, which are deliberately out of scope for this workflow.

## 11. Hardware validation gate

This is the gate that turns a preview into a release. Run the full operator
checklist in [SAFETY.md](SAFETY.md) on the reference FlexiHAL / STM32F446 with
grblHAL `1.1f.20240604`:

- [ ] Simulator scenarios pass on the exact release build.
- [ ] Connect, disconnect, and reconnect with the spindle off.
- [ ] Home and jog each axis; verify DRO directions and units.
- [ ] Verify feed, rapid, and spindle overrides.
- [ ] Verify E-stop, door, feed hold, and cycle start signals.
- [ ] Run a small air-cut program with pause, resume, and stop.
- [ ] Run a tool change with the tool clear.
- [ ] Verify the probing cycles with a test plate and safe Z.
- [ ] Verify limits and alarms, and the unlock path.
- [ ] Confirm the job never resumes after an alarm, E-stop, or reconnect.

If any item fails, stop. Do not publish that build as validated; fix the
problem, cut a new patch tag, and repeat the whole gate. Record the controller
`$I+` response and the asset SHA-256 in the release record, because a later
rebuild from the same tag is not the same artifact.

A release that has not passed this checklist must say so in the release notes.
At the time of writing the reference board is unavailable, so all builds are
simulator-only previews.

## 12. Publish

- [ ] The draft has the right tag, target commit, title, and notes.
- [ ] All five assets and `SHA256SUMS.txt` are attached.
- [ ] The notes state the hardware status and the signing caveat.
- [ ] Mark it pre-release for `-rc`/`-beta` tags or while the hardware gate is
      open.
- [ ] Publish, then announce the exact tag and checksums.

## 13. Rollback

Before publishing:

- [ ] Delete the draft release. The tag and workflow run remain.
- [ ] Delete the run's artifacts if they are wrong.
- [ ] If the tag itself is wrong and nothing has been published or downloaded,
      delete and recreate it. This is the only time moving a tag is acceptable.

After publishing:

- [ ] Do not silently replace binaries under an existing tag. Users and mirrors
      may already hold copies, and the published checksums would no longer
      match.
- [ ] For a broken but not dangerous build: add a warning at the top of the
      release notes, mark the release as a pre-release (or convert it back to a
      draft), and publish a fixed patch release from a revert or fix commit.
- [ ] For a dangerous build, such as a motion-safety regression: convert the
      release back to a draft or delete it, keep the tag for traceability, add
      a prominent warning, and notify users through the same channels used for
      the announcement.
- [ ] `git revert` the bad change on `main`; never force-push `main` and never
      re-point a tag that was published.
- [ ] Record the rollback and the replacement tag in the release record.

## 14. Release record

Copy this table into the release issue, the draft notes, or the maintenance log.

| Field | Value |
|---|---|
| Tag | |
| Commit SHA | |
| CI run URL | |
| Release run URL | |
| `ghs-rs-*-linux-x86_64.tar.gz` SHA-256 | |
| `ghs-rs_*_amd64.deb` SHA-256 | |
| `ghs-rs-*-x86_64.AppImage` SHA-256 | |
| `ghs-rs-*-windows-x86_64.zip` SHA-256 | |
| `ghs-rs-*-macos-<arch>.tar.gz` SHA-256 | |
| Simulator scenarios | pass / fail |
| FlexiHAL checklist | pass / fail / not run |
| Signed / notarized | no |
| Published by | |
| Notes | |
