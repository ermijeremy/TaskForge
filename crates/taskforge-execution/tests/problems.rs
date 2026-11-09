use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};
use taskforge_execution::problem::{MAX_CASE_BYTES, MAX_DEFINITION_BYTES, PROBLEM_IDS};
use taskforge_execution::{ConfigError, Problem, ProblemError, ProblemRuntime, load_problems};

fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/problems")
}

fn document() -> Value {
    serde_json::from_slice(&fs::read(directory().join("sum-two-integers-v1.json")).unwrap())
        .unwrap()
}

fn load(value: &Value) -> Result<Problem, ProblemError> {
    Problem::from_json(&serde_json::to_vec(value).unwrap())
}

#[test]
fn fixed_catalog_has_stable_ids_positions_explicit_limits_and_runtime() {
    let problems = load_problems(directory()).unwrap();
    assert_eq!(problems.len(), 3);
    let mut total = 0;
    for (problem, id) in problems.iter().zip(PROBLEM_IDS) {
        assert_eq!(problem.public().id, id);
        assert_eq!(problem.public().runtime, ProblemRuntime::Python3148);
        assert_eq!(problem.limits().memory_bytes().get(), 128 * 1024 * 1024);
        assert_eq!(problem.limits().output_bytes().get(), 1024 * 1024);
        assert_eq!(problem.limits().processes().get(), 1);
        assert!(problem.test_cases().len() >= 5);
        for (position, case) in problem.test_cases().iter().enumerate() {
            assert_eq!(case.position, position);
        }
        total += problem.test_cases().len();
    }
    assert_eq!(total, 30);
}

#[test]
fn every_stored_answer_and_public_example_matches_an_independent_reference() {
    for problem in load_problems(directory()).unwrap() {
        for case in problem.test_cases() {
            assert_eq!(
                reference(&problem.public().id, &case.input),
                case.expected_output,
                "{} / {}",
                problem.public().id,
                case.id
            );
        }
        for example in &problem.public().examples {
            assert_eq!(
                reference(&problem.public().id, &example.input),
                example.output
            );
        }
    }
}

fn reference(id: &str, input: &str) -> String {
    match id {
        "sum-two-integers-v1" => {
            let values: Vec<i128> = input
                .split_ascii_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            assert_eq!(values.len(), 2);
            assert!(values.iter().all(|v| v.abs() <= 1_000_000_000_000_000_000));
            format!("{}\n", values[0] + values[1])
        }
        "sum-list-v1" => {
            let mut tokens = input.split_ascii_whitespace();
            let count: usize = tokens.next().unwrap().parse().unwrap();
            let values: Vec<i128> = tokens.map(|v| v.parse().unwrap()).collect();
            assert!(count <= 100);
            assert_eq!(count, values.len());
            assert!(values.iter().all(|v| v.abs() <= 1_000_000_000_000_000_000));
            format!("{}\n", values.iter().sum::<i128>())
        }
        "balanced-brackets-v1" => {
            let line = input.strip_suffix('\n').unwrap_or(input);
            assert!(line.len() <= 1000);
            assert!(line.bytes().all(|b| b"()[]{}".contains(&b)));
            let mut expected = Vec::new();
            let mut balanced = true;
            for byte in line.bytes() {
                match byte {
                    b'(' => expected.push(b')'),
                    b'[' => expected.push(b']'),
                    b'{' => expected.push(b'}'),
                    closing if expected.pop() != Some(closing) => {
                        balanced = false;
                        break;
                    }
                    _ => {}
                }
            }
            if balanced && expected.is_empty() {
                "YES\n".into()
            } else {
                "NO\n".into()
            }
        }
        _ => panic!("unknown fixed problem {id}"),
    }
}

#[test]
fn public_serialization_omits_private_cases_and_expected_outputs() {
    let problem = load(&document()).unwrap();
    let public = serde_json::to_value(problem.public()).unwrap();
    assert!(public.get("test_cases").is_none());
    assert!(public.get("expected_output").is_none());
    assert_eq!(public["examples"][0]["output"], "5\n");
    assert_eq!(public["runtime"], "python-3.14.8");
    assert_eq!(public["limits"], document()["limits"]);
    assert!(!problem.test_cases().is_empty());
}

#[test]
fn runtime_and_every_limit_are_required_without_host_defaults() {
    for field in [
        "schema_version",
        "runtime",
        "limits",
        "test_cases",
        "examples",
    ] {
        let mut value = document();
        value.as_object_mut().unwrap().remove(field);
        assert!(
            matches!(load(&value), Err(ProblemError::Parse(_))),
            "missing {field}"
        );
    }
    for field in [
        "cpu_time_ms",
        "wall_time_ms",
        "memory_bytes",
        "output_bytes",
        "processes",
    ] {
        let mut value = document();
        value["limits"].as_object_mut().unwrap().remove(field);
        assert!(
            matches!(load(&value), Err(ProblemError::Parse(_))),
            "missing {field}"
        );
        let mut value = document();
        value["limits"][field] = json!(0);
        assert!(
            matches!(
                load(&value),
                Err(ProblemError::Limits(ConfigError::NonPositive { .. }))
            ),
            "zero {field}"
        );
    }
    for runtime in [json!("python"), json!("python-3.13"), json!(null)] {
        let mut value = document();
        value["runtime"] = runtime;
        assert!(matches!(load(&value), Err(ProblemError::Parse(_))));
    }
}

#[test]
fn unknown_fields_and_invalid_numeric_types_are_rejected() {
    for pointer in ["", "/limits", "/examples/0", "/test_cases/0"] {
        let mut value = document();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("typo".into(), json!(1));
        assert!(
            matches!(load(&value), Err(ProblemError::Parse(_))),
            "{pointer}"
        );
    }
    for (field, invalid) in [
        ("cpu_time_ms", json!(-1)),
        ("wall_time_ms", json!(0.5)),
        ("processes", json!(u64::from(u32::MAX) + 1)),
    ] {
        let mut value = document();
        value["limits"][field] = invalid;
        assert!(matches!(load(&value), Err(ProblemError::Parse(_))));
    }
    let mut value = document();
    value["limits"]["processes"] = json!(2);
    assert!(matches!(load(&value), Err(ProblemError::Invalid(_))));
}

#[test]
fn malformed_metadata_and_invalid_identifiers_are_rejected() {
    for field in ["title", "description", "input_format", "output_format"] {
        let mut value = document();
        value[field] = json!(" \n\t");
        assert!(
            matches!(load(&value), Err(ProblemError::Invalid(_))),
            "{field}"
        );
    }
    for id in ["", "../escape", "Uppercase", "-first", "last-", "has space"] {
        let mut value = document();
        value["id"] = json!(id);
        assert!(
            matches!(load(&value), Err(ProblemError::Invalid(_))),
            "{id}"
        );
    }
    let mut value = document();
    value["schema_version"] = json!(2);
    assert!(matches!(load(&value), Err(ProblemError::Invalid(_))));
}

#[test]
fn tests_must_have_unique_ids_and_contiguous_positions_in_stored_order() {
    let mut value = document();
    value["test_cases"][1]["id"] = value["test_cases"][0]["id"].clone();
    assert!(matches!(load(&value), Err(ProblemError::Invalid(_))));
    let mut value = document();
    value["test_cases"].as_array_mut().unwrap().swap(0, 1);
    assert!(matches!(load(&value), Err(ProblemError::Invalid(_))));
    let mut value = document();
    value["test_cases"][0]["position"] = json!(1);
    assert!(matches!(load(&value), Err(ProblemError::Invalid(_))));
    let mut value = document();
    value["test_cases"][0]["id"] = json!("bad/id");
    assert!(matches!(load(&value), Err(ProblemError::Invalid(_))));
}

#[test]
fn example_and_case_counts_have_explicit_bounds() {
    for count in [0, 9] {
        let mut value = document();
        value["examples"] = json!(vec![value["examples"][0].clone(); count]);
        assert!(matches!(load(&value), Err(ProblemError::Invalid(_))));
    }
    for (count, valid) in [(4, false), (5, true), (128, true), (129, false)] {
        let mut value = document();
        value["test_cases"] = json!((0..count).map(|position| json!({"id": format!("case-{position}"), "position": position, "input": "0 0\n", "expected_output": "0\n"})).collect::<Vec<_>>());
        assert_eq!(load(&value).is_ok(), valid, "count {count}");
    }
}

#[test]
fn case_and_example_byte_limits_allow_empty_inputs_and_reject_oversized_data() {
    for pointer in [
        "/test_cases/0/input",
        "/test_cases/0/expected_output",
        "/examples/0/input",
        "/examples/0/output",
    ] {
        let mut value = document();
        *value.pointer_mut(pointer).unwrap() = json!("x".repeat(MAX_CASE_BYTES));
        assert!(load(&value).is_ok());
        *value.pointer_mut(pointer).unwrap() = json!("x".repeat(MAX_CASE_BYTES + 1));
        assert!(matches!(load(&value), Err(ProblemError::Invalid(_))));
    }
    let brackets = Problem::load(directory().join("balanced-brackets-v1.json")).unwrap();
    assert!(
        brackets
            .test_cases()
            .iter()
            .any(|case| case.input.is_empty() && case.expected_output == "YES\n")
    );
}

#[test]
fn file_loading_reports_missing_malformed_and_oversized_definitions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("problem.json");
    assert!(matches!(
        Problem::load(&path),
        Err(ProblemError::Read { .. })
    ));
    fs::write(&path, "{bad json").unwrap();
    assert!(matches!(Problem::load(&path), Err(ProblemError::Parse(_))));
    fs::write(&path, vec![b' '; MAX_DEFINITION_BYTES + 1]).unwrap();
    assert!(matches!(Problem::load(&path), Err(ProblemError::TooLarge)));
}

#[test]
fn catalog_checks_file_identity_and_loading_leaves_seed_data_unchanged() {
    let temporary = tempfile::tempdir().unwrap();
    let mut original = Vec::new();
    for id in PROBLEM_IDS {
        let name = format!("{id}.json");
        let bytes = fs::read(directory().join(&name)).unwrap();
        fs::write(temporary.path().join(&name), &bytes).unwrap();
        original.push((name, bytes));
    }
    for _ in 0..2 {
        load_problems(temporary.path()).unwrap();
    }
    for (name, bytes) in original {
        assert_eq!(fs::read(temporary.path().join(name)).unwrap(), bytes);
    }
    let mut changed = document();
    changed["id"] = json!("another-v1");
    fs::write(
        temporary.path().join("sum-two-integers-v1.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        load_problems(temporary.path()),
        Err(ProblemError::Invalid(_))
    ));
}
