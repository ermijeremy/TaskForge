//! Inventory checks only: none of the submitted Python programs run on the host.
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use taskforge_execution::{ProblemRuntime, load_problems};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    schema_version: u32,
    runtime: ProblemRuntime,
    host_marker: HostMarker,
    fixtures: Vec<Submission>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostMarker {
    fixture: String,
    host_path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submission {
    id: String,
    path: String,
    problem_id: String,
    category: String,
    expected_behavior: String,
}

#[test]
fn every_required_fixture_has_a_unique_name_source_and_known_problem() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let directory = root.join("submissions");
    let catalog: Catalog =
        serde_json::from_slice(&fs::read(directory.join("catalog.json")).unwrap()).unwrap();
    assert_eq!(catalog.schema_version, 1);
    assert_eq!(catalog.runtime, ProblemRuntime::Python31410);
    let problems = load_problems(root.join("problems")).unwrap();
    let problem_ids: HashSet<_> = problems.iter().map(|p| p.public().id.as_str()).collect();
    let mut fixture_ids = HashSet::new();
    let mut sources = HashSet::new();
    let mut categories = HashSet::new();
    let mut accepted_problems = HashSet::new();
    for fixture in &catalog.fixtures {
        assert!(fixture_ids.insert(&fixture.id));
        assert!(sources.insert(&fixture.path));
        assert!(problem_ids.contains(fixture.problem_id.as_str()));
        assert!(!fixture.expected_behavior.trim().is_empty());
        assert!(fixture.path.ends_with(".py"));
        assert_eq!(PathBuf::from(&fixture.path).components().count(), 1);
        let source = fs::read(directory.join(&fixture.path)).unwrap();
        assert!(!source.is_empty());
        assert!(source.len() <= 8 * 1024);
        categories.insert(fixture.category.as_str());
        if fixture.category == "accepted" {
            assert!(accepted_problems.insert(fixture.problem_id.as_str()));
        }
    }
    assert_eq!(catalog.fixtures.len(), 16);
    assert_eq!(accepted_problems, problem_ids);
    assert_eq!(
        categories,
        HashSet::from([
            "accepted",
            "wrong_answer",
            "whitespace",
            "syntax_error",
            "exception",
            "cpu_limit",
            "wall_limit",
            "memory_limit",
            "stdout_flood",
            "stderr_flood",
            "mixed_flood",
            "subprocess_probe",
            "network_probe",
            "host_file_probe",
        ])
    );
    let files: HashSet<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".py"))
        .collect();
    assert_eq!(files, sources.into_iter().cloned().collect());
    let marker = fs::read_to_string(directory.join(catalog.host_marker.fixture)).unwrap();
    assert_eq!(marker, "TASKFORGE_HOST_ONLY_MARKER_v1\n");
    assert_eq!(
        catalog.host_marker.host_path,
        "/var/local/lib/taskforge/host-only-marker.txt"
    );
}
