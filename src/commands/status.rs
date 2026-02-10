use anyhow::Result;

use crate::commands;

pub fn execute(name: Option<&str>) -> Result<()> {
    commands::ps::execute(name)?;
    println!();
    commands::ports::execute(name, false)?;
    Ok(())
}
