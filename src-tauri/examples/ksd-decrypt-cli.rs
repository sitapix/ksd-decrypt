//! Developer/test CLI using exactly the application's streaming recovery engine.
use ksd_decrypt_lib::recovery::{inspect, recover_one};
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(
        args.next()
            .ok_or("Usage: ksd-decrypt-cli OUTPUT_DIR FILE...")?,
    );
    std::fs::create_dir_all(&output)?;
    let mut failed = false;
    for (i, path) in args.enumerate() {
        let input = inspect(path.into(), i.to_string());
        match recover_one(&input, &output, &AtomicBool::new(false), |_, _| {}) {
            Ok(result) => println!("{}", serde_json::to_string(&result)?),
            Err(e) => {
                eprintln!("{}: {e}", input.name);
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}
