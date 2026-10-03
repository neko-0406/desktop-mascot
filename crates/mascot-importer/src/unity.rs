use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

pub struct UnityDetector;

impl UnityDetector {
    /// Detects the path to Unity.exe based on explicit argument, ProjectVersion.txt, or Unity Hub.
    pub fn find_unity_exe(
        custom_exe: Option<&Path>,
        unity_project: Option<&Path>,
    ) -> Result<PathBuf> {
        if let Some(exe) = custom_exe {
            if exe.exists() {
                return Ok(exe.to_path_buf());
            }
            bail!("Specified Unity executable not found: {}", exe.display());
        }

        // Check ProjectSettings/ProjectVersion.txt for exact version
        let mut target_version = None;
        if let Some(proj) = unity_project {
            let version_file = proj.join("ProjectSettings").join("ProjectVersion.txt");
            if version_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&version_file) {
                    for line in content.lines() {
                        if line.starts_with("m_EditorVersion:") {
                            let ver = line.trim_start_matches("m_EditorVersion:").trim();
                            target_version = Some(ver.to_string());
                            break;
                        }
                    }
                }
            }
        }

        let hub_base = PathBuf::from(r"C:\Program Files\Unity\Hub\Editor");

        if let Some(ref ver) = target_version {
            let ver_path = hub_base.join(ver).join("Editor").join("Unity.exe");
            if ver_path.exists() {
                return Ok(ver_path);
            }
        }

        // Fallback: look for any installed Unity 2022.3 or other versions in Unity Hub
        if hub_base.exists() {
            if let Ok(entries) = std::fs::read_dir(&hub_base) {
                let mut candidates = Vec::new();
                for entry in entries.flatten() {
                    let exe = entry.path().join("Editor").join("Unity.exe");
                    if exe.exists() {
                        candidates.push(exe);
                    }
                }

                // Prefer 2022.3 LTS if present
                if let Some(v2022) = candidates.iter().find(|p| p.to_string_lossy().contains("2022.3")) {
                    return Ok(v2022.clone());
                }

                if let Some(first) = candidates.into_iter().next() {
                    return Ok(first);
                }
            }
        }

        // Fallback to UNITY_PATH environment variable
        if let Ok(env_path) = std::env::var("UNITY_PATH") {
            let p = PathBuf::from(env_path);
            if p.exists() {
                return Ok(p);
            }
        }

        bail!("Could not automatically locate Unity.exe. Please specify via --unity-exe <PATH> or set UNITY_PATH environment variable.");
    }

    /// Spawns Unity batchmode to execute the DmaExporter.ExportCommandLine method.
    pub fn run_export(
        unity_exe: &Path,
        project_path: &Path,
        avatar_name: Option<&str>,
        output_path: &Path,
    ) -> Result<()> {
        let mut cmd = Command::new(unity_exe);
        cmd.arg("-batchmode")
            .arg("-nographics")
            .arg("-quit")
            .arg("-projectPath")
            .arg(project_path)
            .arg("-executeMethod")
            .arg("DesktopMascot.Editor.DmaExporter.ExportCommandLine")
            .arg("-outputPath")
            .arg(output_path);

        if let Some(name) = avatar_name {
            cmd.arg("-avatarName").arg(name);
        }

        println!("[INFO] Invoking Unity Batchmode: {}", unity_exe.display());
        println!("[INFO] Project: {}", project_path.display());
        println!("[INFO] Output:  {}", output_path.display());

        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        let status = cmd
            .status()
            .with_context(|| format!("Failed to launch Unity process at {}", unity_exe.display()))?;

        if !status.success() {
            bail!("Unity export process exited with failure code: {:?}", status.code());
        }

        println!("[OK] Unity batchmode export completed successfully.");
        Ok(())
    }
}
