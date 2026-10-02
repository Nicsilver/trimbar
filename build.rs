fn main() {
    // PerMonitorV2 so monitor rects and the reserved strip are in real physical pixels;
    // otherwise a scaled monitor would get a trim that doesn't match its dead rows.
    let manifest = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <asmv3:application xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
    <asmv3:windowsSettings>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </asmv3:windowsSettings>
  </asmv3:application>
</assembly>"#;

    let mut res = winresource::WindowsResource::new();
    // The tray loads this by id, so it must stay 1.
    res.set_icon_with_id("assets/trimbar.ico", "1");
    res.set_manifest(manifest);
    res.compile().expect("failed to compile Windows resources");
}
