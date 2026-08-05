use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not available."),
    );

    let icon_path = manifest_dir.join("assets").join("app.ico");

    println!("cargo:rerun-if-changed={}", icon_path.display(),);

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    if !icon_path.is_file() {
        panic!("Application icon does not exist: {}", icon_path.display(),);
    }

    let icon_path = icon_path
        .to_str()
        .expect("The application icon path is not valid UTF-8.");

    let mut resource = winres::WindowsResource::new();

    resource
        .set_icon(icon_path)
        .set_language(0x0409)
        .set_manifest(
            r#"
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
    <assemblyIdentity
        version="1.0.0.0"
        processorArchitecture="*"
        name="Nightshade.NightshadeMp3"
        type="win32"
    />

    <description>Nightshade MP3</description>

    <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
        <security>
            <requestedPrivileges>
                <requestedExecutionLevel
                    level="asInvoker"
                    uiAccess="false"
                />
            </requestedPrivileges>
        </security>
    </trustInfo>

    <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
        <application>
            <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
        </application>
    </compatibility>

    <application xmlns="urn:schemas-microsoft-com:asm.v3">
        <windowsSettings>
            <dpiAwareness
                xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings"
            >PerMonitorV2</dpiAwareness>

            <longPathAware
                xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings"
            >true</longPathAware>
        </windowsSettings>
    </application>
</assembly>
"#,
        );

    resource
        .compile()
        .expect("Failed to compile Windows resources.");
}
