use memory_rs::external::process::Process;
use simple_injector::inject_dll;
use std::env::current_exe;

fn main() {
    println!("Waiting for the process to start");
    let p = loop {
        if let Ok(p) = Process::new("Cemu.exe") {
            break p;
        }

        std::thread::sleep(std::time::Duration::from_secs(5));
    };
    println!("Game found");

    let mut path = current_exe().unwrap();
    path.pop();
    let path_string = path.to_string_lossy();

    let dll_path = format!("{}/botw_vr.dll", path_string).to_string();

    inject_dll(&p, &dll_path);
}
