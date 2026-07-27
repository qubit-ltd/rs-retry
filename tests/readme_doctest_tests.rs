// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! README Rust-example compilation tests.

#[cfg(feature = "tokio")]
mod readme_doctest {
    use std::io;
    use std::path::{
        Path,
        PathBuf,
    };
    use std::process::Command;

    /// Finds the newest compiled rlib for one crate in Cargo's dependency output.
    fn newest_rlib(deps_dir: &Path, crate_name: &str) -> io::Result<PathBuf> {
        let prefix = format!("lib{crate_name}-");
        std::fs::read_dir(deps_dir)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().is_some_and(|extension| extension == "rlib")
                    && path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
            })
            .max_by_key(|path| path.metadata().and_then(|metadata| metadata.modified()).ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("compiled rlib for {crate_name} was not found"),
                )
            })
    }

    /// Compiles every non-ignored Rust code fence in the English README.
    #[test]
    fn test_readme_rust_examples_compile() -> Result<(), Box<dyn std::error::Error>> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let readme = manifest_dir.join("README.md");
        let executable = std::env::current_exe()?;
        let deps_dir = executable
            .parent()
            .ok_or_else(|| io::Error::other("test executable has no parent directory"))?;
        let rustdoc = std::env::var_os("RUSTDOC").unwrap_or_else(|| "rustdoc".into());
        let crate_name = "qubit_retry";
        let mut command = Command::new(rustdoc);
        command
            .arg("--test")
            .arg(&readme)
            .arg("--edition")
            .arg("2024")
            .arg("-L")
            .arg(format!("dependency={}", deps_dir.display()))
            .arg("--extern")
            .arg(format!(
                "{crate_name}={}",
                newest_rlib(deps_dir, crate_name)?.display()
            ));
        let output = command.output()?;
        assert!(
            output.status.success(),
            "README Rust examples failed to compile:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        Ok(())
    }
}
