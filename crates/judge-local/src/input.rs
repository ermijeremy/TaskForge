use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use taskforge_execution::problem::PROBLEM_IDS;
use taskforge_execution::sandbox::MAX_SOURCE_BYTES;
use taskforge_execution::{ExecutionConfig, InfrastructureError, InfrastructureReason, Problem};

pub struct JudgingRequest {
    pub config: ExecutionConfig,
    pub source: String,
    pub problem: Problem,
}

pub fn load_request(
    problem_id: &str,
    source_path: &Path,
    config_path: &Path,
    problems_dir: &Path,
) -> Result<JudgingRequest, InfrastructureError> {
    if !PROBLEM_IDS.contains(&problem_id) {
        return Err(invalid("unknown active problem identifier"));
    }
    let problem = Problem::load(problems_dir.join(format!("{problem_id}.json")))
        .map_err(|_| invalid("problem definition is missing or invalid"))?;
    if problem.public().id != problem_id {
        return Err(invalid("problem filename and identifier do not match"));
    }
    let source = read_source(source_path)?;
    let config = ExecutionConfig::load(config_path)
        .map_err(|_| invalid("configuration is invalid; run check-config for details"))?;
    Ok(JudgingRequest {
        config,
        source,
        problem,
    })
}

pub fn read_source(path: &Path) -> Result<String, InfrastructureError> {
    let mut file = open_source(path).map_err(|_| invalid("cannot open submitted source file"))?;
    if !file
        .metadata()
        .map_err(|_| invalid("cannot inspect submitted source file"))?
        .is_file()
    {
        return Err(invalid("submitted source must be a regular file"));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take((MAX_SOURCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("cannot read submitted source file"))?;
    if bytes.is_empty() || bytes.len() > MAX_SOURCE_BYTES {
        return Err(invalid("source must contain 1..=65536 UTF-8 bytes"));
    }
    String::from_utf8(bytes).map_err(|_| invalid("submitted source must be valid UTF-8"))
}
#[cfg(unix)]
fn open_source(path: &Path) -> io::Result<File> {
    // Do not block opening a FIFO; inspect the opened descriptor before reading.
    rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::CLOEXEC | rustix::fs::OFlags::NONBLOCK,
        rustix::fs::Mode::empty(),
    )
    .map(File::from)
    .map_err(io::Error::from)
}
#[cfg(not(unix))]
fn open_source(path: &Path) -> io::Result<File> {
    File::open(path)
}
fn invalid(message: &str) -> InfrastructureError {
    InfrastructureError::new(InfrastructureReason::InvalidConfiguration, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_loading_bounds_bytes_and_rejects_empty_invalid_and_nonregular_files() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("solution.py");
        std::fs::write(&file, vec![b'x'; MAX_SOURCE_BYTES]).unwrap();
        assert_eq!(read_source(&file).unwrap().len(), MAX_SOURCE_BYTES);
        for bytes in [vec![], vec![b'x'; MAX_SOURCE_BYTES + 1], vec![255]] {
            std::fs::write(&file, bytes).unwrap();
            assert_eq!(
                read_source(&file).unwrap_err().reason_code(),
                InfrastructureReason::InvalidConfiguration
            );
        }
        assert!(read_source(directory.path()).is_err());
        assert!(read_source(&directory.path().join("missing.py")).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn fifo_sources_are_rejected_without_waiting_for_a_writer() {
        let directory = tempfile::tempdir().unwrap();
        let fifo = directory.path().join("source.pipe");
        rustix::fs::mkfifoat(
            rustix::fs::CWD,
            &fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .unwrap();
        assert!(read_source(&fifo).is_err());
    }
    #[test]
    fn problem_selection_rejects_traversal_and_archived_ids_before_file_access() {
        for id in ["../private/secret", "sum-two-integers-v1", "", "unknown"] {
            assert_eq!(
                load_request(
                    id,
                    Path::new("missing"),
                    Path::new("missing"),
                    Path::new("missing")
                )
                .err()
                .unwrap()
                .reason_code(),
                InfrastructureReason::InvalidConfiguration
            );
        }
    }
}
