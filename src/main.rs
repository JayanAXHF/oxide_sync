mod cli;
mod errors;
mod logging;
pub mod pipeline;

fn main() -> color_eyre::Result<()> {
    crate::errors::init()?;
    crate::logging::init()?;
    Ok(())
}
