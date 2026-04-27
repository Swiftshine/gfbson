use std::{env, fs};


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        return Err("Need to specify filename.".into());
    }

    let data = fs::read(&args[1])?;
    let bson = gfbson::read(&data)?;

    println!("{:#?}", bson);

    Ok(())
}
