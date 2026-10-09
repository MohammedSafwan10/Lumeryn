# Private CI and staged fuzzing

No remote or hosted CI job has been created; no source is published. A future owner-approved private runner should execute the exact local gate in scripts/check.ps1 on Windows with trusted pinned FFmpeg binaries/generated fixtures and the locked toolchain. Separately retain Cargo doctests if nextest becomes the optional main runner. Linux/macOS support is architectural and has not been verified by this Windows milestone.

Add a Linux nightly ASan/libFuzzer worker when fuzz targets exist for rate/scene/schema/command parsing, migration handling and media metadata boundaries. Pin toolchain/dependencies, use bounded runtime and checked nonconfidential corpora, retain minimized reproducible crashes privately, and convert failures to stable regressions. Do not treat a workflow file as evidence that fuzzing executed. Current cargo-fuzz documentation also describes native Windows prerequisites; Linux remains the simpler initial CI choice. See TEST_TOOLING.md and ADR0007 for official references, license/MSRV observations and alternatives.

Integration/golden acceptance must record decoded content, timing/sample tolerances, actual hardware and tool identities. Fixed fonts/vector fixtures and ten-minute device AV drift await their subsystems. Installer/update signature/rollback/clean-uninstall tests belong to the future distribution gate; no paid signing service is provisioned.

