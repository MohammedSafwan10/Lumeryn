# Checked visual transforms

Date: 2026-10-12. Area1 specialist report; isolated engine helpers are implemented and tested. This document does not certify finished native gestures, migration, CLI or export integration.

## Selection and contract

Keep the existing authoritative Rust graph and external FFmpeg. No new dependency, mandatory runtime, graphics backend or unsafe code is introduced by this module. `visual_transform.rs` owns strict serialized fixed-point values and checked numeric-only lowering. The parent integrator owns schema4 settings for primary and additional visual placements, source metadata admission and history migration.

`VisualTransform` requires all fields: nullable pixel `crop{x,y,width,height}`, `fit` contain/cover, signed `rotation_millidegrees` from -360000 through360000 clockwise, pivot_x/y_basis_points0..10000 (center5000), flip_horizontal/vertical booleans and opacity_basis_points0..10000 (opaque10000). Default is no crop, contain, zero rotation, centered pivot, no flips and opaque. Identity reset may remove the stored setting. Source crop is checked against verified source dimensions with overflow-safe endpoints. Rendering never interprets user-provided filter expressions, paths or scripts.

Order is source crop, flips, alpha premultiplication, fit into a transparent destination rectangle, destination-centered/pivot rotation, unpremultiplication and final Normal opacity. Rotation clips at that rectangle; it does not enlarge the placement, crop audio, change duration or move other clips. Pivots use fractions of the pixel-center extent (width-1,height-1). Source and target surfaces stay1..3840per dimension. Cover crops the source aspect before enlargement; extreme thin sources cannot request an enormous cover surface. Integer cover center rounding assigns the extra remainder to right/bottom.

The existing default contain path must bypass this helper and retain its previous Lanczos/centering appearance. Explicit transformed settings use bounded bilinear interpolation of premultiplied8bit GBRAP, then straightRGBA composition. This controls transparent RGB fringes but is encoded SDR, with8bit quantization and recorded tolerance; no linear-light/HDR claim. Quarter turns use transpose or flips plus bounded pad/crop. Centered arbitrary rotation uses native rotate. Noncenter pivots use an inverse-affine geq reference restricted to720pixels per destination dimension after measured cost; larger targets fail with a center-pivot/smaller-output remedy. This partial noncenter restriction prevents pretending that a slow unrestricted reference is a realtime compositor.

## Official documentation and alternatives

The maintained [FFmpeg filter documentation](https://www.ffmpeg.org/ffmpeg-filters.html) defines exact source cropping, transpose/flip, scaling, rotate, per-plane geq sampling and premultiply/unpremultiply. Those interfaces are exercised against the externally installed9.0.2 Windows build; the helper does not bundle it. The [official legal page](https://ffmpeg.org/legal.html) distinguishes LGPL and optional GPL builds and separately addresses patents. Distribution review of the selected binary remains required; using a subprocess is not a redistribution waiver.

Native rotate supports a fixed output surface efficiently but has a center pivot. Expanded rotate plus translate can exceed the UHD surface budget. Generated inverse affine sampling supports offcenter pivots without expanded allocations, but its expression interpreter cost is substantial. A GPU affine shader or precomputed remap planes could improve that case, subject to pixel/alpha conformance and total memory/cost measurements. A separate Studio-only shader is not authoritative. No Assembly, unsafe Rust, new editing backend or crop-by-modifying-source shortcut is justified.

## Executed measurements

Generated opaque red source, three frames, two filter threads, output to null, three cold process samples per case. Includes process startup, format conversions and filtering; excludes real-file decode, native presentation and encoded export. Baseline evidence is `.lumeryn-scratch/transform-spike/baseline.json` (ignored local fixture).

| Surface | Zero transform median | Inverse-affine reference median | Native center rotate median |
|---|---:|---:|---:|
|1280x720|43.75ms|2559.93ms|68.32ms|
|3840x2160|192.38ms|25750.41ms|406.23ms|

The native centered route improves this isolated arbitrary-rotation baseline by about37.5x at720p and63.4x atUHD; it still does not establish sustainedUHD60FPS. Runtime migration to GPU remains benchmark-gated. Noncenter sampling is explicitly capped; even smaller surfaces have no realtime promise. A preview can use reduced output resolution, while final output must admit its actual rectangle or return unsupported rather than silently change pixels.

## Executed checks and remaining acceptance

Three focused tests passed with real9.0.2 FFmpeg: invalid/overflow/crop/source/surface/unknown-field admission; decoded asymmetric crop/cover/flip/90degree/opacity; decoded33degree centered and offcenter rotation, fixed clipping, transparent-blue/red alpha edge suppression. A top-left90degree pivot moves an inset shape completely outside its fixed surface.8bit color tolerance is2–3units; alpha is inspected separately. Latest strict focused core library/test Clippy passed; owned files were rustfmt formatted. Full workspace fmt/Clippy/tests, integrated schema/history/CLI/native/export and independent review remain parent gates. No owner source/project mutation or binary publication occurred.

Square-pixel verified sources are the current contract. General SAR/display transforms, arbitrary offcenterUHD, expanded unbounded rotation, HDR, GPU parity, high-DPI physical pointer gestures and sustained playback remain additional acceptance work.
