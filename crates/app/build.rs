//! Build script for ChronoBase: compiles Slint UI files and embeds Windows resources.

fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    slint_build::compile_with_config("ui/main.slint", config).expect("Slint UI compilation failed");

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/chronobase.rc");
        println!("cargo:rerun-if-changed=assets/icon.ico");
        // Embed Windows .ico resource when rc.exe is available on MSVC.
        if std::path::Path::new("assets/chronobase.rc").exists()
            && std::path::Path::new("assets/icon.ico").exists()
            && let Ok(out_dir) = std::env::var("OUT_DIR")
        {
            let res_path = std::path::Path::new(&out_dir).join("chronobase.res");
            if let Ok(status) = std::process::Command::new("rc.exe")
                .args(["/nologo", "/fo"])
                .arg(&res_path)
                .arg("assets/chronobase.rc")
                .status()
                && status.success()
            {
                println!("cargo:rustc-link-arg={}", res_path.display());
            }
        }
    }
}
