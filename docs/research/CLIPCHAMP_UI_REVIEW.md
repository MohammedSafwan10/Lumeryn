# Clipchamp reference inventory and native Lumeryn gaps

Reviewed 2026-10-09. Scope: inspect all eighteen owner-supplied screenshots (eleven initial images plus seven additions), research current official Microsoft documentation, and compare the visible interface with the current native Lumeryn source. This is an inventory/planning milestone. No redesign code, media download, account integration or asset acquisition is implemented here. Lumeryn retains its proprietary brand, Rust graph, CLI, shared project service and native egui/eframe/wgpu desktop.

Evidence labels used below:

- **Screenshot** means the label/control is actually visible in an inspected image. It does not establish that the operation was executed successfully.
- **Official** means current Microsoft/Clipchamp documentation describes the behavior. Personal/work versions and releases can differ.
- **Proposed** means a suggested Lumeryn capability or acceptance check, not current functionality.
- **Uncertain** means a cropped label or unlabeled icon cannot be identified confidently. Do not implement an assumed feature from it.

## All eighteen images inspected

Files remain in the owner's local clipboard directory, `C:/Users/Thumbeja/AppData/Local/Temp/`. The identifiers below resolve to `codex-clipboard-<identifier>.png`. They were viewed individually; they have not been copied into the repository or uploaded. Cropped images count as separate evidence, not independent full-screen states.

| ID | Clipboard identifier | Visible evidence and limits |
| --- | --- | --- |
| S01 | c74658ad-9b7e-4d70-9f18-df7637991098 | Empty editor with one imported `Atlas-drive.mp4` card, duration badge, My media drawer and import split button. Left rail: My media, Record & create, Content library, Templates, Text, Transitions. Project menu/cloud-status icon; Undo/Redo; Size menu; two additional unlabeled floating icons; Share and disabled Export. Black preview, inactive playback/time display, illustrated drag/drop timeline empty state. Header includes upgrade, feedback-like icon, settings, help, profile and native window buttons. |
| S02 | 424fd1b6-ae52-4cc5-8d18-24b823302dc9 | Size menu expanded: Wide 16:9 selected, Vertical 9:16, Square 1:1, Classic 4:3, Social 4:5, Cinema 21:9, Portrait 2:3. First three include platform hints. Preview outlined. This proves available choices, not how existing clips are reframed. |
| S03 | dc295786-7d76-47c5-b749-f27062119687 | Dragging a media thumbnail over a highlighted dashed purple timeline drop target with a plus marker. Inactive transport/time display remains visible. Placement/reordering outcome is not shown. |
| S04 | 41cf816e-3649-4213-82fb-5ab6726c5fc8 | Full editor with selected clip: preview bounding rectangle/corner handles and rotation-like handle; floating crop/fit-like icons and ellipsis; fullscreen icon. Playback includes return-to-start, backward/forward five-second symbols, Play and elapsed/total clocks. Timeline has ruler, seeker, selected thumbnail strip, left/right trim handles, embedded-audio icon, Add text and Add audio drop zones, waveform-like audio area, horizontal scroll and bottom-right zoom/fit/collapse icons. Right Audio drawer shows selected-item count, Volume 100%, slider and Detach audio. Right rail: Captions, Audio, Fade, Filters, Effects, Adjust colours, **Upmarket**, Speed. Sparkles and scissors appear near the timeline. The game HUD's FPS/speed numbers are footage content, not editor telemetry. |
| S05 | 7e7be2bf-e9d7-4d74-a830-9d4beaf8fb4e | Hover explanation explicitly reads **Full screen preview**. The selected preview corner and right-side layout remain visible. |
| S06 | b6763096-c7d1-478e-9a2d-ab0d1f5d335a | Clip context menu: Duplicate Ctrl+D; Copy Ctrl+C; Paste Ctrl+V; Delete Del; Split S; Freeze frame F; Edit duration; Rename; Audio submenu; AI suggestions heading; Auto cut; More options. No operation result or More options contents shown. |
| S07 | 5c783462-5784-4271-93af-08c540bf59b8 | Audio context submenu expanded: Mute and Detach. Freeze frame, Edit duration, Rename, Auto cut and More options remain partially visible. These are clip edits, not necessarily monitor-only controls. |
| S08 | 87ceab55-025f-40bb-8ef8-5099c6fd146e | Filters drawer, search field, selected-item count, collapse control, scrollable two-column preview grid and Intensity slider. Readable names: None, Retro, Orange and teal (selected), Bold and blue, Golden hour, Vibrant vlogger, Purple undertone, Winter sunset, 35mm, Contrast, Autumn, Winter, Old Western, Warm coastline. Some cards have diamond badges; lower partial cards are unreadable. Other right-rail tabs remain visible. |
| S09 | fd6291d8-d7c0-42eb-9260-650d73c13b08 | Speed drawer with selected-item count, numeric value 1, slider and marks 0.1x, 1x, 2x, 4x, 16x. Share/Export and right-rail context are visible. An OS Snipping Tool notification and inset capture overlap the editor; they are not product features. |
| S10 | 4aa528e8-3a06-46d8-8d07-d4db861e6fa0 | Transitions left drawer with search, collapse control, Add transitions heading, dismissible drag-between-two-clips instruction, category Fades & blurs and category Wipes. Readable choices: Cross fade, Cross blur, Burn, Tiles; labels `Fade through b…`, `Fade through …`, `Horizontal ban…` are truncated. Wipes: Hard wipe down/up/left/right, Soft wipe down/up. More tiles are cut off. Left rail, timeline scissors/sparkles, selected strip and text/audio targets remain visible. Record & create is only a hover label, not an opened recording panel. |
| S11 | 0ae53785-5b31-4032-910b-68bc06681f24 | Export modal with close button, Quality choices 4K (diamond and source-media hint), 1080p selected, 720p, 480p, Audio-only; filename field and `.mp4` suffix; Store in the cloud switch off; Export action. No output, codec, bitrate, frame-rate, progress or cancellation UI is shown. |
| S12 | 0a7be446-b781-48ec-a860-ba5c6a5c92f4 | Adjust colours drawer with selected-item count/collapse control: Exposure, Contrast, Saturation, Temperature and Transparency sliders; Blend mode dropdown currently Normal; Reset action. Temperature/saturation ramps are visible. Other blend modes, numeric ranges and output behavior are not shown. |
| S13 | c7e7b55d-7c3a-4f1c-ab59-37cb5a54748a | Opened **Upmarket video** drawer for `Atlas-drive.mp4`: Quality dropdown currently 4K, Upmarket action, selected-item count and collapse control. No processing result appears. |
| S14 | 7744a2da-690f-41fc-a883-8ea004e25ea0 | Same Quality dropdown expanded; 4K selected with description **Highest resolution, slowest upscaling**. This establishes an upscaling function despite the unusual displayed Upmarket label. No other qualities or hardware/backend details visible. |
| S15 | 095e663c-f736-4a1f-8c7c-6e3a852285ae | Opened Fade drawer with separate Fade in/Fade out sliders and numeric seconds fields, both 0s; selected-item count/collapse control. Audio hover tooltip overlays the right rail. Curve shape, limits and affected stream are not established by this screenshot alone. |
| S16 | c09b7311-def6-4946-9a7a-3d036b3b3e30 | Text drawer: Plain text card; Text styles heading/See all with Creator, Text box, Pride, Button, Bubble and Retro cards; Title section with typewriter-like preview, Circular, Groovy, Fireworks, Fade, Smoke, Clean Title and Tidal cards. Scrollbar/collapse control and active Text rail marker. These are static card captures; their animation behavior is documented separately. |
| S17 | 47a47cbb-fc70-4e91-a92c-59b48c4b61bc | Opened Record & create drawer: Record cards Screen and camera, Camera, Screen, Audio; Create card Text to speech. No device chooser, permissions, recording session, voice/language settings or generated audio shown. |
| S18 | 19ba2334-5c7d-4631-a137-5d4612c8ed13 | My media asset context menu with Rename/Delete, selected/use-like check badge on the media card, duration and import split button. This is a media-library menu, distinct from S06's timeline-clip menu. No actual source deletion is demonstrated. |

The visible **Upmarket** string is retained verbatim. Its meaning was uncertain from S04/S08 alone; S13/S14 now show the opened panel and explicitly describe upscaling, resolving the function. Microsoft calls the documented category **Upscale**. Exact backend and implementation equivalence remain unverified. Unlabeled floating icons, the sparkles action, More options contents, additional blend modes and truncated transition names remain uncertain.

## Current official documentation cross-check

The editor guide describes the library/toolbar, stage floating toolbar, property panel and timeline as separate regions. It confirms import, timeline placement, clip editing and contextual properties. These regions explain the reference layout without implying that unopened tabs expose every feature in a given installation. [Microsoft editor guide](https://support.microsoft.com/en-us/clipchamp/how-to-edit-a-video-in-clipchamp).

Official keyboard documentation covers navigation between editor regions, sidebar/timeline collapse, transport and editing shortcuts. It supports a keyboard-accessible rail/drawer design; it does not establish Lumeryn's accessibility or reproduce its shortcuts automatically. [Editor keyboard shortcuts](https://support.microsoft.com/en-us/clipchamp/keyboard-shortcuts-for-clipchamp).

Microsoft distinguishes actual full-screen playback from enlarging the stage by collapsing editor panels. This is a material gap in Lumeryn, whose current Expand preview only hides side panels within the editor. [Full-screen and panel-collapse guide](https://support.microsoft.com/en-us/clipchamp/how-to-preview-a-video-in-full-screen-mode).

Duplicate retains a clip's edits; copy/paste can target a chosen timeline position. Freeze frame creates a still-image item at the selected moment. These operations require graph semantics and persistence rather than only context-menu buttons. [Copy/paste behavior](https://support.microsoft.com/en-US/Clipchamp/how-to-copy-and-paste-in-clipchamp), [current freeze-frame guide](https://support.microsoft.com/en-us/clipchamp/how-to-create-freeze-frame). Older community answers denying a native freeze-frame feature are not used here.

Transitions connect adjacent clips and expose editable duration; fades are item properties. Speed changes item duration and can affect its embedded audio. These behaviors must be explicit in Lumeryn's evaluator and preview/export contract. [Transitions/fades](https://support.microsoft.com/en-us/clipchamp/how-to-add-transitions-and-fades-to-videos), [speed semantics](https://support.microsoft.com/topic/297bfa9c-4e8e-441f-bcc1-a6af040df376).

The property-panel documentation confirms Audio, Fade, Filters, Effects, Adjust Colors, Speed and captions as editor categories. Parameters behind unopened tabs are documentary extensions, not screenshot observations. [Microsoft accessible editor guide](https://support.microsoft.com/en-us/accessibility/clipchamp/basic-tasks-to-create-a-video-in-clipchamp-using-a-screen-reader).

The current Clipchamp product page describes Auto cut as AI-assisted silence removal; S06 establishes the menu entry only. This does not establish automatic artistic shot selection or an embedded agent requirement. [Official AI feature description](https://clipchamp.com/en/ai-video-editing-clipchamp/?country=us&culture=en-us).

Microsoft documents Windows-only, device-dependent local video super resolution with an original-restoration path. S13/S14 independently identify the displayed Upmarket panel's function as upscaling; no exact backend equivalence or Lumeryn model selection follows. This remains a future optional enhancement category. [Official upscale documentation](https://support.microsoft.com/en-us/clipchamp/how-to-upscale-videos-with-video-super-resolution).

The seven additions expose color/fade controls, plain/styled text/title cards, four recording choices and Text to speech. Official guides describe editable text/title placement and screen/camera capture; the screenshot cards do not demonstrate an actual recording or title animation. Voiceover documentation also lists text-to-speech as a creation route. [Editable text/titles](https://support.microsoft.com/en-gb/clipchamp/how-to-add-text-and-titles-to-a-video), [screen/camera recording](https://support.microsoft.com/en-us/clipchamp/how-to-make-a-screen-recording), [voiceover/TTS routes](https://support.microsoft.com/en-us/clipchamp/how-to-add-voiceovers-to-videos).

Current export documentation includes resolution choices and audio-only export. It also mentions formats/integrations not visible in S11; GIF and named cloud integrations are outside the screenshot inventory. Cloud sharing/storage and subscription badges are reference product choices, not mandatory Lumeryn requirements. [Export-resolution documentation](https://support.microsoft.com/en-us/clipchamp/how-to-choose-the-right-export-resolution).

## Feature-by-feature Lumeryn status and gaps

Status is based on current source and the completed B milestone in PROGRESS, not a fresh execution of all features in this review. **Done** is narrowly scoped to existing behavior; **Partial** means an implemented subset differs from the reference; **Missing** means no matching public editing/presentation capability was found. **Reference-only** and **Uncertain** do not become implementation promises. Proposed build-order stages are defined below.

| Visible feature | Evidence | Current Lumeryn | Gap and proposed stage |
| --- | --- | --- | --- |
| Left icon rail with active marker and expandable resource drawer | S01/S04/S10 | Missing; B uses a permanent Media panel | Native rail/drawer state, active indicator, close/reopen and keyboard focus; stage 1 |
| Right icon rail and contextual property drawer | S04/S08/S09 | Partial; selected Clip inspector and compact window | Rail plus selection-aware drawer; initially expose only actual Clip properties; stage 1 |
| Independent drawer-collapse controls | S01/S04/S08/S10 | Partial; compact inspector toggle and advanced tools | Separate left/right collapse; preserve reachable controls and preview space; stage 1 |
| Project menu/name, local save state and app branding | S01/S04 | Partial; project menu/name and saved state exist | Editable project/clip names need shared rename commands; stage 2. Keep Lumeryn brand |
| Account, subscription upgrade, cloud-status symbol, settings/help/feedback | S01/S04 | Reference-only; local advanced settings/diagnostics exist | No mandatory account/plan/cloud. User help/shortcut discoverability proposed at stage 1; remote services optional future scope |
| Import button and import dropdown | S01/S04 | Partial; native picker, path and file drop implemented | Source-selection dropdown/integrations not implemented; keep local import actual; stage 1 polish |
| Imported thumbnail, filename, duration, selection/use indicator | S01/S04 | Partial; source card, duration and selection implemented | Presentation cache covers first 128 assets, one representative frame; used-in-project badge not implemented; stage 1 polish |
| Drag imported card to timeline with insertion target | S03 | Missing; Add to timeline appends | File drop imports but does not place library cards. Stable-ID payload, insertion ghost and validated commit; stage 2 |
| Empty timeline instructional drop zone | S01/S03 | Partial; empty preview/media guidance exists | Contextual timeline empty/drag states without fake successful placement; stages 1–2 |
| Screen+camera / Camera / Screen / Audio recording | S17 | Missing | Native capture/device permissions, synchronized recordings and immutable generated-asset import; stage 7. Controls are visible, recording not executed |
| Text to speech creation | S17 | Missing | Optional local or explicitly authorized provider, voice/model licensing, generated audio provenance and cue alignment; stage 7 |
| Content library and Templates | S01/S04/S10 | Missing; tabs visible, contents unobserved | Searchable licensed assets and editable native scene templates; stages 3/6 |
| Plain text, styled text, titles and Add text lane target | S04/S10/S16 | Missing | Editable Rust text/scene nodes plus CLI/Studio placement; stage 3, retained explainer priority |
| Creator/Text box/Pride/Button/Bubble/Retro style cards and See all | S16 | Missing; reference names/presentation | Original editable style specifications, fonts/rights and real preview; stage 3 |
| Typewriter/Circular/Groovy/Fireworks/Fade/Smoke/Clean Title/Tidal title cards | S16 | Missing; animation documented, not demonstrated by still captures | Native keyframes/easing and reusable semantic motion titles, same preview/export; stage 3. Catalog names do not authorize copying assets |
| Aspect-ratio/Size presets | S02 | Partial; project creation dimensions, read-only settings, aspect-correct display/export | Change-format command with exact dimensions/reframing semantics; stage 4. Seven screenshot ratios are reference presets |
| Undo/Redo toolbar | S01/S04/S10 | Missing; durable history exists | Validated reversible commands, conflict-aware undo/redo and CLI parity; stage 2 |
| Stage bounding box, corner handles and rotation handle | S04 | Missing | Rust transform properties and preview/export evaluation; stage 4. Handle display alone proves no behavior |
| Floating crop/fit controls and ellipsis | S04 | Partial; baseline letterbox/fit is automatic | User crop/fill/rotate/flip/PIP are proposed from official guide, not all shown labels; stage 4 |
| Play, elapsed/total clock and seeker | S01/S04 | Done within baseline | Preserve exact rational frame identity and current service revision through shell change; stage 1 regression |
| Start and backward/forward-five-second transport | S04 | Partial; start and one-frame buttons/arrows exist | Optional explicit five-second seek commands; do not silently change frame-step keys; stage 1 |
| Full screen preview | S05 | Missing; Expand only rearranges editor upper panes | True native fullscreen viewer, Escape restore and same playback state; stage 1 |
| Timeline ruler, playhead, selection and clip duration | S04 | Done within baseline | Preserve current functional timeline during rail migration; stage 1 regression |
| Left/right clip-edge trim handles | S04 | Partial; right-edge drag and inspector source/duration implemented | Left trim gesture and precise source/position policy; stage 2 |
| Temporal thumbnail strip | S04 | Partial; one source thumbnail repeated across clip | Multi-time thumbnail cache is staged; never describe current tiles as temporal sequence; stage 6 benchmark |
| Audio waveform and Add audio target | S04 | Partial; real linked-source peak lane exists | Independent audio clips/mixing and placement missing; stage 5 |
| Horizontal scroll, zoom buttons, fit and timeline-collapse icon | S04 | Partial; horizontal scroll/zoom slider/Fit exist | Keyboard zoom and accessible collapse/resize; stage 1 |
| Timeline scissors and sparkles actions | S04/S10 | Split missing; sparkles uncertain | Split after stage-2 shared command; do not label sparkles as a known AI action |
| Duplicate / Copy / Paste | S06 | Missing | Typed stable-ID copy/duplicate semantics, position, selection and grouped transaction; stage 2 |
| Delete | S06 | Missing | Shared delete/remove operation and gap/ripple policy; stage 2 |
| Split | S06 | Missing | Source/time bounds, stable child IDs, source preservation, same CLI operation; stage 2 |
| Freeze frame | S06 | Missing | Still/hold node with exact source frame, duration and preview/export parity; stage 3 |
| Edit duration | S06 | Partial; validated clip duration trim exists | Shared still/hold durations follow scene/still support; stages 2–3 |
| Rename | S06 | Missing; clip label derives source filename | Persistent user display names through shared command; stage 2 |
| Media-library Rename / Delete | S18 | Missing | Distinct shared media display-name/remove-reference operations; define in-use references and preserve disk source; stage 2 |
| Context Audio > Mute / Detach | S07 | Missing as edits | Existing mute is monitor-only. Project clip mute and detach/linked audio graph need stage 5 |
| AI suggestions / Auto cut | S06/S07 | Missing | Optional analysis produces reviewable edit proposals, dry-run/atomic commit and external-agent review; stage 7 |
| More options | S06/S07 | Uncertain | Contents not visible; no speculative submenu |
| Captions right tab | S04/S08 | Missing; no opened panel shown | Editable caption cues and optional transcription, preview/export and CLI parity; stage 3/7 |
| Audio property Volume 100% / slider | S04 | Partial only in unrelated monitor path | Persistent clip gain is missing; monitor gain must stay explicitly distinct; stage 5 |
| Detach audio property action | S04 | Missing | Independent linked audio item with integer/rational synchronization; stage 5 |
| Fade in/out slider and numeric seconds | S15 | Missing | Exact time conversion, shared video/audio fade properties and deterministic evaluation; stages 4–5 |
| Filters search/grid/preview/intensity/None | S08 | Missing | Deterministic named presets, original/none state, bounded real previews and strength; stage 4 |
| Fourteen readable filter names and premium diamonds | S08 | Reference-only names/entitlements | Implement original audited presets, not copied brand/premium scheme; stage 4 |
| Effects tab | S04/S08 | Missing; tab only | Specific effects require separate evaluator contracts. No blur/effect claim from the tab alone; stage 4 |
| Exposure / Contrast / Saturation / Temperature / Transparency sliders | S12 | Missing | Typed color/opacity controls, reset/defaults and color-space/parity tests; stage 4 |
| Blend mode Normal dropdown / Reset | S12 | Missing | Compositor blend semantics, safe reset and exact defaults; stage 4. Other modes are not visible |
| Upmarket video / Quality 4K / upscaling action | S13/S14 | Missing; upscaling function now confirmed | Optional enhanced derived asset, immutable original/restore, quality/runtime discovery and resource budget; stage 7 |
| Speed value/slider 0.1x–16x | S09 | Missing | Rational retime plus audio policy, duration rounding and preview/export parity; stage 5 |
| Transitions search/category/tiles/instruction/drag target | S10 | Missing | Core transition/handle-duration model before drop interaction; stage 4 |
| Cross fade / Cross blur / Burn / Tiles / wipes | S10 | Missing; visible presets | Start with deterministic crossfade; other originals follow measured rendering; stage 4 |
| Truncated fade/band labels and lower tiles | S10 | Uncertain | Preserve cropped evidence; no assumed exact catalog |
| Share dropdown | S01/S04/S09 | Reference-only | Local export remains first. Upload requires explicit owner authorization and future integration |
| Export button and modal, filename/new-path selection | S11 | Partial; native destination and asynchronous service export implemented | Preserve no-overwrite, cancellation and decoded validation; stage 1 regression |
| Export 480p/720p/1080p/4K quality choices | S11 | Partial; current output uses project dimensions, not export presets | Explicit export configuration and resource estimates; stage 5. Preview quality is not export quality |
| Audio-only output | S11 | Missing | Independent audio export preset, format/license review and decoded verification; stage 5 |
| Cloud-store toggle and 4K premium badge | S11 | Reference-only | Optional future storage integration; do not impose account, subscription or cloud on local Lumeryn |

No timeline move/reorder, multi-selection, group, gap removal or transition duration editor is visibly opened in these eighteen screenshots. Official documentation describes related operations, so they are reasonable **proposed** traditional-editing additions, not screenshot-proven or implemented features. A specific blur effect, recording-session controls, automatic caption language settings and content-library asset categories likewise cannot be inferred from the remaining unopened tabs or static cards.

## Native shell and interaction contract proposed for the next slice

Use Lumeryn's own visual language with fixed narrow left/right icon rails and independently expandable drawers. Left means resource/workflow selection; right means properties of the current project selection. Clicking an active rail item toggles its drawer; selecting another implemented item opens that content. Selection changes refresh context without changing the graph. Rails, drawer width/open state and focus are presentation state, never edit history. Do not expose empty working feature tabs: initial rails can contain actual Media, Clip and Project tools, while staged categories belong in an explicitly labelled roadmap or remain absent.

Keep preview and transport between drawers, with the timeline full width below. At compact widths permit one main drawer at a time or use a reachable overlay with scrolling and close/Escape behavior; preserve selected clip/playhead. At wide widths both drawers may coexist. Use actual available logical bounds instead of making every pane permanently visible. Propose validation at 1440×900, 1280×800, 980×640 and 100%/150%/200% DPI, with minimum touch targets, readable tooltips, focus rings and no hidden Apply/Export actions. Main B-style composition can migrate to this shell without replacing its underlying controls.

Define Tab traversal, rail roving focus, Enter activation, drawer close/reopen, region navigation and keyboard help. Preserve Space playback and frame arrows outside text fields. Text entry and dialogs consume shortcuts; Escape dismisses the current popup/fullscreen/drawer before clearing selection. A custom painter and egui hover text alone do not establish screen-reader accessibility; test keyboard/focus and native accessibility separately. Clipchamp's keyboard lists inform discoverability, not a requirement to duplicate every key.

Distinguish three drag operations: OS files import, library media inserts a clip, and timeline clips reposition. The existing app implements the first and a button for append, not all three. Proposed drag feedback carries stable IDs and previews a valid placement without mutating; release submits the same revision-checked Rust command as CLI. Cancel, stale revision or invalid placement leaves the graph unchanged. Future transition drops require valid neighbors and sufficient source handles before commit. No widget may change clip position/track directly.

Implement true native fullscreen playback separately from Expand preview: reuse the same preview session/device/frame, suppress unrelated editor chrome, preserve pause/seek/monitor state, support Escape and restore previous window geometry/focus. Do not claim that hiding side panels is fullscreen or create a second decoder/audio clock just for a viewer.

All new edits must follow Rust types → evaluator/validation/storage → shared service request → CLI schema/capability/operation → Studio control. Commands need stable IDs, exact frame/rational semantics, expected revisions, dry-run/atomic batches where applicable, explicit errors and event-driven resync. Preview and export must evaluate the same semantics. UI-only rail state need not be a CLI editing command; the actual resource operations behind it must remain discoverable to external agents.

## Suggested build order and acceptance

| Stage | Work and dependencies | Concrete acceptance before claiming completion |
| --- | --- | --- |
| 0 — this inventory | Eighteen screenshot observations, official cross-check, current-source gap list | Owner can review complete visible inventory; ambiguous labels and documentary extensions separated; no redesign/downloads in this milestone |
| 1 — native shell parity | Rails/drawers around existing Media/Clip/Project tools; keyboard/focus; real fullscreen; existing import/trim/transport/export | Actual native wide/compact/DPI captures, drawer/focus tests, fullscreen/Escape restore; existing CLI-live-edit/trim/source-integrity/decoded-export workflow still passes |
| 2 — basic editing semantics | Split/delete/duplicate/copy-paste/rename/move, left trim, undo/redo and explicit gap/ripple policy; core/service/CLI first | Exact integer-frame tests, transaction rollback/revision conflict/idempotency/history tests; CLI and Studio produce same snapshots; preview/export handle supported layout rather than silently dropping gaps |
| 3 — editable explainer foundation | Native text/stills/vectors/scenes, hold/freeze frame, duration, reusable native templates and editable caption cues; retain approved explainer roadmap | Same Rust graph, stable semantic node IDs; multilingual/font/license checks; preview/export parity and editable Demo A slice. Do not defer this indefinitely behind editor breadth |
| 4 — visual properties and transitions | Format/aspect change, transforms/crop/fill, original filters/color/fades and initial crossfade; shared compositor | Explicit color/time/transition-handle rules; real before/after previews, output-frame checks and app CPU/GPU measurements. No controls before evaluator/CLI capability exists |
| 5 — audio/retime/export configuration | Independent audio, project gain/mute/detach/fades/mixing, rational speed, export resolution and audio-only | Monitor/project gain distinct; source unchanged; audio clocks/latency/resample limits measured; retimed frame/duration/synchronization and decoded export validated |
| 6 — reusable content and cache | Licensed asset manifests/search/preview/import, editable templates, then benchmark-gated temporal thumbnails/proxies/persistent cache | Rights/provenance available before install/use; checksum verification and local-offline paths; bounded memory/storage jobs; no unlicensed redistributed media |
| 7 — optional recording/AI/services | Recording, optional transcription/silence-cut/upscale; cloud/share only if explicitly selected | Runtime/capability/license discovery; immutable originals; cancellable resource-budgeted jobs; reviewable proposals and explicit consent for uploads/charges. No mandatory models/account/embedded agent |

This is a dependency order, not a delivery-date promise. Independent professional editing and explainer work can proceed in parallel after the authoritative command foundation; advanced optional integrations must not block a useful local install.

## Commercially usable assets/audio: direction and boundary

Content library/Templates are visible categories; their full asset catalog and terms are not visible. Microsoft permits covered stock in videos exported from Clipchamp but prohibits redistributing the underlying stock files or using them in other editing programs. Therefore Clipchamp/Storyblocks assets shown or available in that product are not a source pack for Lumeryn. [Microsoft stock-audio terms](https://support.microsoft.com/en-us/clipchamp/how-to-add-stock-music-and-audio-to-videos). Commercially licensed music can still receive platform Content ID claims; store supporting proof rather than advertising guaranteed monetization or zero claims. [Microsoft rights/claim guidance](https://support.microsoft.com/en-us/clipchamp/why-does-my-clipchamp-video-get-a-copyright-warning-on-youtube-or-other-sites).

Proposed Lumeryn asset records should retain provider/source URL, author, exact license/version/text, checksum, permitted commercial output use, modification rights, distribution/embedding rights, attribution, font/model dependencies, release restrictions and any music claim-support evidence. Distinguish owner-created packs, separately licensed redistributable packs and user-imported assets; permission to use a rendered video is not permission to bundle the source asset in an editor. Unverified rights remain unknown and cannot receive a commercial-ready badge. Editable templates should be Lumeryn semantic project/scene specifications with original or appropriately licensed constituent assets, not flattened Clipchamp screens or copied proprietary presets.

Future candidate libraries/providers and release evidence need their own reviewed acquisition register. Nothing is downloaded, licensed, paid for, redistributed or promised claim-free by this inventory. Lumeryn source stays proprietary regardless of a constituent asset's permissive terms; obligations apply to each component and distribution path.

## Source inspection and review limits

Inspected current `apps/lumeryn-studio/src/app.rs`, `design.rs`, `timeline.rs`, `assets.rs`, `crates/lumeryn-core/src/model.rs`, `render.rs`, `crates/lumeryn-cli/src/main.rs`, ARCHITECTURE, PRODUCT_SPEC, TASKS and PROGRESS. The graph's current edits are Import/AddClip/TrimClip. CLI capabilities report one contiguous video track, 1–64 clips, matching CFR, embedded audio/silence and baseline MPEG4/AAC export. Existing B evidence records real native three-size workflows, but physical dialog focus/cancellation, non-default DPI and speaker/display drift remain manual limits. This review runs no new app behavior or tests and does not certify Clipchamp functionality, future Lumeryn rails or commercial asset rights.

Canonical delivery sequence/task IDs: [CC-00–14](../roadmap/CLIPCHAMP_FEATURE_INVENTORY.md). The stage recommendations in this specialist report support that consolidated plan and do not override its composition/audio dependencies or early native-scene priority.
