//! Collection strategy trait for app listing handlers

use std::collections::HashSet;
use std::path::PathBuf;

use serde_json::Value;
use serde_json::json;

use super::cargo_detector::BevyTarget;
use super::cargo_detector::BrpLevel;
use super::cargo_detector::CargoDetector;
use super::constants::BRP_LEVEL_FIELD;
use super::constants::BUILD_BUILT_FIELD;
use super::constants::BUILDS_FIELD;
use super::constants::KIND_FIELD;
use super::constants::NAME_FIELD;
use super::constants::PACKAGE_NAME_FIELD;
use super::constants::PATH_FIELD;
use super::constants::RELATIVE_PATH_FIELD;
use super::constants::WORKSPACE_ROOT_FIELD;
use super::scanning;
use crate::app_tools::constants::MANIFEST_PATH_FIELD;
use crate::app_tools::constants::PROFILE_DEBUG;
use crate::app_tools::constants::PROFILE_RELEASE;

/// Unified strategy for collecting all Bevy targets (apps and examples)
/// with `kind` and `brp_level` fields on each item.
///
/// Only targets declared in workspace `Cargo.toml` files are included (via `cargo metadata`).
pub(super) struct AllBevyTargetsStrategy;

/// A `BevyTarget` enriched with BRP status.
///
/// For examples and explicitly registered bins, `brp_level` checks the target source
/// file for recognized BRP plugin imports. Other bins also require package-level BRP detection.
pub(super) struct EnrichedTarget {
    pub(super) target: BevyTarget,
    pub(super) brp_level: BrpLevel,
}

impl AllBevyTargetsStrategy {
    pub(super) fn collect_items(detector: &CargoDetector) -> Vec<EnrichedTarget> {
        let all_targets = detector.find_bevy_targets();

        // Identify bins with package-level BRP evidence or explicit registration.
        let brp_targets = detector.find_brp_targets();
        let brp_keys: HashSet<String> = brp_targets
            .iter()
            .map(|t| format!("{}::{}", t.manifest.display(), t.name))
            .collect();

        all_targets
            .into_iter()
            .map(|target| {
                let brp_level = if target.is_app() {
                    let key = format!("{}::{}", target.manifest.display(), target.name);
                    if brp_keys.contains(&key) {
                        CargoDetector::file_brp_level(&target.source)
                    } else {
                        BrpLevel::None
                    }
                } else {
                    CargoDetector::file_brp_level(&target.source)
                };
                EnrichedTarget { target, brp_level }
            })
            .collect()
    }

    pub(super) fn create_unique_key(item: &EnrichedTarget) -> String {
        format!(
            "{}::{}::{}",
            item.target.manifest.display(),
            item.target.name,
            item.target.target_type.as_ref()
        )
    }

    pub(super) fn get_path_for_relative(item: &EnrichedTarget) -> PathBuf {
        item.target
            .manifest
            .parent()
            .unwrap_or(&item.target.manifest)
            .to_path_buf()
    }

    pub(super) fn serialize_item(item: &EnrichedTarget, relative_path: String) -> Value {
        json!({
            NAME_FIELD: item.target.name,
            KIND_FIELD: item.target.target_type.as_ref(),
            PACKAGE_NAME_FIELD: item.target.package_name,
            BRP_LEVEL_FIELD: item.brp_level.as_str(),
            WORKSPACE_ROOT_FIELD: item.target.workspace_root.display().to_string(),
            MANIFEST_PATH_FIELD: item.target.manifest.display().to_string(),
            // The relative_path field is designed for round-trip compatibility with launch functions.
            // This path can be used directly in `brp_launch`'s path parameter
            // to disambiguate between targets with the same name in different locations.
            RELATIVE_PATH_FIELD: relative_path,
            BUILDS_FIELD: create_builds_json(&item.target)
        })
    }
}

/// Collect all Bevy targets (apps and examples) with `kind` and `brp_enabled` fields
pub fn collect_all_bevy_targets(search_paths: &[PathBuf]) -> Vec<Value> {
    let mut all_items = Vec::new();
    let mut seen_items = HashSet::new();

    // Use the iterator to find all cargo projects
    for path in scanning::iter_cargo_project_paths(search_paths) {
        if let Ok(detector) = CargoDetector::try_from(path.as_path()) {
            let items = AllBevyTargetsStrategy::collect_items(&detector);
            for item in items {
                let key = AllBevyTargetsStrategy::create_unique_key(&item);
                if seen_items.insert(key) {
                    let item_path = AllBevyTargetsStrategy::get_path_for_relative(&item);
                    let relative_path = scanning::compute_relative_path(&item_path, search_paths);

                    let serialized_item = AllBevyTargetsStrategy::serialize_item(
                        &item,
                        relative_path.display().to_string(),
                    );
                    all_items.push(serialized_item);
                }
            }
        }
    }

    all_items
}

/// Helper function to create builds JSON for binary items
fn create_builds_json(item: &BevyTarget) -> Value {
    let profiles = vec![PROFILE_DEBUG, PROFILE_RELEASE];
    let mut builds = json!({});
    for profile in &profiles {
        let binary_path = item.get_binary_path(profile);
        builds[profile] = json!({
            PATH_FIELD: binary_path.display().to_string(),
            BUILD_BUILT_FIELD: binary_path.exists()
        });
    }
    builds
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use cargo_metadata::MetadataCommand;

    use super::CargoDetector;
    use super::collect_all_bevy_targets;

    #[test]
    fn lists_only_the_explicitly_registered_application_binary() {
        let root = tempfile::tempdir().expect("temporary Cargo package");
        fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = 'wrapper'\nversion = '0.1.0'\nedition = '2024'\n[package.metadata.bevy_brp]\napps = ['launcher']\n[dependencies]\nbevy_brp_extras = { path = 'extras' }\n",
        )
        .expect("package manifest");
        let extras = root.path().join("extras");
        fs::create_dir_all(extras.join("src")).expect("extras source directory");
        fs::write(
            extras.join("Cargo.toml"),
            "[package]\nname = 'bevy_brp_extras'\nversion = '0.1.0'\nedition = '2024'\n",
        )
        .expect("extras manifest");
        fs::write(extras.join("src/lib.rs"), "pub struct BrpExtrasPlugin;").expect("extras source");
        let sources = root.path().join("src/bin");
        fs::create_dir_all(&sources).expect("binary source directory");
        fs::write(
            sources.join("launcher.rs"),
            "use bevy_brp_extras::BrpExtrasPlugin;\nfn main() { let _plugin = BrpExtrasPlugin; }\n",
        )
        .expect("registered binary source");
        fs::write(sources.join("helper.rs"), "fn main() {}\n").expect("helper binary source");

        let targets = collect_all_bevy_targets(&[root.path().to_owned()]);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0]["name"], "launcher");
        assert_eq!(targets[0]["kind"], "app");
        assert_eq!(targets[0]["brp_level"], "extras");
    }

    #[test]
    fn detects_output_collisions_with_unlisted_workspace_binaries() {
        let root = tempfile::tempdir().expect("temporary Cargo workspace");
        write_workspace_with_colliding_binaries(root.path());
        let detector = CargoDetector::try_from(root.path()).expect("workspace metadata");
        let targets = detector.find_bevy_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].package_name, "app");
        assert!(targets[0].isolated_target_dir.is_some());
    }

    #[test]
    fn isolated_outputs_are_distinct_between_workspaces_with_one_target_directory() {
        let root = tempfile::tempdir().expect("temporary Cargo workspaces");
        let shared = root.path().join("shared");
        let directories = ["first", "second"].map(|name| {
            let workspace = root.path().join(name);
            fs::create_dir(&workspace).expect("workspace directory");
            write_workspace_with_colliding_binaries(&workspace);
            let metadata = MetadataCommand::new()
                .current_dir(&workspace)
                .env("CARGO_TARGET_DIR", &shared)
                .exec()
                .expect("workspace metadata");
            let detector = CargoDetector::from(metadata);
            let targets = detector.find_bevy_targets();
            let directory = targets[0]
                .isolated_target_dir
                .clone()
                .expect("isolated target directory");
            assert!(directory.starts_with(&shared));
            directory
        });
        assert_ne!(directories[0], directories[1]);
    }

    fn write_workspace_with_colliding_binaries(root: &Path) {
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver = '3'\nmembers = ['app', 'helper']\n",
        )
        .expect("workspace manifest");

        for (name, registration) in [
            ("app", "[package.metadata.bevy_brp]\napps = ['shared']\n"),
            ("helper", ""),
        ] {
            let package = root.join(name);
            let sources = package.join("src/bin");
            fs::create_dir_all(&sources).expect("binary source directory");
            fs::write(
                package.join("Cargo.toml"),
                format!("[package]\nname = '{name}'\nversion = '0.1.0'\nedition = '2024'\n{registration}"),
            )
            .expect("package manifest");
            fs::write(sources.join("shared.rs"), "fn main() {}\n").expect("binary source");
        }
    }

    #[test]
    fn lists_runtime_plugin_targets_with_extras_support() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/test-app");
        let targets = collect_all_bevy_targets(&[fixture]);

        for (name, kind, expected_level) in [
            ("test_app", "app", "extras"),
            ("test_app", "example", "extras"),
            ("no_extras_plugin", "example", "brp_only"),
        ] {
            let target = targets
                .iter()
                .find(|target| target["name"] == name && target["kind"] == kind)
                .expect("fixture target should be listed");
            assert_eq!(target["brp_level"], expected_level);
        }
    }
}
