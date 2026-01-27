use anyhow::Result;

use crate::lima::ssh;
use crate::state::State;
use crate::utils;

pub fn execute(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let mut state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?.clone();
    let name = instance.name.as_str();

    let session = match instance.active_browser.clone() {
        Some(s) => s,
        None => {
            println!("No active Playwright session for instance '{}'", name);
            return Ok(());
        }
    };

    println!("Stopping Playwright (PID {})...", session.pid);

    if ssh::is_process_alive(session.pid) {
        ssh::stop_forward(session.pid)?;
    } else {
        println!("Playwright process (PID {}) was already stopped.", session.pid);
    }

    state.remove_browser(name)?;
    state.save(&main_repo)?;

    println!("Playwright stopped successfully.");

    Ok(())
}
