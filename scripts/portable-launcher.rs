#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() -> std::io::Result<()> {
    let executable = std::env::current_exe()?;
    let root = executable.parent().ok_or_else(|| std::io::Error::other("Portable folder unavailable"))?;
    let mut command = std::process::Command::new(root.join("App/Flux/flux-desktop.exe"));
    command.args(std::env::args_os().skip(1)).current_dir(root);
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn()?;
    Ok(())
}
