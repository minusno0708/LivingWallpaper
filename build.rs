use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icons/fish.ico");
    println!("cargo:rerun-if-changed=assets/icons/fish-tray.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let gnu = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu");
    let output = out.join(if gnu { "app-icon.o" } else { "app-icon.res" });
    let mut command;
    if gnu {
        command = Command::new(env::var_os("WINDRES").unwrap_or_else(|| "windres".into()));
        command
            .args(["-i", "assets/app.rc", "-O", "coff", "-o"])
            .arg(&output);
    } else {
        let mut compiler = PathBuf::from("rc.exe");
        // GitHub Actions and Visual Studio installations need not put SDK tools on PATH.
        if Command::new(&compiler).arg("/?").output().is_err() {
            let root =
                PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK required"))
                    .join("Windows Kits/10/bin");
            let mut versions: Vec<_> = std::fs::read_dir(root)
                .expect("Windows SDK required")
                .filter_map(Result::ok)
                .map(|e| e.path())
                .collect();
            versions.sort();
            compiler = versions
                .into_iter()
                .rev()
                .map(|p| p.join("x64/rc.exe"))
                .find(|p| p.is_file())
                .expect("Windows SDK resource compiler required");
        }
        command = Command::new(compiler);
        command
            .arg("/nologo")
            .arg("/fo")
            .arg(&output)
            .arg("assets/app.rc");
    }
    assert!(
        command
            .status()
            .expect("Cannot run Windows resource compiler")
            .success(),
        "Icon resource compilation failed"
    );
    println!("cargo:rustc-link-arg={}", output.display());
}
