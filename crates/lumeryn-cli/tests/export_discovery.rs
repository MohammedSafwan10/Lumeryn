use serde_json::Value;
use std::{path::Path, process::Command};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn cli(args: &[&str]) -> TestResult<Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .args(args)
        .env_remove("RUST_LOG")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout)?;
    Ok(envelope["result"].clone())
}
#[test]
fn export_schema_and_capabilities_are_actual_shared_options() -> TestResult {
    let schema = cli(&["schema", "--kind", "render"])?;
    assert_eq!(schema["title"], "RenderOptions");
    assert!(schema["properties"].get("bitrate_kbps").is_some());
    assert!(schema.to_string().contains("uhd2160"));
    assert!(schema.to_string().contains("h265"));
    let batch = cli(&["schema"])?;
    assert_eq!(batch["title"], "Batch");
    let capabilities = cli(&["capabilities"])?;
    assert_eq!(capabilities["export_options"]["watermark"], false);
    assert_eq!(
        capabilities["export_options"]["subscription_required"],
        false
    );
    assert_eq!(
        capabilities["export_options"]["uhd_portrait"],
        serde_json::json!([2160, 3840])
    );
    Ok(())
}
#[test]
fn local_asset_catalog_can_be_enumerated_in_bounded_pages() -> TestResult {
    let library = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/library");
    let library = library.to_str().ok_or("non-UTF8 test path")?;
    let mut offset = 0;
    let mut ids = std::collections::HashSet::new();
    loop {
        let offset_arg = offset.to_string();
        let page = cli(&[
            "assets",
            "list",
            "--library",
            library,
            "--offset",
            &offset_arg,
            "--limit",
            "128",
        ])?;
        let assets = page["assets"].as_array().ok_or("assets array missing")?;
        assert!(assets.len() <= 128);
        for asset in assets {
            assert!(ids.insert(asset["id"].as_str().ok_or("asset ID missing")?.to_owned()));
        }
        let total = page["total"].as_u64().ok_or("catalog total missing")?;
        match page["next_offset"].as_u64() {
            Some(next) => {
                assert!(next > offset);
                offset = next;
            }
            None => {
                assert_eq!(ids.len() as u64, total);
                break;
            }
        }
    }
    assert!(
        ids.len() > 128,
        "fixture exercises more than the first page"
    );
    let output = Command::new(env!("CARGO_BIN_EXE_lumeryn"))
        .args(["assets", "list", "--library", library, "--offset", "10001"])
        .output()?;
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(error["error"]["code"], "invalid_input");
    Ok(())
}
