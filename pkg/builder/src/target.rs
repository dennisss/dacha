use common::errors::*;

use crate::proto::config::*;

#[derive(Clone)]
pub struct BuildTarget {
    pub raw: BuildTargetRaw,
}

// TODO: Consider instad using Arcs to arene memory for each of the protos to
// avoid memory fragmentation.
#[derive(Clone)]
pub enum BuildTargetRaw {
    Bundle(Bundle),
    RustBinary(RustBinary),
    FileGroup(FileGroup),
    Webpack(Webpack),
    BuildConfig(BuildConfig),
    LocalBinary(LocalBinary),
}

impl BuildTarget {
    pub fn list_all(file: &BuildFile) -> Vec<BuildTarget> {
        let mut out = vec![];

        for raw in file.filegroup() {
            out.push(BuildTarget {
                raw: BuildTargetRaw::FileGroup(raw.clone()),
            });
        }

        for raw in file.rust_binary() {
            out.push(BuildTarget {
                raw: BuildTargetRaw::RustBinary(raw.clone()),
            });
        }

        for raw in file.bundle() {
            out.push(BuildTarget {
                raw: BuildTargetRaw::Bundle(raw.clone()),
            });
        }

        for raw in file.webpack() {
            out.push(BuildTarget {
                raw: BuildTargetRaw::Webpack(raw.clone()),
            });
        }

        for raw in file.build_config() {
            out.push(BuildTarget {
                raw: BuildTargetRaw::BuildConfig(raw.clone()),
            });
        }

        for raw in file.local_binary() {
            out.push(BuildTarget {
                raw: BuildTargetRaw::LocalBinary(raw.clone()),
            });
        }

        out
    }

    pub fn name(&self) -> &str {
        match &self.raw {
            BuildTargetRaw::Bundle(v) => v.name(),
            BuildTargetRaw::RustBinary(v) => v.name(),
            BuildTargetRaw::FileGroup(v) => v.name(),
            BuildTargetRaw::Webpack(v) => v.name(),
            BuildTargetRaw::BuildConfig(v) => v.name(),
            BuildTargetRaw::LocalBinary(v) => v.name(),
        }
    }

    pub fn deps(&self) -> &[String] {
        match &self.raw {
            // NOTE: Bundles have virtual dependencies that are indirectly compiled.
            BuildTargetRaw::Bundle(_) => &[],
            BuildTargetRaw::RustBinary(v) => v.deps(),
            BuildTargetRaw::FileGroup(v) => v.deps(),
            BuildTargetRaw::Webpack(v) => v.deps(),
            BuildTargetRaw::BuildConfig(_) => &[],
            BuildTargetRaw::LocalBinary(_) => &[],
        }
    }
}
