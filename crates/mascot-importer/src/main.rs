mod unity;

use std::fs::File;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use mascot_format::DmaFile;
use unity::UnityDetector;

#[derive(Parser, Debug)]
#[command(
    name = "mascot-importer",
    about = "Desktop Mascot Avatar (.dma) Importer and Validator CLI",
    version = "0.1.0"
)]
struct Args {
    /// Path to Unity project root directory
    #[arg(short = 'p', long = "unity-project")]
    unity_project: Option<PathBuf>,

    /// Path to Unity.exe (auto-detected if omitted)
    #[arg(short = 'u', long = "unity-exe")]
    unity_exe: Option<PathBuf>,

    /// Specific avatar name or GameObject name to export
    #[arg(short = 'a', long = "avatar-name")]
    avatar_name: Option<String>,

    /// Destination path for generated .dma file
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    /// Existing .dma file path to inspect, validate, or dump
    #[arg(short = 'i', long = "input")]
    input: Option<PathBuf>,

    /// Validate .dma file structure and internal references
    #[arg(long = "validate", default_value_t = true)]
    validate: bool,

    /// Print comprehensive inspection dump of avatar chunks
    #[arg(short = 'd', long = "dump")]
    dump: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Determine target .dma file to inspect/validate
    let target_dma_path = if let Some(ref proj) = args.unity_project {
        let out_path = args.output.clone().unwrap_or_else(|| {
            let name = args.avatar_name.as_deref().unwrap_or("avatar");
            PathBuf::from(format!("{}.dma", name))
        });

        let unity_path = UnityDetector::find_unity_exe(
            args.unity_exe.as_deref(),
            Some(proj.as_path()),
        )?;

        UnityDetector::run_export(
            &unity_path,
            proj,
            args.avatar_name.as_deref(),
            &out_path,
        )?;

        out_path
    } else if let Some(ref inp) = args.input {
        inp.clone()
    } else if let Some(ref out) = args.output {
        out.clone()
    } else {
        eprintln!("[ERROR] Please provide either --unity-project <PATH> to export, or --input <FILE> to inspect.");
        std::process::exit(1);
    };

    println!("[INFO] Inspecting DMA file: {}", target_dma_path.display());

    let mut file = File::open(&target_dma_path)
        .with_context(|| format!("Failed to open DMA file at {}", target_dma_path.display()))?;

    let dma = DmaFile::from_reader(&mut file)
        .with_context(|| "Failed to parse DMA binary container")?;

    println!("[OK] Successfully parsed DMA binary container.");
    println!("     Total file size: {} bytes", dma.header.total_file_size);
    println!("     Active chunks:   {}", dma.toc.len());

    if args.validate {
        println!("[INFO] Validating avatar cross-references and geometry...");
        match dma.validate() {
            Ok(()) => println!("[OK] All validation checks passed cleanly!"),
            Err(e) => {
                eprintln!("[FAIL] Validation error encountered: {}", e);
                std::process::exit(2);
            }
        }
    }

    if args.dump {
        println!("\n{}", dma.dump_summary());
    } else {
        // Quick summary
        if let Some(ref meta) = dma.metadata {
            println!("     Avatar:   {}", meta.avatar_name);
        }
        if !dma.skeleton.is_empty() {
            println!("     Bones:    {}", dma.skeleton.len());
        }
        if let Some(ref mesh) = dma.mesh {
            println!("     Vertices: {}", mesh.vertices.len());
            println!("     Indices:  {}", mesh.indices.len());
        }
        if !dma.morph_targets.is_empty() {
            println!("     Morphs:   {}", dma.morph_targets.len());
        }
        if !dma.materials.is_empty() {
            println!("     Mats:     {}", dma.materials.len());
        }
        if !dma.phys_chains.is_empty() {
            println!("     PhysBone: {} chains", dma.phys_chains.len());
        }
    }

    Ok(())
}
