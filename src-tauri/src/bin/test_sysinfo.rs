use sysinfo::{System, ProcessesToUpdate};
fn main() {
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    for (pid, process) in system.processes() {
        let name = process.name().to_string_lossy();
        let cmd = process.cmd().iter().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
        if cmd.to_lowercase().contains("wine") || name.to_lowercase().contains("wine") {
            println!("PID: {}, Name: {}, CMD: {}", pid, name, cmd);
        }
    }
}
