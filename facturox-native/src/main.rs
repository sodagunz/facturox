//! `facturox-native` is also the native app.

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    facturox_native::run()?;
    Ok(())
}
