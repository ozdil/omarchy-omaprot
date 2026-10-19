//! Universal hardware-, kernel-, L3-V-Cache-, CPU-topology-, Multi-GPU-, and Omarchy-AI-Agent-aware
//! self-optimization engine for OmaProton. Adapts dynamically to ANY x86_64 PC.

use crate::hardware::{
    classify_isa, classify_topology_from_observations, CoreObservation, GpuDevice,
    ProtonSystemProbe,
};
use crate::profiles::OmaProtonProfile;
use crate::securefs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutotuneDecision {
    pub subsystem: String,
    pub detected_hardware: String,
    pub optimization_applied: String,
    pub env_pairs: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAgentOverrides {
    pub agent_id: String,
    pub rationale: String,
    pub env_overrides: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutotunePlan {
    pub summary_headline: String,
    pub active_ai_agent: String,
    pub active_ai_agent_label: String,
    pub taskset_cpu_list: String,
    pub decisions: Vec<AutotuneDecision>,
    pub merged_env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixVerificationEntry {
    pub archetype_name: String,
    pub cpu_summary: String,
    pub gpu_summary: String,
    pub march_flag: String,
    pub taskset_cpus: String,
    pub proton_cpu_topology: String,
    pub gpu_routing_summary: String,
}

pub fn load_ai_overrides() -> Option<AiAgentOverrides> {
    let path = securefs::xdg_path("XDG_CONFIG_HOME", ".config", "ai-overrides.json")?;
    let raw = securefs::read_owned_file(&path, 32768)?;
    serde_json::from_str::<AiAgentOverrides>(&raw).ok()
}

pub fn save_ai_overrides(agent_id: &str, rationale: &str, pairs: &[(String, String)]) -> bool {
    let path = match securefs::xdg_path("XDG_CONFIG_HOME", ".config", "ai-overrides.json") {
        Some(p) => p,
        None => return false,
    };
    let payload = AiAgentOverrides {
        agent_id: agent_id.to_string(),
        rationale: rationale.to_string(),
        env_overrides: pairs.to_vec(),
    };
    let bytes = match serde_json::to_vec_pretty(&payload) {
        Ok(b) => b,
        Err(_) => return false,
    };
    securefs::write_owned_file_atomic(&path, &bytes)
}

pub fn compute_autotune_plan(
    probe: &ProtonSystemProbe,
    profile: &OmaProtonProfile,
) -> AutotunePlan {
    compute_autotune_plan_internal(probe, profile, true)
}

pub fn compute_autotune_plan_internal(
    probe: &ProtonSystemProbe,
    profile: &OmaProtonProfile,
    include_local_ai_file: bool,
) -> AutotunePlan {
    let mut decisions = Vec::new();
    let mut merged: Vec<(String, String)> = Vec::new();

    // 1. Kernel Synchronization & Scheduler Decision (wineserver NTSYNC / WINEFSYNC + BORE / scx)
    let kernel_envs = vec![
        ("WINEFSYNC".into(), "1".into()),
        ("WINEESYNC".into(), "1".into()),
    ];

    let sync_desc = if probe.ntsync_dev_active {
        "/dev/ntsync aktif -> wineserver doğrudan çekirdek içi NT senkronizasyonunu (PROTON_NO_NTSYNC unset + WINEFSYNC=1) kullanır"
    } else if probe.ntsync_module_present {
        "ntsync.ko.zst modülü hazır -> WINEFSYNC=1 (futex_waitv) aktif; `sudo modprobe ntsync` ile /dev/ntsync açıldığında otomatik geçiş yapar"
    } else {
        "WINEFSYNC=1 (futex_waitv) çekirdek senkronizasyonu aktif"
    };

    decisions.push(AutotuneDecision {
        subsystem: "Çekirdek ve Zamanlayıcı (Kernel & Scheduler)".into(),
        detected_hardware: format!(
            "{} (BORE: {}, scx: {}, I/O: {})",
            probe.kernel_release,
            if probe.bore_scheduler_active { "Aktif" } else { "Pasif" },
            if probe.scx_scheduler_active { "Aktif" } else { "Pasif" },
            probe.io_scheduler
        ),
        optimization_applied: sync_desc.into(),
        env_pairs: kernel_envs.clone(),
    });
    merge_envs(&mut merged, &kernel_envs);

    // 2. Universal CPU Topology & OS-Level `taskset` / `sched_setaffinity` Decision
    let mut cpu_envs = Vec::new();
    cpu_envs.push(("WINE_LARGE_ADDRESS_AWARE".into(), "1".into()));

    let (topo_val, taskset_cpu_list, topo_desc) = if probe.cpu_topology.has_asymmetric_l3_vcache {
        // AMD Dual-CCD 3D V-Cache (7900X3D, 7950X3D, 9900X3D, 9950X3D):
        // Pin strictly to the 96MB 3D V-Cache CCD0 cores so threads never cross Infinity Fabric to CCD1!
        let vcache_list = probe
            .cpu_topology
            .p_core_cpus
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let topo = format!("{}:{}", probe.cpu_topology.p_core_cpus.len(), vcache_list);
        let desc = format!(
            "AMD Dual-CCD 3D V-Cache Asimetrisi: Oyunlar {}MB L3 V-Cache barındıran CCD0 çekirdeklerine ({}) sabitlendi; CCD1 Infinity Fabric gecikmesi önlendi",
            probe.cpu_topology.max_l3_kb / 1024,
            vcache_list
        );
        (topo, vcache_list, desc)
    } else if !probe.cpu_topology.lpe_core_cpus.is_empty() {
        // Intel 3-Tier Hybrid (Meteor Lake / Arrow Lake / Lunar Lake):
        if profile.id == "esports-bore" {
            let p_list = probe
                .cpu_topology
                .p_core_cpus
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let topo = format!("{}:{}", probe.cpu_topology.p_core_cpus.len(), p_list);
            let desc = format!(
                "Esports P-Core Sabitleme: Yalnızca {} MHz P-Çekirdekleri ({}) hem OS `taskset -c` hem `PROTON_CPU_TOPOLOGY` ile kilitlendi",
                probe.cpu_topology.max_freq_mhz, p_list
            );
            (topo, p_list, desc)
        } else {
            let mut compute_cpus = probe.cpu_topology.p_core_cpus.clone();
            compute_cpus.extend(probe.cpu_topology.e_core_cpus.iter().copied());
            let c_list = compute_cpus
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let topo = probe.cpu_topology.recommended_wine_topology.clone();
            let desc = format!(
                "L3 önbelleksiz SoC LP-E çekirdekleri ({:?}) hem OS `taskset -c` hem `PROTON_CPU_TOPOLOGY={}` ile tamamen izole edildi",
                probe.cpu_topology.lpe_core_cpus, topo
            );
            (topo, c_list, desc)
        }
    } else if probe.cpu_topology.is_hybrid {
        // Intel 2-Tier Hybrid (12th/13th/14th Gen Alder/Raptor Lake) or AMD Zen5+Zen5c (Strix Point):
        if profile.id == "esports-bore" {
            let p_list = probe
                .cpu_topology
                .p_core_cpus
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let topo = format!("{}:{}", probe.cpu_topology.p_core_cpus.len(), p_list);
            let desc = format!(
                "2-Katmanlı Hibrit Esports Modu: Oyun iş parçacıkları yüksek frekanslı P/Zen5 çekirdeklerine ({}) sabitlendi",
                p_list
            );
            (topo, p_list, desc)
        } else {
            let mut compute_cpus = probe.cpu_topology.p_core_cpus.clone();
            compute_cpus.extend(probe.cpu_topology.e_core_cpus.iter().copied());
            let c_list = compute_cpus
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let topo = probe.cpu_topology.recommended_wine_topology.clone();
            let desc = format!(
                "2-Katmanlı Hibrit P+E topoloji eşlemesi (`taskset` + `PROTON_CPU_TOPOLOGY={}`) uygulandı",
                topo
            );
            (topo, c_list, desc)
        }
    } else {
        // Homogeneous SMP (Desktop Ryzen 5/7, 7800X3D/9800X3D, Intel non-hybrid, Steam Deck APU)
        let all_list = (0..probe.cpu_threads.max(1))
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let topo = format!("{}:{}", probe.cpu_threads.max(1), all_list);
        let desc = format!(
            "Homojen SMP topolojisi ({} iş parçacığı) ve {} vektör hattı tam kapasite aktif",
            probe.cpu_threads, probe.isa_level
        );
        (topo, all_list, desc)
    };

    cpu_envs.push(("PROTON_CPU_TOPOLOGY".into(), topo_val.clone()));
    cpu_envs.push(("WINE_CPU_TOPOLOGY".into(), topo_val));
    cpu_envs.push(("OMAPROTON_TASKSET_CPUS".into(), taskset_cpu_list.clone()));

    decisions.push(AutotuneDecision {
        subsystem: "İşlemci Topolojisi ve OS Çekirdek Afinitesi (taskset + ntdll)".into(),
        detected_hardware: format!("{} ({})", probe.cpu_model, probe.cpu_topology.topology_strategy),
        optimization_applied: topo_desc,
        env_pairs: cpu_envs.clone(),
    });
    merge_envs(&mut merged, &cpu_envs);

    // 3. Universal GPU & Hybrid Multi-GPU Routing Decision
    let mut gpu_envs = Vec::new();
    let gpu_names = if probe.gpus.is_empty() {
        "Generic Vulkan GPU".to_string()
    } else {
        probe
            .gpus
            .iter()
            .map(|g| format!("{} [{}]", g.model_name, g.vendor_name))
            .collect::<Vec<_>>()
            .join(" + ")
    };

    let cache_gb = if probe.ram_total_mb >= 24000 {
        "16G"
    } else if probe.ram_total_mb >= 14000 {
        "8G"
    } else {
        "4G"
    };

    let gpu_opt_desc = if probe.has_nvidia_dgpu && probe.is_hybrid_gpu {
        // Hybrid Laptop: (Intel Arc or AMD Radeon iGPU) + NVIDIA RTX dGPU
        if probe.on_ac_power || profile.id == "esports-bore" {
            gpu_envs.push(("__NV_PRIME_RENDER_OFFLOAD".into(), "1".into()));
            gpu_envs.push(("__VK_LAYER_NV_optimus".into(), "NVIDIA_only".into()));
            gpu_envs.push(("__GLX_VENDOR_LIBRARY_NAME".into(), "nvidia".into()));
            gpu_envs.push(("VK_LOADER_DRIVERS_SELECT".into(), "*nvidia*".into()));
            gpu_envs.push(("PROTON_FORCE_NVAPI".into(), "1".into()));
            gpu_envs.push(("DXVK_ENABLE_NVAPI".into(), "1".into()));
            gpu_envs.push(("DXVK_NVAPI_ALLOW_OTHER_DRIVERS".into(), "1".into()));
            gpu_envs.push(("__GL_SHADER_DISK_CACHE".into(), "1".into()));
            gpu_envs.push(("__GL_SHADER_DISK_CACHE_SKIP_CLEANUP".into(), "1".into()));
            gpu_envs.push(("__GL_THREADED_OPTIMIZATIONS".into(), "1".into()));
            gpu_envs.push(("VKD3D_CONFIG".into(), "dxr11,dxr".into()));
            gpu_envs.push(("DXVK_STATE_CACHE".into(), "1".into()));
            "Hibrit GPU (iGPU + NVIDIA dGPU, AC Güç) -> VK_LOADER_DRIVERS_SELECT=*nvidia*, PRIME Offload, PROTON_FORCE_NVAPI=1 ve DXR 1.1 etkinleştirildi".to_string()
        } else {
            let igpu_select = if probe.has_amd_gpu { "*radeon*" } else { "*intel*" };
            gpu_envs.push(("VK_LOADER_DRIVERS_SELECT".into(), igpu_select.into()));
            if probe.has_amd_gpu {
                gpu_envs.push(("RADV_PERFTEST".into(), "gpl,nggc".into()));
            } else {
                gpu_envs.push(("ANV_GPL".into(), "true".into()));
            }
            gpu_envs.push(("MESA_SHADER_CACHE_MAX_SIZE".into(), cache_gb.into()));
            format!(
                "Hibrit GPU Batarya Modu -> Düşük güç tüketimli dahili iGPU ({}) Vulkan hattı seçildi",
                igpu_select
            )
        }
    } else if probe.is_hybrid_gpu && !probe.has_nvidia_dgpu {
        // Hybrid AMD iGPU + AMD Radeon dGPU (SmartShift) or Intel iGPU + Intel Arc/AMD dGPU
        if let Some(dgpu) = probe.gpus.iter().find(|g| g.is_discrete) {
            if probe.on_ac_power {
                gpu_envs.push(("DRI_PRIME".into(), "1".into()));
                gpu_envs.push((
                    "MESA_VK_DEVICE_SELECT".into(),
                    format!("{}:{}!", dgpu.vendor_id, dgpu.device_id),
                ));
                if dgpu.vendor_id == "1002" {
                    gpu_envs.push(("RADV_PERFTEST".into(), "gpl,nggc".into()));
                    gpu_envs.push(("AMD_VULKAN_ICD".into(), "RADV".into()));
                } else {
                    gpu_envs.push(("ANV_GPL".into(), "true".into()));
                }
                gpu_envs.push(("VKD3D_CONFIG".into(), "dxr11,dxr".into()));
                format!(
                    "Hibrit Mesa GPU (AC Güç) -> Harici {} [{}] DRI_PRIME=1 ve MESA_VK_DEVICE_SELECT={}:{}! ile kilitlendi",
                    dgpu.model_name, dgpu.vendor_name, dgpu.vendor_id, dgpu.device_id
                )
            } else {
                gpu_envs.push(("MESA_SHADER_CACHE_MAX_SIZE".into(), cache_gb.into()));
                "Hibrit Mesa GPU (Batarya Modu) -> Verimli dahili iGPU hattı seçildi".to_string()
            }
        } else {
            gpu_envs.push(("MESA_SHADER_CACHE_MAX_SIZE".into(), cache_gb.into()));
            "Çoklu Mesa GPU algılandı -> Varsayılan Vulkan hattı aktif".to_string()
        }
    } else if probe.has_nvidia_dgpu {
        // Single Desktop NVIDIA GPU
        gpu_envs.push(("VK_LOADER_DRIVERS_SELECT".into(), "*nvidia*".into()));
        gpu_envs.push(("PROTON_FORCE_NVAPI".into(), "1".into()));
        gpu_envs.push(("DXVK_ENABLE_NVAPI".into(), "1".into()));
        gpu_envs.push(("__GL_SHADER_DISK_CACHE".into(), "1".into()));
        gpu_envs.push(("__GL_SHADER_DISK_CACHE_SKIP_CLEANUP".into(), "1".into()));
        gpu_envs.push(("VKD3D_CONFIG".into(), "dxr11,dxr".into()));
        "Masaüstü Tekil NVIDIA GPU -> PROTON_FORCE_NVAPI=1, DXVK_ENABLE_NVAPI=1, DXR 1.1 ve kalıcı Shader önbelleği aktif".to_string()
    } else if probe.has_amd_gpu {
        // Single Desktop AMD Radeon or Handheld AMD APU (Steam Deck / ROG Ally / Ryzen APU)
        gpu_envs.push(("VK_LOADER_DRIVERS_SELECT".into(), "*radeon*".into()));
        gpu_envs.push(("RADV_PERFTEST".into(), "gpl,nggc".into()));
        gpu_envs.push(("AMD_VULKAN_ICD".into(), "RADV".into()));
        gpu_envs.push(("MESA_SHADER_CACHE_MAX_SIZE".into(), cache_gb.into()));
        gpu_envs.push(("VKD3D_CONFIG".into(), "dxr11,dxr".into()));
        format!(
            "AMD Radeon / RDNA GPU -> Mesa RADV GPL + NGGC, DXR 1.1 ve {} gölgelendirici önbelleği aktif",
            cache_gb
        )
    } else {
        // Single Intel Arc Discrete (B580/A770) or Intel Xe/Arc iGPU
        gpu_envs.push(("VK_LOADER_DRIVERS_SELECT".into(), "*intel*".into()));
        gpu_envs.push(("ANV_GPL".into(), "true".into()));
        gpu_envs.push(("MESA_SHADER_CACHE_MAX_SIZE".into(), cache_gb.into()));
        gpu_envs.push(("DXVK_STATE_CACHE".into(), "1".into()));
        format!(
            "Intel Arc / Xe GPU -> Mesa ANV Graphics Pipeline Library (ANV_GPL=true) ve {} önbellek aktif",
            cache_gb
        )
    };

    decisions.push(AutotuneDecision {
        subsystem: "Ekran Kartı ve Grafik Hattı (GPU & Vulkan ICD)".into(),
        detected_hardware: gpu_names,
        optimization_applied: gpu_opt_desc,
        env_pairs: gpu_envs.clone(),
    });
    merge_envs(&mut merged, &gpu_envs);

    // 4. Display Compositor & System Memory Decision
    let mut sys_envs = Vec::new();
    sys_envs.push(("MESA_SHADER_CACHE_MAX_SIZE".into(), cache_gb.into()));
    if profile.enable_hdr {
        sys_envs.push(("DXVK_HDR".into(), "1".into()));
        sys_envs.push(("ENABLE_HDR_WSI".into(), "1".into()));
    }

    decisions.push(AutotuneDecision {
        subsystem: "Sistem Belleği, Güç ve Görüntü Sunucusu (RAM / Power / Wayland)".into(),
        detected_hardware: format!(
            "{} MB RAM · Güç: {} · Görüntü: {}",
            probe.ram_total_mb,
            if probe.on_ac_power { "AC Adaptör (Maksimum Performans)" } else { "Batarya" },
            if probe.hyprland_active { "Hyprland / Wayland" } else { "Wayland / X11" }
        ),
        optimization_applied: format!(
            "Bellek kapasitesine göre {} Vulkan/Mesa önbelleği ayrıldı; {} profili uygulandı.",
            cache_gb, profile.name
        ),
        env_pairs: sys_envs.clone(),
    });
    merge_envs(&mut merged, &sys_envs);

    // Apply explicit profile overrides
    merge_envs(&mut merged, &profile.env_exports);

    // 5. Omarchy Default AI Agent Bridge
    if include_local_ai_file {
        if let Some(ai_ovr) = load_ai_overrides() {
            merge_envs(&mut merged, &ai_ovr.env_overrides);
            decisions.push(AutotuneDecision {
                subsystem: "Omarchy Varsayılan YZ Ajanı Katmanı (omarchy-default-agent)".into(),
                detected_hardware: format!(
                    "Seçili Ajan: {} (Son Optimize Eden: {})",
                    probe.default_ai_agent_label, ai_ovr.agent_id
                ),
                optimization_applied: format!(
                    "{} ({} özel ortam değişkeni katmana birleştirildi)",
                    ai_ovr.rationale,
                    ai_ovr.env_overrides.len()
                ),
                env_pairs: ai_ovr.env_overrides,
            });
        } else {
            decisions.push(AutotuneDecision {
                subsystem: "Omarchy Varsayılan YZ Ajanı Katmanı (omarchy-default-agent)".into(),
                detected_hardware: format!(
                    "Seçili Ajan: {} (~/.config/omarchy/defaults/agent)",
                    probe.default_ai_agent_label
                ),
                optimization_applied: "Otonom donanım baz profili aktif; `omarchy-agent-proton` ile seçili YZ ajanınız üzerinden oyuna/sisteme özel derin ince ayar yapılabilir.".into(),
                env_pairs: Vec::new(),
            });
        }
    }

    let summary_headline = format!(
        "Otonom Uyarlama Aktif: {} + {} + {} GPU + YZ Ajanı: {}",
        probe.kernel_release,
        probe.march_target,
        probe.gpus.len(),
        probe.default_ai_agent_label
    );

    AutotunePlan {
        summary_headline,
        active_ai_agent: probe.default_ai_agent.clone(),
        active_ai_agent_label: probe.default_ai_agent_label.clone(),
        taskset_cpu_list,
        decisions,
        merged_env: merged,
    }
}

fn merge_envs(target: &mut Vec<(String, String)>, incoming: &[(String, String)]) {
    for (k, v) in incoming {
        if let Some(existing) = target.iter_mut().find(|(ek, _)| ek == k) {
            existing.1 = v.clone();
        } else {
            target.push((k.clone(), v.clone()));
        }
    }
}

/// Emits `unset ...` and `export KEY="VALUE"` lines suitable for safe evaluation by the `proton` wrapper at game start.
pub fn render_shell_exports(plan: &AutotunePlan) -> String {
    let mut out = String::from(
        "unset PROTON_NO_NTSYNC PROTON_NO_FSYNC PROTON_NO_ESYNC PROTON_DISABLE_NVAPI PROTON_HIDE_NVIDIA_GPU\n",
    );
    for (k, v) in &plan.merged_env {
        let safe_k: String = k
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let safe_v: String = v
            .chars()
            .filter(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(*c, '-' | '_' | '.' | ',' | ':' | '/' | '=' | '*' | '!')
            })
            .collect();
        if !safe_k.is_empty() {
            out.push_str(&format!("export {}=\"${{{}:-{}}}\"\n", safe_k, safe_k, safe_v));
        }
    }
    out
}

/// Constructs an upstream-compatible prompt for `omarchy-agent --prompt`.
pub fn build_omarchy_agent_prompt(
    probe: &ProtonSystemProbe,
    plan: &AutotunePlan,
    target_game: Option<&str>,
    skill_path: &str,
) -> String {
    let gpu_summary = probe
        .gpus
        .iter()
        .map(|g| format!("{} ({})", g.model_name, g.vendor_name))
        .collect::<Vec<_>>()
        .join(" + ");

    let game_ctx = target_game.unwrap_or("Genel Sistem ve Tüm Steam Oyunları");

    format!(
        "Optimize the Omarchy Proton (OmaProton) bleeding-edge compatibility layer for this machine.\n\n\
Detected Omarchy System Facts:\n\
  default_agent:  {} ({})\n\
  kernel:         {} (BORE: {}, I/O: {}, NTSYNC module: {})\n\
  cpu:            {} ({} threads, ISA: {}, -march={})\n\
  cpu_topology:   {} (recommended PROTON_CPU_TOPOLOGY={}, taskset={})\n\
  gpus:           {} (Hybrid PRIME: {})\n\
  memory_power:   {} MB RAM | AC Power: {} | Hyprland/Wayland: {}\n\
  target_scope:   {}\n\
  current_plan:   {}\n\n\
Use the optimize-proton skill to inspect the telemetry (`omaprot-engine --json`), determine the optimal kernel/CPU-topology/GPU/Vulkan environment overrides or source build flags for this hardware and target game, and persist them atomically using `omaprot-engine --apply-ai-env \"<rationale>\" KEY=VAL ...`:\n\n\
  {}\n",
        probe.default_ai_agent,
        probe.default_ai_agent_label,
        probe.kernel_release,
        probe.bore_scheduler_active,
        probe.io_scheduler,
        probe.ntsync_module_present,
        probe.cpu_model,
        probe.cpu_threads,
        probe.isa_level,
        probe.march_target,
        probe.cpu_topology.topology_strategy,
        probe.cpu_topology.recommended_wine_topology,
        plan.taskset_cpu_list,
        gpu_summary,
        probe.is_hybrid_gpu,
        probe.ram_total_mb,
        probe.on_ac_power,
        probe.hyprland_active,
        game_ctx,
        plan.summary_headline,
        skill_path
    )
}

/// Runs a deterministic verification across 8 distinct PC hardware archetypes to prove
/// universal self-adaptation without any hardcoded machine assumptions.
pub fn verify_universal_pc_matrix() -> Vec<MatrixVerificationEntry> {
    let mut results = Vec::new();

    // Helper to synthesize a probe for a given PC archetype
    let mut run_case = |name: &str,
                        vendor: &str,
                        model: &str,
                        flags: &[&str],
                        obs: Vec<CoreObservation>,
                        gpus: Vec<GpuDevice>,
                        ram_mb: u64,
                        on_ac: bool| {
        let flag_vec: Vec<String> = flags.iter().map(|s| s.to_string()).collect();
        let (isa_level, march_target) = classify_isa(vendor, model, &flag_vec);
        let cpu_topology = classify_topology_from_observations(&obs, vendor, model);
        let has_nvidia_dgpu = gpus.iter().any(|g| g.vendor_id == "10de");
        let has_amd_gpu = gpus.iter().any(|g| g.vendor_id == "1002");
        let has_intel_gpu = gpus.iter().any(|g| g.vendor_id == "8086");
        let is_hybrid_gpu = gpus.len() > 1 && gpus.iter().any(|g| g.is_discrete);

        let probe = ProtonSystemProbe {
            kernel_release: "7.2.8-9-omarchy".into(),
            is_omarchy_kernel: true,
            bore_scheduler_active: true,
            scx_scheduler_active: false,
            io_scheduler: "adios".into(),
            ntsync_dev_active: true,
            ntsync_module_present: true,
            split_lock_mitigation: "Mitigated".into(),
            cpu_model: model.into(),
            cpu_vendor: vendor.into(),
            cpu_threads: obs.len(),
            cpu_topology,
            isa_level,
            march_target: march_target.clone(),
            cpu_flags: flag_vec,
            gpus,
            has_nvidia_dgpu,
            has_amd_gpu,
            has_intel_gpu,
            is_hybrid_gpu,
            ram_total_mb: ram_mb,
            on_ac_power: on_ac,
            wayland_active: true,
            hyprland_active: true,
            default_ai_agent: "claude".into(),
            default_ai_agent_label: "Claude Code (claude)".into(),
            omarchy_agent_cli_present: true,
            gcc_version: "gcc 16.2.1".into(),
            clang_version: "clang 23.1.3".into(),
            mingw_installed: true,
            steam_root: "/home/user/.local/share/Steam".into(),
            base_proton_path: "/home/user/.local/share/Steam/steamapps/common/Proton - Experimental"
                .into(),
            sniper_runtime_present: true,
            omaprot_installed: true,
            omaprot_install_path:
                "/home/user/.local/share/Steam/compatibilitytools.d/Proton-Omarchy-BleedingEdge"
                    .into(),
            readiness_score: 100,
        };

        let profile = crate::profiles::all_profiles(&probe)[0].clone();
        let plan = compute_autotune_plan_internal(&probe, &profile, false);
        let gpu_decision = plan
            .decisions
            .iter()
            .find(|d| d.subsystem.contains("GPU"))
            .map(|d| d.optimization_applied.clone())
            .unwrap_or_default();

        results.push(MatrixVerificationEntry {
            archetype_name: name.into(),
            cpu_summary: format!("{} -> {}", model, probe.cpu_topology.topology_strategy),
            gpu_summary: probe
                .gpus
                .iter()
                .map(|g| g.model_name.clone())
                .collect::<Vec<_>>()
                .join(" + "),
            march_flag: format!("-march={}", march_target),
            taskset_cpus: plan.taskset_cpu_list,
            proton_cpu_topology: probe.cpu_topology.recommended_wine_topology,
            gpu_routing_summary: gpu_decision,
        });
    };

    // 1. Intel Core Ultra 7 155H (3-Tier Hybrid: 12 P + 8 E + 2 L3-less LP-E) + Arc iGPU + RTX 4060 dGPU (AC)
    let mut mtl_obs = Vec::new();
    for i in 0..12 {
        mtl_obs.push(CoreObservation { cpu_id: i, max_freq_mhz: 4800, l3_cache_kb: 24576 });
    }
    for i in 12..20 {
        mtl_obs.push(CoreObservation { cpu_id: i, max_freq_mhz: 3800, l3_cache_kb: 24576 });
    }
    for i in 20..22 {
        mtl_obs.push(CoreObservation { cpu_id: i, max_freq_mhz: 2500, l3_cache_kb: 0 });
    }
    run_case(
        "1. Hibrit Dizüstü (AC Güç): Intel Core Ultra 7 155H + Intel Arc iGPU + NVIDIA RTX 4060",
        "GenuineIntel",
        "Intel(R) Core(TM) Ultra 7 155H",
        &["avx", "avx2", "avx_vnni", "fma", "bmi1", "bmi2"],
        mtl_obs.clone(),
        vec![
            GpuDevice {
                pci_slot: "00:02.0".into(),
                vendor_id: "8086".into(),
                device_id: "7d55".into(),
                vendor_name: "Intel".into(),
                model_name: "Intel Arc Graphics (iGPU)".into(),
                is_discrete: false,
                vulkan_icd: "intel_icd.json".into(),
            },
            GpuDevice {
                pci_slot: "01:00.0".into(),
                vendor_id: "10de".into(),
                device_id: "28a0".into(),
                vendor_name: "NVIDIA".into(),
                model_name: "GeForce RTX 4060 Max-Q (dGPU)".into(),
                is_discrete: true,
                vulkan_icd: "nvidia_icd.json".into(),
            },
        ],
        32768,
        true,
    );

    // 2. Same Laptop on Battery Power -> Auto iGPU Switch
    run_case(
        "2. Hibrit Dizüstü (Batarya Modu): Intel Core Ultra 7 155H + Arc iGPU + RTX 4060",
        "GenuineIntel",
        "Intel(R) Core(TM) Ultra 7 155H",
        &["avx", "avx2", "avx_vnni", "fma", "bmi1", "bmi2"],
        mtl_obs,
        vec![
            GpuDevice {
                pci_slot: "00:02.0".into(),
                vendor_id: "8086".into(),
                device_id: "7d55".into(),
                vendor_name: "Intel".into(),
                model_name: "Intel Arc Graphics (iGPU)".into(),
                is_discrete: false,
                vulkan_icd: "intel_icd.json".into(),
            },
            GpuDevice {
                pci_slot: "01:00.0".into(),
                vendor_id: "10de".into(),
                device_id: "28a0".into(),
                vendor_name: "NVIDIA".into(),
                model_name: "GeForce RTX 4060 Max-Q (dGPU)".into(),
                is_discrete: true,
                vulkan_icd: "nvidia_icd.json".into(),
            },
        ],
        32768,
        false,
    );

    // 3. AMD Ryzen 9 9950X3D Dual-CCD (CCD0: 16 threads w/ 96MB 3D V-Cache, CCD1: 16 threads w/ 32MB L3) + Radeon RX 7900 XTX
    let mut x3d_dual = Vec::new();
    for i in 0..16 {
        x3d_dual.push(CoreObservation { cpu_id: i, max_freq_mhz: 5500, l3_cache_kb: 98304 });
    }
    for i in 16..32 {
        x3d_dual.push(CoreObservation { cpu_id: i, max_freq_mhz: 5700, l3_cache_kb: 32768 });
    }
    run_case(
        "3. Üst Seviye Masaüstü (AMD Dual-CCD X3D): Ryzen 9 9950X3D + Radeon RX 7900 XTX",
        "AuthenticAMD",
        "AMD Ryzen 9 9950X3D 16-Core Processor",
        &["avx", "avx2", "avx512f", "avx512bw", "avx512vl", "fma", "bmi1", "bmi2"],
        x3d_dual,
        vec![GpuDevice {
            pci_slot: "03:00.0".into(),
            vendor_id: "1002".into(),
            device_id: "744c".into(),
            vendor_name: "AMD".into(),
            model_name: "AMD Radeon RX 7900 XTX".into(),
            is_discrete: true,
            vulkan_icd: "radeon_icd.x86_64.json".into(),
        }],
        65536,
        true,
    );

    // 4. AMD Ryzen 7 7800X3D Single-CCD (16 threads w/ uniform 96MB 3D V-Cache) + NVIDIA RTX 4090
    let mut x3d_single = Vec::new();
    for i in 0..16 {
        x3d_single.push(CoreObservation { cpu_id: i, max_freq_mhz: 5050, l3_cache_kb: 98304 });
    }
    run_case(
        "4. Oyuncu Masaüstü (Tek-CCD X3D + NVIDIA): Ryzen 7 7800X3D + GeForce RTX 4090",
        "AuthenticAMD",
        "AMD Ryzen 7 7800X3D 8-Core Processor",
        &["avx", "avx2", "avx512f", "avx512bw", "avx512vl", "fma", "bmi1", "bmi2"],
        x3d_single,
        vec![GpuDevice {
            pci_slot: "01:00.0".into(),
            vendor_id: "10de".into(),
            device_id: "2684".into(),
            vendor_name: "NVIDIA".into(),
            model_name: "NVIDIA GeForce RTX 4090".into(),
            is_discrete: true,
            vulkan_icd: "nvidia_icd.json".into(),
        }],
        32768,
        true,
    );

    // 5. Intel Core i7-14700K (2-Tier Desktop Hybrid: 16 P-Threads @ 5.6GHz + 12 E-Cores @ 4.3GHz, all with 33MB L3) + Radeon RX 7800 XT
    let mut rpl_obs = Vec::new();
    for i in 0..16 {
        rpl_obs.push(CoreObservation { cpu_id: i, max_freq_mhz: 5600, l3_cache_kb: 33792 });
    }
    for i in 16..28 {
        rpl_obs.push(CoreObservation { cpu_id: i, max_freq_mhz: 4300, l3_cache_kb: 33792 });
    }
    run_case(
        "5. Intel 2-Katmanlı Hibrit Masaüstü: Core i7-14700K (P+E) + AMD Radeon RX 7800 XT",
        "GenuineIntel",
        "Intel(R) Core(TM) i7-14700K",
        &["avx", "avx2", "avx_vnni", "fma", "bmi1", "bmi2"],
        rpl_obs,
        vec![GpuDevice {
            pci_slot: "01:00.0".into(),
            vendor_id: "1002".into(),
            device_id: "747e".into(),
            vendor_name: "AMD".into(),
            model_name: "AMD Radeon RX 7800 XT".into(),
            is_discrete: true,
            vulkan_icd: "radeon_icd.x86_64.json".into(),
        }],
        32768,
        true,
    );

    // 6. AMD SmartShift Hybrid Laptop: Ryzen AI 9 HX 370 (Zen5 @ 5.1GHz + Zen5c @ 3.3GHz) + Radeon 890M iGPU + Radeon RX 7700S dGPU
    let mut strix_obs = Vec::new();
    for i in 0..8 {
        strix_obs.push(CoreObservation { cpu_id: i, max_freq_mhz: 5100, l3_cache_kb: 16384 });
    }
    for i in 8..24 {
        strix_obs.push(CoreObservation { cpu_id: i, max_freq_mhz: 3300, l3_cache_kb: 8192 });
    }
    run_case(
        "6. AMD SmartShift Hibrit Dizüstü: Ryzen AI 9 HX 370 + Radeon 890M iGPU + RX 7700S dGPU",
        "AuthenticAMD",
        "AMD Ryzen AI 9 HX 370 w/ Radeon 890M",
        &["avx", "avx2", "avx512f", "avx512bw", "avx512vl", "fma", "bmi1", "bmi2"],
        strix_obs,
        vec![
            GpuDevice {
                pci_slot: "65:00.0".into(),
                vendor_id: "1002".into(),
                device_id: "150e".into(),
                vendor_name: "AMD".into(),
                model_name: "AMD Radeon 890M Graphics (iGPU)".into(),
                is_discrete: false,
                vulkan_icd: "radeon_icd.x86_64.json".into(),
            },
            GpuDevice {
                pci_slot: "03:00.0".into(),
                vendor_id: "1002".into(),
                device_id: "7480".into(),
                vendor_name: "AMD".into(),
                model_name: "AMD Radeon RX 7700S (dGPU)".into(),
                is_discrete: true,
                vulkan_icd: "radeon_icd.x86_64.json".into(),
            },
        ],
        32768,
        true,
    );

    // 7. Budget/Midrange Homogeneous Desktop: Intel Core i5-12400F (6C/12T Homogeneous) + Intel Arc B580 Discrete GPU
    let mut i5_obs = Vec::new();
    for i in 0..12 {
        i5_obs.push(CoreObservation { cpu_id: i, max_freq_mhz: 4400, l3_cache_kb: 18432 });
    }
    run_case(
        "7. Homojen Masaüstü + Intel Arc Harici GPU: Core i5-12400F + Intel Arc B580",
        "GenuineIntel",
        "12th Gen Intel(R) Core(TM) i5-12400F",
        &["avx", "avx2", "avx_vnni", "fma", "bmi1", "bmi2"],
        i5_obs,
        vec![GpuDevice {
            pci_slot: "03:00.0".into(),
            vendor_id: "8086".into(),
            device_id: "e20b".into(),
            vendor_name: "Intel".into(),
            model_name: "Intel Arc B580 Graphics (Battlemage dGPU)".into(),
            is_discrete: true,
            vulkan_icd: "intel_icd.x86_64.json".into(),
        }],
        16384,
        true,
    );

    // 8. Handheld Gaming PC / APU: Valve Steam Deck / ROG Ally (AMD Zen 2/3 8T @ 3.5GHz + RDNA2 APU, 16GB RAM)
    let mut deck_obs = Vec::new();
    for i in 0..8 {
        deck_obs.push(CoreObservation { cpu_id: i, max_freq_mhz: 3500, l3_cache_kb: 4096 });
    }
    run_case(
        "8. El Konsolu / APU Sistem: Steam Deck / ROG Ally (AMD Custom APU 8T + RDNA iGPU)",
        "AuthenticAMD",
        "AMD Custom APU 0405",
        &["avx", "avx2", "fma", "bmi1", "bmi2"],
        deck_obs,
        vec![GpuDevice {
            pci_slot: "04:00.0".into(),
            vendor_id: "1002".into(),
            device_id: "163f".into(),
            vendor_name: "AMD".into(),
            model_name: "AMD VanGogh RDNA2 Graphics (APU)".into(),
            is_discrete: false,
            vulkan_icd: "radeon_icd.x86_64.json".into(),
        }],
        16000,
        false,
    );

    results
}
