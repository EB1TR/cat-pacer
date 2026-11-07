use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if env::var("CARGO_CFG_TARGET_OS")? != "windows" {
        return Ok(());
    }

    // Un solo icono, en tamaños nativos de Windows: 16, 32 y 48 px.
    // El EXE, la ventana y la bandeja cargan este mismo recurso.
    let sizes = [16_u32, 32, 48];
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    let mut images = Vec::new();

    for size in sizes {
        let pixel_bytes = size * size * 4;
        let mask_bytes = size.div_ceil(32) * 4 * size;
        let image_bytes = 40 + pixel_bytes + mask_bytes;
        ico.extend_from_slice(&[size as u8, size as u8, 0, 0]);
        ico.extend_from_slice(&1_u16.to_le_bytes());
        ico.extend_from_slice(&32_u16.to_le_bytes());
        ico.extend_from_slice(&image_bytes.to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += image_bytes;

        let mut image = Vec::with_capacity(image_bytes as usize);
        image.extend_from_slice(&40_u32.to_le_bytes());
        image.extend_from_slice(&(size as i32).to_le_bytes());
        image.extend_from_slice(&((size * 2) as i32).to_le_bytes());
        image.extend_from_slice(&1_u16.to_le_bytes());
        image.extend_from_slice(&32_u16.to_le_bytes());
        image.extend_from_slice(&0_u32.to_le_bytes());
        image.extend_from_slice(&pixel_bytes.to_le_bytes());
        image.extend_from_slice(&[0; 16]);

        // DIB usa filas de abajo arriba y color BGRA.
        for y in (0..size).rev() {
            for x in 0..size {
                let x = (x * 32 / size) as i32;
                let y = (y * 32 / size) as i32;
                let trace = match x {
                    5..=10 => 18,
                    11..=13 => 18 - (x - 10) * 3,
                    14..=17 => 9 + (x - 13) * 4,
                    18..=21 => 25 - (x - 17) * 3,
                    _ => 13,
                };
                let color = if (5..=26).contains(&x) && (y - trace).abs() <= 1 {
                    [0x7b, 0xfa, 0x50, 255]
                } else {
                    [0x36, 0x2a, 0x28, 255]
                };
                image.extend_from_slice(&color);
            }
        }
        image.resize(image_bytes as usize, 0);
        images.push(image);
    }
    for image in images {
        ico.extend_from_slice(&image);
    }

    let path =
        PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR no disponible")?).join("cat-pacer.ico");
    fs::write(&path, ico)?;
    winresource::WindowsResource::new()
        .set_icon(path.to_str().ok_or("ruta de icono no valida")?)
        .set("ProductName", "CAT Pacer")
        .set("FileDescription", "CAT Pacer")
        .set("OriginalFilename", "cat-pacer.exe")
        .compile()?;
    Ok(())
}
