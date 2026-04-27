use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        return Err("Need to specify filename.".into());
    }

    let data = fs::read(&args[1])?;
    let root = gfbson::read(&data)?;
    let json_output = gfbson::to_json(&root, true)?;
    fs::write(&args[2], json_output)?;
    // let bytes = gfbson::write(&root, 3)?;

    // fs::write(&args[2], bytes)?;

    Ok(())
}
