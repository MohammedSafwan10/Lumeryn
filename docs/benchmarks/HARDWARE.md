# Hardware codec initialization baseline

Measured 2026-10-09 on Windows x64, Intel i7-12700F, 20 logical cores, OS-reported 34,188,763,136 bytes RAM. OS inventory identifies NVIDIA GeForce RTX3050, driver32.0.16.1088. VRAM and GPU utilization remain unmeasured; do not infer the advertised model's memory size.

Run `./scripts/benchmark-hardware.ps1`. Three repetitions use generated one-second1280x720/30fps testsrc2, two CPU threads and a4Mbit/s target. The installed FFmpeg8.1.1 GPLv3+ build is user-provided and used only on generated trusted fixtures; it is not approved for redistribution. It predates current maintenance/security fixes. No downloads or driver changes were made.

| Backend experiment | Wall times ms | Actual result |
|---|---|---|
| CPU MPEG4 encode | 73.43 /76.27 /71.34 | All output30frames; fully CPU-decoded successfully |
| NVIDIA H264 NVENC encode | 332.73 /244.45 /219.51 | Device/codec initialization succeeded; all output30frames and fully CPU-decoded |
| Intel H264 QSV encode | 90.27 | Failed MFX session initialization(-9); no successful output |
| AMD H264 AMF encode | 32.81 | Failed loading amfrt64.dll; no successful output |
| CPU H264 decode to null | 82.29 /81.08 /79.21 | Successful decoding of NVENC-generated source |
| CUDA decode plus CPU download | 171.50 /174.25 /168.36 | Successful NVDEC surface readback to NV12/null |
| CUDA-resident decode→NVENC→null | 229.11 /233.19 /228.23 | Successful hardware decode/encode pipeline initialization |

These are backend initialization experiments, not Lumeryn export/preview implementation claims. MPEG4/H264 differ in codec and quality; their times are not equal-quality speed comparisons. The tiny workload is dominated by process/device startup and cannot predict sustained1080p/4K throughput. CUDA downloads are deliberately measured separately from resident transcoding. No wgpu/CUDA texture interoperability, hardware zero-copy preview, color fidelity or sustained synchronization has been proved.

The observed small-workload cost supports retaining the existing narrow CPU fallback while designing per-codec/device benchmark selection. Next compare identical codecs at measured objective quality and representative duration/resolution, repeated cold/warm setup, seek/GOP behavior and source formats, plus peak memory and failure recovery. Integration must verify source-in/frame count, timestamps, audio and fully decoded output before capability advertisement.

Artifacts: `.lumeryn-scratch/hardware-1b637a4f3df64cd19af3b071985bcabd/hardware.json` plus generated MP4s/logs; ignored and not committed. RAM is sampled every25ms and can miss peaks; VRAM is unknown. Error logs are bounded and generated inputs contain no private media.

Research: NVIDIA documents CUDA-resident FFmpeg workflows and codec/device prerequisites, while FFmpeg's hardware flags/encoder list establish compiled availability only. SDK build examples enabling nonfree components are not a redistribution recommendation. [NVIDIA official integration guide](https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/ffmpeg-with-nvidia-gpu/index.html), [FFmpeg options](https://ffmpeg.org/ffmpeg.html).
