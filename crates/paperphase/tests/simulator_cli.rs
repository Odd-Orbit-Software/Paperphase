use std::{
    path::Path,
    process::{Command, Output},
};

use serde_json::{Value, json};

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_paperphase"))
}

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/simulator")
        .join(name)
}

fn successful(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn simulator_exports_both_formats_and_reports_source_freshness() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("frame.png");
    let pbm = temp.path().join("frame.pbm");
    let report = successful(
        command()
            .arg("simulate")
            .arg("--layout")
            .arg(fixture("layout.json"))
            .arg("--data")
            .arg(fixture("data.json"))
            .args(["--now", "1700000000"])
            .arg("--output")
            .arg(&output)
            .arg("--pbm")
            .arg(&pbm)
            .output()
            .unwrap(),
    );
    assert_eq!(report["display_status"], "simulated");
    assert_eq!(report["packed_bytes"], 48000);
    let components = report["components"].as_array().unwrap();
    assert_eq!(
        components.iter().find(|c| c["id"] == "luas").unwrap()["state"],
        "stale"
    );
    assert!(
        components
            .iter()
            .all(|c| c["warnings"].as_array().unwrap().is_empty())
    );
    assert!(
        std::fs::read(output)
            .unwrap()
            .starts_with(b"\x89PNG\r\n\x1a\n")
    );
    let packed = std::fs::read(pbm).unwrap();
    assert!(packed.starts_with(b"P4\n800 480\n"));
    assert_eq!(packed.len(), 48000 + b"P4\n800 480\n".len());
}

#[test]
fn invalid_layout_does_not_replace_an_existing_frame() {
    let temp = tempfile::tempdir().unwrap();
    let mut layout: Value =
        serde_json::from_slice(&std::fs::read(fixture("layout.json")).unwrap()).unwrap();
    layout["components"][1]["rect"] = layout["components"][0]["rect"].clone();
    let input = temp.path().join("invalid.json");
    let output = temp.path().join("preserved.png");
    std::fs::write(&input, serde_json::to_vec(&layout).unwrap()).unwrap();
    std::fs::write(&output, b"previous frame").unwrap();
    let result = command()
        .arg("simulate")
        .arg("--layout")
        .arg(input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("overlaps"));
    assert_eq!(std::fs::read(output).unwrap(), b"previous frame");
}

#[test]
fn conflicting_output_paths_are_rejected_before_writing() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("frame.png");
    std::fs::write(&output, b"previous frame").unwrap();
    let result = command()
        .arg("simulate")
        .arg("--layout")
        .arg(fixture("layout.json"))
        .arg("--output")
        .arg(&output)
        .arg("--pbm")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(output).unwrap(), b"previous frame");
}

#[test]
fn aliases_cannot_overwrite_inputs_or_share_an_output() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("layout.json");
    let original = std::fs::read(fixture("layout.json")).unwrap();
    std::fs::write(&input, &original).unwrap();
    let alias = temp.path().join(".").join("layout.json");
    let result = command()
        .arg("simulate")
        .arg("--layout")
        .arg(&input)
        .arg("--output")
        .arg(&alias)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(&input).unwrap(), original);
    let other = temp.path().join("linked.json");
    std::fs::hard_link(&input, &other).unwrap();
    let result = command()
        .arg("simulate")
        .arg("--layout")
        .arg(&input)
        .arg("--output")
        .arg(other)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read(input).unwrap(), original);
    let output = temp.path().join("frame.png");
    let pbm_alias = temp.path().join(".").join("frame.png");
    let result = command()
        .arg("simulate")
        .arg("--layout")
        .arg(fixture("layout.json"))
        .arg("--output")
        .arg(&output)
        .arg("--pbm")
        .arg(pbm_alias)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!output.exists());
}

#[test]
#[ignore = "build the Rust guest and set PAPERPHASE_TEST_PLUGIN; scripts/check.sh does this"]
fn compiled_plugin_data_can_be_bound_to_a_simulated_component() {
    let plugin = std::env::var("PAPERPHASE_TEST_PLUGIN").expect("set PAPERPHASE_TEST_PLUGIN");
    let fetched = successful(
        command()
            .arg("demo")
            .arg("--plugin")
            .arg(plugin)
            .output()
            .unwrap(),
    );
    let temp = tempfile::tempdir().unwrap();
    let dataset = temp.path().join("snapshots.json");
    std::fs::write(&dataset,serde_json::to_vec(&json!({"schema_version":1,"snapshots":[{
        "connection_id":"weather.dublin","last_success_at":1700000000,"stale_after_secs":60,"dataset":fetched["dataset"]
    }]})).unwrap()).unwrap();
    let mut layout: Value =
        serde_json::from_slice(&std::fs::read(fixture("layout.json")).unwrap()).unwrap();
    layout["components"]
        .as_array_mut()
        .unwrap()
        .retain(|c| c["id"] == "weather");
    layout["components"][0]["blocks"]
        .as_array_mut()
        .unwrap()
        .retain(|b| b["id"] != "summary");
    let input = temp.path().join("layout.json");
    std::fs::write(&input, serde_json::to_vec(&layout).unwrap()).unwrap();
    let output = temp.path().join("frame.png");
    let result = successful(
        command()
            .arg("simulate")
            .arg("--layout")
            .arg(input)
            .arg("--data")
            .arg(dataset)
            .args(["--now", "1700000000"])
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap(),
    );
    assert_eq!(result["components"][0]["state"], "fresh");
    assert_eq!(result["components"][0]["warnings"], json!([]));
    assert!(std::fs::metadata(output).unwrap().len() > 100);
}
