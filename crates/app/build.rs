use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let ui = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR")).join("ui");

    // (Cargo feature, library name used in laptop.slint, real shell, stub)
    let shells = [
        ("UI_CLASSIC", "shell-classic", "tv/shell.slint", "stubs/classic.slint"),
        ("UI_BLADES", "shell-blades", "v3/shell.slint", "stubs/blades.slint"),
        ("UI_PS5", "shell-ps5", "ps5/shell.slint", "stubs/ps5.slint"),
        ("UI_ORBIT", "shell-orbit", "orbit/shell.slint", "stubs/orbit.slint"),
    ];

    let mut any = false;
    let mut library_paths = HashMap::new();
    for (feature, lib, real, stub) in shells {
        let on = std::env::var_os(format!("CARGO_FEATURE_{feature}")).is_some();
        any |= on;
        library_paths.insert(lib.to_string(), ui.join(if on { real } else { stub }));
    }
    assert!(
        any,
        "tvdash needs at least one dashboard UI feature (ui-classic, ui-blades, ui-ps5, ui-orbit)"
    );

    let config = slint_build::CompilerConfiguration::new().with_library_paths(library_paths);
    slint_build::compile_with_config("ui/app.slint", config).expect("failed to compile Slint UI");
}
