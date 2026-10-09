# Native desktop dependency review

Reviewed 2026-10-09 against the resolved workspace and current official terms. Original Lumeryn source remains unpublished proprietary code. Cargo-deny checks metadata; this review records obligations that metadata alone cannot establish. Packaging/model/codec distribution remains separately gated.

Native selections: egui/eframe/egui-wgpu 0.36.2 and wgpu 30.0.1 use MIT OR Apache-2.0; egui_dock 0.21.1 uses MIT; CPAL 0.18.2 uses Apache-2.0; image 0.25.10 uses MIT OR Apache-2.0. Retain the selected upstream license/copyright notices in binary distribution. The locked graph has one egui and one wgpu version; other platform-specific duplicates are warning-level and reviewed in the generated inventory. Rust1.97.1 satisfies the selected crates' toolchain requirement; successful Windows compilation is still required.

## Individually reviewed exceptions

`clipboard-win` 5.4.1 and `error-code` 3.4.0 use Boost Software License1.0. It permits commercial use and redistribution with notice requirements and an object-code exception. Keep their notices in the third-party distribution inventory regardless. This is a narrow crate/version allowlist, not blanket acceptance of unknown terms. [Official Boost text](https://www.boost.org/LICENSE_1_0.txt).

`epaint_default_fonts` 0.36.2 embeds unmodified fonts and reports (MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0. OFL allows embedding/bundling in commercial software with copyright/license notices; fonts cannot be sold by themselves and modified reserved names require review. Ubuntu's font terms also permit proprietary software embedding, retain font licensing/notices and impose modification naming rules. These terms apply to the fonts, without requiring Lumeryn source or rendered documents to adopt them. Narrowly allow this package/version; do not assume other fonts are approved. [OFL official terms](https://openfontlicense.org/open-font-license-official-text/), [Canonical font terms](https://canonical.com/legal/font-licence).

The crate also includes Hack-Regular's MIT/Bitstream Vera lineage and emoji-icon-font MIT notices; its SPDX field is insufficient to capture every notice. Exact upstream text is retained in `docs/licensing/native-fonts/` from the locked source package, including the Bitstream reserved-name/standalone-sale conditions. Ship these texts with any distribution that embeds these fonts. No font bytes are modified or separately sold. UI fonts are separate from the later professional export typography pipeline.

## Maintenance advisory

RUSTSEC-2024-0436 identifies `paste` 1.0.15 as unmaintained. The required egui_dock 0.21.1 depends on this build-time procedural macro. The advisory reports maintenance cessation, with no patched upstream version; it does not identify a runtime exploit. Accept a specifically documented maintenance exception for this pinned dependency while keeping all other advisory checks active. Alternatives include upstream adoption of pastey/ordinary macros or a reviewed local patch; avoid a disruptive vendored fork before proving the native shell. Review this exception by 2027-01-09 and on every docking dependency upgrade. [RustSec advisory and alternatives](https://rustsec.org/advisories/RUSTSEC-2024-0436.html).

The exception does not certify dependency security. Re-run cargo-deny on the final lockfile and keep audit date/hash evidence. No GPL/AGPL application dependency, model weight, installer plugin or user FFmpeg binary is thereby approved for redistribution.
