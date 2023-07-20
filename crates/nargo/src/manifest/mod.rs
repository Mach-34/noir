use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub mod errors;
use self::errors::GlobalConfigError;
pub use self::errors::InvalidPackageError;

#[derive(Debug, Deserialize, Clone)]
pub struct PackageManifest {
    pub package: PackageMetadata,
    pub dependencies: BTreeMap<String, Dependency>,
}

/// Contains all the information about a package, as loaded from a `Nargo.toml`.
/// Represents a manifest, which can be either a package manifest or a workspace manifest.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum Manifest {
    /// Represents a package manifest.
    Package(PackageManifest),
    /// Represents a workspace manifest.
    Workspace(Workspace),
}

impl Manifest {
    pub fn from_toml_str(toml_as_string: &str) -> Result<Self, InvalidPackageError> {
        let manifest = toml::from_str(toml_as_string)?;
        Ok(manifest)
    }

    pub fn to_package(self) -> Option<PackageManifest> {
        match self {
            Self::Package(v) => Some(v),
            _ => None,
        }
    }
}

impl PackageManifest {
    /// Returns whether the package has a local dependency.
    // Local paths are usually relative and are discouraged when sharing libraries
    // It is better to separate these into different packages.
    pub fn has_local_dependency(&self) -> bool {
        self.dependencies.values().any(|dep| matches!(dep, Dependency::Path { .. }))
    }
}

/// Configuration of a workspace in a manifest.
/// Indicates that `[workspace]` was present and the members were specified as well.
#[derive(Debug, Deserialize, Clone)]
pub struct Workspace {
    #[serde(rename = "workspace")]
    pub config: WorkspaceConfig,
}

#[derive(Default, Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct WorkspaceConfig {
    /// List of members in this workspace.
    pub members: Vec<PathBuf>,
    /// Specifies the default crate to interact with in the context (similarly to how we have nargo as the default crate in this repository).
    pub default_member: Option<PathBuf>,
}

#[allow(dead_code)]
#[derive(Default, Debug, Deserialize, Clone)]
pub struct PackageMetadata {
    pub name: Option<String>,
    // Note: a package name is not needed unless there is a registry
    authors: Vec<String>,
    // If not compiler version is supplied, the latest is used
    // For now, we state that all packages must be compiled under the same
    // compiler version.
    // We also state that ACIR and the compiler will upgrade in lockstep.
    // so you will not need to supply an ACIR and compiler version
    compiler_version: Option<String>,
    backend: Option<String>,
    license: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
/// Enum representing the different types of ways to
/// supply a source for the dependency
pub enum Dependency {
    Github { git: String, tag: String },
    Path { path: String },
}

#[test]
fn parse_standard_toml() {
    let src = r#"

        [package]
        authors = ["kev", "foo"]
        compiler_version = "0.1"

        [dependencies]
        rand = { tag = "next", git = "https://github.com/rust-lang-nursery/rand"}
        cool = { tag = "next", git = "https://github.com/rust-lang-nursery/rand"}
        hello = {path = "./noir_driver"}
    "#;

    assert!(Manifest::from_toml_str(src).is_ok());
}

#[test]
fn parse_workspace_toml() {
    let src = r#"
        [workspace]
        members = ["a", "b"]
    "#;

    assert!(Manifest::from_toml_str(src).is_ok());
}

#[test]
fn parse_workspace_default_member_toml() {
    let src = r#"
        [workspace]
        members = ["a", "b"]
        default-member = "a"
    "#;

    assert!(Manifest::from_toml_str(src).is_ok());
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GlobalConfig {
    pub backends: Option<Backends>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Backends {
    pub default: Option<String>
}
impl GlobalConfig {

    pub fn from_toml_str(toml_as_string: &str) -> Result<Self, GlobalConfigError> {
        let global_config = toml::from_str::<GlobalConfig>(toml_as_string)?;
        Ok(global_config)
    }

    pub fn to_toml_str(&self) -> Result<String, GlobalConfigError> {
        let global_config = toml::to_string_pretty::<GlobalConfig>(self)?;
        Ok(global_config)
    }

}
