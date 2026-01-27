use anyhow::Result;

use crate::state::State;
use crate::utils;

pub fn execute() -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;

    let mut any = false;
    for inst in &state.instances {
        if let Some(proxy) = &inst.active_proxy {
            any = true;
            println!(
                "{}\tlocalhost:{}\tPID:{}",
                inst.name, proxy.local_port, proxy.pid
            );
        }
    }

    if !any {
        println!("No active SOCKS5 proxies.");
    }

    Ok(())
}
