use anyhow::Result;

use crate::lima::client as lima;
use crate::state::State;
use crate::utils;

pub fn execute() -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;

    if state.instances.is_empty() {
        println!("No instances found.");
        return Ok(());
    }

    println!("=== Instances ===");
    println!(
        "{:<20} {:<25} {:<10} {:<10} {}",
        "NAME", "LIMA VM", "VM STATUS", "FORWARDS", "PATH"
    );
    println!("{}", "-".repeat(100));

    for inst in &state.instances {
        let vm_status = match lima::info(&inst.lima_instance) {
            Ok(status) => status.to_string(),
            Err(_) => "Unknown".to_string(),
        };

        let forwards_count = inst.active_forwards.len();
        let forwards_str = if forwards_count > 0 {
            forwards_count.to_string()
        } else {
            "-".to_string()
        };

        println!(
            "{:<20} {:<25} {:<10} {:<10} {}",
            inst.name, inst.lima_instance, vm_status, forwards_str, inst.path
        );
    }

    Ok(())
}
