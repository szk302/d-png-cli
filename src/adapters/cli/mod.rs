//! The command line (spec 2-11): parses arguments, reads input, assembles
//! the adapters for the use cases, and prints results and diagnostics with
//! the exit codes of spec 2.1.
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use crate::{
    adapters::{
        config::{self, Configuration},
        fs,
        project::{self, InstalledLibraries, LibraryArea, Project, ProjectLibraries},
        render::RendererKind,
        sources::{self, Cache, SourceDirectory, Sources},
        vscode::VsCodeSettings,
    },
    app::{
        diagram::{self, Pixels, WriteRequest},
        install,
        libraries::{Libraries, SearchQuery},
        library::{self, Declared, PreviewRequest},
        ports::{DependencyStore, LibraryRepository, LockStore},
        register,
        vscode::{self, EntryIds},
    },
    domain::{
        diagnostic::{Code, Diagnostic, Diagnostics},
        fonts::{self, FontPolicy},
        library::EntryKind,
        library_file::Source,
        reference::LibraryName,
        settings::{Scope, Settings},
        version::Range,
    },
};

mod output;

#[derive(Parser)]
#[command(
    name = "d-png-cli",
    version,
    about = "Write draw.io diagrams, given as XML on standard input, into PNG images that VS Code's draw.io extension can edit",
    after_help = "Environment:\n  D_PNG_PROJECT          Directory holding the project area (.d-png/)\n  D_PNG_HOME             User area: the user's config.toml and the global libraries\n  D_PNG_CACHE            Cache of files fetched from library sources\n  D_PNG_RENDERER         Default renderer: vscode, desktop-compat, raw, desktop-app\n  D_PNG_CHROME_PATH      Chromium or Chrome executable (or CHROME_PATH)\n  D_PNG_CHROME_ARGS      Additional POSIX-quoted Chromium options (no shell expansion)\n  D_PNG_DRAWIO_PATH      draw.io Desktop executable (desktop-app)\n  D_PNG_DRAWIO_ARGS      Additional draw.io Desktop options\n  D_PNG_DRAWIO_WEB_PATH  Local draw.io webapp directory (default: bundled assets)"
)]
struct Cli {
    /// Directory holding the project area (.d-png/); default: the nearest above the current directory
    #[arg(long, global = true, value_name = "DIR")]
    project: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print a diagram's XML for editing; library images become image=lib: references
    Read {
        /// XML or .drawio.png (omit or "-" for standard input)
        input: Option<PathBuf>,
        /// XML file to write (default: standard output)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the XML stored in the PNG as it is
        #[arg(long)]
        raw: bool,
        /// Print a JSON summary to standard output (needs --output)
        #[arg(long)]
        json: bool,
    },
    /// Expand references, check, render and save a diagram as a .drawio.png
    Write {
        /// XML or .drawio.png (omit or "-" for standard input)
        input: Option<PathBuf>,
        /// PNG file to write ("-" for standard output)
        #[arg(short, long)]
        output: PathBuf,
        /// Page to render, starting at 1; every page is saved
        #[arg(long, default_value_t = 1)]
        page: usize,
        /// vscode (default), desktop-compat, raw or desktop-app
        #[arg(long, value_parser = renderer_name, conflicts_with = "metadata_only")]
        renderer: Option<RendererKind>,
        /// Allow external HTTP(S) images and fonts (not with desktop-app)
        #[arg(long, conflicts_with = "metadata_only")]
        allow_network: bool,
        /// Set and save the font for cells without an explicit fontFamily
        #[arg(long, value_parser = fonts::parse_family)]
        default_font: Option<String>,
        /// Append a fallback family (repeatable, in priority order); requires --default-font
        #[arg(long, value_parser = fonts::parse_family, requires = "default_font")]
        fallback_font: Vec<String>,
        /// Save the XML without rendering; pixels come from --base-image or a transparent 1x1 image
        #[arg(long)]
        metadata_only: bool,
        /// PNG whose pixels to keep with --metadata-only
        #[arg(long, requires = "metadata_only")]
        base_image: Option<PathBuf>,
        /// Print a JSON summary to standard output
        #[arg(long)]
        json: bool,
    },
    /// Print the diagram with references expanded: the XML that write saves
    Expand {
        /// XML or .drawio.png (omit or "-" for standard input)
        input: Option<PathBuf>,
        /// XML file to write (default: standard output)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Set and save the font for cells without an explicit fontFamily
        #[arg(long, value_parser = fonts::parse_family)]
        default_font: Option<String>,
        /// Append a fallback family (repeatable, in priority order); requires --default-font
        #[arg(long, value_parser = fonts::parse_family, requires = "default_font")]
        fallback_font: Vec<String>,
        /// Print a JSON summary to standard output (needs --output)
        #[arg(long)]
        json: bool,
    },
    /// Check a diagram as write would before rendering; writes nothing
    Validate {
        /// XML or .drawio.png (omit or "-" for standard input)
        input: Option<PathBuf>,
        /// Print a JSON summary to standard output
        #[arg(long)]
        json: bool,
    },
    /// Create the project area (.d-png/)
    Init {
        /// Directory to create it in (default: the current directory)
        dir: Option<PathBuf>,
        /// Print a JSON summary to standard output
        #[arg(long)]
        json: bool,
    },
    /// Register, install and share libraries; find, inspect and preview their entries
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    /// Print the agent skill guide for this version of d-png-cli
    Skill,
    /// Print licenses and notices for d-png-cli, dependencies, and bundled assets
    Licenses,
}

#[derive(Subcommand)]
enum LibraryCommand {
    /// Register library or image files as a project library (no scope), or a global one
    Add {
        /// <mxlibrary> files or PNG, JPEG, GIF, SVG and WebP images ("-" for standard input)
        #[arg(required = true)]
        sources: Vec<PathBuf>,
        /// Library name (default: from the first file name)
        #[arg(long)]
        name: Option<String>,
        /// Title shown in draw.io and VS Code (default: the source's, or the name)
        #[arg(long)]
        title: Option<String>,
        /// Replace an existing library of this name
        #[arg(long, conflicts_with = "merge")]
        replace: bool,
        /// Replace entries with the same ID in an existing library and append the rest
        #[arg(long)]
        merge: bool,
        /// Leave out entries that cannot be expanded instead of failing
        #[arg(long)]
        skip_invalid: bool,
        /// Use the global libraries in the user area, for every project, instead of the project's
        #[arg(short, long)]
        global: bool,
        /// Print a JSON summary to standard output
        #[arg(long)]
        json: bool,
    },
    /// Install scoped libraries from their sources and record them in lock.json
    Install {
        /// Libraries to add, such as @acme/brand or @acme/brand@^1.2.0 (default: install the dependencies)
        names: Vec<String>,
        /// Change neither config.toml nor lock.json; fail if they disagree (for CI)
        #[arg(long)]
        frozen: bool,
        /// Use only the cache of fetched files
        #[arg(long)]
        offline: bool,
        /// Use the global libraries in the user area, for every project, instead of the project's
        #[arg(short, long)]
        global: bool,
        /// Print a JSON summary to standard output
        #[arg(long)]
        json: bool,
    },
    /// Update installed libraries to the newest version in their range
    Update {
        /// Libraries to update (default: all dependencies)
        names: Vec<String>,
        /// Go past the range to the newest version and record ^<version>
        #[arg(long)]
        latest: bool,
        /// Report the changes without writing
        #[arg(long)]
        dry_run: bool,
        /// Use the global libraries in the user area, for every project, instead of the project's
        #[arg(short, long)]
        global: bool,
        /// Print a JSON summary to standard output
        #[arg(long)]
        json: bool,
    },
    /// Write a version of a scoped library into a source directory
    Pack {
        /// <mxlibrary> files or images ("-" for standard input)
        #[arg(required = true)]
        sources: Vec<PathBuf>,
        /// The library, such as @acme/brand
        #[arg(long)]
        name: String,
        /// The new version, MAJOR.MINOR.PATCH
        #[arg(long)]
        version: String,
        /// The source directory (a file: directory, a Git working tree, files for a web server)
        #[arg(long, value_name = "DIR")]
        to: PathBuf,
        /// Title (default: the previous version's, the source's, or the name)
        #[arg(long)]
        title: Option<String>,
        /// Leave out entries that cannot be expanded instead of failing
        #[arg(long)]
        skip_invalid: bool,
        /// Print a JSON summary to standard output
        #[arg(long)]
        json: bool,
    },
    /// Print or write the VS Code extension's customLibraries setting for these libraries
    Vscode {
        /// First part of the extension's headings (default: d-png-cli)
        #[arg(long, value_name = "TEXT")]
        entry_id: Option<String>,
        /// Update .vscode/settings.json, keeping other settings, comments and items
        #[arg(long)]
        write: bool,
        /// With --write, print a JSON summary to standard output (the setting is always JSON)
        #[arg(long)]
        json: bool,
    },
    /// Remove an unscoped library or one of its entries, or uninstall a scoped library
    Remove {
        /// Library name or reference string
        reference: String,
        /// Use the global libraries in the user area, for every project, instead of the project's
        #[arg(short, long)]
        global: bool,
        /// Print a JSON summary to standard output
        #[arg(long)]
        json: bool,
    },
    /// Rewrite unscoped libraries in the form draw.io and VS Code both read, giving entries IDs
    Sync {
        /// Libraries to rewrite (default: all)
        names: Vec<String>,
        /// Only report libraries that need it, with exit code 1
        #[arg(long)]
        check: bool,
        /// Use the global libraries in the user area, for every project, instead of the project's
        #[arg(short, long)]
        global: bool,
        /// Print a JSON summary to standard output
        #[arg(long)]
        json: bool,
    },
    /// List libraries: name, version, entries and title
    List {
        /// Only libraries of this scope
        #[arg(long)]
        scope: Option<String>,
        /// Only the global libraries in the user area
        #[arg(short, long)]
        global: bool,
        /// Print the result as JSON to standard output
        #[arg(long)]
        json: bool,
    },
    /// Search entries; prints reference, kind, size and title, without their content
    Search {
        /// Words the title or ID must contain (case-insensitive); omit for every entry
        query: Vec<String>,
        #[command(flatten)]
        filter: Filter,
        /// Most results to print; 0 for all
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Print the result as JSON to standard output
        #[arg(long)]
        json: bool,
    },
    /// Show a library, or an entry's style, cells and a placeholder example
    Show {
        /// Library name or reference string
        reference: String,
        /// Print the result as JSON to standard output
        #[arg(long)]
        json: bool,
    },
    /// Render matching entries, labelled with their references, into a PNG
    Preview {
        /// References, library names (all their entries) or search words
        #[arg(required_unless_present_any = ["library", "scope", "kind"])]
        targets: Vec<String>,
        /// PNG file to write ("-" for standard output)
        #[arg(short, long)]
        output: PathBuf,
        #[command(flatten)]
        filter: Filter,
        /// Allow external HTTP(S) images and fonts
        #[arg(long)]
        allow_network: bool,
        /// Print the image size and the entries shown as JSON to standard output
        #[arg(long)]
        json: bool,
    },
}

#[derive(clap::Args)]
struct Filter {
    /// Only this library
    #[arg(long, value_name = "NAME")]
    library: Option<String>,
    /// Only libraries of this scope
    #[arg(long)]
    scope: Option<String>,
    /// Only entries of this kind: image, shape, edge, group, set or invalid
    #[arg(long, value_parser = kind_name)]
    kind: Option<EntryKind>,
}

fn renderer_name(text: &str) -> Result<RendererKind, String> {
    RendererKind::parse(text).ok_or_else(|| format!("expected {}", RendererKind::NAMES.join(", ")))
}

fn kind_name(text: &str) -> Result<EntryKind, String> {
    EntryKind::parse(text)
        .ok_or_else(|| "expected image, shape, edge, group, set or invalid".to_owned())
}

/// A failed command: the problems, and whether they go out as JSON.
struct Failure {
    problems: Diagnostics,
    warnings: Vec<Diagnostic>,
}

impl From<Diagnostic> for Failure {
    fn from(d: Diagnostic) -> Self {
        Failure {
            problems: d.into(),
            warnings: Vec::new(),
        }
    }
}

impl From<Diagnostics> for Failure {
    fn from(problems: Diagnostics) -> Self {
        Failure {
            problems,
            warnings: Vec::new(),
        }
    }
}

type Outcome = Result<(), Failure>;

/// Parses the arguments, runs the command and returns the exit code.
pub fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.command.json();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprint!(
                "{}",
                output::diagnostics(&failure.problems.0, &failure.warnings, json)
            );
            if failure.problems.0.iter().any(|d| d.code == Code::Usage) {
                ExitCode::from(2)
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

impl Command {
    fn json(&self) -> bool {
        match self {
            Command::Read { json, .. }
            | Command::Write { json, .. }
            | Command::Expand { json, .. }
            | Command::Validate { json, .. }
            | Command::Init { json, .. } => *json,
            Command::Library { command } => match command {
                LibraryCommand::List { json, .. }
                | LibraryCommand::Search { json, .. }
                | LibraryCommand::Show { json, .. }
                | LibraryCommand::Preview { json, .. } => *json,
                LibraryCommand::Add { json, .. }
                | LibraryCommand::Remove { json, .. }
                | LibraryCommand::Sync { json, .. }
                | LibraryCommand::Install { json, .. }
                | LibraryCommand::Update { json, .. }
                | LibraryCommand::Pack { json, .. }
                | LibraryCommand::Vscode { json, .. } => *json,
            },
            Command::Skill | Command::Licenses => false,
        }
    }
}

/// The adapters a command works with.
struct Context {
    project: Option<Project>,
    /// The global libraries in the user area.
    global: Option<LibraryArea>,
}

impl Context {
    fn new(explicit: Option<PathBuf>) -> Result<Self, Diagnostic> {
        let explicit = explicit.or_else(|| env::var_os("D_PNG_PROJECT").map(PathBuf::from));
        let current = env::current_dir()
            .map_err(|e| Diagnostic::new(Code::InputRead, format!("no current directory: {e}")))?;
        Ok(Self {
            project: Project::discover(explicit.as_deref(), &current)?,
            global: config::user_area().as_deref().map(LibraryArea::global),
        })
    }

    fn configuration(&self) -> Configuration {
        Configuration::new(self.project.as_ref().map(Project::area).as_deref())
    }

    /// The project's `config.toml` over the user's (spec 2.4).
    fn settings(&self) -> Result<Settings, Diagnostic> {
        self.configuration().settings()
    }

    fn project(&self) -> Result<&Project, Diagnostic> {
        self.project.as_ref().ok_or_else(|| {
            Diagnostic::new(
                Code::ProjectNotFound,
                "no project (.d-png/) here or above; run `d-png-cli init`",
            )
        })
    }

    /// The font policy: the options, or else the settings.
    fn fonts(
        &self,
        default: Option<String>,
        fallbacks: &[String],
    ) -> Result<Option<FontPolicy>, Diagnostic> {
        if default.is_some() {
            return font_policy(default, fallbacks);
        }
        let render = self.settings()?.render;
        match (render.default_font, render.fallback_fonts) {
            (Some(default), fallbacks) => FontPolicy::new(&default, &fallbacks.unwrap_or_default())
                .map(Some)
                .map_err(|e| {
                    Diagnostic::new(Code::ConfigInvalid, format!("[render] fonts: {e:#}"))
                }),
            (None, Some(_)) => Err(Diagnostic::new(
                Code::ConfigInvalid,
                "config.toml: [render] fallback-fonts needs default-font",
            )),
            (None, None) => Ok(None),
        }
    }

    /// The renderer: the option, `D_PNG_RENDERER`, the settings, then `vscode`.
    fn renderer(&self, option: Option<RendererKind>) -> Result<RendererKind, Diagnostic> {
        if let Some(kind) = option {
            return Ok(kind);
        }
        if let Some(kind) = RendererKind::from_env()? {
            return Ok(kind);
        }
        match self.settings()?.render.renderer {
            Some(name) => RendererKind::parse(&name).ok_or_else(|| {
                Diagnostic::new(
                    Code::ConfigInvalid,
                    format!(
                        "[render] renderer {name:?}: expected {}",
                        RendererKind::NAMES.join(", ")
                    ),
                )
            }),
            None => Ok(RendererKind::default()),
        }
    }

    /// `--allow-network`, or the settings.
    fn allow_network(&self, option: bool) -> Result<bool, Diagnostic> {
        Ok(option || self.settings()?.render.allow_network.unwrap_or(false))
    }

    /// The project's libraries, or with `global` the global ones, which
    /// commands that change libraries need.
    fn area(&self, global: bool) -> Result<LibraryArea, Diagnostic> {
        if global {
            return self.global.clone().ok_or_else(config::no_user_area);
        }
        self.project
            .as_ref()
            .map(Project::library_area)
            .ok_or_else(|| {
                Diagnostic::new(
                    Code::ProjectNotFound,
                    "no project (.d-png/) here or above; run `d-png-cli init`, or use --global for the global libraries",
                )
            })
    }

    /// The unscoped libraries that `library add`, `remove` and `sync` change.
    fn store(&self, global: bool) -> Result<ProjectLibraries, Diagnostic> {
        Ok(self.area(global)?.libraries())
    }

    /// `[dependencies]` and `lock.json` of the project and of the global
    /// libraries (only these with `only_global`), which `library list` and
    /// `show` describe installed libraries with.
    fn declared(&self, only_global: bool) -> Result<Vec<Declared>, Diagnostic> {
        let mut declared = Vec::new();
        if let Some(project) = self.project.as_ref().filter(|_| !only_global) {
            declared.push(Declared {
                global: false,
                ranges: self.settings()?.dependencies,
                lock: project.lock().load()?,
            });
        }
        if let Some(area) = &self.global {
            declared.push(Declared {
                global: true,
                ranges: Configuration::global().settings()?.dependencies,
                lock: area.lock().load()?,
            });
        }
        Ok(declared)
    }

    /// The libraries references can name: the project's, then the global
    /// ones; only these with `only_global`.
    fn with_libraries<T>(&self, only_global: bool, run: impl FnOnce(&Libraries<'_>) -> T) -> T {
        let areas: Vec<LibraryArea> = self
            .project
            .iter()
            .filter(|_| !only_global)
            .map(Project::library_area)
            .chain(self.global.clone())
            .collect();
        let unscoped: Vec<ProjectLibraries> = areas.iter().map(LibraryArea::libraries).collect();
        let installed: Vec<InstalledLibraries> = areas.iter().map(LibraryArea::installed).collect();
        let mut repositories: Vec<&dyn LibraryRepository> = Vec::new();
        for (unscoped, installed) in unscoped.iter().zip(&installed) {
            repositories.push(unscoped);
            repositories.push(installed);
        }
        run(&Libraries::new(repositories))
    }

    /// Suggests `library vscode --write` when VS Code's settings lack a library.
    fn hint_vscode(&self, locations: Vec<String>) {
        let Some(project) = &self.project else {
            return;
        };
        let settings = VsCodeSettings {
            path: project.root().join(".vscode/settings.json"),
        };
        if locations
            .iter()
            .any(|l| !settings.lists(&format!("${{workspaceFolder}}/{l}")))
        {
            eprintln!("hint: run `d-png-cli library vscode --write` to use it in VS Code");
        }
    }

    /// When `library list` or `search` finds nothing because there is no
    /// library, says how to add one: to the project, or to the global
    /// libraries (only these with `only_global`).
    fn hint_empty(&self, only_global: bool) {
        let (Ok(listed), Ok(declared)) = (
            self.with_libraries(only_global, |libraries| libraries.list()),
            self.declared(only_global),
        ) else {
            return;
        };
        if !listed.is_empty() || declared.iter().any(|d| !d.ranges.is_empty()) {
            return;
        }
        if only_global {
            let Ok(settings) = Configuration::global().settings() else {
                return;
            };
            eprintln!(
                "hint: no global libraries yet; register a file for every project with `d-png-cli library add --global FILE`, {}",
                install_hint(&settings.scopes, " --global", "the user's config.toml")
            );
        } else if self.project.is_none() {
            eprintln!(
                "hint: no project (.d-png/) here or above, and no global libraries; run `d-png-cli init`, or register a file for every project with `d-png-cli library add --global FILE`"
            );
        } else if let Ok(settings) = self.settings() {
            eprintln!(
                "hint: no libraries yet; register a file with `d-png-cli library add FILE`, {}",
                install_hint(&settings.scopes, "", "config.toml")
            );
        }
    }

    /// The stores for installing into the project, or with `global` into
    /// the global libraries, with the sources as the registry.
    fn with_stores<T>(
        &self,
        global: bool,
        offline: bool,
        run: impl FnOnce(&install::Stores<'_>) -> T,
    ) -> Result<T, Diagnostic> {
        let area = self.area(global)?;
        let (configuration, root, command) = if global {
            (
                Configuration::global(),
                None,
                "d-png-cli library install --global",
            )
        } else {
            (
                self.configuration(),
                self.project.as_ref().map(|p| p.root().to_owned()),
                "d-png-cli library install",
            )
        };
        let lock = area.lock();
        let installed = area.installed();
        let registry = Sources::new(
            root,
            config::user_area(),
            Cache::new(sources::cache_dir()),
            offline,
        );
        Ok(run(&install::Stores {
            dependencies: &configuration,
            lock: &lock,
            installed: &installed,
            registry: &registry,
            command,
        }))
    }
}

/// How to install a scoped library: the scopes that have a source, or where
/// to give one a source.
fn install_hint(scopes: &BTreeMap<String, Scope>, flag: &str, config: &str) -> String {
    if scopes.is_empty() {
        format!(
            "or give a scope a source in [scopes] of {config} and run `d-png-cli library install{flag} @SCOPE/NAME`"
        )
    } else {
        let scopes: Vec<String> = scopes.keys().map(|s| format!("@{s}")).collect();
        format!(
            "or install one with `d-png-cli library install{flag} @SCOPE/NAME` (scopes with a source: {})",
            scopes.join(", ")
        )
    }
}

fn run(cli: Cli) -> Outcome {
    match cli.command {
        Command::Skill => print(include_str!("../../../assets/skill/d-png.md")),
        Command::Licenses => print(crate::BUNDLED_LICENSES),
        Command::Init { dir, json } => {
            let directory = match dir {
                Some(dir) => dir,
                None => env::current_dir().map_err(|e| {
                    Diagnostic::new(Code::InputRead, format!("no current directory: {e}"))
                })?,
            };
            let done = project::init(&directory)?;
            if json {
                print_json(
                    &json!({"area": done.area.display().to_string(), "created": done.created}),
                )
            } else {
                print(&format!(
                    "{} {}\n",
                    if done.created { "created" } else { "exists" },
                    done.area.display()
                ))
            }
        }
        Command::Read {
            input,
            output,
            raw,
            json,
        } => {
            require_output_for_json(json, output.as_deref())?;
            let bytes = read_input(input.as_deref())?;
            let context = Context::new(cli.project)?;
            let read =
                context.with_libraries(false, |libraries| diagram::read(&bytes, raw, libraries))?;
            write_text_output(output.as_deref(), &read.xml)?;
            if !raw && !json {
                eprintln!(
                    "{} image(s) replaced with library references, {} kept; use `d-png-cli expand` for the image data",
                    read.compacted.len(),
                    read.kept.len()
                );
            }
            if json {
                print_json(&output::read_json(&read, &display(output.as_deref())))?;
            }
            Ok(())
        }
        Command::Expand {
            input,
            output,
            default_font,
            fallback_font,
            json,
        } => {
            require_output_for_json(json, output.as_deref())?;
            let bytes = read_input(input.as_deref())?;
            let context = Context::new(cli.project)?;
            let fonts = context.fonts(default_font, &fallback_font)?;
            let expanded = context.with_libraries(false, |libraries| {
                diagram::expand(&bytes, fonts.as_ref(), libraries)
            })?;
            write_text_output(output.as_deref(), &expanded.xml)?;
            if json {
                print_json(&json!({
                    "output": display(output.as_deref()),
                    "pages": expanded.pages,
                    "expanded": output::expanded_json(&expanded.expanded),
                    "warnings": [],
                }))?;
            }
            Ok(())
        }
        Command::Validate { input, json } => {
            let bytes = read_input(input.as_deref())?;
            let context = Context::new(cli.project)?;
            let validation =
                context.with_libraries(false, |libraries| diagram::validate(&bytes, libraries))?;
            if json {
                print_json(&json!({
                    "valid": true,
                    "pages": validation.pages,
                    "cells": validation.cells,
                    "refs": validation.refs,
                    "warnings": [],
                }))?;
            }
            Ok(())
        }
        Command::Write {
            input,
            output,
            page,
            renderer,
            allow_network,
            default_font,
            fallback_font,
            metadata_only,
            base_image,
            json,
        } => {
            let to_stdout = output.as_os_str() == "-";
            if to_stdout && json {
                return Err(usage("--json writes to standard output; give -o a file").into());
            }
            // Usage errors before any input is read.
            font_policy(default_font.clone(), &fallback_font)?;
            let bytes = read_input(input.as_deref())?;
            let base = base_image.as_deref().map(read_file).transpose()?;
            let context = Context::new(cli.project)?;
            let fonts = context.fonts(default_font, &fallback_font)?;
            // Rendering settings are read only when rendering.
            let (kind, allow_network) = if metadata_only {
                (RendererKind::default(), false)
            } else {
                (
                    context.renderer(renderer)?,
                    context.allow_network(allow_network)?,
                )
            };
            let renderer = kind.renderer();
            let pixels = if metadata_only {
                Pixels::Keep(base.as_deref())
            } else {
                Pixels::Render {
                    renderer: renderer.as_ref(),
                    allow_network,
                }
            };
            let written = context.with_libraries(false, |libraries| {
                diagram::write(
                    WriteRequest {
                        input: &bytes,
                        page,
                        fonts: fonts.as_ref(),
                        pixels,
                    },
                    libraries,
                )
            })?;
            if to_stdout {
                write_stdout(&written.png)?;
            } else {
                write_file(&output, &written.png)?;
            }
            if json {
                print_json(&output::written_json(
                    &written,
                    &output.display().to_string(),
                    (!metadata_only).then_some(kind.as_str()),
                ))?;
            } else {
                eprint!("{}", output::diagnostics(&[], &written.warnings, false));
            }
            Ok(())
        }
        Command::Library { command } => {
            let context = Context::new(cli.project)?;
            library_command(&context, command)
        }
    }
}

fn library_command(context: &Context, command: LibraryCommand) -> Outcome {
    match command {
        LibraryCommand::Add {
            sources,
            name,
            title,
            replace,
            merge,
            skip_invalid,
            global,
            json,
        } => {
            let store = context.store(global)?;
            let mut files = Vec::new();
            for path in &sources {
                let file_name = (path.as_os_str() != "-")
                    .then(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .flatten();
                files.push((file_name, read_input(Some(path))?));
            }
            let request = register::AddRequest {
                sources: files
                    .iter()
                    .map(|(file_name, bytes)| Source {
                        file_name: file_name.as_deref(),
                        bytes,
                    })
                    .collect(),
                name: name.as_deref(),
                title: title.as_deref(),
                existing: if replace {
                    register::Existing::Replace
                } else if merge {
                    register::Existing::Merge
                } else {
                    register::Existing::Fail
                },
                skip_invalid,
            };
            let added = register::add(&store, request)?;
            if json {
                print_json(&output::added_json(&added))?;
            } else {
                eprint!("{}", output::diagnostics(&[], &added.skipped, false));
                print(&output::added_text(&added))?;
            }
            if !global {
                context.hint_vscode(added.location.iter().cloned().collect());
            }
            Ok(())
        }
        LibraryCommand::Install {
            names,
            frozen,
            offline,
            global,
            json,
        } => {
            let add = names
                .iter()
                .map(|n| requested(n))
                .collect::<Result<Vec<_>, _>>()?;
            let changes = context.with_stores(global, offline, |stores| {
                install::install(stores, &install::InstallRequest { add, frozen })
            })??;
            if json {
                print_json(&output::changes_json(&changes))?;
            } else {
                print(&output::changes_text(&changes, false))?;
            }
            if global {
                return Ok(());
            }
            context.hint_vscode(
                changes
                    .iter()
                    .filter_map(|change| match change {
                        install::Change::Installed { name, .. } => Some(format!(
                            "{}/libraries/@{}/{}.xml",
                            project::AREA,
                            name.scope().unwrap_or(""),
                            name.name()
                        )),
                        _ => None,
                    })
                    .collect(),
            );
            Ok(())
        }
        LibraryCommand::Update {
            names,
            latest,
            dry_run,
            global,
            json,
        } => {
            let names = names
                .iter()
                .map(|n| LibraryName::parse(n))
                .collect::<Result<Vec<_>, _>>()?;
            let changes = context.with_stores(global, false, |stores| {
                install::update(
                    stores,
                    &install::UpdateRequest {
                        names,
                        latest,
                        dry_run,
                    },
                )
            })??;
            if json {
                print_json(&output::changes_json(&changes))
            } else {
                print(&output::changes_text(&changes, dry_run))
            }
        }
        LibraryCommand::Pack {
            sources,
            name,
            version,
            to,
            title,
            skip_invalid,
            json,
        } => {
            let name = LibraryName::parse(&name).map_err(|d| usage(d.message))?;
            let version = version.parse().map_err(usage)?;
            let mut files = Vec::new();
            for path in &sources {
                let file_name = (path.as_os_str() != "-")
                    .then(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .flatten();
                files.push((file_name, read_input(Some(path))?));
            }
            let target = SourceDirectory::new(to.clone());
            let packed = install::pack(
                &target,
                install::PackRequest {
                    sources: files
                        .iter()
                        .map(|(file_name, bytes)| Source {
                            file_name: file_name.as_deref(),
                            bytes,
                        })
                        .collect(),
                    name,
                    version,
                    title: title.as_deref(),
                    skip_invalid,
                },
            )?;
            let path = to
                .join(format!("@{}", packed.name.scope().unwrap_or("")))
                .join(packed.name.name())
                .join(&packed.file);
            if json {
                print_json(&output::packed_json(&packed, &path.display().to_string()))
            } else {
                eprint!("{}", output::diagnostics(&[], &packed.warnings, false));
                print(&output::packed_text(&packed, &path.display().to_string()))
            }
        }
        LibraryCommand::Vscode {
            entry_id,
            write,
            json,
        } => {
            let project = context.project()?;
            let ids = EntryIds::new(entry_id.as_deref())?;
            if !write {
                let items = context
                    .with_libraries(false, |libraries| vscode::custom_libraries(libraries, &ids))?;
                return print(&crate::adapters::vscode::value(&items));
            }
            let settings = VsCodeSettings {
                path: project.root().join(".vscode/settings.json"),
            };
            let items = context
                .with_libraries(false, |libraries| vscode::write(&settings, libraries, &ids))?;
            if json {
                print_json(&json!({"file": ".vscode/settings.json", "libraries": items.len()}))
            } else {
                print(&format!(
                    "wrote {} {} to .vscode/settings.json\n",
                    items.len(),
                    if items.len() == 1 {
                        "library"
                    } else {
                        "libraries"
                    }
                ))
            }
        }
        LibraryCommand::Remove {
            reference,
            global,
            json,
        } => {
            if let Ok(name) = LibraryName::parse(&reference)
                && name.scope().is_some()
            {
                context
                    .with_stores(global, false, |stores| install::uninstall(stores, &name))??;
                return if json {
                    print_json(&json!({"removed": name.to_string(), "type": "installed"}))
                } else {
                    print(&format!("removed {name}\n"))
                };
            }
            let store = context.store(global)?;
            let removed = register::remove(&store, &reference)?;
            let (kind, name) = match &removed {
                register::Removed::Library(name) => ("library", name.to_string()),
                register::Removed::Entry(reference) => ("entry", reference.to_string()),
            };
            if json {
                print_json(&json!({"removed": name, "type": kind}))
            } else {
                print(&format!("removed {name}\n"))
            }
        }
        LibraryCommand::Sync {
            names,
            check,
            global,
            json,
        } => {
            let store = context.store(global)?;
            let synced = register::sync(&store, &names, check)?;
            if check {
                // Installed libraries must match lock.json too.
                context.with_stores(global, true, install::verify)??;
            }
            let names: Vec<String> = synced.normalized.iter().map(|n| n.to_string()).collect();
            if json {
                print_json(&json!({"normalized": names, "checked": synced.checked}))
            } else {
                print(
                    &names
                        .iter()
                        .map(|n| format!("normalized {n}\n"))
                        .collect::<String>(),
                )
            }
        }
        LibraryCommand::List {
            scope,
            global,
            json,
        } => {
            let declared = context.declared(global)?;
            let listed = context.with_libraries(global, |libraries| {
                library::list(libraries, &declared, scope.as_deref())
            })?;
            if json {
                print_json(&Value::Array(
                    listed.iter().map(output::library_json).collect(),
                ))?;
            } else {
                for listed in &listed {
                    let info = &listed.info;
                    if !listed.installed {
                        eprintln!(
                            "warning: {} is not installed; run `d-png-cli library install{}`",
                            info.name,
                            if info.global { " --global" } else { "" }
                        );
                    } else if !info.normalized {
                        eprintln!(
                            "warning: {} is not in the form the VS Code extension reads; run `d-png-cli library sync {}`",
                            info.location.as_deref().unwrap_or("-"),
                            info.name
                        );
                    }
                }
                print(&listed.iter().map(output::library_line).collect::<String>())?;
            }
            if listed.is_empty() {
                context.hint_empty(global);
            }
            Ok(())
        }
        LibraryCommand::Search {
            query,
            filter,
            limit,
            json,
        } => {
            let query = SearchQuery {
                words: query,
                ..filter.query()?
            };
            let found = context
                .with_libraries(false, |libraries| library::search(libraries, &query, limit))?;
            if found.hits.len() < found.total {
                eprintln!(
                    "{} of {} entries shown; narrow the search or use --limit 0",
                    found.hits.len(),
                    found.total
                );
            }
            if json {
                print_json(&Value::Array(
                    found.hits.iter().map(output::hit_json).collect(),
                ))?;
            } else {
                print(&found.hits.iter().map(output::hit_line).collect::<String>())?;
            }
            if found.total == 0 {
                context.hint_empty(false);
            }
            Ok(())
        }
        LibraryCommand::Show { reference, json } => {
            let declared = context.declared(false)?;
            let shown = context.with_libraries(false, |libraries| {
                library::show(libraries, &declared, &reference)
            })?;
            if json {
                print_json(&output::shown_json(&shown))
            } else {
                print(&output::shown_text(&shown))
            }
        }
        LibraryCommand::Preview {
            targets,
            output,
            filter,
            allow_network,
            json,
        } => {
            let to_stdout = output.as_os_str() == "-";
            if to_stdout && json {
                return Err(usage("--json writes to standard output; give -o a file").into());
            }
            let request = PreviewRequest {
                targets,
                query: filter.query()?,
            };
            let renderer = RendererKind::Vscode.renderer();
            let allow_network = context.allow_network(allow_network)?;
            let previewed = context.with_libraries(false, |libraries| {
                library::preview(libraries, &request, renderer.as_ref(), allow_network)
            })?;
            if to_stdout {
                write_stdout(&previewed.png)?;
            } else {
                write_file(&output, &previewed.png)?;
            }
            if json {
                print_json(&output::previewed_json(
                    &previewed,
                    &output.display().to_string(),
                ))?;
            }
            Ok(())
        }
    }
}

impl Filter {
    fn query(&self) -> Result<SearchQuery, Diagnostic> {
        Ok(SearchQuery {
            words: Vec::new(),
            library: self
                .library
                .as_deref()
                .map(LibraryName::parse)
                .transpose()?,
            scope: self.scope.clone(),
            kind: self.kind,
        })
    }
}

/// `@acme/brand` or `@acme/brand@^1.2.0`: a library and an optional range.
fn requested(text: &str) -> Result<(LibraryName, Option<Range>), Diagnostic> {
    let (name, range) = match text.rfind('@').filter(|at| *at > 0) {
        Some(at) => (
            &text[..at],
            Some(text[at + 1..].parse::<Range>().map_err(usage)?),
        ),
        None => (text, None),
    };
    Ok((
        LibraryName::parse(name).map_err(|d| usage(d.message))?,
        range,
    ))
}

fn usage(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Code::Usage, message)
}

fn require_output_for_json(json: bool, output: Option<&Path>) -> Result<(), Diagnostic> {
    if json && output.is_none_or(|p| p.as_os_str() == "-") {
        return Err(usage(
            "--json prints to standard output; give -o a file for the XML",
        ));
    }
    Ok(())
}

fn font_policy(
    default: Option<String>,
    fallbacks: &[String],
) -> Result<Option<FontPolicy>, Diagnostic> {
    default
        .map(|default| FontPolicy::new(&default, fallbacks))
        .transpose()
        .map_err(|e| Diagnostic::from_error(Code::Usage, &e))
}

/// Codes a failure to read: size limits, or anything else.
fn read_error(error: &anyhow::Error) -> Diagnostic {
    let message = format!("{error:#}");
    if message.contains("exceeds 64 MiB") {
        Diagnostic::new(Code::LimitExceeded, message)
    } else {
        Diagnostic::new(Code::InputRead, message)
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>, Diagnostic> {
    fs::read(path).map_err(|e| read_error(&e))
}

/// The file, or standard input for none or "-".
fn read_input(path: Option<&Path>) -> Result<Vec<u8>, Diagnostic> {
    match path {
        Some(path) if path.as_os_str() != "-" => read_file(path),
        _ => fs::read_limited(io::stdin().lock()).map_err(|e| read_error(&e)),
    }
}

fn display(path: Option<&Path>) -> String {
    path.map_or("-".into(), |p| p.display().to_string())
}

fn write_file(path: &Path, bytes: &[u8]) -> Outcome {
    fs::atomic_write(path, bytes).map_err(|e| Diagnostic::from_error(Code::OutputWrite, &e).into())
}

fn write_stdout(bytes: &[u8]) -> Outcome {
    match io::stdout().lock().write_all(bytes) {
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(Diagnostic::new(Code::OutputWrite, format!("cannot write: {e}")).into()),
        Ok(()) => Ok(()),
    }
}

/// XML to the file, or standard output for none or "-".
fn write_text_output(path: Option<&Path>, text: &str) -> Outcome {
    match path {
        Some(path) if path.as_os_str() != "-" => write_file(path, text.as_bytes()),
        _ => write_stdout(text.as_bytes()),
    }
}

/// Writes to stdout; a consumer such as `head` may stop reading early.
fn print(text: &str) -> Outcome {
    write_stdout(text.as_bytes())
}

fn print_json(value: &Value) -> Outcome {
    print(&format!("{value}\n"))
}
