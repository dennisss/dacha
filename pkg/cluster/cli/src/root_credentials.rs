use common::errors::*;
use file::{LocalPathBuf, LocalPath};

pub fn get_root_credentials_dir(zone: &str, user_arg: &Option<LocalPathBuf>) -> Result<LocalPathBuf> {
    let path = {
        if let Some(path) = user_arg.clone() {
            path.clone()
        } else {
            let home = std::env::var("HOME")?;
            LocalPath::new(&home).join(".dacha/zone").join(zone).join("root")
        }
    };
    
    println!("Root Credentials Dir: {}", path.as_str());
    Ok(path)
}