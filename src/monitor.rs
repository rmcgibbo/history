use anyhow::Result;
use std::time::Duration;
use sysinfo::{ProcessExt, System, SystemExt};
use tracing::info;

pub async fn server_monitor_log_forever() -> Result<()> {
    let mut system = System::new();
    let pid = sysinfo::get_current_pid().expect("failed to get current pid");

    /*
    tokio::spawn(async {
    loop {
        // cpu
            let x = (0..1_000).into_iter().sum::<i128>();
        //println!("_ = {}", x);
    }
    });
    */

    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        system.refresh_process(pid);
        let proc = system
            .process(pid)
            .expect("Unable to load sysinfo. Process doesn't exist?");
        let disk = proc.disk_usage();

        info!(
            "[CPU]: {:.2}%, [Disk In]: {} KB, [Disk Out] {} KB, Mem: {} KB",
            proc.cpu_usage(),
            disk.read_bytes / 1_000,
            disk.written_bytes / 1_000,
            proc.memory(),
        );
    }
}
