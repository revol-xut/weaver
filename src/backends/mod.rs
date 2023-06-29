use std::collections::HashMap;
use std::path::PathBuf;

use rayon::prelude::*;

use crate::util::errors::BuildResult;
use crate::{args::BuildSystem, package::App};

pub mod cmake;
pub mod lfc;

pub fn execute_command(command: BatchLingoCommand) -> BatchBuildResults {
    // Group apps by build system
    let mut by_build_system = HashMap::<BuildSystem, Vec<&App>>::new();
    for &app in &command.apps {
        by_build_system
            .entry(app.build_system())
            .or_default()
            .push(app);
    }

    let mut result = BatchBuildResults::new();
    for (bs, apps) in by_build_system {
        let command = command.with_apps(apps);
        let sub_res = match bs {
            BuildSystem::LFC => lfc::LFC.execute_command(command),
            BuildSystem::CMake => cmake::Cmake.execute_command(command),
            BuildSystem::Cargo => todo!(),
        };
        result.append(sub_res);
    }
    result
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum BuildProfile {
    /// Compile with optimizations.
    Release,
    /// Compile with debug info.
    Debug,
}

#[derive(Clone)]
pub struct BuildCommandOptions {
    /// Build profile, mostly relevant for target compilation.
    pub profile: BuildProfile,
    /// Whether to compile the target code.
    pub compile_target_code: bool,
    /// Path to the LFC executable.
    pub lfc_exec_path: PathBuf,
}

/// Description of a lingo command
#[derive(Clone)]
pub enum CommandSpec {
    /// Compile generated code with the target compiler.
    Build(BuildCommandOptions),
    /// Update dependencies
    Update,
    /// Clean build artifacts
    Clean,
}

/// Batch of apps to process, possibly in parallel.
pub struct BatchLingoCommand<'a> {
    /// List of apps to build.
    pub apps: Vec<&'a App>,
    /// Action to take.
    pub task: CommandSpec,
}

impl<'a> BatchLingoCommand<'a> {
    fn with_apps<'b>(&self, apps: Vec<&'b App>) -> BatchLingoCommand<'b> {
        BatchLingoCommand {
            apps,
            task: self.task.clone(),
        }
    }

    pub fn new_results(&self) -> BatchBuildResults<'a> {
        BatchBuildResults::for_apps(&self.apps)
    }
}

/// trait that all different build backends need to implement
pub trait BatchBackend {
    /// Build all apps, possibly in parallel.
    fn execute_command<'a>(&mut self, command: BatchLingoCommand<'a>) -> BatchBuildResults<'a>;
}

/// Collects build results by app.
pub struct BatchBuildResults<'a> {
    results: Vec<(&'a App, BuildResult)>,
}

impl<'a> BatchBuildResults<'a> {
    fn new() -> Self {
        Self {
            results: Vec::new(),
        }
    }

    fn for_apps(apps: &[&'a App]) -> Self {
        Self {
            results: apps.iter().map(|&a| (a, Ok(()))).collect(),
        }
    }

    pub fn print_results(&self) {
        for (app, b) in &self.results {
            match b {
                Ok(()) => {
                    println!("- {}: Success", &app.name);
                }
                Err(e) => {
                    println!("- {}: Error: {}", &app.name, e);
                }
            }
        }
    }

    /// Absorb some results into this vector. Apps are not deduplicated, so this
    /// is only ok if the other is disjoint from this result.
    fn append(&mut self, mut other: BatchBuildResults<'a>) {
        self.results.append(&mut other.results);
        self.results.sort_by_key(|(app, _)| &app.name);
    }

    fn record_result(&mut self, app: &'a App, result: BuildResult) {
        self.results.push((app, (result)));
    }

    // Note: the duplication of the bodies of the following functions is benign, and
    // allows the sequential map to be bounded more loosely than if we were to extract
    // a function to get rid of the dup.

    /// Map results sequentially. Apps that already have a failing result recorded
    /// are not fed to the mapping function.
    pub fn map<F, R>(mut self, f: F) -> BatchBuildResults<'a>
    where
        F: Fn(&'a App) -> R,
        R: Into<BuildResult>,
    {
        self.results.iter_mut().for_each(|(app, res)| match res {
            Ok(()) => {
                *res = f(app).into();
            }
            _ => {}
        });
        self
    }

    /// Map results in parallel. Apps that already have a failing result recorded
    /// are not fed to the mapping function.
    pub fn par_map<F>(mut self, f: F) -> BatchBuildResults<'a>
    where
        F: Fn(&'a App) -> BuildResult + Send + Sync,
    {
        self.results
            .par_iter_mut()
            .for_each(|(app, res)| match res {
                Ok(()) => {
                    *res = f(app);
                }
                _ => {}
            });
        self
    }
}
