---
name: optimize-proton
description: >
  Optimize the Omarchy Proton (OmaProton) bleeding-edge compatibility layer for the host
  machine's kernel (linux-omarchy BORE/ADIOS/NTSYNC), CPU microarchitecture and hybrid
  core topology (Intel P/E/LP-E cores, AMD 3D V-Cache CCDs, x86-64-v3/v4), and GPU stack
  (NVIDIA RTX PRIME/NVAPI/DLSS, AMD RADV GPL/NGGC, Intel Arc ANV GPL). Works with any
  default coding agent configured via `omarchy default agent`.
---

# Optimizing Proton on Omarchy Linux (`optimize-proton`)

Work from the actual silicon and kernel running on this machine. Never guess hardware
capabilities when `omaprot-engine --json` gives you the exact probe.

## 1. Inspect the Machine Telemetry

Run the native probe to read the current kernel, scheduler, CPU topology, GPUs, and active profile:

```bash
omaprot-engine --json
```

Pay close attention to:
- **Kernel & Synchronization (`system.kernel_release`, `system.ntsync_dev_active`, `system.ntsync_module_present`, `system.bore_scheduler_active`):**
  - When `/dev/ntsync` or `ntsync.ko.zst` is present on Linux 6.14+/7.x (`linux-omarchy`), prefer `PROTON_USE_NTSYNC=1`.
  - When BORE scheduler is active, keep `WINE_RT_PRIO=88` (or `92` for competitive low-latency titles) and `OMAPROTON_BORE_BOOST=1`.
- **Hybrid CPU Core Topology (`system.cpu_topology`):**
  - On Intel Core Ultra / Alder-Meteor-Raptor Lake processors with 3-tier cores (`P-Core`, `E-Core`, and low-frequency `LP-E` island cores), always set `WINE_CPU_TOPOLOGY` to exclude `lpe_core_cpus` (or pin strictly to `p_core_cpus` for CPU-bound esports titles) so game render threads never stall on 2.5 GHz island cores.
  - On AMD Ryzen Dual-CCD X3D processors, pin `WINE_CPU_TOPOLOGY` to the 3D V-Cache CCD (`0..7` / `0..15`).
- **GPU & Multi-GPU Hybrid Routing (`system.gpus`, `system.is_hybrid_gpu`, `system.on_ac_power`):**
  - **Hybrid Laptop (Intel/AMD iGPU + NVIDIA RTX dGPU):** On AC power, enforce `__NV_PRIME_RENDER_OFFLOAD=1`, `__VK_LAYER_NV_optimus=NVIDIA_only`, `__GLX_VENDOR_LIBRARY_NAME=nvidia`, `PROTON_ENABLE_NVAPI=1`, `DXVK_ENABLE_NVAPI=1`, and `VKD3D_CONFIG=dxr11,dxr,pipeline_library_app_cache`.
  - **AMD Radeon (`RADV`):** Enforce `RADV_PERFTEST=gpl,nggc` and `AMD_VULKAN_ICD=RADV`.
  - **Intel Arc (`ANV`):** Enforce `ANV_GPL=true`.

## 2. Persist Agent-Computed Overrides Atomically

Once you determine the optimal environment variables for the user's hardware (or specific game), apply them through `omaprot-engine` so they are written atomically (`0600`/`0700` HANCORE permissions) into `~/.config/omaprot/ai-overrides.json` and synchronized into `~/.local/share/Steam/compatibilitytools.d/Proton-Omarchy-BleedingEdge/user_settings.py`:

```bash
omaprot-engine --apply-ai-env "<concise technical rationale>" \
  PROTON_USE_NTSYNC=1 \
  WINE_CPU_TOPOLOGY=20:0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19 \
  __NV_PRIME_RENDER_OFFLOAD=1 \
  PROTON_ENABLE_NVAPI=1
```

## 3. Source Compilation Recipe (Optional Bleeding-Edge Build)

If the user asks to compile Proton/Wine from source with silicon-matched `-march` (`alderlake`, `znver4`, `x86-64-v3`, `x86-64-v4`), `ThinLTO`, and `-mno-tls-direct-seg-refs`, generate the build recipe:

```bash
omaprot-engine --generate-pkgbuild
```
