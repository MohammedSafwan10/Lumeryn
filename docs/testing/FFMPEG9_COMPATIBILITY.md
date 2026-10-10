# FFmpeg9.0.2 local compatibility

Executed2026-10-10 following the owner's explicit PC-update request. Installation/provenance/checksum/rollback and inherited-environment behaviour: [tool update](../research/FFMPEG_TOOL_UPDATE.md). Official latest stable was checked at [FFmpeg downloads](https://ffmpeg.org/download.html); Windows provider is linked there.

Root passed40 tests in9 existing compiled suites with explicit `LUMERYN_FFMPEG`/`LUMERYN_FFPROBE` paths to the new9.0.2 pair. Tests ran serially with `--include-ignored --test-threads=1`; source/binaries were unchanged and the owner's running Studio/project was not restarted or edited. No claims are made about every installed filter/device or newly planned features.

| Suite | Passed |
|---|---:|
| CLI workflow |2|
| Media regressions |4|
| UHD export |4|
| Independent audio composition |3|
| Import stages |4|
| FPS workflow |3|
| Preview library |12|
| Generated preview integration |6|
| Audio audition integration |2|
| Total |40|

UHD fixtures cover landscape/portrait output rates, actual source detail, audio/pixel/source identity and encoder/fallback behaviour. Generated preview integration exercises real decode, rational seek/EOS and audio lifecycle; unit clock/device checks are not physical speaker/display sync measurements. Reports and exact test-binary/tool hashes are in [evidence](ffmpeg9-compatibility-evidence.json). Logs remain ignored under `.lumeryn-scratch/ffmpeg9-regression`.

The external composition script also passed with9.0.2:1280x720/30FPS/60frames/2seconds/full decode and frame45 final black-gap start. Root inspected actual frames24/50. Its baseline/composition medians were99.637/141.748ms for3runs, including process/decode/filter-to-null cost. This is not a controlled version-speedup claim. Version-specific output hash `21C5E5038B7DCA2A7ED71B0CEBD53640B8D3497F75C26155BEBE567BD97EB6EF`; generated source/PNG hashes and graph are in the recorded scratch report. See [spike limitations](COMPOSITION_SPIKE.md).

No Rust/dependency/schema code changed, so no new build/fmt/Clippy/default/media/deny gate was represented as run. The selected existing binaries match the previously verified implementation milestone; hashes identify exactly what executed. No new native-window/high-DPI/manual/device-loss capture was made. Existing apps retain inherited tool paths until refreshed/relaunched; explicit CLI/Studio tool flags select the new pair immediately for a new session. Both previous8.1.1 executables remain intact.
