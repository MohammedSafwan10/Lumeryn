# Independent audio acceptance

Date: 2026-10-10. Verified schema-2 independent audio vertical slice on top of local milestone e9f5b3d. This is not complete professional-editor or physical-device certification.

## Contract and focused verification

- `audio_graph` / `audio_migration`: strict add/move/trim/range/gain/mute/delete, rational source bounds, immutable visual ranges, overlap, dry-run/rollback/replay/stale revision/undo and prepared-proof binding. Genuine schema-1 migration updates every historical snapshot with one stable track identity; command/receipt bytes and editing revision remain unchanged. Malformed late history and mixed track/schema failures roll back. Source proof is not invented by migration.
- `audio_probe`: real 44.1 kHz mono and 48 kHz stereo sources produce 2304000 canonical sample frames (18.432 MiB interleaved float output), exceeding the old whole-buffer cap while streaming bounded memory. Wrong digest, missing tool, invalid/delayed/multichannel source, timeout and changed-source cases fail explicitly. Video-only import does not require eager full audio decoding.
- `audio_plan` / `audio_composition`: actual lossless PCM covers placement, source offset, overlap, gain/mute, stable contribution dropout, positive/negative clamp, silence padding, source integrity and rational scheduling. Sequential/seek slices match exactly; bounded preview PCM at a different device format matches the same plan. Actual AAC timing/amplitude windows and UHD landscape/portrait exports fully decode. The small composition fixtures are upscaled; native UHD-detail coverage remains in `uhd_export`.
- `audio_workflow`: actual CLI/service schema/capability discovery, legacy backup/migration/live same-revision event, audio add/move/trim/range/properties/delete/undo, idempotency/conflicts/remove-reference protection, negative gain arguments, live snapshots, source preservation, muted/audible decoded MP4 windows.
- Native timeline tests use real egui RawInput press/move/release passes; audio placement, grabbed-offset move, right-edge trim, bounds, secondary input and cancellation identities require one release action or zero on cancellation. These are synthetic pointer events, not physical mouse/high-DPI acceptance.

No dependency or binary is bundled by this change. Generated test sources, databases, exports and raw logs stay in ignored scratch. Both clients use the service and Rust graph; sources remain external immutable files.

## Executed integrated gate

`./scripts/check.ps1 -Media -Deny` passed on final source: 136 default tests plus 38 additional media tests = 174 distinct successful Rust tests, formatting, strict locked all-target Clippy, and dependency/advisory/license checks with existing reviewed exceptions. Current locked workspace build passed. Logs: `.lumeryn-scratch/audio-handoff-{gate,build}.log`.

`./scripts/native-smoke.ps1 -SkipBuild -History -Audio -Uhd` passed all 34 actual-window checks on newly generated disposable footage. Eight independent-audio transitions verified import/place/trim/move/atomic range and gain/mute/unmute/delete/undo, live committed events and refreshed preview, with unchanged visual clips. Mixed device playback reported stereo48k and zero underruns during this short run. Final revision29 output fully decoded: H.264 NVENC,3840×2160,60/1FPS,120frames,2seconds,48k stereoAAC. The 880Hz independent contribution measured0.000002433 early and0.044140628 late; embedded440Hz remained0.08849/0.08821. AAC padding produced96256 decoded sample frames. Marker windows establish digital placement/amplitude, not physical speaker/display synchronization.

Report directory: `.lumeryn-scratch/studio-699e0675cad74163b845cdca729de147`. Its screenshot was visually inspected. Debug CPU update p50/p95=1.7759/4.6765ms across555updates; these are not display FPS, input latency or a scale benchmark. This720p synthetic source is upscaled; original UHD-detail tests passed separately in the media gate.

A consistent new copy of the owner's neural demonstration was explicitly migrated and given three independent clips from the already audited CC0 `oga.hernandack.short-loops.v1.winter-dust` source. Actual44.1kHz stereoOGG decoding proved1101882 canonical48k sample frames. Its revision12,1497-frame49.9-second export fully decoded at1920×1080,H.264NVENC,30/1FPS (upscaled720p source;9137ms CLI elapsed). All10 source hashes and project/history integrity passed afterward. Original project remained schema1 revision10; it was not migrated or edited.

Final readonly native captures at1440×900(audio),980×640(audio) and1280×800(video) were visually inspected at100%DPI: usable compact drawer, full-width volume slider, true audio card/timeline waveforms, plain clocks and the imported PNG thumbnail. Each retained revision12,9 thumbnails,9 waveforms,zero failed assets. Raw captures: `.lumeryn-scratch/audio-final-captures-54d07f5cd39047acbd758a5a6de5fc88`. Selected unmodified screenshots are retained in `docs/design/native-audio-v1`; report/binary/source hashes and scope are in `audio-composition-evidence.json`.

The integrated review found and corrected old tests assuming schema1 was current, a PowerShell case-insensitive Audio switch/probe variable collision, a cramped slider and static PNGs with unset sample aspect. The thumbnail exception checks actual PNG signature, one-frame zero-origin PNG metadata and source identity; video non-square/HDR rejection remains intact. Final real PNG worker tests cover source preservation/replacement rejection. These fixes preceded the successful final gate/build/native/captures; earlier failed attempts are not accepted evidence.

## Clean project and CLI handoff

After the owner requested removing demonstrations before agent testing, normal Studio was checked for automatic demo/mock insertion: none exists. Created the separate `projects/agent-test/project.lumeryn` at schema2 revision0 with zero media/video/audio placements. Fixed preview loading so a validated empty graph clears to idle, cancels previous sessions/builds, resets counters/audio/errors and rejects old-generation frames; invalid empty graphs still fail. One tool/device-free test and one real populated→empty→reload/cancel test passed in the final174-test gate.

The actual clean native screenshot was visually inspected at1440×900: no demo imagery/clips/waveforms/error banner; Export disabled, normal import/drop targets visible. Capture `.lumeryn-scratch/agent-clean-capture-cabd86060cd14ab6aae213973a86a29e` retained revision0, no presented frame, zero assets/failures and no session error. Its unmodified PNG is retained as `docs/design/native-audio-v1/empty.png`. Historical mockups/test fixtures remain outside this normal project. The entire local projects directory is ignored to avoid committing user media.

Locked local CLI installation passed; installed capabilities, strict graph/render schema and all352 catalogue hashes/notices passed. [Agent handoff](../cli/AGENT_QUICKSTART.md) identifies the clean project, safe revision workflow, supported operations and staged features. The final visible launch uses this clean project, not the audio review copy.

## Remaining limits

One independent painted audio lane permits overlap; visual duration bounds it. Combined embedded/independent contribution cap is 64 and is not a benchmarked capacity. Unproven Studio sources require reimport; CLI can lazily acquire the proof. Embedded audio remains at unity and linked to video, so this slice does not implement detach or embedded mute/gain. Audio split/duplicate/clipboard, fades, additional topology, audio-only export, professional metering and long-program optimization remain staged.

Physical speaker/display A/V offset/drift, device changes, dialogs, actual mouse/touch/high-DPI usability and subjective musical quality remain separately unexecuted unless explicitly recorded. Native command-path automation, output marker windows, device status and CPU timings do not prove those checks.
