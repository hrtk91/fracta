use anyhow::Result;

use crate::lima::ssh;
use crate::state::State;
use crate::utils;

pub fn execute(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let mut state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?.clone();
    let name = instance.name.as_str();

    let proxy = match instance.active_proxy.clone() {
        Some(p) => p,
        None => {
            println!("No active SOCKS5 proxy for instance '{}'", name);
            return Ok(());
        }
    };

    println!("Stopping SOCKS5 proxy: localhost:{}", proxy.local_port);

    if ssh::is_process_alive(proxy.pid) {
        ssh::stop_forward(proxy.pid)?;
    } else {
        println!("Proxy process (PID {}) was already stopped.", proxy.pid);
    }

    state.remove_proxy(name)?;
    state.save(&main_repo)?;

    println!("SOCKS5 proxy stopped successfully.");

    Ok(())
}
