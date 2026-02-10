use anyhow::Result;
use std::process::{Command, Stdio};

use crate::lima::ssh;
use crate::state::{BrowserSession, State};
use crate::utils;

fn build_script(browser: &str, proxy_port: u16, url: &str) -> String {
    let browser = match browser {
        "chrome" | "chromium" => "chromium",
        "firefox" => "firefox",
        other => other,
    };

    format!(
        r#"
const {{ {} }} = require('playwright');

(async () => {{
  const browser = await {}.launch({{
    headless: false,
    proxy: {{ server: 'socks5://127.0.0.1:{}' }}
  }});
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto('{}');
  console.log('Playwright launched with SOCKS5 proxy. Press Ctrl+C to exit.');
  await new Promise(() => {{}});
}})().catch((err) => {{
  console.error(err);
  process.exit(1);
}});
"#,
        browser, browser, proxy_port, url
    )
}

pub fn execute(name: Option<&str>, browser: &str, url: &str) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let mut state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?.clone();
    let name = instance.name.as_str();

    let proxy = match instance.active_proxy.clone() {
        Some(p) => p,
        None => {
            anyhow::bail!(
                "No active SOCKS5 proxy for instance '{}'. Run 'fracta browser proxy {}' first.",
                name,
                name
            );
        }
    };

    if !ssh::is_process_alive(proxy.pid) {
        state.remove_proxy(name)?;
        state.save(&main_repo)?;
        anyhow::bail!(
            "SOCKS5 proxy for '{}' is not running. Run 'fracta browser proxy {}' again.",
            name,
            name
        );
    }

    if browser != "chrome" && browser != "chromium" && browser != "firefox" {
        anyhow::bail!("Unsupported browser '{}'. Use chrome or firefox.", browser);
    }

    if let Some(active) = instance.active_browser {
        if ssh::is_process_alive(active.pid) {
            anyhow::bail!(
                "Playwright is already running for '{}' (PID {}). Run 'fracta browser close {}' first.",
                name,
                active.pid,
                name
            );
        }
        state.remove_browser(name)?;
    }

    let script = build_script(browser, proxy.local_port, url);

    let child = Command::new("node")
        .args(["-e", &script])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to launch node. Is Node.js installed? ({})", e))?;

    let pid = child.id();
    let session = BrowserSession {
        browser: browser.to_string(),
        url: url.to_string(),
        pid,
    };
    state.add_browser(name, session)?;
    state.save(&main_repo)?;

    println!("Playwright started (PID {}).", pid);

    Ok(())
}
