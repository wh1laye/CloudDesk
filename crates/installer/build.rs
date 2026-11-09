use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CLOUDDESK_PAYLOAD");
    let out = PathBuf::from(env::var("OUT_DIR")?).join("payload.bin");
    let data = match env::var("CLOUDDESK_PAYLOAD") {
        Ok(p) if !p.is_empty() => {
            println!("cargo:rerun-if-changed={p}");
            fs::read(&p)?
        }
        _ => Vec::new(),
    };
    fs::write(out, data)?;
    Ok(())
}
