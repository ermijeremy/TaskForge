//! Restricted runtime image for the documented Fedora x86_64 / CPython host.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::{ExecutionConfig, InfrastructureError, InfrastructureReason};

const PYTHON: &str = "/usr/bin/python3.14";
const STDLIB: &str = "/usr/lib64/python3.14";
const LIBRARIES: [&str; 4] = [
    "/lib64/ld-linux-x86-64.so.2",
    "/lib64/libpython3.14.so.1.0",
    "/lib64/libc.so.6",
    "/lib64/libm.so.6",
];
const EXTENSIONS: [&str; 3] = [
    "math.cpython-314-x86_64-linux-gnu.so",
    "select.cpython-314-x86_64-linux-gnu.so",
    "_socket.cpython-314-x86_64-linux-gnu.so",
];

#[derive(Debug)]
pub(super) struct RuntimeImage {
    directory: TempDir,
}

impl RuntimeImage {
    pub fn stage(config: &ExecutionConfig) -> Result<Self, InfrastructureError> {
        if !cfg!(all(target_os = "linux", target_arch = "x86_64"))
            || config.python_path() != Path::new(PYTHON)
        {
            return Err(super::error(
                InfrastructureReason::InvalidConfiguration,
                "restricted runtime requires the documented /usr/bin/python3.14 x86_64 host",
            ));
        }
        let image = Self {
            directory: tempfile::tempdir().map_err(runtime_error)?,
        };
        super::mount_path(image.directory.path())?;
        let root = image.directory.path();
        copy_file(Path::new(PYTHON), &root.join("usr/bin/python3.14"), 0o555)?;
        for library in LIBRARIES {
            copy_file(
                Path::new(library),
                &root.join(library.trim_start_matches('/')),
                if library.ends_with("ld-linux-x86-64.so.2") {
                    0o555
                } else {
                    0o444
                },
            )?;
        }
        copy_python_sources(Path::new(STDLIB), &root.join("usr/lib64/python3.14"))?;
        for extension in EXTENSIONS {
            copy_file(
                &Path::new(STDLIB).join("lib-dynload").join(extension),
                &root
                    .join("usr/lib64/python3.14/lib-dynload")
                    .join(extension),
                0o444,
            )?;
        }
        readable_directories(root)?;
        Ok(image)
    }

    #[cfg(test)]
    pub(super) fn empty_for_test() -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("usr")).unwrap();
        fs::create_dir(directory.path().join("lib64")).unwrap();
        Self { directory }
    }

    pub fn usr(&self) -> PathBuf {
        self.directory.path().join("usr")
    }
    pub fn lib64(&self) -> PathBuf {
        self.directory.path().join("lib64")
    }
}

// Only standard-library .py files: no site packages, .pth files, native modules
// outside EXTENSIONS, caches, external symlinks, virtual environments or tools.
fn copy_python_sources(source: &Path, destination: &Path) -> Result<(), InfrastructureError> {
    fs::create_dir_all(destination).map_err(runtime_error)?;
    for entry in fs::read_dir(source).map_err(runtime_error)? {
        let entry = entry.map_err(runtime_error)?;
        let kind = entry.file_type().map_err(runtime_error)?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if kind.is_symlink() {
            return Err(super::error(
                InfrastructureReason::SandboxSetupFailed,
                "symlink in standard-library runtime source",
            ));
        }
        if kind.is_dir() {
            if [
                "site-packages",
                "__pycache__",
                "lib-dynload",
                "ensurepip",
                "venv",
                "test",
                "tests",
                "idlelib",
                "tkinter",
            ]
            .contains(&name.as_ref())
                || name.starts_with("config-")
            {
                continue;
            }
            copy_python_sources(&entry.path(), &destination.join(name.as_ref()))?;
        } else if kind.is_file() && entry.path().extension().is_some_and(|ext| ext == "py") {
            copy_file(&entry.path(), &destination.join(name.as_ref()), 0o444)?;
        }
    }
    Ok(())
}

fn readable_directories(root: &Path) -> Result<(), InfrastructureError> {
    for entry in fs::read_dir(root).map_err(runtime_error)? {
        let entry = entry.map_err(runtime_error)?;
        if entry.file_type().map_err(runtime_error)?.is_dir() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o755))
                    .map_err(runtime_error)?;
            }
            readable_directories(&entry.path())?;
        }
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path, mode: u32) -> Result<(), InfrastructureError> {
    if !fs::metadata(source).map_err(runtime_error)?.is_file() {
        return Err(super::error(
            InfrastructureReason::SandboxSetupFailed,
            "runtime source is not a regular file",
        ));
    }
    fs::create_dir_all(destination.parent().expect("runtime target has a parent"))
        .map_err(runtime_error)?;
    fs::copy(source, destination).map_err(runtime_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(destination, fs::Permissions::from_mode(mode))
            .map_err(runtime_error)?;
    }
    Ok(())
}

fn runtime_error(_: io::Error) -> InfrastructureError {
    super::error(
        InfrastructureReason::SandboxSetupFailed,
        "cannot stage the restricted Python runtime image",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_source_filter_omits_site_packages_caches_and_unapproved_native_code() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        for path in ["encodings", "site-packages", "__pycache__", "lib-dynload"] {
            fs::create_dir(source.path().join(path)).unwrap();
        }
        for path in [
            "os.py",
            "encodings/utf_8.py",
            "site-packages/secret.py",
            "__pycache__/os.py",
            "lib-dynload/unapproved.so",
            "credentials.txt",
            "startup.pth",
        ] {
            fs::write(source.path().join(path), "fixture").unwrap();
        }
        copy_python_sources(source.path(), dest.path()).unwrap();
        assert!(dest.path().join("os.py").is_file());
        assert!(dest.path().join("encodings/utf_8.py").is_file());
        for path in [
            "site-packages",
            "__pycache__",
            "lib-dynload",
            "credentials.txt",
            "startup.pth",
        ] {
            assert!(!dest.path().join(path).exists(), "{path}");
        }
    }

    #[test]
    #[cfg(unix)]
    fn runtime_filter_rejects_symlinks_instead_of_copying_external_files() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/etc/passwd", source.path().join("escape.py")).unwrap();
        assert!(copy_python_sources(source.path(), dest.path()).is_err());
    }
    #[test]
    #[ignore = "requires the documented Fedora x86_64 CPython runtime files; stages but executes no submission"]
    fn real_restricted_runtime_image_contents() {
        let path = std::env::var_os("TASKFORGE_REAL_CONFIG").expect("set TASKFORGE_REAL_CONFIG");
        let config = ExecutionConfig::load(path).unwrap();
        let image = RuntimeImage::stage(&config).unwrap();
        assert!(image.usr().join("bin/python3.14").is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(image.lib64().join("ld-linux-x86-64.so.2"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
        }
        assert!(
            image
                .usr()
                .join("lib64/python3.14/encodings/utf_8.py")
                .is_file()
        );
        assert!(!image.usr().join("lib64/python3.14/site-packages").exists());
        assert!(!image.directory.path().join("etc").exists());
        assert!(!image.directory.path().join("tmp").exists());
        let extensions: Vec<_> = fs::read_dir(image.usr().join("lib64/python3.14/lib-dynload"))
            .unwrap()
            .collect();
        assert_eq!(extensions.len(), 3);
        println!(
            "Restricted runtime image staged; interpreter, four ELF libraries, filtered .py sources and three approved extensions. No submission executed."
        );
    }
}
