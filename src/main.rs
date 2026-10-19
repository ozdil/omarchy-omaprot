mod autotune;
mod benchmark;
mod hardware;
mod installer;
mod profiles;
mod securefs;
pub mod subproc;

use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmaProtonReport {
    pub active_profile_id: String,
    pub active_profile: profiles::OmaProtonProfile,
    pub available_profiles: Vec<profiles::OmaProtonProfile>,
    pub system: hardware::ProtonSystemProbe,
    pub autotune_plan: autotune::AutotunePlan,
    pub compiler_recipe: profiles::CompilerRecipe,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Live self-optimization hook called by `Proton-Omarchy-BleedingEdge/proton` on every game start
    if args.iter().any(|a| a == "--emit-runtime-env") {
        let report = collect_report();
        if let Some(state_file) =
            securefs::xdg_path("XDG_STATE_HOME", ".local/state", "last-launch.json")
        {
            if let Ok(bytes) = serde_json::to_vec_pretty(&report.autotune_plan) {
                let _ = securefs::write_owned_file_atomic(&state_file, &bytes);
            }
        }
        print!("{}", autotune::render_shell_exports(&report.autotune_plan));
        return;
    }

    // Handle --verify-matrix (Universal 8-PC Hardware Archetype Adaptation Proof)
    if args.iter().any(|a| a == "--verify-matrix") {
        let matrix = autotune::verify_universal_pc_matrix();
        if args.iter().any(|a| a == "--json") {
            println!("{}", serde_json::to_string_pretty(&matrix).unwrap_or_default());
        } else {
            print_universal_matrix(&matrix);
        }
        return;
    }

    // Handle --benchmark-ab (Fullstack Empirical A/B Hardware & Frame-Barrier Benchmark)
    if args.iter().any(|a| a == "--benchmark-ab") {
        let probe = hardware::probe_system();
        let ab = benchmark::run_ab_benchmarks(&probe);
        if args.iter().any(|a| a == "--json") {
            println!("{}", serde_json::to_string_pretty(&ab).unwrap_or_default());
        } else {
            print_ab_benchmark(&probe, &ab);
        }
        return;
    }

    // Handle --print-ai-prompt [game_name] for `omarchy-agent-proton` bridge
    if let Some(pos) = args.iter().position(|a| a == "--print-ai-prompt") {
        let target_game = args.get(pos + 1).map(|s| s.as_str());
        let report = collect_report();
        let skill_path = resolve_skill_path();
        let prompt = autotune::build_omarchy_agent_prompt(
            &report.system,
            &report.autotune_plan,
            target_game,
            &skill_path,
        );
        print!("{}", prompt);
        return;
    }

    // Handle --apply-ai-env "<rationale>" KEY=VAL [KEY2=VAL2 ...]
    if let Some(pos) = args.iter().position(|a| a == "--apply-ai-env") {
        let rationale = args
            .get(pos + 1)
            .cloned()
            .unwrap_or_else(|| "Varsayılan Omarchy YZ ajanı tarafından optimize edildi.".into());
        let mut pairs = Vec::new();
        for kv in args.iter().skip(pos + 2) {
            if let Some((k, v)) = kv.split_once('=') {
                let key = k.trim();
                let val = v.trim();
                if !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    pairs.push((key.to_string(), val.to_string()));
                }
            }
        }
        let probe = hardware::probe_system();
        let ok = autotune::save_ai_overrides(&probe.default_ai_agent_label, &rationale, &pairs);
        let rep = collect_report();
        let _ = installer::install_steam_compatibility_tool(
            &rep.system,
            &rep.active_profile,
            &rep.autotune_plan,
        );
        let out = serde_json::json!({
            "success": ok,
            "agent": rep.system.default_ai_agent_label,
            "applied_count": pairs.len(),
            "rationale": rationale,
            "message": format!("{} tarafından üretilen {} adet Proton optimizasyonu Steam katmanına kaydedildi.", rep.system.default_ai_agent_label, pairs.len())
        });
        println!("{}", serde_json::to_string(&out).unwrap_or_default());
        return;
    }

    // Handle --set-profile <id>
    if let Some(pos) = args.iter().position(|a| a == "--set-profile") {
        if let Some(target_id) = args.get(pos + 1) {
            if profiles::save_active_profile_id(target_id) {
                let rep = collect_report();
                let _ = installer::install_steam_compatibility_tool(
                    &rep.system,
                    &rep.active_profile,
                    &rep.autotune_plan,
                );
                let out = serde_json::json!({
                    "success": true,
                    "active_profile": target_id,
                    "message": format!("OmaProton profili '{}' olarak güncellendi ve donanım otonom ayarları Steam katmanına uygulandı.", target_id)
                });
                println!("{}", serde_json::to_string(&out).unwrap_or_default());
                return;
            }
        }
        let out = serde_json::json!({
            "success": false,
            "message": "Geçersiz profil kimliği. Seçenekler: bleeding-edge, wayland-hdr, esports-bore, stability-safe"
        });
        println!("{}", serde_json::to_string(&out).unwrap_or_default());
        return;
    }

    // Handle --install-steam-tool
    if args.iter().any(|a| a == "--install-steam-tool") {
        let report = collect_report();
        match installer::install_steam_compatibility_tool(
            &report.system,
            &report.active_profile,
            &report.autotune_plan,
        ) {
            Ok(path) => {
                let out = serde_json::json!({
                    "success": true,
                    "install_path": path,
                    "profile": report.active_profile_id,
                    "default_ai_agent": report.system.default_ai_agent_label,
                    "autotune": report.autotune_plan.summary_headline,
                    "message": "Proton-Omarchy-BleedingEdge otonom uyumluluk katmanı Steam compatibilitytools.d dizinine kuruldu."
                });
                println!("{}", serde_json::to_string(&out).unwrap_or_default());
            }
            Err(err) => {
                let out = serde_json::json!({
                    "success": false,
                    "message": err
                });
                println!("{}", serde_json::to_string(&out).unwrap_or_default());
            }
        }
        return;
    }

    // Handle --generate-pkgbuild
    if args.iter().any(|a| a == "--generate-pkgbuild") {
        let report = collect_report();
        let target_dir = securefs::xdg_path("XDG_DATA_HOME", ".local/share", "build-recipe")
            .unwrap_or_else(|| PathBuf::from("/tmp/omaprot-recipe"));
        match installer::generate_source_build_recipe(
            &report.system,
            &report.compiler_recipe,
            &target_dir,
        ) {
            Ok(dir) => {
                let out = serde_json::json!({
                    "success": true,
                    "recipe_dir": dir,
                    "march": report.compiler_recipe.march_flag,
                    "message": "Omarchy özel Proton kaynak derleme reçetesi (PKGBUILD.proton-omarchy + ntsync/sysctl) üretildi."
                });
                println!("{}", serde_json::to_string(&out).unwrap_or_default());
            }
            Err(err) => {
                let out = serde_json::json!({
                    "success": false,
                    "message": err
                });
                println!("{}", serde_json::to_string(&out).unwrap_or_default());
            }
        }
        return;
    }

    let report = collect_report();

    if args.iter().any(|a| a == "--json" || a == "--rescan") {
        if let Ok(s) = serde_json::to_string(&report) {
            if s.len() <= 65536 {
                println!("{}", s);
            }
        }
        return;
    }

    if args.iter().any(|a| a == "--status") {
        print_bar_status(&report);
        return;
    }

    print_terminal_summary(&report);
}

fn print_universal_matrix(entries: &[autotune::MatrixVerificationEntry]) {
    println!("================================================================================");
    println!("  OMAPROTON 10.0 EVRENSEL PC ADAPTASYON MATRİSİ (8 FARKLI DONANIM MİMARİSİ)");
    println!("================================================================================");
    for e in entries {
        println!("  [{}]", e.archetype_name);
        println!("    CPU & Topoloji : {}", e.cpu_summary);
        println!("    GPU Yığını     : {}", e.gpu_summary);
        println!(
            "    Derleyici & OS : {} | taskset -c {} | PROTON_CPU_TOPOLOGY={}",
            e.march_flag, e.taskset_cpus, e.proton_cpu_topology
        );
        println!("    Otonom Karar   : {}", e.gpu_routing_summary);
        println!("--------------------------------------------------------------------------------");
    }
}

fn print_ab_benchmark(probe: &hardware::ProtonSystemProbe, ab: &benchmark::AbBenchmarkReport) {
    println!("================================================================================");
    println!("  OMAPROTON 10.0 FULLSTACK A/B PERFORMANS VE ÇERÇEVE BARİYERİ TEST RAPORU");
    println!("================================================================================");
    println!(
        "  Donanım   : {} ({} iş parçacığı) | Çekirdek: {}",
        probe.cpu_model, probe.cpu_threads, probe.kernel_release
    );
    println!("  Topoloji  : {}", probe.cpu_topology.topology_strategy);
    println!("--------------------------------------------------------------------------------");
    for m in [
        &ab.baseline_a,
        &ab.omaprot_compute_tile,
        &ab.omaprot_pcore_esports,
    ] {
        println!(
            "  * {}\n    Ortalama Kare Süresi: {:.3} ms ({:.1} FPS) | %99 Kuyruk (1% Low): {:.3} ms ({:.1} FPS)",
            m.label, m.avg_frame_ms, m.avg_fps, m.p99_frame_ms, m.low_1pct_fps
        );
    }
    println!("--------------------------------------------------------------------------------");
    println!(
        "  NET KAZANIM (Grup B2 vs Grup A): Ortalama FPS: {:+.1}% | 1% Low FPS: {:+.1}%",
        ab.avg_fps_gain_pct, ab.low_1pct_fps_gain_pct
    );
    println!(
        "  Çekirdek Split-Lock Durumu     : kernel.split_lock_mitigate = {} {}",
        ab.split_lock_mitigate_sysctl,
        if ab.split_lock_mitigate_sysctl == "1" {
            "(UYARI: Açık! 28.000x atomik ceza yaratır -> `sudo sysctl -w kernel.split_lock_mitigate=0` önerilir)"
        } else {
            "(Optimum: Kapalı)"
        }
    );
    println!(
        "  OmaProton Başlangıç Ek Yükü    : {:.3} ms (Sıfır çalışma zamanı darboğazı)",
        ab.runtime_hook_overhead_ms
    );
    println!("================================================================================");
}

fn resolve_skill_path() -> String {
    let candidates = [
        "/usr/share/omarchy/default/agents/skills/optimize-proton/SKILL.md",
        "/usr/share/omarchy/plugins/ozdil.omaprot/skills/optimize-proton/SKILL.md",
        "/home/ozdil/Projects/omarchy/omarchy-omaprot/skills/optimize-proton/SKILL.md",
    ];
    for c in &candidates {
        if std::path::Path::new(c).exists() {
            return c.to_string();
        }
    }
    "/usr/share/omarchy/plugins/ozdil.omaprot/skills/optimize-proton/SKILL.md".into()
}

fn collect_report() -> OmaProtonReport {
    let system = hardware::probe_system();
    let available_profiles = profiles::all_profiles(&system);
    let active_id = profiles::load_active_profile_id();
    let active_profile = available_profiles
        .iter()
        .find(|p| p.id == active_id)
        .cloned()
        .unwrap_or_else(|| available_profiles[0].clone());
    let autotune_plan = autotune::compute_autotune_plan(&system, &active_profile);
    let compiler_recipe = profiles::build_compiler_recipe(&system);

    OmaProtonReport {
        active_profile_id: active_profile.id.clone(),
        active_profile,
        available_profiles,
        system,
        autotune_plan,
        compiler_recipe,
    }
}

fn print_bar_status(rep: &OmaProtonReport) {
    let ntsync_badge = if rep.system.ntsync_dev_active {
        "NTSYNC:ON"
    } else if rep.system.ntsync_module_present {
        "NTSYNC:MOD"
    } else {
        "FSYNC"
    };
    let gpu_badge = if rep.system.has_nvidia_dgpu && rep.system.is_hybrid_gpu {
        "PRIME:RTX"
    } else if rep.system.has_nvidia_dgpu {
        "NV:RTX"
    } else if rep.system.has_amd_gpu {
        "RADV"
    } else {
        "ANV:ARC"
    };
    let text = format!(
        "{} OmaProton [{} · {} · AI:{}]",
        rep.active_profile.nerd_icon,
        rep.system.march_target,
        gpu_badge,
        rep.system.default_ai_agent
    );
    let tooltip = format!(
        "OmaProton 10.0 Auto-Adaptive ({}/100)\n• Profil: {}\n• Omarchy YZ Ajanı: {}\n• Çekirdek: {} (BORE: {}, I/O: {})\n• CPU Topoloji: {}\n• GPU Hattı: {} ({})\n• Mimari Hedef: {} (-march={})\n• Steam Katmanı: {}\n\n[Sol Tık] OmaProton Kontrol Merkezini Aç",
        rep.system.readiness_score,
        rep.active_profile.name,
        rep.system.default_ai_agent_label,
        rep.system.kernel_release,
        if rep.system.bore_scheduler_active { "Aktif" } else { "Pasif" },
        rep.system.io_scheduler,
        rep.system.cpu_topology.topology_strategy,
        gpu_badge,
        ntsync_badge,
        rep.system.isa_level,
        rep.system.march_target,
        if rep.system.omaprot_installed { "Kurulu (Hazır)" } else { "Kurulmadı" }
    );

    let status = serde_json::json!({
        "text": text,
        "tooltip": tooltip,
        "score": rep.system.readiness_score,
        "profile": rep.active_profile_id,
        "march": rep.system.march_target,
        "gpu_mode": gpu_badge,
        "ai_agent": rep.system.default_ai_agent,
        "installed": rep.system.omaprot_installed,
        "color": rep.active_profile.badge_color,
        "icon": rep.active_profile.nerd_icon,
    });
    println!("{}", serde_json::to_string(&status).unwrap_or_default());
}

fn print_terminal_summary(rep: &OmaProtonReport) {
    println!("================================================================================");
    println!("  OMAPROTON 10.0 AUTO-ADAPTIVE • KERNEL + CPU + GPU + OMARCHY AI AGENT ENGINE");
    println!("================================================================================");
    println!(
        "  0. OMARCHY YZ AJANI  : {} (~/.config/omarchy/defaults/agent)",
        rep.system.default_ai_agent_label
    );
    println!(
        "  1. ÇEKİRDEK (KERNEL) : {} (BORE: {}, I/O: {}, NTSYNC Modül: {})",
        rep.system.kernel_release,
        if rep.system.bore_scheduler_active { "Aktif" } else { "Pasif" },
        rep.system.io_scheduler,
        if rep.system.ntsync_module_present { "Mevcut" } else { "Yok" }
    );
    println!(
        "  2. İŞLEMCİ (CPU)     : {} ({} iş parçacığı)",
        rep.system.cpu_model, rep.system.cpu_threads
    );
    println!(
        "     Topoloji Strateji : {}",
        rep.system.cpu_topology.topology_strategy
    );
    println!(
        "     PROTON_CPU_TOPOL. : {} (OS taskset: {})",
        rep.system.cpu_topology.recommended_wine_topology, rep.autotune_plan.taskset_cpu_list
    );
    println!(
        "     Komut Seti (ISA)  : {} -> Derleyici Hedefi: {}",
        rep.system.isa_level, rep.compiler_recipe.march_flag
    );
    println!(
        "  3. EKRAN KARTI (GPU) : {} adet GPU tespit edildi (Hibrit PRIME: {})",
        rep.system.gpus.len(),
        if rep.system.is_hybrid_gpu { "Evet" } else { "Hayır" }
    );
    for (idx, g) in rep.system.gpus.iter().enumerate() {
        println!(
            "     GPU #{} [{}]     : {} ({}) -> ICD: {}",
            idx + 1,
            g.pci_slot,
            g.model_name,
            if g.is_discrete { "Harici dGPU" } else { "Dahili iGPU" },
            g.vulkan_icd
        );
    }
    println!(
        "  4. SİSTEM & GÜÇ      : {} MB RAM | Güç: {} | Görüntü: {}",
        rep.system.ram_total_mb,
        if rep.system.on_ac_power { "AC Adaptör (Tam Performans)" } else { "Batarya" },
        if rep.system.hyprland_active { "Hyprland / Wayland" } else { "Wayland / X11" }
    );
    println!("--------------------------------------------------------------------------------");
    println!("  OTONOM KARAR MOTORU (AUTOTUNE & AI DECISIONS):");
    for d in &rep.autotune_plan.decisions {
        println!("  * [{}]", d.subsystem);
        println!("    Algılanan : {}", d.detected_hardware);
        println!("    Uygulanan : {}", d.optimization_applied);
    }
    println!("--------------------------------------------------------------------------------");
    println!(
        "  Steam Katmanı     : {} ({})",
        if rep.system.omaprot_installed { "KURULU" } else { "KURULMADI" },
        rep.system.omaprot_install_path
    );
    println!(
        "  Aktif Profil      : {} ({}) | Skor: {}/100",
        rep.active_profile.name, rep.active_profile_id, rep.system.readiness_score
    );
    println!("================================================================================");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_report_and_autotune() {
        let rep = collect_report();
        assert!(!rep.available_profiles.is_empty());
        assert!(!rep.autotune_plan.decisions.is_empty());
        assert!(!rep.autotune_plan.merged_env.is_empty());
        let exports = autotune::render_shell_exports(&rep.autotune_plan);
        assert!(exports.contains("unset PROTON_NO_NTSYNC"));
        assert!(exports.contains("export PROTON_CPU_TOPOLOGY="));
    }

    #[test]
    fn test_universal_8_pc_matrix_adaptation() {
        let matrix = autotune::verify_universal_pc_matrix();
        assert_eq!(matrix.len(), 8);
        // 1. Intel 3-Tier Hybrid Laptop on AC -> isolates LP-E (0..19) and selects NVIDIA PRIME
        assert_eq!(matrix[0].proton_cpu_topology, "20:0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19");
        assert!(matrix[0].gpu_routing_summary.contains("VK_LOADER_DRIVERS_SELECT=*nvidia*"));
        // 2. Same Laptop on Battery -> switches to Intel Arc iGPU
        assert!(matrix[1].gpu_routing_summary.contains("*intel*"));
        // 3. AMD Ryzen 9 9950X3D Dual-CCD -> pins strictly to 16 threads of 96MB V-Cache CCD0 (0..15)
        assert_eq!(matrix[2].march_flag, "-march=znver5");
        assert_eq!(matrix[2].proton_cpu_topology, "16:0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15");
        assert!(matrix[2].gpu_routing_summary.contains("RADV"));
        // 4. AMD Ryzen 7 7800X3D Single-CCD + RTX 4090 -> uses all 16 threads + NVIDIA
        assert_eq!(matrix[3].march_flag, "-march=znver4");
        assert_eq!(matrix[3].proton_cpu_topology, "16:0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15");
        assert!(matrix[3].gpu_routing_summary.contains("PROTON_FORCE_NVAPI=1"));
        // 6. AMD SmartShift Hybrid Laptop -> selects discrete Radeon RX 7700S via MESA_VK_DEVICE_SELECT=1002:7480!
        assert!(matrix[5].gpu_routing_summary.contains("MESA_VK_DEVICE_SELECT=1002:7480!"));
        // 7. Intel Core i5-12400F + Intel Arc B580 -> Homogeneous 12T + ANV_GPL=true
        assert_eq!(matrix[6].proton_cpu_topology, "12:0,1,2,3,4,5,6,7,8,9,10,11");
        assert!(matrix[6].gpu_routing_summary.contains("ANV_GPL=true"));
    }
}
