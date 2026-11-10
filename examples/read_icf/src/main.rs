fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: read_icf <settings.icf>")?;
    let source = std::fs::read(path)?;
    let file = cs705::icf::Icf::parse(&source)?;
    println!("Verified {} memory bytes", file.memory().len());
    Ok(())
}
