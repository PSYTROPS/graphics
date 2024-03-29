use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::string::String;

//Compile shader source code to SPIR-V
fn compile_shaders() {
    let shader_dir = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("shaders");
    let output_dir = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("spv");
    if !output_dir.exists() {
        std::fs::create_dir(output_dir.clone()).unwrap();
    }
    let compiler = shaderc::Compiler::new().unwrap();
    for entry in shader_dir.read_dir().unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            let filename = entry.path();
            if let Some(extension) = filename.extension() {
                if let Some(extension) = extension.to_str() {
                    let shader_type = match extension {
                        "vert" => shaderc::ShaderKind::Vertex,
                        "frag" => shaderc::ShaderKind::Fragment,
                        "comp" => shaderc::ShaderKind::Compute,
                        _ => shaderc::ShaderKind::InferFromSource
                    };
                    //Read shader source
                    let mut source_file = File::open(filename.clone())
                        .expect("Error opening shader source file!");
                    let mut source_text = String::new();
                    source_file.read_to_string(&mut source_text)
                        .expect("Error reading from shader source file!");
                    //Compile shader
                    let name = filename.file_name().unwrap().to_str().unwrap();
                    let module = compiler.compile_into_spirv(
                        &source_text,
                        shader_type,
                        name,
                        "main",
                        None
                    ).expect("Error compiling shader!");
                    //Write to target
                    let mut target = output_dir.clone();
                    target.push(String::from(name) + ".spv");
                    std::fs::write(target, module.as_binary_u8())
                        .expect("Error writing shader binary!");
                }
            }
        }
    }
}

//Create symlink to assets directory
#[cfg(unix)]
fn copy_assets() -> std::io::Result<()> {
    let original = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("assets");
    let link = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("target")
        .join(&std::env::var("PROFILE").unwrap())
        .join("assets");
    if !link.exists() {
        std::os::unix::fs::symlink(original, link)?;
    }
    Ok(())
}

//Create symlink to assets directory
#[cfg(windows)]
fn copy_assets() -> std::io::Result<()> {
    let original = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("assets");
    let link = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("target")
        .join(&std::env::var("PROFILE").unwrap())
        .join("assets");
    if !link.exists() {
        std::os::windows::fs::symlink_dir(original, link)?;
    }
    Ok(())
}

fn main() {
    //println!("cargo:rerun-if-changed=shaders/*");
    compile_shaders();
    copy_assets().unwrap();
}
