//! Build script: embeds a Windows version resource in the binary.
//!
//! Windows shows it under the executable's Properties → Details, and code
//! signing through `SignPath` Foundation requires every signed binary to carry
//! its product name and version there. The values come from `Cargo.toml`, so
//! they cannot drift from the package version. Other platforms have no such
//! resource, and the script does nothing there.

fn main() {
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set("ProductName", "altium-designer-mcp")
            .set(
                "FileDescription",
                "altium-designer-mcp — MCP server for Altium Designer libraries",
            )
            .set("CompanyName", "The Embedded Society")
            .set(
                "LegalCopyright",
                "Copyright (C) 2026 The Embedded Society. GPL-3.0-or-later.",
            );
        if let Err(e) = resource.compile() {
            // A missing resource compiler must fail the build loudly rather
            // than ship a release binary without its metadata.
            panic!("could not embed the Windows version resource: {e}");
        }
    }
}
