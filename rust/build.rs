use std::process::Command;
use std::path::Path;

fn main() {
    let font_path = "fonts/Inter-Regular.ttf";
    
    if !Path::new(font_path).exists() {
        println!("cargo:warning=Font not found, running get-fonts.sh...");
        let status = Command::new("bash")
            .arg("get-fonts.sh")
            .status()
            .expect("Failed to run get-fonts.sh");
        
        if !status.success() {
            panic!("get-fonts.sh failed");
        }
    }

    // Re-run this build script if the font disappears
    println!("cargo:rerun-if-changed=fonts/Inter-Regular.ttf");
    println!("cargo:rerun-if-changed=get-fonts.sh");
}
