use anyhow::Result;

use crate::commands;
use crate::lima::ssh;
use crate::state::State;
use crate::utils;

pub fn open(
    name: Option<&str>,
    browser: &str,
    url: &str,
    proxy_port: Option<u16>,
) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;
    let instance = state.resolve_instance(name)?.clone();
    let instance_name = instance.name;

    let has_live_proxy = instance
        .active_proxy
        .as_ref()
        .map(|p| ssh::is_process_alive(p.pid))
        .unwrap_or(false);

    if !has_live_proxy {
        commands::proxy::execute(Some(&instance_name), proxy_port)?;
    }

    commands::open::execute(Some(&instance_name), browser, url)
}

pub fn close(name: Option<&str>) -> Result<()> {
    commands::close::execute(name)
}

pub fn proxy(name: Option<&str>, port: Option<u16>) -> Result<()> {
    commands::proxy::execute(name, port)
}

pub fn unproxy(name: Option<&str>) -> Result<()> {
    commands::unproxy::execute(name)
}

pub fn status() -> Result<()> {
    commands::proxies::execute()
}
