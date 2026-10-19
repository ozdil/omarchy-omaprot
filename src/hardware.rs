//! Universal Hardware, Kernel Scheduler (BORE/scx/NTSYNC), L3-Cache & Hybrid CPU Topology
//! (Intel P/E/LP-E, AMD 3D V-Cache Dual-CCD, Zen5+Zen5c, Homogeneous SMP), and Multi-GPU Probe.

use crate::securefs;
use crate::subproc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuDevice {
    pub pci_slot: String,
    pub vendor_id: String,
    pub device_id: String,
    pub vendor_name: String,
    pub model_name: String,
    pub is_discrete: bool,
    pub vulkan_icd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuTopologyInfo {
    pub total_threads: usize,
    pub is_hybrid: bool,
    pub has_asymmetric_l3_vcache: bool,
    pub p_core_cpus: Vec<usize>,
    pub e_core_cpus: Vec<usize>,
    pub lpe_core_cpus: Vec<usize>,
    pub max_freq_mhz: u32,
    pub min_core_max_freq_mhz: u32,
    pub max_l3_kb: u64,
    pub min_l3_kb: u64,
    pub recommended_wine_topology: String,
    pub topology_strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtonSystemProbe {
    pub kernel_release: String,
    pub is_omarchy_kernel: bool,
    pub bore_scheduler_active: bool,
    pub scx_scheduler_active: bool,
    pub io_scheduler: String,
    pub ntsync_dev_active: bool,
    pub ntsync_module_present: bool,
    pub split_lock_mitigation: String,
    pub cpu_model: String,
    pub cpu_vendor: String,
    pub cpu_threads: usize,
    pub cpu_topology: CpuTopologyInfo,
    pub isa_level: String,
    pub march_target: String,
    pub cpu_flags: Vec<String>,
    pub gpus: Vec<GpuDevice>,
    pub has_nvidia_dgpu: bool,
    pub has_amd_gpu: bool,
    pub has_intel_gpu: bool,
    pub is_hybrid_gpu: bool,
    pub ram_total_mb: u64,
    pub on_ac_power: bool,
    pub wayland_active: bool,
    pub hyprland_active: bool,
    pub default_ai_agent: String,
    pub default_ai_agent_label: String,
    pub omarchy_agent_cli_present: bool,
    pub gcc_version: String,
    pub clang_version: String,
    pub mingw_installed: bool,
    pub steam_root: String,
    pub base_proton_path: String,
    pub sniper_runtime_present: bool,
    pub omaprot_installed: bool,
    pub omaprot_install_path: String,
    pub readiness_score: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct CoreObservation {
    pub cpu_id: usize,
    pub max_freq_mhz: u32,
    pub l3_cache_kb: u64,
}

pub fn probe_system() -> ProtonSystemProbe {
    let kernel_release = securefs::read_bounded_file(
        Path::new("/proc/sys/kernel/osrelease"),
        256,
        false,
    )
    .map(|s| s.trim().to_string())
    .unwrap_or_else(|| "unknown".into());

    let is_omarchy_kernel = kernel_release.to_lowercase().contains("omarchy")
        || kernel_release.to_lowercase().contains("cachyos");

    let bore_scheduler_active = securefs::read_bounded_file(
        Path::new("/proc/sys/kernel/sched_bore"),
        64,
        false,
    )
    .map(|s| s.trim() == "1")
    .unwrap_or(is_omarchy_kernel);

    let scx_scheduler_active = securefs::read_bounded_file(
        Path::new("/sys/kernel/sched_ext/state"),
        64,
        false,
    )
    .map(|s| s.trim() == "enabled")
    .unwrap_or(false);

    let io_scheduler = detect_io_scheduler();

    let ntsync_dev_active = Path::new("/dev/ntsync").exists();
    let module_path = format!(
        "/lib/modules/{}/kernel/drivers/misc/ntsync.ko.zst",
        kernel_release
    );
    let module_path_alt = format!(
        "/lib/modules/{}/kernel/drivers/misc/ntsync.ko",
        kernel_release
    );
    let ntsync_module_present = ntsync_dev_active
        || Path::new(&module_path).exists()
        || Path::new(&module_path_alt).exists();

    let (cpu_model, cpu_vendor, cpu_threads, all_flags) = parse_cpuinfo();
    let cpu_topology = detect_cpu_topology(cpu_threads, &cpu_vendor, &cpu_model);
    let (isa_level, march_target) = classify_isa(&cpu_vendor, &cpu_model, &all_flags);

    let tracked_flags = [
        "avx",
        "avx2",
        "avx_vnni",
        "avx512f",
        "fma",
        "bmi1",
        "bmi2",
        "aes",
        "vaes",
        "sha_ni",
        "fsgsbase",
        "split_lock_detect",
        "bus_lock_detect",
        "user_shstk",
        "ibt",
    ];
    let cpu_flags: Vec<String> = tracked_flags
        .iter()
        .filter(|&&f| all_flags.iter().any(|x| x == f))
        .map(|&s| s.to_string())
        .collect();

    let split_lock_mitigation = if cpu_flags.iter().any(|f| f == "split_lock_detect") {
        "Active (Mitigate via sysctl kernel.split_lock_mitigate=0 for zero FPS drops)".into()
    } else {
        "Not applicable".into()
    };

    let gpus = detect_gpus();
    let has_nvidia_dgpu = gpus.iter().any(|g| g.vendor_id == "10de");
    let has_amd_gpu = gpus.iter().any(|g| g.vendor_id == "1002");
    let has_intel_gpu = gpus.iter().any(|g| g.vendor_id == "8086");
    let is_hybrid_gpu = gpus.len() > 1 && gpus.iter().any(|g| g.is_discrete);

    let ram_total_mb = detect_ram_mb();
    let on_ac_power = detect_ac_power();
    let wayland_active = std::env::var("WAYLAND_DISPLAY")
        .map(|v| !v.is_empty())
        .unwrap_or(true);
    let hyprland_active = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map(|v| !v.is_empty())
        .unwrap_or(true);

    let (default_ai_agent, default_ai_agent_label, omarchy_agent_cli_present) =
        detect_omarchy_default_agent();

    let gcc_version = detect_tool_version("/usr/bin/gcc", &["--version"]);
    let clang_version = detect_tool_version("/usr/bin/clang", &["--version"]);
    let mingw_installed = Path::new("/usr/bin/x86_64-w64-mingw32-gcc").exists();

    let (
        steam_root,
        base_proton_path,
        sniper_runtime_present,
        omaprot_installed,
        omaprot_install_path,
    ) = detect_steam_environment();

    let mut readiness_score = 45u32;
    if is_omarchy_kernel {
        readiness_score += 15;
    }
    if bore_scheduler_active || scx_scheduler_active {
        readiness_score += 10;
    }
    if ntsync_module_present {
        readiness_score += 10;
    }
    if !gpus.is_empty() {
        readiness_score += 10;
    }
    if !base_proton_path.is_empty() {
        readiness_score += 5;
    }
    if omaprot_installed {
        readiness_score += 5;
    }
    readiness_score = readiness_score.min(100);

    ProtonSystemProbe {
        kernel_release,
        is_omarchy_kernel,
        bore_scheduler_active,
        scx_scheduler_active,
        io_scheduler,
        ntsync_dev_active,
        ntsync_module_present,
        split_lock_mitigation,
        cpu_model,
        cpu_vendor,
        cpu_threads,
        cpu_topology,
        isa_level,
        march_target,
        cpu_flags,
        gpus,
        has_nvidia_dgpu,
        has_amd_gpu,
        has_intel_gpu,
        is_hybrid_gpu,
        ram_total_mb,
        on_ac_power,
        wayland_active,
        hyprland_active,
        default_ai_agent,
        default_ai_agent_label,
        omarchy_agent_cli_present,
        gcc_version,
        clang_version,
        mingw_installed,
        steam_root,
        base_proton_path,
        sniper_runtime_present,
        omaprot_installed,
        omaprot_install_path,
        readiness_score,
    }
}

fn detect_omarchy_default_agent() -> (String, String, bool) {
    let home = std::env::var("HOME").unwrap_or_default();
    let agent_path = PathBuf::from(&home).join(".config/omarchy/defaults/agent");
    let raw_agent = securefs::read_owned_file(&agent_path, 256)
        .map(|s| s.lines().next().unwrap_or("").trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unset".into());

    let label = match raw_agent.as_str() {
        "agy" | "antigravity" | "gemini" => "Antigravity (agy)",
        "claude" | "claude-code" => "Claude Code (claude)",
        "codex" => "OpenAI Codex (codex)",
        "opencode" | "open-code" => "OpenCode (opencode)",
        "grok" => "Grok (grok)",
        "openclaw" => "OpenClaw (openclaw)",
        "hermes" => "Hermes (hermes)",
        "copilot" => "GitHub Copilot (copilot)",
        "crush" => "Crush (crush)",
        "cursor-agent" | "cursor" => "Cursor CLI (cursor-agent)",
        "muse" => "Muse Code (muse)",
        "omp" => "Oh My Pi (omp)",
        "ori" => "Ori (ori)",
        "pi" => "Pi (pi)",
        "unset" => "Seçilmedi (omarchy default agent <isim>)",
        other => other,
    }
    .to_string();

    let cli_present = Path::new("/usr/share/omarchy/bin/omarchy-agent").exists()
        || Path::new("/usr/bin/omarchy-agent").exists()
        || Path::new("/home/ozdil/Projects/omarchy/omarchy-core/bin/omarchy-agent").exists();

    (raw_agent, label, cli_present)
}

fn detect_io_scheduler() -> String {
    let entries = match fs::read_dir("/sys/block") {
        Ok(e) => e,
        Err(_) => return "none".into(),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("loop") || name.starts_with("zram") {
            continue;
        }
        let sched_path = entry.path().join("queue/scheduler");
        if let Some(txt) = securefs::read_bounded_file(&sched_path, 256, false) {
            if let Some(start) = txt.find('[') {
                if let Some(end) = txt[start + 1..].find(']') {
                    return txt[start + 1..start + 1 + end].to_string();
                }
            }
        }
    }
    "none".into()
}

fn detect_cpu_topology(total_threads: usize, vendor: &str, model: &str) -> CpuTopologyInfo {
    let mut obs = Vec::new();
    for cpu_id in 0..total_threads.max(1) {
        let freq_path = format!(
            "/sys/devices/system/cpu/cpu{}/cpufreq/cpuinfo_max_freq",
            cpu_id
        );
        let max_freq_mhz = securefs::read_bounded_file(Path::new(&freq_path), 64, false)
            .and_then(|s| s.trim().parse::<u32>().ok())
            .map(|khz| khz / 1000)
            .unwrap_or(0);

        let l3_path = format!(
            "/sys/devices/system/cpu/cpu{}/cache/index3/size",
            cpu_id
        );
        let l3_cache_kb = securefs::read_bounded_file(Path::new(&l3_path), 64, false)
            .and_then(|s| parse_cache_size_kb(&s))
            .unwrap_or(0);

        obs.push(CoreObservation {
            cpu_id,
            max_freq_mhz,
            l3_cache_kb,
        });
    }

    classify_topology_from_observations(&obs, vendor, model)
}

fn parse_cache_size_kb(raw: &str) -> Option<u64> {
    let s = raw.trim().to_uppercase();
    if let Some(num) = s.strip_suffix('K') {
        num.trim().parse::<u64>().ok()
    } else if let Some(num) = s.strip_suffix('M') {
        num.trim().parse::<u64>().ok().map(|m| m * 1024)
    } else {
        s.parse::<u64>().ok()
    }
}

/// Pure function that classifies ANY PC's CPU topology from per-core frequency and L3 cache size:
/// - Handles AMD Dual-CCD 3D V-Cache (`7900X3D`, `7950X3D`, `9900X3D`, `9950X3D`) via L3 cache asymmetry (`96MB` vs `32MB`).
/// - Handles Intel 3-Tier Hybrid (`Meteor Lake`, `Lunar Lake`, `Arrow Lake`, `Panther Lake`) via L3-less (`l3_cache_kb == 0`) LP-E island detection + P/E frequency tiers.
/// - Handles Intel 2-Tier Hybrid (`12th/13th/14th Gen Alder/Raptor Lake`) & AMD Zen5+Zen5c (`Strix Point`) via P/E frequency tiers.
/// - Handles Homogeneous SMP (`Ryzen 5/7`, `7800X3D`, `9800X3D`, `Intel Core i5/i7 non-hybrid`, `Steam Deck APU`).
pub fn classify_topology_from_observations(
    obs: &[CoreObservation],
    vendor: &str,
    model: &str,
) -> CpuTopologyInfo {
    let total_threads = obs.len().max(1);
    let max_freq = obs.iter().map(|o| o.max_freq_mhz).max().unwrap_or(0);
    let min_freq = obs.iter().map(|o| o.max_freq_mhz).min().unwrap_or(0);
    let max_l3_kb = obs.iter().map(|o| o.l3_cache_kb).max().unwrap_or(0);
    let min_l3_kb = obs.iter().map(|o| o.l3_cache_kb).min().unwrap_or(0);

    let mut p_core_cpus = Vec::new();
    let mut e_core_cpus = Vec::new();
    let mut lpe_core_cpus = Vec::new();

    // Case A: Asymmetric L3 3D V-Cache Dual-CCD (e.g. AMD Ryzen 9 7950X3D / 9950X3D: CCD0=96MB, CCD1=32MB)
    let has_asymmetric_l3_vcache =
        max_l3_kb >= 65536 && min_l3_kb > 0 && max_l3_kb >= min_l3_kb.saturating_mul(2);

    if has_asymmetric_l3_vcache {
        for o in obs {
            if o.l3_cache_kb == max_l3_kb {
                p_core_cpus.push(o.cpu_id);
            } else {
                e_core_cpus.push(o.cpu_id);
            }
        }
        let vcache_list = p_core_cpus
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let recommended_wine_topology = format!("{}:{}", p_core_cpus.len(), vcache_list);
        let topology_strategy = format!(
            "AMD Dual-CCD 3D V-Cache Asimetrik ({} V-Cache Thread @ {}MB L3 [CCD0] + {} Frekans Thread @ {}MB L3 [CCD1] -> Oyunlar CCD0 V-Cache'e Sabitlendi)",
            p_core_cpus.len(),
            max_l3_kb / 1024,
            e_core_cpus.len(),
            min_l3_kb / 1024
        );
        return CpuTopologyInfo {
            total_threads,
            is_hybrid: true,
            has_asymmetric_l3_vcache: true,
            p_core_cpus,
            e_core_cpus,
            lpe_core_cpus,
            max_freq_mhz: max_freq,
            min_core_max_freq_mhz: min_freq,
            max_l3_kb,
            min_l3_kb,
            recommended_wine_topology,
            topology_strategy,
        };
    }

    // Case B: Hybrid Frequency / L3-less SoC Island Architecture
    let is_freq_hybrid = max_freq.saturating_sub(min_freq) >= 450;
    let has_l3less_cores = max_l3_kb > 0 && min_l3_kb == 0;
    let is_hybrid = is_freq_hybrid || has_l3less_cores;

    if is_hybrid {
        for o in obs {
            // Any core lacking L3 cache when the CPU has an L3 cache is a SoC-Tile LP-E island core
            if max_l3_kb > 0 && o.l3_cache_kb == 0 {
                lpe_core_cpus.push(o.cpu_id);
            } else if o.max_freq_mhz + 400 >= max_freq {
                p_core_cpus.push(o.cpu_id);
            } else if max_l3_kb == 0
                && o.max_freq_mhz + 1500 < max_freq
                && o.max_freq_mhz <= min_freq + 100
                && max_freq.saturating_sub(min_freq) >= 1800
            {
                // Fallback only if sysfs cache info is completely unavailable on a 3-tier chip
                lpe_core_cpus.push(o.cpu_id);
            } else {
                e_core_cpus.push(o.cpu_id);
            }
        }
    } else {
        for o in obs {
            p_core_cpus.push(o.cpu_id);
        }
    }

    let mut active_game_cpus = p_core_cpus.clone();
    active_game_cpus.extend(e_core_cpus.iter().copied());
    if active_game_cpus.is_empty() {
        active_game_cpus = (0..total_threads).collect();
    }

    let list_str = active_game_cpus
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let recommended_wine_topology = format!("{}:{}", active_game_cpus.len(), list_str);

    let topology_strategy = if !lpe_core_cpus.is_empty() {
        format!(
            "Intel Hybrid 3-Tier ({} P-Thread @ {}MHz + {} E-Core + {} L3-Önbelleksiz LP-E İzole Edildi -> LP-E Oyun Dışı)",
            p_core_cpus.len(),
            max_freq,
            e_core_cpus.len(),
            lpe_core_cpus.len()
        )
    } else if is_hybrid {
        format!(
            "Hybrid 2-Tier P+E Mimari ({} P-Thread @ {}MHz + {} E/c-Core @ {}MHz)",
            p_core_cpus.len(),
            max_freq,
            e_core_cpus.len(),
            min_freq
        )
    } else if vendor.contains("AMD") && model.to_lowercase().contains("x3d") {
        format!(
            "AMD Tek-CCD 3D V-Cache Homojen ({} Thread @ {}MB L3)",
            total_threads,
            max_l3_kb / 1024
        )
    } else {
        format!(
            "Homojen Çok Çekirdek SMP ({} İş Parçacığı @ {}MHz)",
            total_threads, max_freq
        )
    };

    CpuTopologyInfo {
        total_threads,
        is_hybrid,
        has_asymmetric_l3_vcache: false,
        p_core_cpus,
        e_core_cpus,
        lpe_core_cpus,
        max_freq_mhz: max_freq,
        min_core_max_freq_mhz: min_freq,
        max_l3_kb,
        min_l3_kb,
        recommended_wine_topology,
        topology_strategy,
    }
}

fn detect_gpus() -> Vec<GpuDevice> {
    let mut gpus = Vec::new();
    let pci_root = Path::new("/sys/bus/pci/devices");
    if let Ok(entries) = fs::read_dir(pci_root) {
        let mut dev_paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        dev_paths.sort();
        for dev in dev_paths {
            let cls = securefs::read_bounded_file(&dev.join("class"), 32, false)
                .map(|s| s.trim().to_lowercase())
                .unwrap_or_default();
            if !cls.starts_with("0x03") {
                continue;
            }
            let ven = securefs::read_bounded_file(&dev.join("vendor"), 32, false)
                .map(|s| s.trim().trim_start_matches("0x").to_lowercase())
                .unwrap_or_default();
            let did = securefs::read_bounded_file(&dev.join("device"), 32, false)
                .map(|s| s.trim().trim_start_matches("0x").to_lowercase())
                .unwrap_or_default();
            let bdf = dev
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "0000:00:00.0".into());
            let real_dev = fs::canonicalize(&dev)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let label = securefs::read_bounded_file(&dev.join("label"), 128, false)
                .map(|s| s.trim().to_lowercase())
                .unwrap_or_default();
            let link_width =
                securefs::read_bounded_file(&dev.join("current_link_width"), 32, false)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
            let vram_bytes =
                securefs::read_bounded_file(&dev.join("mem_info_vram_total"), 64, false)
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);

            let is_igpu = label.contains("onboard")
                || label.contains("integrated")
                || bdf.ends_with(":00:02.0")
                || bdf.ends_with(":00:02.1")
                || link_width == "0"
                || real_dev.contains("/0000:00:08.");

            let is_discrete = if ven == "10de" || cls.starts_with("0x0302") {
                true
            } else if is_igpu {
                false
            } else if vram_bytes >= 4_294_967_296 {
                true
            } else if vram_bytes > 0 {
                false
            } else {
                cls.starts_with("0x0380") || !bdf.contains(":00:")
            };

            let (vendor_name, vulkan_icd) = match ven.as_str() {
                "10de" => (
                    "NVIDIA".to_string(),
                    "/usr/share/vulkan/icd.d/nvidia_icd.json".to_string(),
                ),
                "1002" => (
                    "AMD".to_string(),
                    "/usr/share/vulkan/icd.d/radeon_icd.x86_64.json".to_string(),
                ),
                "8086" => (
                    "Intel".to_string(),
                    "/usr/share/vulkan/icd.d/intel_icd.x86_64.json".to_string(),
                ),
                _ => ("Generic".to_string(), String::new()),
            };

            let model_name = format!(
                "{} {} [{}:{}]",
                vendor_name,
                if is_discrete { "dGPU" } else { "iGPU" },
                ven,
                did
            );

            gpus.push(GpuDevice {
                pci_slot: bdf,
                vendor_id: ven,
                device_id: did,
                vendor_name,
                model_name,
                is_discrete,
                vulkan_icd,
            });
        }
    }

    // Enrich human-readable model names from lspci if available
    let deadline = Instant::now() + Duration::from_millis(600);
    if let Some(out) = subproc::run_cmd_bounded("/usr/bin/lspci", &["-nn"], deadline, 65536) {
        for line in out.lines() {
            let (ven, did) = extract_pci_ids(line);
            if let Some(gpu) = gpus
                .iter_mut()
                .find(|g| g.vendor_id == ven && g.device_id == did)
            {
                if let Some(desc) = line.splitn(3, ':').nth(2) {
                    gpu.model_name = desc.trim().to_string();
                }
            }
        }
    }
    gpus
}

fn extract_pci_ids(line: &str) -> (String, String) {
    for part in line.split('[') {
        if let Some(bracket_content) = part.split(']').next() {
            if bracket_content.len() == 9 && bracket_content.as_bytes()[4] == b':' {
                let v = &bracket_content[0..4];
                let d = &bracket_content[5..9];
                if v.chars().all(|c| c.is_ascii_hexdigit())
                    && d.chars().all(|c| c.is_ascii_hexdigit())
                {
                    return (v.to_lowercase(), d.to_lowercase());
                }
            }
        }
    }
    ("0000".into(), "0000".into())
}

fn detect_ram_mb() -> u64 {
    let raw = securefs::read_bounded_file(Path::new("/proc/meminfo"), 16384, false)
        .unwrap_or_default();
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            if let Some(kb_str) = rest.split_whitespace().next() {
                if let Ok(kb) = kb_str.parse::<u64>() {
                    return kb / 1024;
                }
            }
        }
    }
    16384
}

fn detect_ac_power() -> bool {
    let entries = match fs::read_dir("/sys/class/power_supply") {
        Ok(e) => e,
        Err(_) => return true,
    };
    let mut has_battery = false;
    let mut battery_discharging = false;
    for entry in entries.flatten() {
        let p = entry.path();
        let supply_type = securefs::read_bounded_file(&p.join("type"), 32, false)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        match supply_type.as_str() {
            "Mains" | "USB" => {
                if let Some(v) = securefs::read_bounded_file(&p.join("online"), 16, false) {
                    if v.trim() == "1" {
                        return true;
                    }
                }
            }
            "Battery" => {
                has_battery = true;
                if let Some(st) = securefs::read_bounded_file(&p.join("status"), 32, false) {
                    if st.trim().eq_ignore_ascii_case("Discharging") {
                        battery_discharging = true;
                    }
                }
            }
            _ => {}
        }
    }
    !(has_battery && battery_discharging)
}

fn parse_cpuinfo() -> (String, String, usize, Vec<String>) {
    let raw = securefs::read_bounded_file(Path::new("/proc/cpuinfo"), 524_288, false)
        .unwrap_or_default();
    let mut model = String::from("Unknown x86_64 Processor");
    let mut vendor = String::from("Generic");
    let mut threads = 0usize;
    let mut flags = Vec::new();

    for line in raw.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim();
            let val = v.trim();
            match key {
                "processor" => threads += 1,
                "model name" if model == "Unknown x86_64 Processor" => model = val.to_string(),
                "vendor_id" if vendor == "Generic" => vendor = val.to_string(),
                "flags" if flags.is_empty() => {
                    flags = val.split_whitespace().map(|s| s.to_string()).collect();
                }
                _ => {}
            }
        }
    }
    if threads == 0 {
        threads = 1;
    }
    (model, vendor, threads, flags)
}

pub fn classify_isa(vendor: &str, model: &str, flags: &[String]) -> (String, String) {
    let has = |f: &str| flags.iter().any(|x| x == f);
    let v3 = has("avx") && has("avx2") && has("bmi1") && has("bmi2") && has("fma");
    let v4 = (v3 && has("avx512f") && has("avx512bw")) || has("avx512vl");

    let is_amd = vendor.contains("AMD") || model.to_lowercase().contains("ryzen");
    if is_amd && v4 {
        if model.contains("9950")
            || model.contains("9900")
            || model.contains("9800")
            || model.contains("9700")
            || model.contains("9600")
            || model.contains("HX 370")
        {
            return ("x86-64-v4 (AMD Zen 5)".into(), "znver5".into());
        }
        return ("x86-64-v4 (AMD Zen 4)".into(), "znver4".into());
    }
    if is_amd && v3 {
        return ("x86-64-v3 (AMD Zen 2/3)".into(), "znver3".into());
    }
    if v4 {
        return ("x86-64-v4 (AVX-512)".into(), "x86-64-v4".into());
    }
    if v3 && has("avx_vnni") {
        return (
            "x86-64-v3 + AVX-VNNI (Intel Core Ultra / Alder-Meteor-Arrow Lake)".into(),
            "alderlake".into(),
        );
    }
    if v3 {
        return ("x86-64-v3 (AVX2 + FMA + BMI2)".into(), "x86-64-v3".into());
    }
    ("x86-64-v2 (Baseline SSE4.2)".into(), "x86-64-v2".into())
}

fn detect_tool_version(bin: &str, args: &[&str]) -> String {
    if !Path::new(bin).exists() {
        return "Not installed".into();
    }
    let deadline = Instant::now() + Duration::from_millis(800);
    subproc::run_cmd_bounded(bin, args, deadline, 4096)
        .and_then(|out| out.lines().next().map(|l| l.trim().to_string()))
        .unwrap_or_else(|| "Installed".into())
}

pub fn discover_steam_libraries(steam_root: &Path) -> Vec<PathBuf> {
    let mut libs = vec![steam_root.to_path_buf()];
    let vdf_path = steam_root.join("steamapps/libraryfolders.vdf");
    if let Some(raw) = securefs::read_bounded_file(&vdf_path, 65536, false) {
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("\"path\"") {
                let parts: Vec<&str> = trimmed.split('"').collect();
                if parts.len() >= 4 {
                    let unescaped = parts[3].replace("\\\\", "\\");
                    let p = PathBuf::from(unescaped);
                    if !libs.contains(&p) {
                        libs.push(p);
                    }
                }
            }
        }
    }
    libs
}

fn detect_steam_environment() -> (String, String, bool, bool, String) {
    let home = match std::env::var("HOME") {
        Ok(h) if h.starts_with('/') && !h.contains("..") => PathBuf::from(h),
        _ => return (String::new(), String::new(), false, false, String::new()),
    };

    let candidates = [
        home.join(".local/share/Steam"),
        home.join(".steam/root"),
        home.join(".steam/steam"),
    ];

    let mut steam_root = PathBuf::new();
    for c in &candidates {
        if c.exists() {
            steam_root = c.clone();
            break;
        }
    }

    if steam_root.as_os_str().is_empty() {
        steam_root = home.join(".local/share/Steam");
    }

    let libs = discover_steam_libraries(&steam_root);
    let mut base_proton_path = String::new();
    let mut sniper_runtime_present = false;

    for name in &["Proton - Experimental", "Proton Hotfix"] {
        for lib in &libs {
            let candidate = lib.join("steamapps/common").join(name);
            if candidate.join("proton").exists() {
                base_proton_path = candidate.to_string_lossy().to_string();
                break;
            }
        }
        if !base_proton_path.is_empty() {
            break;
        }
    }

    for lib in &libs {
        let common = lib.join("steamapps/common");
        if common.join("SteamLinuxRuntime_sniper").exists()
            || common.join("SteamLinuxRuntime_4").exists()
        {
            sniper_runtime_present = true;
        }
    }

    let compat_d = steam_root.join("compatibilitytools.d");
    let native_dir = compat_d.join("Proton-Omarchy");
    let bleeding_dir = compat_d.join("Proton-Omarchy-BleedingEdge");
    let (omaprot_installed, omaprot_dir) =
        if native_dir.join("compatibilitytool.vdf").exists() && native_dir.join("proton").exists()
        {
            (true, native_dir)
        } else {
            (
                bleeding_dir.join("compatibilitytool.vdf").exists()
                    && bleeding_dir.join("proton").exists(),
                bleeding_dir,
            )
        };

    (
        steam_root.to_string_lossy().to_string(),
        base_proton_path,
        sniper_runtime_present,
        omaprot_installed,
        omaprot_dir.to_string_lossy().to_string(),
    )
}
