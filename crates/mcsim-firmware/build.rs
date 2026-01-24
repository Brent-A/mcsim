//! Build script for mcsim-firmware
//!
//! Compiles the MeshCore simulator DLLs (companion, repeater, room_server)
//! using clang compiler. Each DLL is isolated to prevent symbol conflicts
//! between firmware variants.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Find the clang compiler on the system.
/// On Windows, we look for clang-cl first (LLVM toolchain with MSVC compatibility),
/// then clang from LLVM installation or PATH.
fn find_clang() -> String {
    let candidates = if cfg!(windows) {
        vec![
            "clang-cl",
            "clang",
            // Common LLVM installation paths on Windows
            "C:\\Program Files\\LLVM\\bin\\clang-cl.exe",
            "C:\\Program Files\\LLVM\\bin\\clang.exe",
            "C:\\Program Files (x86)\\LLVM\\bin\\clang-cl.exe",
            "C:\\Program Files (x86)\\LLVM\\bin\\clang.exe",
        ]
    } else {
        vec!["clang++", "clang"]
    };

    for candidate in &candidates {
        let result = if cfg!(windows) {
            Command::new("where").arg(candidate).output()
        } else {
            Command::new("which").arg(candidate).output()
        };

        if let Ok(output) = result {
            if output.status.success() {
                return candidate.to_string();
            }
        }

        // Check if it's a direct path that exists
        if Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }

    // Fallback: just return "clang" and let it fail with a clear error
    println!("cargo:warning=Clang not found in standard locations, trying 'clang'");
    "clang".to_string()
}

/// Find the linker for creating DLLs
fn find_linker() -> String {
    if cfg!(windows) {
        // On Windows with MSVC toolchain, use lld-link or link.exe
        let candidates = vec![
            "lld-link",
            "C:\\Program Files\\LLVM\\bin\\lld-link.exe",
            "link.exe",
        ];

        for candidate in &candidates {
            let result = Command::new("where").arg(candidate).output();
            if let Ok(output) = result {
                if output.status.success() {
                    return candidate.to_string();
                }
            }
            if Path::new(candidate).exists() {
                return candidate.to_string();
            }
        }
        "link.exe".to_string()
    } else {
        "clang++".to_string()
    }
}

/// Check if we're using clang-cl (MSVC-compatible clang)
fn is_clang_cl(compiler: &str) -> bool {
    compiler.contains("clang-cl")
}

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    // Project root is two levels up from crates/mcsim-firmware
    let project_root = manifest_dir.parent().unwrap().parent().unwrap();

    // Key directories
    let simulator_dir = project_root.join("simulator");
    let meshcore_dir = project_root.join("MeshCore");
    let meshcore_src = meshcore_dir.join("src");
    let meshcore_lib = meshcore_dir.join("lib");
    let meshcore_examples = meshcore_dir.join("examples");
    let sim_common_dir = simulator_dir.join("common");

    // Find the clang compiler and linker
    let clang = find_clang();
    let linker = find_linker();

    // Tell Cargo to rerun if any source files change
    println!("cargo:rerun-if-changed={}", simulator_dir.display());
    println!("cargo:rerun-if-changed={}", meshcore_src.display());
    println!("cargo:rerun-if-changed={}", meshcore_lib.display());

    // Build each firmware DLL
    build_firmware_dll(
        "meshcore_repeater",
        &clang,
        &linker,
        &out_dir,
        &sim_common_dir,
        &meshcore_src,
        &meshcore_lib,
        &meshcore_examples.join("simple_repeater"),
        &simulator_dir.join("repeater"),
        &["MyMesh.cpp"], // Only MyMesh.cpp from example
    );

    build_firmware_dll(
        "meshcore_room_server",
        &clang,
        &linker,
        &out_dir,
        &sim_common_dir,
        &meshcore_src,
        &meshcore_lib,
        &meshcore_examples.join("simple_room_server"),
        &simulator_dir.join("room_server"),
        &["MyMesh.cpp"], // Only MyMesh.cpp from example
    );

    build_firmware_dll(
        "meshcore_companion",
        &clang,
        &linker,
        &out_dir,
        &sim_common_dir,
        &meshcore_src,
        &meshcore_lib,
        &meshcore_examples.join("companion_radio"),
        &simulator_dir.join("companion"),
        &["MyMesh.cpp", "DataStore.cpp"], // MyMesh.cpp and DataStore.cpp from example
    );

    // Output the library search path for Rust to find the DLLs
    println!("cargo:rustc-link-search=native={}", out_dir.display());
}

/// Compile a single source file to an object file
fn compile_source(
    clang: &str,
    source: &Path,
    output: &Path,
    includes: &[&Path],
    defines: &[(&str, Option<&str>)],
    prefix_header: Option<&Path>,
    is_cpp: bool,
) -> bool {
    let use_clang_cl = is_clang_cl(clang);
    let debug_build = env::var("PROFILE").map(|p| p == "debug").unwrap_or(false);

    let mut cmd = Command::new(clang);

    // Compile only, don't link
    if use_clang_cl {
        cmd.arg("/c");
        cmd.arg("/nologo");
        cmd.arg("/MD"); // Use dynamic CRT
        if debug_build {
            cmd.arg("/Od"); // Disable optimization for debugging
            cmd.arg("/Zi"); // Generate debug info (PDB)
        } else {
            cmd.arg("/O2");
        }
        cmd.arg("/W0"); // Disable warnings
        cmd.arg("/EHsc"); // Enable C++ exceptions
        if is_cpp {
            cmd.arg("/std:c++17");
            cmd.arg("/TP"); // Treat as C++
        }
    } else {
        cmd.arg("-c");
        if debug_build {
            cmd.arg("-O0"); // No optimization
            cmd.arg("-g");  // Debug symbols
        } else {
            cmd.arg("-O2");
        }
        cmd.arg("-w"); // Disable warnings
        cmd.arg("-fPIC");
        if is_cpp {
            cmd.arg("-std=c++17");
            cmd.arg("-x").arg("c++");
        }
    }

    // Include directories
    for inc in includes {
        if use_clang_cl {
            cmd.arg(format!("/I{}", inc.display()));
        } else {
            cmd.arg("-I").arg(inc);
        }
    }

    // Preprocessor definitions
    for (name, value) in defines {
        if use_clang_cl {
            if let Some(v) = value {
                cmd.arg(format!("/D{}={}", name, v));
            } else {
                cmd.arg(format!("/D{}", name));
            }
        } else if let Some(v) = value {
            cmd.arg(format!("-D{}={}", name, v));
        } else {
            cmd.arg(format!("-D{}", name));
        }
    }

    // Force include prefix header
    if let Some(prefix) = prefix_header {
        if use_clang_cl {
            cmd.arg(format!("/FI{}", prefix.display()));
        } else {
            cmd.arg("-include").arg(prefix);
        }
    }

    // Output file
    if use_clang_cl {
        cmd.arg(format!("/Fo{}", output.display()));
    } else {
        cmd.arg("-o").arg(output);
    }

    // Input file
    cmd.arg(source);

    let status = cmd.status();
    match status {
        Ok(s) if s.success() => true,
        Ok(s) => {
            println!(
                "cargo:warning=Compilation failed for {}: exit code {:?}",
                source.display(),
                s.code()
            );
            false
        }
        Err(e) => {
            println!(
                "cargo:warning=Failed to run compiler for {}: {}",
                source.display(),
                e
            );
            false
        }
    }
}

/// Link object files into a DLL
fn link_dll(linker: &str, objects: &[PathBuf], output: &Path, clang: &str) -> bool {
    let use_clang_cl = is_clang_cl(clang);
    let debug_build = env::var("PROFILE").map(|p| p == "debug").unwrap_or(false);

    if cfg!(windows) {
        // Use MSVC-style linking
        let mut cmd = Command::new(linker);

        cmd.arg("/DLL");
        cmd.arg("/NOLOGO");
        cmd.arg(format!("/OUT:{}", output.display()));

        // Generate debug info (PDB file) for debug builds
        if debug_build {
            cmd.arg("/DEBUG:FULL");
            // PDB will be named after the DLL (e.g., meshcore_repeater.pdb)
            let pdb_path = output.with_extension("pdb");
            cmd.arg(format!("/PDB:{}", pdb_path.display()));
        }

        // Link against required Windows libraries
        cmd.arg("kernel32.lib");
        cmd.arg("user32.lib");
        cmd.arg("ws2_32.lib");
        cmd.arg("advapi32.lib"); // For crypto functions (CryptAcquireContext, etc.)
        cmd.arg("msvcrt.lib");

        // Add all object files
        for obj in objects {
            cmd.arg(obj);
        }

        // If using lld-link, we might need additional flags
        if linker.contains("lld-link") {
            cmd.arg("/MACHINE:X64");
        }

        let status = cmd.status();
        match status {
            Ok(s) if s.success() => true,
            Ok(s) => {
                println!("cargo:warning=Linking failed: exit code {:?}", s.code());
                false
            }
            Err(e) => {
                println!("cargo:warning=Failed to run linker: {}", e);
                false
            }
        }
    } else {
        // Use clang to link on Unix
        let mut cmd = Command::new(if use_clang_cl { "clang++" } else { clang });

        cmd.arg("-shared");
        cmd.arg("-o").arg(output);

        for obj in objects {
            cmd.arg(obj);
        }

        // Link pthread on Unix
        cmd.arg("-lpthread");

        let status = cmd.status();
        match status {
            Ok(s) if s.success() => true,
            Ok(s) => {
                println!("cargo:warning=Linking failed: exit code {:?}", s.code());
                false
            }
            Err(e) => {
                println!("cargo:warning=Failed to run linker: {}", e);
                false
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_firmware_dll(
    name: &str,
    clang: &str,
    linker: &str,
    out_dir: &Path,
    sim_common_dir: &Path,
    meshcore_src: &Path,
    meshcore_lib: &Path,
    example_dir: &Path,
    node_dir: &Path,
    example_sources: &[&str],
) {
    let sim_include_dir = sim_common_dir.join("include");
    let ed25519_dir = meshcore_lib.join("ed25519");
    let prefix_header = sim_include_dir.join("sim_prefix.h");
    let helpers_dir = meshcore_src.join("helpers");

    // Create a subdirectory for this DLL's object files to avoid conflicts
    let obj_dir = out_dir.join(format!("{}_obj", name));
    fs::create_dir_all(&obj_dir).expect("Failed to create object directory");

    // Include directories (need to be owned PathBufs for the references)
    let includes: Vec<PathBuf> = vec![
        sim_include_dir.clone(),
        meshcore_src.to_path_buf(),
        helpers_dir.clone(),
        ed25519_dir.clone(),
        example_dir.to_path_buf(),
    ];
    let include_refs: Vec<&Path> = includes.iter().map(|p| p.as_path()).collect();

    // Preprocessor definitions
    let defines: Vec<(&str, Option<&str>)> = vec![
        ("SIM_BUILD", Some("1")),
        ("ARDUINO", Some("100")),
        ("SIM_PLATFORM", Some("1")),
        ("SIM_DLL_EXPORT", Some("1")),
        ("ESP32", Some("1")),
        ("_CRT_SECURE_NO_WARNINGS", None),
        ("WIN32", None),
        ("_WINDOWS", None),
    ];

    let mut objects: Vec<PathBuf> = Vec::new();

    // =========================================================================
    // Compile common source files (sim_common)
    // =========================================================================
    let sim_common_sources = [
        "Arduino.cpp",
        "sim_radio.cpp",
        "sim_board.cpp",
        "sim_clock.cpp",
        "sim_rng.cpp",
        "sim_serial.cpp",
        "sim_filesystem.cpp",
        "sim_node_base.cpp",
        "target.cpp",
    ];

    for src in &sim_common_sources {
        let source = sim_common_dir.join("src").join(src);
        let obj = obj_dir.join(format!("{}.obj", src.replace(".cpp", "")));
        if compile_source(
            clang,
            &source,
            &obj,
            &include_refs,
            &defines,
            Some(&prefix_header),
            true,
        ) {
            objects.push(obj);
        } else {
            panic!("Failed to compile {}", source.display());
        }
    }

    // =========================================================================
    // Compile MeshCore sources
    // =========================================================================
    let meshcore_sources = [
        "Dispatcher.cpp",
        "Identity.cpp",
        "Mesh.cpp",
        "Packet.cpp",
        "Utils.cpp",
    ];

    for src in &meshcore_sources {
        let source = meshcore_src.join(src);
        let obj = obj_dir.join(format!("mc_{}.obj", src.replace(".cpp", "")));
        if compile_source(
            clang,
            &source,
            &obj,
            &include_refs,
            &defines,
            Some(&prefix_header),
            true,
        ) {
            objects.push(obj);
        } else {
            panic!("Failed to compile {}", source.display());
        }
    }

    // MeshCore helpers
    let meshcore_helpers = [
        "BaseChatMesh.cpp",
        "IdentityStore.cpp",
        "StaticPoolPacketManager.cpp",
        "AdvertDataHelpers.cpp",
        "TxtDataHelpers.cpp",
        "CommonCLI.cpp",
        "ClientACL.cpp",
        "TransportKeyStore.cpp",
        "RegionMap.cpp",
        "ArduinoSerialInterface.cpp",
    ];

    for src in &meshcore_helpers {
        let source = helpers_dir.join(src);
        let obj = obj_dir.join(format!("mch_{}.obj", src.replace(".cpp", "")));
        if compile_source(
            clang,
            &source,
            &obj,
            &include_refs,
            &defines,
            Some(&prefix_header),
            true,
        ) {
            objects.push(obj);
        } else {
            panic!("Failed to compile {}", source.display());
        }
    }

    // =========================================================================
    // Compile Ed25519 crypto library (C files)
    // =========================================================================
    let ed25519_sources = [
        "fe.c",
        "ge.c",
        "sc.c",
        "sha512.c",
        "verify.c",
        "keypair.c",
        "sign.c",
        "seed.c",
        "add_scalar.c",
        "key_exchange.c",
    ];

    // Ed25519 only needs its own include dir and minimal defines
    let ed25519_includes: Vec<&Path> = vec![ed25519_dir.as_path()];
    let ed25519_defines: Vec<(&str, Option<&str>)> = vec![("SIM_BUILD", Some("1"))];

    for src in &ed25519_sources {
        let source = ed25519_dir.join(src);
        let obj = obj_dir.join(format!("ed_{}.obj", src.replace(".c", "")));
        if compile_source(
            clang,
            &source,
            &obj,
            &ed25519_includes,
            &ed25519_defines,
            None,
            false, // C, not C++
        ) {
            objects.push(obj);
        } else {
            panic!("Failed to compile {}", source.display());
        }
    }

    // =========================================================================
    // Compile node-specific sources
    // =========================================================================

    // Main entry point
    let sim_main = node_dir.join("sim_main.cpp");
    let sim_main_obj = obj_dir.join("sim_main.obj");
    if compile_source(
        clang,
        &sim_main,
        &sim_main_obj,
        &include_refs,
        &defines,
        Some(&prefix_header),
        true,
    ) {
        objects.push(sim_main_obj);
    } else {
        panic!("Failed to compile {}", sim_main.display());
    }

    // Example sources (explicitly listed)
    for src in example_sources {
        let source = example_dir.join(src);
        if source.exists() {
            let obj = obj_dir.join(format!("ex_{}.obj", src.replace(".cpp", "")));
            if compile_source(
                clang,
                &source,
                &obj,
                &include_refs,
                &defines,
                Some(&prefix_header),
                true,
            ) {
                objects.push(obj);
            } else {
                panic!("Failed to compile {}", source.display());
            }
        } else {
            println!("cargo:warning=Missing source file: {}", source.display());
        }
    }

    // =========================================================================
    // Link into DLL
    // =========================================================================
    let dll_name = if cfg!(windows) {
        format!("{}.dll", name)
    } else if cfg!(target_os = "macos") {
        format!("lib{}.dylib", name)
    } else {
        format!("lib{}.so", name)
    };

    let dll_path = out_dir.join(&dll_name);

    if link_dll(linker, &objects, &dll_path, clang) {
        // Copy DLL to target directory for easier access
        if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
            let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
            let target_dll = PathBuf::from(&target_dir).join(&profile).join(&dll_name);
            let _ = fs::copy(&dll_path, &target_dll);
        }
    } else {
        panic!("Failed to link DLL: {}", name);
    }
}
